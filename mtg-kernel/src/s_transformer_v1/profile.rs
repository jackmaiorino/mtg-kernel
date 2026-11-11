//! Measurement-only attribution of the S learner (issue #598, first
//! milestone). Compiled in only with the `s-learner-profile-v1` feature;
//! `s_transformer_v1 gae-bench` then adds one report per repetition. Default
//! builds get zero-sized no-op spans and counters, so uninstrumented timings
//! come from a build without the feature. Nothing here reads or writes a
//! value the learner computes: every loss, gradient and Adam bit is the same
//! with and without it.
//!
//! One report covers one update:
//! - inclusive span times of the train step's gradient, Adam step and state
//!   hash; each substep's forward, policy row and backward; the backward's
//!   heads, per-layer sections and token inputs; the gradient sink's `Linear`
//!   work (the weight-gradient GEMM on the serial path, the operand copies on
//!   the parallel path); and the parallel path's record drain, score merge and
//!   replay stages. With [`SProfileAllocatorV1`] installed (the binary does
//!   so), each span also counts the allocator events of its thread;
//! - the bytes the parallel path copies into its term records: `Linear`
//!   inputs (activation operands) and output gradients (adjoints), and the
//!   LayerNorm gain terms it materializes and bias terms it copies;
//! - each drain's helper spawns, spawn failures and joins, the caller's spawn
//!   and join-wait time and every worker's items and busy time;
//! - each replay shard's parameter count against the rows and multiply-adds
//!   it replays, its time and its workers;
//! - the peak live and reserved term bytes of one block and, with the
//!   allocator, the process's peak live heap bytes;
//! - an analytic multiply-add census of the work the backward runs, by layer
//!   and section, with the final block's object query rows (which no output
//!   reads) apart: their analytic work and the part the backward skipped.
//!
//! Spans nest: `gradient` encloses the substep and replay spans, `backward`
//! its sections, and those `sink_linear`. Every time is inclusive. Helper
//! threads merge their counts into the process total when their drain ends;
//! [`take_v1`] merges the caller's and returns and clears the total.

/// Inclusive span kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StageV1 {
    /// One train step's gradient: the zeroed gradient buffer, every
    /// substep's work, the gauge zeroing and the finite check.
    Gradient = 0,
    /// The Adam update applied to the parameters (with the transposes).
    Adam,
    /// The full train-state hash the step returns.
    StateHash,
    /// One substep's forward.
    Forward,
    /// One substep's log-softmax row and logit gradient.
    PolicyRow,
    /// One substep's backward, including its sink work.
    Backward,
    /// The backward's value and policy heads and final LayerNorm.
    BackHeads,
    /// The backward's token inputs, edge bias and reference bias.
    BackInputs,
    /// One `Linear` term in the gradient sink: the weight and bias gradient
    /// accumulation (serial) or the input and output-gradient copies
    /// (parallel).
    SinkLinear,
    /// One block's forward and backward drain (parallel path, caller).
    RecordDrain,
    /// One block's ordered loss-term merge (parallel path).
    ScoreMerge,
    /// One block's serial additions and `Linear` bias gradients.
    ReplayAdditions,
    /// One block's weight-shard partition and sort.
    ReplayShardBuild,
    /// One block's weight-shard drain (parallel path, caller).
    ReplayDrain,
}

/// Per-layer backward sections, in backward order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SectionV1 {
    Ffn = 0,
    Ln2,
    Proj,
    Attention,
    Qkv,
    Ln1,
}

/// Which scoped drain a census describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DrainV1 {
    /// Forward and recorded backward of one block's substeps.
    Record = 0,
    /// Replay of one block's weight-gradient terms by shard.
    Replay,
}

#[derive(Default)]
pub(crate) struct BlockBytesV1 {
    pub(super) values: usize,
    pub(super) records: usize,
    pub(super) active_capacity: usize,
    pub(super) retained_capacity: usize,
}

#[cfg(feature = "s-learner-profile-v1")]
mod imp {
    use super::{DrainV1, SectionV1, StageV1};
    use crate::s_transformer_v1::{
        CountsV1, LayoutV1, SConfigV1, ACTION_FEATURE_DIM_V1, EDGE_FEATURE_DIM_V1,
        OBJECT_FEATURE_DIM_V1, REF_FEATURE_DIM_V1, STATE_KEEP_V1,
    };
    use serde_json::{json, Value};
    use std::alloc::{GlobalAlloc, Layout};
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
    use std::sync::{Mutex, PoisonError};
    use std::time::Instant;

    const STAGES_V1: usize = 14;
    const STAGE_NAMES_V1: [&str; STAGES_V1] = [
        "gradient",
        "adam",
        "state_hash",
        "forward",
        "policy_row",
        "backward",
        "back_heads",
        "back_inputs",
        "sink_linear",
        "record_drain",
        "score_merge",
        "replay_additions",
        "replay_shard_build",
        "replay_drain",
    ];
    const SECTIONS_V1: usize = 6;
    const SECTION_NAMES_V1: [&str; SECTIONS_V1] = ["ffn", "ln2", "proj", "attention", "qkv", "ln1"];
    const F32_BYTES_V1: u64 = std::mem::size_of::<f32>() as u64;

    // Allocator counts. Per-thread events and bytes (const-initialized
    // cells without destructors, safe to touch from the allocator), and a
    // process-wide live-byte level and peak.
    static COUNTED: AtomicBool = AtomicBool::new(false);
    static LIVE: AtomicI64 = AtomicI64::new(0);
    static PEAK: AtomicI64 = AtomicI64::new(0);
    static START: AtomicI64 = AtomicI64::new(0);

    thread_local! {
        static ALLOCS: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
        static PAUSED: Cell<bool> = const { Cell::new(false) };
        static WORKER: Cell<usize> = const { Cell::new(0) };
    }

    fn note_alloc_v1(bytes: usize) {
        if !COUNTED.load(Ordering::Relaxed) {
            COUNTED.store(true, Ordering::Relaxed);
        }
        let paused = PAUSED.try_with(Cell::get).unwrap_or(true);
        if !paused {
            let _ = ALLOCS.try_with(|cell| {
                let (events, total) = cell.get();
                cell.set((events + 1, total + bytes as u64));
            });
        }
    }

    fn note_live_v1(delta: i64) {
        let live = LIVE.fetch_add(delta, Ordering::Relaxed) + delta;
        PEAK.fetch_max(live, Ordering::Relaxed);
    }

    /// The `s_transformer_v1` binary's global allocator in profile builds:
    /// [`crate::FastAllocatorV1`] plus per-thread event and byte counts and a
    /// process-wide live-byte peak. It changes cost, not results.
    pub struct SProfileAllocatorV1;

    // SAFETY: every call forwards to `FastAllocatorV1` with the caller's
    // arguments and only updates counters afterwards.
    unsafe impl GlobalAlloc for SProfileAllocatorV1 {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // SAFETY: forwarded unchanged.
            let pointer = unsafe { crate::FastAllocatorV1.alloc(layout) };
            if !pointer.is_null() {
                note_alloc_v1(layout.size());
                note_live_v1(layout.size() as i64);
            }
            pointer
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            // SAFETY: forwarded unchanged.
            let pointer = unsafe { crate::FastAllocatorV1.alloc_zeroed(layout) };
            if !pointer.is_null() {
                note_alloc_v1(layout.size());
                note_live_v1(layout.size() as i64);
            }
            pointer
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            // SAFETY: forwarded unchanged.
            unsafe { crate::FastAllocatorV1.dealloc(pointer, layout) };
            note_live_v1(-(layout.size() as i64));
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            // SAFETY: forwarded unchanged.
            let moved = unsafe { crate::FastAllocatorV1.realloc(pointer, layout, new_size) };
            if !moved.is_null() {
                note_alloc_v1(new_size);
                note_live_v1(new_size as i64 - layout.size() as i64);
            }
            moved
        }
    }

    fn allocs_now_v1() -> (u64, u64) {
        ALLOCS.try_with(Cell::get).unwrap_or((0, 0))
    }

    /// Runs profile bookkeeping without counting its own allocations.
    fn untracked_v1<R>(f: impl FnOnce() -> R) -> R {
        let previous = PAUSED.with(|paused| paused.replace(true));
        let result = f();
        PAUSED.with(|paused| paused.set(previous));
        result
    }

    #[derive(Clone, Copy, Default)]
    struct CostV1 {
        spans: u64,
        ns: u64,
        allocs: u64,
        alloc_bytes: u64,
    }

    impl CostV1 {
        fn add(&mut self, other: &Self) {
            self.spans += other.spans;
            self.ns += other.ns;
            self.allocs += other.allocs;
            self.alloc_bytes += other.alloc_bytes;
        }

        fn json(&self) -> Value {
            json!({
                "spans": self.spans,
                "ns": self.ns,
                "allocs": self.allocs,
                "alloc_bytes": self.alloc_bytes,
            })
        }
    }

    /// Multiply-adds of input gradients (`dx`), weight gradients (`dw`, run
    /// in the sink on the serial path and in replay on the parallel path) and
    /// attention products, plus row-wise LayerNorm or softmax rows.
    #[derive(Clone, Copy, Default)]
    struct MacsV1 {
        dx: u64,
        dw: u64,
        attention: u64,
        rows: u64,
    }

    impl MacsV1 {
        fn linear(rows: usize, in_dim: usize, out_dim: usize, dx: bool) -> Self {
            let product = (rows * in_dim * out_dim) as u64;
            Self {
                dx: if dx { product } else { 0 },
                dw: product,
                attention: 0,
                rows: 0,
            }
        }

        fn add(&mut self, other: &Self) {
            self.dx += other.dx;
            self.dw += other.dw;
            self.attention += other.attention;
            self.rows += other.rows;
        }

        fn json(&self) -> Value {
            json!({
                "dx": self.dx,
                "dw": self.dw,
                "attention": self.attention,
                "rows": self.rows,
            })
        }
    }

    #[derive(Default)]
    struct TermsV1 {
        linear_terms: u64,
        linear_input_bytes: u64,
        linear_output_gradient_bytes: u64,
        layer_norm_rows: u64,
        layer_norm_gain_bytes: u64,
        layer_norm_bias_bytes: u64,
        additions: u64,
        addition_bytes: u64,
        blocks: u64,
        block_substeps: u64,
        block_active_workers: u64,
        value_bytes: u64,
        record_bytes: u64,
        peak_live_bytes: u64,
        peak_reserved_bytes: u64,
        peak_retained_capacity_bytes: u64,
    }

    #[derive(Default)]
    struct DrainStatV1 {
        drains: u64,
        worker_slots: u64,
        helper_spawns: u64,
        spawn_failures: u64,
        joins: u64,
        spawn_ns: u64,
        join_wait_ns: u64,
        wall_ns: u64,
        max_busy_ns: u64,
        busy_ns: Vec<u64>,
        items: Vec<u64>,
        allocs: Vec<u64>,
    }

    #[derive(Default)]
    struct ShardStatV1 {
        tensor: String,
        rows: u64,
        params: u64,
        visits: u64,
        macs: u64,
        term_rows: u64,
        scanned: u64,
        matched: u64,
        ns: u64,
        by_worker: Vec<u64>,
    }

    #[derive(Default)]
    struct AccV1 {
        stages: [CostV1; STAGES_V1],
        sections: Vec<[CostV1; SECTIONS_V1]>,
        layers: usize,
        substeps: u64,
        heads: MacsV1,
        inputs: MacsV1,
        macs: Vec<[MacsV1; SECTIONS_V1]>,
        dead: [MacsV1; SECTIONS_V1],
        removed: [MacsV1; SECTIONS_V1],
        live_final_substeps: u64,
        terms: TermsV1,
        drains: [DrainStatV1; 2],
        shards: BTreeMap<(usize, usize), ShardStatV1>,
    }

    fn add_into_v1(target: &mut Vec<u64>, source: &[u64]) {
        if target.len() < source.len() {
            target.resize(source.len(), 0);
        }
        for (slot, value) in target.iter_mut().zip(source) {
            *slot += *value;
        }
    }

    impl AccV1 {
        fn merge(&mut self, other: Self) {
            for (slot, cost) in self.stages.iter_mut().zip(&other.stages) {
                slot.add(cost);
            }
            if self.sections.len() < other.sections.len() {
                self.sections
                    .resize(other.sections.len(), Default::default());
            }
            for (slot, layer) in self.sections.iter_mut().zip(&other.sections) {
                for (cost, other) in slot.iter_mut().zip(layer) {
                    cost.add(other);
                }
            }
            self.layers = self.layers.max(other.layers);
            self.substeps += other.substeps;
            self.heads.add(&other.heads);
            self.inputs.add(&other.inputs);
            if self.macs.len() < other.macs.len() {
                self.macs.resize(other.macs.len(), Default::default());
            }
            for (slot, layer) in self.macs.iter_mut().zip(&other.macs) {
                for (macs, other) in slot.iter_mut().zip(layer) {
                    macs.add(other);
                }
            }
            for (slot, other) in self.dead.iter_mut().zip(&other.dead) {
                slot.add(other);
            }
            for (slot, other) in self.removed.iter_mut().zip(&other.removed) {
                slot.add(other);
            }
            self.live_final_substeps += other.live_final_substeps;
            let (t, o) = (&mut self.terms, &other.terms);
            t.linear_terms += o.linear_terms;
            t.linear_input_bytes += o.linear_input_bytes;
            t.linear_output_gradient_bytes += o.linear_output_gradient_bytes;
            t.layer_norm_rows += o.layer_norm_rows;
            t.layer_norm_gain_bytes += o.layer_norm_gain_bytes;
            t.layer_norm_bias_bytes += o.layer_norm_bias_bytes;
            t.additions += o.additions;
            t.addition_bytes += o.addition_bytes;
            t.blocks += o.blocks;
            t.block_substeps += o.block_substeps;
            t.block_active_workers += o.block_active_workers;
            t.value_bytes += o.value_bytes;
            t.record_bytes += o.record_bytes;
            t.peak_live_bytes = t.peak_live_bytes.max(o.peak_live_bytes);
            t.peak_reserved_bytes = t.peak_reserved_bytes.max(o.peak_reserved_bytes);
            t.peak_retained_capacity_bytes = t
                .peak_retained_capacity_bytes
                .max(o.peak_retained_capacity_bytes);
            for (slot, other) in self.drains.iter_mut().zip(&other.drains) {
                slot.drains += other.drains;
                slot.worker_slots += other.worker_slots;
                slot.helper_spawns += other.helper_spawns;
                slot.spawn_failures += other.spawn_failures;
                slot.joins += other.joins;
                slot.spawn_ns += other.spawn_ns;
                slot.join_wait_ns += other.join_wait_ns;
                slot.wall_ns += other.wall_ns;
                slot.max_busy_ns += other.max_busy_ns;
                add_into_v1(&mut slot.busy_ns, &other.busy_ns);
                add_into_v1(&mut slot.items, &other.items);
                add_into_v1(&mut slot.allocs, &other.allocs);
            }
            for (key, other) in other.shards {
                let slot = self.shards.entry(key).or_default();
                if slot.tensor.is_empty() {
                    slot.tensor = other.tensor;
                    slot.rows = other.rows;
                    slot.params = other.params;
                }
                slot.visits += other.visits;
                slot.macs += other.macs;
                slot.term_rows += other.term_rows;
                slot.scanned += other.scanned;
                slot.matched += other.matched;
                slot.ns += other.ns;
                add_into_v1(&mut slot.by_worker, &other.by_worker);
            }
        }
    }

    thread_local! {
        static ACC: RefCell<AccV1> = RefCell::new(AccV1::default());
    }

    static GLOBAL: Mutex<Option<AccV1>> = Mutex::new(None);

    fn with_acc_v1(f: impl FnOnce(&mut AccV1)) {
        untracked_v1(|| ACC.with(|acc| f(&mut acc.borrow_mut())));
    }

    /// Moves this thread's counts into the process total.
    fn flush_thread_v1() {
        untracked_v1(|| {
            let acc = ACC.with(|acc| std::mem::take(&mut *acc.borrow_mut()));
            GLOBAL
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert_with(AccV1::default)
                .merge(acc);
        });
    }

    enum TargetV1 {
        Stage(StageV1),
        Section(SectionV1, usize),
    }

    pub(crate) struct SpanV1 {
        target: TargetV1,
        allocs: (u64, u64),
        start: Instant,
    }

    impl Drop for SpanV1 {
        fn drop(&mut self) {
            let ns = self.start.elapsed().as_nanos() as u64;
            let (events, bytes) = allocs_now_v1();
            let cost = CostV1 {
                spans: 1,
                ns,
                allocs: events - self.allocs.0,
                alloc_bytes: bytes - self.allocs.1,
            };
            let target = &self.target;
            with_acc_v1(|acc| match *target {
                TargetV1::Stage(stage) => acc.stages[stage as usize].add(&cost),
                TargetV1::Section(section, layer) => {
                    if acc.sections.len() <= layer {
                        acc.sections.resize(layer + 1, Default::default());
                    }
                    acc.sections[layer][section as usize].add(&cost);
                }
            });
        }
    }

    fn start_v1(target: TargetV1) -> SpanV1 {
        let allocs = allocs_now_v1();
        SpanV1 {
            target,
            allocs,
            start: Instant::now(),
        }
    }

    pub(crate) fn span_v1(stage: StageV1) -> SpanV1 {
        start_v1(TargetV1::Stage(stage))
    }

    pub(crate) fn section_v1(section: SectionV1, layer: usize) -> SpanV1 {
        start_v1(TargetV1::Section(section, layer))
    }

    /// A recorded `Linear` term: `rows * in_dim` copied inputs and
    /// `rows * out_dim` copied output gradients.
    pub(crate) fn linear_term_v1(rows: usize, in_dim: usize, out_dim: usize) {
        with_acc_v1(|acc| {
            let terms = &mut acc.terms;
            terms.linear_terms += 1;
            terms.linear_input_bytes += (rows * in_dim) as u64 * F32_BYTES_V1;
            terms.linear_output_gradient_bytes += (rows * out_dim) as u64 * F32_BYTES_V1;
        });
    }

    /// A recorded LayerNorm row: `width` materialized gain terms and
    /// `width` copied bias terms.
    pub(crate) fn layer_norm_term_v1(width: usize) {
        with_acc_v1(|acc| {
            let terms = &mut acc.terms;
            terms.layer_norm_rows += 1;
            terms.layer_norm_gain_bytes += width as u64 * F32_BYTES_V1;
            terms.layer_norm_bias_bytes += width as u64 * F32_BYTES_V1;
        });
    }

    /// A recorded embedding or reference-bias addition of `len` values.
    pub(crate) fn addition_term_v1(len: usize) {
        with_acc_v1(|acc| {
            acc.terms.additions += 1;
            acc.terms.addition_bytes += len as u64 * F32_BYTES_V1;
        });
    }

    /// One recorded block's live records and capacity, including the
    /// capacity still held by inactive slots in a partial block.
    pub(crate) fn block_v1(
        substeps: usize,
        active_workers: usize,
        bytes: impl FnOnce() -> super::BlockBytesV1,
    ) {
        let bytes = bytes();
        with_acc_v1(|acc| {
            let terms = &mut acc.terms;
            terms.blocks += 1;
            terms.block_substeps += substeps as u64;
            terms.block_active_workers += active_workers as u64;
            terms.value_bytes += bytes.values as u64;
            terms.record_bytes += bytes.records as u64;
            terms.peak_live_bytes = terms
                .peak_live_bytes
                .max((bytes.values + bytes.records) as u64);
            terms.peak_reserved_bytes = terms.peak_reserved_bytes.max(bytes.active_capacity as u64);
            terms.peak_retained_capacity_bytes = terms
                .peak_retained_capacity_bytes
                .max(bytes.retained_capacity as u64);
        });
    }

    /// The backward's multiply-adds for one substep, from its counts and the
    /// number of query rows its final block visits (`final_rows`: every
    /// token, or the state and action tokens when it skips the object query
    /// rows).
    pub(crate) fn backward_census_v1(config: &SConfigV1, counts: CountsV1, final_rows: usize) {
        let d = config.d_model;
        let h = config.heads;
        let dh = config.head_dim_v1();
        let ffn = config.ffn;
        let cd = config.card_dim;
        let (n, e, a, r) = (counts.objects, counts.edges, counts.actions, counts.refs);
        let l = 1 + n + a;
        let linear = MacsV1::linear;
        let mut heads = MacsV1::default();
        for term in [
            linear(1, d, 1, true),
            linear(1, d, d, true),
            linear(a, d, 1, true),
            linear(a, 2 * d, d, true),
        ] {
            heads.add(&term);
        }
        heads.rows = (1 + a) as u64;
        let mut inputs = MacsV1::default();
        for term in [
            linear(1, d, d, true),
            linear(1, STATE_KEEP_V1, d, false),
            linear(n, d, d, true),
            linear(n, OBJECT_FEATURE_DIM_V1 + cd, d, true),
            linear(a, d, d, true),
            linear(a, ACTION_FEATURE_DIM_V1, d, false),
            linear(r, d, d, true),
            linear(r, REF_FEATURE_DIM_V1 + cd, d, true),
            linear(e, EDGE_FEATURE_DIM_V1, 2 * h, false),
        ] {
            inputs.add(&term);
        }
        // A block's backward over `m` query rows and all `l` keys. Attention
        // per head: dP = dO V^T, dV = P^T dO, dQ = dS K and dK = dS^T Q,
        // each `m * l * dh`. With fewer query rows than tokens, QKV splits
        // into its query rows (the live rows) and its key and value rows
        // (every token).
        let block = |m: usize| {
            let mut block = [MacsV1::default(); SECTIONS_V1];
            block[SectionV1::Ffn as usize] = linear(m, ffn, d, true);
            block[SectionV1::Ffn as usize].add(&linear(m, d, ffn, true));
            block[SectionV1::Ln2 as usize].rows = m as u64;
            block[SectionV1::Proj as usize] = linear(m, d, d, true);
            block[SectionV1::Attention as usize].attention = (4 * m * l * dh * h) as u64;
            block[SectionV1::Attention as usize].rows = (h * m) as u64;
            block[SectionV1::Qkv as usize] = if m < l {
                let mut qkv = linear(m, d, d, true);
                qkv.add(&linear(l, d, 2 * d, true));
                qkv
            } else {
                linear(l, d, 3 * d, true)
            };
            block[SectionV1::Ln1 as usize].rows = l as u64;
            block
        };
        let full = block(l);
        let last = block(final_rows);
        // The final block's forward skips its object query rows, so their
        // adjoints are zero: the FFN, LN2 and projection rows, the dP and dQ
        // rows and their zero terms in dV and dK, and the zero Q columns of
        // the QKV input and weight gradients. Their object keys and values
        // stay live. The backward skips this work when every final-block
        // Q, K and V value is finite (`final_block_compacts_v1`).
        let mut dead = [MacsV1::default(); SECTIONS_V1];
        dead[SectionV1::Ffn as usize] = linear(n, ffn, d, true);
        dead[SectionV1::Ffn as usize].add(&linear(n, d, ffn, true));
        dead[SectionV1::Ln2 as usize].rows = n as u64;
        dead[SectionV1::Proj as usize] = linear(n, d, d, true);
        dead[SectionV1::Attention as usize].attention = (4 * n * l * dh * h) as u64;
        dead[SectionV1::Attention as usize].rows = (h * n) as u64;
        dead[SectionV1::Qkv as usize] = linear(n, d, d, true);
        let live = final_rows < l;
        with_acc_v1(|acc| {
            acc.substeps += 1;
            acc.live_final_substeps += u64::from(live);
            acc.layers = acc.layers.max(config.layers);
            acc.heads.add(&heads);
            acc.inputs.add(&inputs);
            if acc.macs.len() < config.layers {
                acc.macs.resize(config.layers, Default::default());
            }
            for (layer, slots) in acc.macs[..config.layers].iter_mut().enumerate() {
                let macs = if layer + 1 == config.layers {
                    &last
                } else {
                    &full
                };
                for (slot, macs) in slots.iter_mut().zip(macs) {
                    slot.add(macs);
                }
            }
            for (slot, macs) in acc.dead.iter_mut().zip(&dead) {
                slot.add(macs);
            }
            if live {
                for (slot, macs) in acc.removed.iter_mut().zip(&dead) {
                    slot.add(macs);
                }
            }
        });
    }

    #[derive(Default)]
    struct DrainRunV1 {
        helper_spawns: u64,
        spawn_failures: u64,
        joins: u64,
        spawn_ns: u64,
        join_wait_ns: u64,
        busy_ns: Vec<u64>,
        items: Vec<u64>,
        allocs: Vec<u64>,
    }

    pub(crate) struct DrainCensusV1 {
        kind: DrainV1,
        workers: usize,
        run: Mutex<DrainRunV1>,
        start: Instant,
    }

    pub(crate) struct MarkV1(Instant);

    pub(crate) fn drain_v1(kind: DrainV1, workers: usize) -> DrainCensusV1 {
        let run = untracked_v1(|| DrainRunV1 {
            busy_ns: vec![0; workers],
            items: vec![0; workers],
            allocs: vec![0; workers],
            ..DrainRunV1::default()
        });
        DrainCensusV1 {
            kind,
            workers,
            run: Mutex::new(run),
            start: Instant::now(),
        }
    }

    impl DrainCensusV1 {
        fn run_v1(&self, f: impl FnOnce(&mut DrainRunV1)) {
            untracked_v1(|| f(&mut self.run.lock().unwrap_or_else(PoisonError::into_inner)));
        }

        pub(crate) fn mark_v1(&self) -> MarkV1 {
            MarkV1(Instant::now())
        }

        pub(crate) fn spawned_v1(&self, spawned: bool) {
            self.run_v1(|run| {
                if spawned {
                    run.helper_spawns += 1;
                } else {
                    run.spawn_failures += 1;
                }
            });
        }

        pub(crate) fn spawn_done_v1(&self, mark: MarkV1) {
            let ns = mark.0.elapsed().as_nanos() as u64;
            self.run_v1(|run| run.spawn_ns += ns);
        }

        pub(crate) fn join_done_v1(&self, mark: MarkV1, joins: usize) {
            let ns = mark.0.elapsed().as_nanos() as u64;
            self.run_v1(|run| {
                run.join_wait_ns += ns;
                run.joins += joins as u64;
            });
        }

        /// The census of worker `worker` (0: the caller) for this drain.
        pub(crate) fn worker_v1(&self, worker: usize) -> WorkerV1<'_> {
            WORKER.with(|slot| slot.set(worker));
            WorkerV1 {
                census: self,
                worker,
                items: 0,
                busy_ns: 0,
                allocs: 0,
            }
        }
    }

    impl Drop for DrainCensusV1 {
        fn drop(&mut self) {
            let wall_ns = self.start.elapsed().as_nanos() as u64;
            let run = std::mem::take(&mut *self.run.lock().unwrap_or_else(PoisonError::into_inner));
            let kind = self.kind as usize;
            let workers = self.workers as u64;
            with_acc_v1(|acc| {
                let stat = &mut acc.drains[kind];
                stat.drains += 1;
                stat.worker_slots += workers;
                stat.helper_spawns += run.helper_spawns;
                stat.spawn_failures += run.spawn_failures;
                stat.joins += run.joins;
                stat.spawn_ns += run.spawn_ns;
                stat.join_wait_ns += run.join_wait_ns;
                stat.wall_ns += wall_ns;
                stat.max_busy_ns += run.busy_ns.iter().copied().max().unwrap_or(0);
                add_into_v1(&mut stat.busy_ns, &run.busy_ns);
                add_into_v1(&mut stat.items, &run.items);
                add_into_v1(&mut stat.allocs, &run.allocs);
            });
        }
    }

    pub(crate) struct WorkerV1<'c> {
        census: &'c DrainCensusV1,
        worker: usize,
        items: u64,
        busy_ns: u64,
        allocs: u64,
    }

    pub(crate) struct ItemV1(Instant, u64);

    impl WorkerV1<'_> {
        pub(crate) fn item_started_v1(&self) -> ItemV1 {
            let allocs = allocs_now_v1().0;
            ItemV1(Instant::now(), allocs)
        }

        pub(crate) fn item_done_v1(&mut self, item: ItemV1) {
            self.busy_ns += item.0.elapsed().as_nanos() as u64;
            self.items += 1;
            self.allocs += allocs_now_v1().0 - item.1;
        }
    }

    impl Drop for WorkerV1<'_> {
        fn drop(&mut self) {
            let (worker, items, busy_ns, allocs) =
                (self.worker, self.items, self.busy_ns, self.allocs);
            self.census.run_v1(|run| {
                run.busy_ns[worker] += busy_ns;
                run.items[worker] += items;
                run.allocs[worker] += allocs;
            });
            if worker != 0 {
                // A helper thread ends with its drain.
                flush_thread_v1();
            }
        }
    }

    pub(crate) struct ShardCensusV1<'a> {
        layout: &'a LayoutV1,
        offset: usize,
        start_row: usize,
        rows: usize,
        params: usize,
        scanned: u64,
        matched: u64,
        macs: u64,
        term_rows: u64,
        start: Instant,
    }

    pub(crate) fn shard_v1(
        layout: &LayoutV1,
        offset: usize,
        start_row: usize,
        rows: usize,
        params: usize,
    ) -> ShardCensusV1<'_> {
        ShardCensusV1 {
            layout,
            offset,
            start_row,
            rows,
            params,
            scanned: 0,
            matched: 0,
            macs: 0,
            term_rows: 0,
            start: Instant::now(),
        }
    }

    impl ShardCensusV1<'_> {
        pub(crate) fn scanned_v1(&mut self) {
            self.scanned += 1;
        }

        /// A matching term of `term_rows` rows and `in_dim` inputs that adds
        /// to `weight_rows` of the shard's rows.
        pub(crate) fn matched_v1(&mut self, weight_rows: usize, term_rows: usize, in_dim: usize) {
            self.matched += 1;
            self.term_rows += term_rows as u64;
            self.macs += (weight_rows * in_dim * term_rows) as u64;
        }
    }

    impl Drop for ShardCensusV1<'_> {
        fn drop(&mut self) {
            let ns = self.start.elapsed().as_nanos() as u64;
            let worker = WORKER.with(Cell::get);
            with_acc_v1(|acc| {
                let stat = acc
                    .shards
                    .entry((self.offset, self.start_row))
                    .or_insert_with(|| ShardStatV1 {
                        tensor: self
                            .layout
                            .tensors
                            .iter()
                            .find(|t| t.offset == self.offset)
                            .map_or_else(String::new, |t| t.name.clone()),
                        rows: self.rows as u64,
                        params: self.params as u64,
                        ..ShardStatV1::default()
                    });
                stat.visits += 1;
                stat.macs += self.macs;
                stat.term_rows += self.term_rows;
                stat.scanned += self.scanned;
                stat.matched += self.matched;
                stat.ns += ns;
                if stat.by_worker.len() <= worker {
                    stat.by_worker.resize(worker + 1, 0);
                }
                stat.by_worker[worker] += 1;
            });
        }
    }

    fn report_v1(acc: &AccV1, heap: (bool, i64, i64, i64)) -> Value {
        let stages: serde_json::Map<String, Value> = STAGE_NAMES_V1
            .iter()
            .zip(&acc.stages)
            .map(|(name, cost)| ((*name).to_owned(), cost.json()))
            .collect();
        let final_layer = acc.layers.checked_sub(1);
        let sections: Vec<Value> = acc
            .sections
            .iter()
            .enumerate()
            .map(|(layer, costs)| {
                let mut row = serde_json::Map::new();
                row.insert("layer".into(), json!(layer));
                row.insert("final_block".into(), json!(Some(layer) == final_layer));
                for (name, cost) in SECTION_NAMES_V1.iter().zip(costs) {
                    row.insert((*name).to_owned(), cost.json());
                }
                Value::Object(row)
            })
            .collect();
        let sections_macs = |macs: &[MacsV1; SECTIONS_V1]| -> serde_json::Map<String, Value> {
            SECTION_NAMES_V1
                .iter()
                .zip(macs)
                .map(|(name, m)| ((*name).to_owned(), m.json()))
                .collect()
        };
        let mut total = acc.heads;
        total.add(&acc.inputs);
        let layers: Vec<Value> = acc
            .macs
            .iter()
            .enumerate()
            .map(|(layer, macs)| {
                for m in macs {
                    total.add(m);
                }
                let mut row = sections_macs(macs);
                row.insert("layer".into(), json!(layer));
                Value::Object(row)
            })
            .collect();
        let mut dead_total = MacsV1::default();
        for m in &acc.dead {
            dead_total.add(m);
        }
        let mut removed_total = MacsV1::default();
        for m in &acc.removed {
            removed_total.add(m);
        }
        let terms = &acc.terms;
        let drains: serde_json::Map<String, Value> = ["record", "replay"]
            .iter()
            .zip(&acc.drains)
            .map(|(name, stat)| {
                (
                    (*name).to_owned(),
                    json!({
                        "drains": stat.drains,
                        "worker_slots": stat.worker_slots,
                        "helper_spawns": stat.helper_spawns,
                        "spawn_failures": stat.spawn_failures,
                        "joins": stat.joins,
                        "spawn_ns": stat.spawn_ns,
                        "join_wait_ns": stat.join_wait_ns,
                        "wall_ns": stat.wall_ns,
                        "sum_of_max_worker_busy_ns": stat.max_busy_ns,
                        "busy_ns_by_worker": stat.busy_ns,
                        "items_by_worker": stat.items,
                        "allocs_by_worker": stat.allocs,
                    }),
                )
            })
            .collect();
        let shards: Vec<Value> = acc
            .shards
            .iter()
            .map(|(&(offset, start_row), stat)| {
                json!({
                    "tensor": stat.tensor,
                    "offset": offset,
                    "start_row": start_row,
                    "rows": stat.rows,
                    "params": stat.params,
                    "visits": stat.visits,
                    "macs": stat.macs,
                    "term_rows": stat.term_rows,
                    "terms_scanned": stat.scanned,
                    "terms_matched": stat.matched,
                    "ns": stat.ns,
                    "visits_by_worker": stat.by_worker,
                })
            })
            .collect();
        let (counted, start, peak, end) = heap;
        json!({
            "schema": "s-learner-profile-v1",
            "scope": "measurement-only build: spans and counters add overhead, so time with a build without the feature; MAC counts are analytic",
            "allocator_counted": counted,
            "substeps": acc.substeps,
            "stages": stages,
            "backward_sections": sections,
            "macs": {
                "heads": acc.heads.json(),
                "inputs": acc.inputs.json(),
                "layers": layers,
                "total": total.json(),
                "final_block_object_query_rows": Value::Object(sections_macs(&acc.dead)),
                "final_block_object_query_rows_total": dead_total.json(),
                "final_block_removed": Value::Object(sections_macs(&acc.removed)),
                "final_block_removed_total": removed_total.json(),
                "final_block_live_substeps": acc.live_final_substeps,
            },
            "terms": {
                "linear_terms": terms.linear_terms,
                "linear_input_bytes": terms.linear_input_bytes,
                "linear_output_gradient_bytes": terms.linear_output_gradient_bytes,
                "layer_norm_rows": terms.layer_norm_rows,
                "layer_norm_gain_bytes": terms.layer_norm_gain_bytes,
                "layer_norm_bias_bytes": terms.layer_norm_bias_bytes,
                "additions": terms.additions,
                "addition_bytes": terms.addition_bytes,
                "blocks": terms.blocks,
                "block_substeps": terms.block_substeps,
                "block_active_workers": terms.block_active_workers,
                "block_value_bytes": terms.value_bytes,
                "block_record_bytes": terms.record_bytes,
                "peak_block_live_bytes": terms.peak_live_bytes,
                "peak_block_reserved_bytes": terms.peak_reserved_bytes,
                "peak_retained_term_capacity_bytes": terms.peak_retained_capacity_bytes,
            },
            "drains": drains,
            "shards": shards,
            "heap": {
                "start_live_bytes": start,
                "peak_live_bytes": peak,
                "end_live_bytes": end,
            },
        })
    }

    /// Merges the calling thread's counts into the process total and returns
    /// and clears that total (one update's report when called after each
    /// step). Heap levels are process-wide, from the previous call.
    pub fn take_v1() -> Option<Value> {
        flush_thread_v1();
        untracked_v1(|| {
            let acc = GLOBAL
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .unwrap_or_default();
            let end = LIVE.load(Ordering::Relaxed);
            let peak = PEAK.swap(end, Ordering::Relaxed);
            let start = START.swap(end, Ordering::Relaxed);
            let counted = COUNTED.load(Ordering::Relaxed);
            Some(report_v1(&acc, (counted, start, peak, end)))
        })
    }
}

#[cfg(not(feature = "s-learner-profile-v1"))]
mod imp {
    use super::{DrainV1, SectionV1, StageV1};
    use crate::s_transformer_v1::{CountsV1, LayoutV1, SConfigV1};

    pub(crate) struct SpanV1;

    // Ends a span explicitly at `drop(span)` call sites in both builds.
    impl Drop for SpanV1 {
        #[inline(always)]
        fn drop(&mut self) {}
    }

    #[inline(always)]
    pub(crate) fn span_v1(_: StageV1) -> SpanV1 {
        SpanV1
    }

    #[inline(always)]
    pub(crate) fn section_v1(_: SectionV1, _: usize) -> SpanV1 {
        SpanV1
    }

    #[inline(always)]
    pub(crate) fn linear_term_v1(_: usize, _: usize, _: usize) {}

    #[inline(always)]
    pub(crate) fn layer_norm_term_v1(_: usize) {}

    #[inline(always)]
    pub(crate) fn addition_term_v1(_: usize) {}

    #[inline(always)]
    pub(crate) fn block_v1(_: usize, _: usize, _: impl FnOnce() -> super::BlockBytesV1) {}

    #[inline(always)]
    pub(crate) fn backward_census_v1(_: &SConfigV1, _: CountsV1, _: usize) {}

    pub(crate) struct DrainCensusV1;
    pub(crate) struct MarkV1;
    pub(crate) struct WorkerV1;
    pub(crate) struct ItemV1;

    #[inline(always)]
    pub(crate) fn drain_v1(_: DrainV1, _: usize) -> DrainCensusV1 {
        DrainCensusV1
    }

    impl DrainCensusV1 {
        #[inline(always)]
        pub(crate) fn mark_v1(&self) -> MarkV1 {
            MarkV1
        }

        #[inline(always)]
        pub(crate) fn spawned_v1(&self, _: bool) {}

        #[inline(always)]
        pub(crate) fn spawn_done_v1(&self, _: MarkV1) {}

        #[inline(always)]
        pub(crate) fn join_done_v1(&self, _: MarkV1, _: usize) {}

        #[inline(always)]
        pub(crate) fn worker_v1(&self, _: usize) -> WorkerV1 {
            WorkerV1
        }
    }

    impl WorkerV1 {
        #[inline(always)]
        pub(crate) fn item_started_v1(&self) -> ItemV1 {
            ItemV1
        }

        #[inline(always)]
        pub(crate) fn item_done_v1(&mut self, _: ItemV1) {}
    }

    pub(crate) struct ShardCensusV1;

    #[inline(always)]
    pub(crate) fn shard_v1(_: &LayoutV1, _: usize, _: usize, _: usize, _: usize) -> ShardCensusV1 {
        ShardCensusV1
    }

    impl ShardCensusV1 {
        #[inline(always)]
        pub(crate) fn scanned_v1(&mut self) {}

        #[inline(always)]
        pub(crate) fn matched_v1(&mut self, _: usize, _: usize, _: usize) {}
    }

    /// `None`: this build has no profile.
    pub fn take_v1() -> Option<serde_json::Value> {
        None
    }
}

pub use imp::take_v1;
#[cfg(feature = "s-learner-profile-v1")]
pub use imp::SProfileAllocatorV1;
pub(super) use imp::{
    addition_term_v1, backward_census_v1, block_v1, drain_v1, layer_norm_term_v1, linear_term_v1,
    section_v1, shard_v1, span_v1,
};
