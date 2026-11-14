//! The S learner's independent forward and adjoint pass records the terms
//! of each parameter gradient. Replay keeps each parameter's original
//! substep and row order, including additions to shared embeddings.
//!
//! One update runs in phases on workers it starts once: the caller and up to
//! `workers - 1` scoped helpers. A block of substeps records its terms in
//! one set of slots. With two sets, phase `k` records block `k` while block
//! `k - 1` replays from the other set, so replay work fills the gaps that
//! unequal substeps leave; with one set, record and replay phases alternate.
//! Each parameter still receives its terms in substep order: a weight shard
//! replays block `k - 1` in phase `k`, before block `k` in phase `k + 1`,
//! and one task per block applies its non-weight additions in order. Every
//! slot reserves an analytic bound ([`TermBoundV1`]) once per update, and
//! all slots together reserve at most [`TERM_BUDGET_BYTES_V1`]
//! ([`TermPlanV1`]).
//!
//! The bounded verified update (`verified_row_v1::VerifiedJobsV1`) records
//! the same tasks in the same order; each first reserves its worker's tape
//! within that worker's cap, outside the term budget, and checks its forward
//! against the substep's stored row.

use super::profile::{self, DrainV1, StageV1};
use super::verified_row_v1::VerifiedJobsV1;
use super::*;
use std::mem::size_of;
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError, RwLock};
use std::thread;

struct LinearTermV1 {
    index: LinearIdxV1,
    rows: usize,
    x: Range<usize>,
    dy: Range<usize>,
}

/// The term storage one update may reserve, all slots together. A substep
/// whose bound alone exceeds it still runs, in a block of its own.
pub(super) const TERM_BUDGET_BYTES_V1: usize = 64 << 20;

/// The most one substep records: values, `Linear` terms and additions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TermBoundV1 {
    pub(super) values: usize,
    pub(super) linears: usize,
    pub(super) additions: usize,
}

impl TermBoundV1 {
    /// The most a backward of a decision with these counts records, full or
    /// with a compact final block (`QueryRowsV1::Live`), with every card id
    /// non-padding (a padding card records no embedding row). The compact
    /// block's FFN, projection and LN2 terms cover `m = 1 + actions` of the
    /// `l` rows, but its QKV is two terms whose values, `m (d + d)` and
    /// `l (d + 2d)`, exceed the full `l (d + 3d)` by `d (2m - l)` when
    /// `2m > l`; the bound adds that excess.
    pub(super) fn of_counts_v1(layout: &LayoutV1, counts: CountsV1) -> Self {
        let config = layout.config;
        let d = config.d_model;
        let h = config.heads;
        let cd = config.card_dim;
        let (n, e, a, r) = (counts.objects, counts.edges, counts.actions, counts.refs);
        let l = 1 + n + a;
        let mut bound = Self::default();
        let mut linear = |index: LinearIdxV1, rows: usize| {
            if rows > 0 {
                bound.values += rows * (index.in_dim + index.out_dim);
                bound.linears += 1;
            }
        };
        // Heads; then per block FFN, projection and QKV (the compact QKV
        // records two terms); then the token inputs and the edge bias.
        linear(layout.val2, 1);
        linear(layout.val0, 1);
        linear(layout.pol2, a);
        linear(layout.pol0, a);
        for block in &layout.blocks {
            linear(block.ff2, l);
            linear(block.ff0, l);
            linear(block.proj, l);
            linear(block.qkv, l);
        }
        linear(layout.state2, 1);
        linear(layout.state0, 1);
        linear(layout.obj2, n);
        linear(layout.obj0, n);
        linear(layout.act2, a);
        linear(layout.act0, a);
        linear(layout.ref2, r);
        linear(layout.ref0, r);
        linear(layout.edge, e);
        bound.linears += 1;
        bound.values += d * (2 * (1 + a)).saturating_sub(l);
        // LayerNorm rows (gain and bias terms, two additions each): the
        // final norm's live rows and both norms of every block.
        let norm_rows = 1 + a + 2 * l * layout.blocks.len();
        bound.values += norm_rows * 2 * d;
        bound.additions += norm_rows * 2;
        // Group rows, card rows of objects and references, and the reference
        // bias's single values.
        bound.values += n * d + (n + r) * cd + 2 * r * h;
        bound.additions += n + n + r + 2 * r * h;
        bound
    }

    /// The bound of a decision before validation: that of its counts when
    /// its tensor shapes agree, and nothing otherwise, since the forward
    /// refuses such a decision before it records. A decision the forward
    /// accepts has exactly these counts, and a malformed one never makes an
    /// update reserve more than a well-formed decision of its size.
    pub(super) fn of_input_v1(layout: &LayoutV1, input: SInputV1<'_>) -> Self {
        match input.shape_counts_v1() {
            Ok(counts) => Self::of_counts_v1(layout, counts),
            Err(_) => Self::default(),
        }
    }

    fn max_v1(self, other: Self) -> Self {
        Self {
            values: self.values.max(other.values),
            linears: self.linears.max(other.linears),
            additions: self.additions.max(other.additions),
        }
    }

    /// The bytes a slot reserves for this bound.
    pub(super) fn bytes_v1(self) -> usize {
        self.values * size_of::<f32>()
            + self.linears * size_of::<LinearTermV1>()
            + self.additions * size_of::<(usize, Range<usize>)>()
    }
}

/// How one update holds its term records: `sets` sets of `slots` slots,
/// each reserving `bound`, the largest substep's bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TermPlanV1 {
    pub(super) sets: usize,
    pub(super) slots: usize,
    pub(super) bound: TermBoundV1,
}

impl TermPlanV1 {
    /// Two sets of one slot per worker when they fit in `budget_bytes`, so
    /// a block records while the previous one replays. Otherwise one set of
    /// as many slots as fit, at most two per worker and at least one, so the
    /// reserved bytes never exceed `budget_bytes` or one slot's bound.
    pub(super) fn new_v1(bound: TermBoundV1, workers: usize, budget_bytes: usize) -> Self {
        let fit = budget_bytes / bound.bytes_v1().max(1);
        if fit >= 2 * workers {
            Self {
                sets: 2,
                slots: workers,
                bound,
            }
        } else {
            Self {
                sets: 1,
                slots: fit.clamp(1, 2 * workers),
                bound,
            }
        }
    }

    /// The bytes all slots reserve when every slot is used.
    #[cfg(test)]
    pub(super) fn reserved_bytes_v1(self) -> usize {
        self.sets * self.slots * self.bound.bytes_v1()
    }
}

#[derive(Default)]
pub(super) struct SGradientTermsV1 {
    data: Vec<f32>,
    linears: Vec<LinearTermV1>,
    additions: Vec<(usize, Range<usize>)>,
}

impl SGradientTermsV1 {
    fn reset_v1(&mut self) {
        self.data.clear();
        self.linears.clear();
        self.additions.clear();
    }

    /// Reserves room for `bound`, so recording a substep within it never
    /// reallocates. A slot keeps its room across the blocks of an update.
    fn reserve_v1(&mut self, bound: TermBoundV1) {
        self.data.reserve_exact(bound.values);
        self.linears.reserve_exact(bound.linears);
        self.additions.reserve_exact(bound.additions);
    }

    /// What these records hold, in the units of [`TermBoundV1`].
    #[cfg(test)]
    pub(super) fn recorded_v1(&self) -> TermBoundV1 {
        TermBoundV1 {
            values: self.data.len(),
            linears: self.linears.len(),
            additions: self.additions.len(),
        }
    }

    fn store_v1(&mut self, values: &[f32]) -> Range<usize> {
        let start = self.data.len();
        self.data.extend_from_slice(values);
        start..self.data.len()
    }

    /// Live value bytes, live term-record bytes and reserved bytes of these
    /// records.
    fn bytes_v1(&self) -> (usize, usize, usize) {
        let addition = size_of::<(usize, Range<usize>)>();
        (
            self.data.len() * size_of::<f32>(),
            self.linears.len() * size_of::<LinearTermV1>() + self.additions.len() * addition,
            self.data.capacity() * size_of::<f32>()
                + self.linears.capacity() * size_of::<LinearTermV1>()
                + self.additions.capacity() * addition,
        )
    }
}

pub(super) enum SGradientSinkV1<'a> {
    Direct(&'a mut [f32]),
    Record(&'a mut SGradientTermsV1),
}

fn add_values_v1(gradient: &mut [f32], offset: usize, values: &[f32]) {
    for (acc, value) in gradient[offset..offset + values.len()]
        .iter_mut()
        .zip(values)
    {
        *acc += *value;
    }
}

impl SGradientSinkV1<'_> {
    pub(super) fn add_v1(&mut self, offset: usize, values: &[f32]) {
        match self {
            Self::Direct(gradient) => add_values_v1(gradient, offset, values),
            Self::Record(terms) => {
                profile::addition_term_v1(values.len());
                let range = terms.store_v1(values);
                terms.additions.push((offset, range));
            }
        }
    }

    pub(super) fn linear_v1(&mut self, index: LinearIdxV1, rows: usize, x: &[f32], dy: &[f32]) {
        let _span = profile::span_v1(StageV1::SinkLinear);
        match self {
            Self::Direct(gradient) => {
                // dW[o, i] += sum_r dy[r, o] x[r, i]
                gemm_v1(
                    index.out_dim,
                    index.in_dim,
                    rows,
                    dy,
                    1,
                    index.out_dim,
                    x,
                    InitV1::Existing,
                    &mut gradient[index.w..index.w + index.out_dim * index.in_dim],
                );
                kernels::bias_grad_v1(
                    rows,
                    index.out_dim,
                    dy,
                    &mut gradient[index.b..index.b + index.out_dim],
                );
            }
            Self::Record(terms) => {
                profile::linear_term_v1(rows, index.in_dim, index.out_dim);
                let x = terms.store_v1(&x[..rows * index.in_dim]);
                let dy = terms.store_v1(&dy[..rows * index.out_dim]);
                terms.linears.push(LinearTermV1 { index, rows, x, dy });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn layer_norm_v1(
        &mut self,
        gain_offset: usize,
        bias_offset: usize,
        x: &[f32],
        mean: f32,
        rstd: f32,
        gain: &[f32],
        dy: &[f32],
        dx: &mut [f32],
        scratch: &mut [f32],
    ) {
        let width = x.len();
        match self {
            Self::Direct(gradient) => {
                let (gains, biases) = gradient.split_at_mut(bias_offset);
                let dgain = &mut gains[gain_offset..gain_offset + width];
                let dbias = &mut biases[..width];
                kernels::layer_norm_row_backward_v1(
                    x,
                    mean,
                    rstd,
                    gain,
                    dy,
                    |i, g, b| {
                        dgain[i] += g;
                        dbias[i] += b;
                    },
                    dx,
                    scratch,
                );
            }
            Self::Record(terms) => {
                profile::layer_norm_term_v1(width);
                let bias = terms.store_v1(dy);
                let gain_start = terms.data.len();
                terms.data.resize(gain_start + width, 0.0);
                kernels::layer_norm_row_backward_v1(
                    x,
                    mean,
                    rstd,
                    gain,
                    dy,
                    |i, g, _| {
                        terms.data[gain_start + i] = g;
                    },
                    dx,
                    scratch,
                );
                terms
                    .additions
                    .push((gain_offset, gain_start..gain_start + width));
                terms.additions.push((bias_offset, bias));
            }
        }
    }
}

struct WeightShardV1<'g> {
    offset: usize,
    start_row: usize,
    rows: usize,
    values: &'g mut [f32],
}

/// One update's gradient, split once for concurrent replay: the rows of
/// every `Linear` weight in shards (largest first), and the other tensors
/// (biases, LayerNorm parameters, embeddings, the reference bias) as runs of
/// adjacent tensors in offset order.
struct GradientPartsV1<'g> {
    shards: Vec<Mutex<WeightShardV1<'g>>>,
    rest: Mutex<Vec<(usize, &'g mut [f32])>>,
}

impl<'g> GradientPartsV1<'g> {
    fn new_v1(layout: &LayoutV1, gradient: &'g mut [f32], workers: usize) -> Self {
        let _build = profile::span_v1(StageV1::ShardBuild);
        let mut shards = Vec::new();
        let mut rest = Vec::new();
        let mut remaining = gradient;
        // `remaining` starts at `offset`; its first `run` values are
        // non-weight tensors not yet split off.
        let mut offset = 0;
        let mut run = 0;
        let target_size = layout.total.div_ceil(workers * 4);
        for tensor in &layout.tensors {
            if !tensor.linear {
                run += tensor.len;
                continue;
            }
            if run > 0 {
                let (values, tail) = remaining.split_at_mut(run);
                rest.push((offset, values));
                remaining = tail;
                offset += run;
                run = 0;
            }
            let (values, tail) = remaining.split_at_mut(tensor.len);
            remaining = tail;
            offset += tensor.len;
            let width = tensor.shape[1];
            let rows_per_shard = target_size.div_ceil(width).max(4).next_multiple_of(4);
            for (shard, values) in values.chunks_mut(rows_per_shard * width).enumerate() {
                shards.push(WeightShardV1 {
                    offset: tensor.offset,
                    start_row: shard * rows_per_shard,
                    rows: values.len() / width,
                    values,
                });
            }
        }
        if run > 0 {
            rest.push((offset, remaining));
        }
        shards.sort_by_key(|shard| std::cmp::Reverse(shard.values.len()));
        Self {
            shards: shards.into_iter().map(Mutex::new).collect(),
            rest: Mutex::new(rest),
        }
    }
}

/// The non-weight values `offset..offset + len`. Every addition and bias
/// term lies in one run, never in a `Linear` weight.
fn rest_values_v1<'r>(
    rest: &'r mut [(usize, &mut [f32])],
    offset: usize,
    len: usize,
) -> &'r mut [f32] {
    let run = rest
        .partition_point(|(start, _)| *start <= offset)
        .checked_sub(1)
        .expect("S term before the first non-weight tensor");
    let (start, values) = &mut rest[run];
    &mut values[offset - *start..offset - *start + len]
}

/// One substep's non-weight terms, in its order: the LayerNorm, embedding
/// and reference-bias additions, then the `Linear` bias terms. The shared
/// card and reference tables receive each addition in turn.
fn add_terms_v1(rest: &mut [(usize, &mut [f32])], terms: &SGradientTermsV1) {
    for &(offset, ref range) in &terms.additions {
        add_values_v1(
            rest_values_v1(rest, offset, range.len()),
            0,
            &terms.data[range.clone()],
        );
    }
    for term in &terms.linears {
        let index = term.index;
        kernels::bias_grad_v1(
            term.rows,
            index.out_dim,
            &terms.data[term.dy.clone()],
            rest_values_v1(rest, index.b, index.out_dim),
        );
    }
}

/// One substep's weight terms for one shard. A term covers whole rows of one
/// weight, possibly only some of them (`LinearIdxV1::rows_v1`); it adds to
/// the rows it shares with the shard. Weights are disjoint, so other
/// weights' terms share nothing.
fn replay_shard_terms_v1(
    shard: &mut WeightShardV1<'_>,
    terms: &SGradientTermsV1,
    census: &mut profile::ShardCensusV1,
) {
    let width = shard.values.len() / shard.rows;
    let shard_start = shard.offset + shard.start_row * width;
    let shard_end = shard_start + shard.values.len();
    for term in &terms.linears {
        census.scanned_v1();
        let index = term.index;
        let start = shard_start.max(index.w);
        let end = shard_end.min(index.w + index.out_dim * index.in_dim);
        if start < end {
            let rows = (end - start) / index.in_dim;
            census.matched_v1(rows, term.rows, index.in_dim);
            gemm_v1(
                rows,
                index.in_dim,
                term.rows,
                &terms.data[term.dy.start + (start - index.w) / index.in_dim..term.dy.end],
                1,
                index.out_dim,
                &terms.data[term.x.clone()],
                InitV1::Existing,
                &mut shard.values[start - shard_start..end - shard_start],
            );
        }
    }
}

pub(super) fn replay_terms_v1(
    layout: &LayoutV1,
    gradient: &mut [f32],
    terms: &[SGradientTermsV1],
    workers: usize,
) -> Result<(), String> {
    ensure_mxcsr_v1()?;
    let workers = workers.max(1);
    let parts = GradientPartsV1::new_v1(layout, gradient, workers);
    if workers == 1 || terms.is_empty() {
        let mut rest = parts
            .rest
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
        for substep in terms {
            add_terms_v1(&mut rest, substep);
        }
        for shard in parts.shards {
            let mut shard = shard.into_inner().unwrap_or_else(PoisonError::into_inner);
            let mut census = profile::shard_v1(
                layout,
                shard.offset,
                shard.start_row,
                shard.rows,
                shard.values.len(),
            );
            for substep in terms {
                replay_shard_terms_v1(&mut shard, substep, &mut census);
            }
        }
        return Ok(());
    }
    let next_shard = AtomicUsize::new(0);
    thread::scope(|scope| {
        let run_shards = || -> Result<(), String> {
            ensure_mxcsr_v1()?;
            loop {
                let idx = next_shard.fetch_add(1, Ordering::Relaxed);
                let Some(slot) = parts.shards.get(idx) else {
                    break;
                };
                let mut shard = slot.lock().unwrap_or_else(PoisonError::into_inner);
                let mut census = profile::shard_v1(
                    layout,
                    shard.offset,
                    shard.start_row,
                    shard.rows,
                    shard.values.len(),
                );
                for substep in terms {
                    replay_shard_terms_v1(&mut shard, substep, &mut census);
                }
            }
            Ok(())
        };
        let mut handles = Vec::with_capacity(workers - 1);
        for _ in 1..workers {
            handles.push(scope.spawn(run_shards));
        }
        {
            let mut rest = parts.rest.lock().unwrap_or_else(PoisonError::into_inner);
            for substep in terms {
                add_terms_v1(&mut rest, substep);
            }
        }
        let mut first_err = run_shards().err();
        for handle in handles {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) if first_err.is_none() => first_err = Some(e),
                Ok(Err(_)) => {}
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    })
}

/// Replays recorded substeps into `gradient` on the calling thread, in
/// order, as one block.
#[cfg(test)]
pub(super) fn replay_verified_test_terms_v1(
    layout: &LayoutV1,
    gradient: &mut [f32],
    terms: Vec<SGradientTermsV1>,
) -> Result<(), String> {
    replay_terms_v1(layout, gradient, &terms, 1)
}

#[derive(Clone, Copy)]
struct GaeJobV1<'a> {
    substep: SSubstepV1<'a>,
    index: usize,
    sub: usize,
    last: bool,
    is_masked: bool,
    d_joint: f32,
    advantage: f32,
    target: f32,
}

#[derive(Default)]
struct PreparedGaeV1<'a> {
    job: Option<Result<GaeJobV1<'a>, String>>,
    terms: SGradientTermsV1,
    score: Option<Result<(f32, f32), String>>,
}

fn read_v1<'s, T>(slot: &'s RwLock<T>) -> std::sync::RwLockReadGuard<'s, T> {
    slot.read().unwrap_or_else(PoisonError::into_inner)
}

fn write_v1<'s, T>(slot: &'s RwLock<T>) -> std::sync::RwLockWriteGuard<'s, T> {
    slot.write().unwrap_or_else(PoisonError::into_inner)
}

/// The term bytes of the block recorded in slots `active`, with the
/// capacity every slot retains (the other set and unused slots too).
fn term_block_bytes_v1(
    slots: &[RwLock<PreparedGaeV1<'_>>],
    active: Range<usize>,
) -> profile::BlockBytesV1 {
    let mut bytes = profile::BlockBytesV1::default();
    for (index, slot) in slots.iter().enumerate() {
        let (values, records, capacity) = read_v1(slot).terms.bytes_v1();
        if active.contains(&index) {
            bytes.values += values;
            bytes.records += records;
            bytes.active_capacity += capacity;
        }
        bytes.retained_capacity += capacity;
    }
    bytes
}

#[cfg(all(test, feature = "s-learner-profile-v1"))]
#[test]
fn partial_block_counts_inactive_slot_capacity_separately() {
    let slots = [
        RwLock::new(PreparedGaeV1::default()),
        RwLock::new(PreparedGaeV1::default()),
    ];
    {
        let mut active = write_v1(&slots[0]);
        let active_range = active.terms.store_v1(&[1.0, 2.0]);
        active.terms.additions.push((0, active_range));
        let mut old = write_v1(&slots[1]);
        let old_range = old.terms.store_v1(&[0.0; 32]);
        old.terms.additions.push((0, old_range));
    }

    let bytes = term_block_bytes_v1(&slots, 0..1);
    let (values, records, active_capacity) = read_v1(&slots[0]).terms.bytes_v1();
    assert_eq!(bytes.values, values);
    assert_eq!(bytes.records, records);
    assert_eq!(bytes.active_capacity, active_capacity);
    assert_eq!(
        bytes.retained_capacity,
        active_capacity + read_v1(&slots[1]).terms.bytes_v1().2
    );
    assert!(bytes.retained_capacity > bytes.active_capacity);
}

fn jobs_v1<'a>(
    groups: &'a [SGroupV1<'a>],
    mask: Option<&'a super::auxiliary::SPolicyMaskV1>,
) -> impl Iterator<Item = Result<GaeJobV1<'a>, String>> + 'a {
    let group_count = groups.len() as f32;
    groups.iter().enumerate().flat_map(move |(index, group)| {
        let invalid = if group.substeps.is_empty() {
            Some(format!("S group {index} has no substeps"))
        } else if !group.advantage.is_finite() || !group.value_target.is_finite() {
            Some(format!("S group {index} has a non-finite target"))
        } else {
            None
        };
        let substeps = if invalid.is_some() {
            &[][..]
        } else {
            &group.substeps
        };
        let masked = mask.map(|m| &m.masked[index]);
        let d_unmasked = match mask {
            None => -group.advantage / group_count,
            Some(m) => -group.advantage / m.policy_count,
        };
        invalid
            .into_iter()
            .map(Err)
            .chain(substeps.iter().enumerate().map(move |(sub, &substep)| {
                let is_masked = masked.is_some_and(|m| m[sub]);
                Ok(GaeJobV1 {
                    substep,
                    index,
                    sub,
                    last: sub + 1 == group.substeps.len(),
                    is_masked,
                    d_joint: if is_masked { 0.0 } else { d_unmasked },
                    advantage: group.advantage,
                    target: group.value_target,
                })
            }))
    })
}

/// One task of a phase.
#[derive(Clone, Copy, Debug)]
enum TaskV1 {
    /// The forward and recorded backward of the substep in slot `i`.
    Record(usize),
    /// The replaying block's non-weight terms, substep by substep.
    Additions,
    /// The replaying block's weight terms in shard `i`, substep by substep.
    Shard(usize),
}

/// The phase every worker of an update is in.
#[derive(Default)]
struct PhaseV1 {
    /// Counts phases; each helper works once in every one.
    generation: u64,
    stop: bool,
    tasks: Vec<TaskV1>,
    next: usize,
    /// The slots of the block this phase replays.
    replaying: Range<usize>,
    /// Helpers still working in this phase.
    running: usize,
    /// A helper panicked; the update ends and the caller re-raises it.
    panicked: bool,
    /// Each worker's refusal in this phase (an unpinned MXCSR).
    errors: Vec<Option<String>>,
}

/// Settings of one parallel gradient; production uses the defaults.
#[derive(Clone, Copy, Debug)]
pub(super) struct PipelineOptionsV1 {
    pub(super) budget_bytes: usize,
    /// The worker that panics as it starts the phase of this generation.
    #[cfg(test)]
    pub(super) panic_at: Option<(usize, u64)>,
}

impl Default for PipelineOptionsV1 {
    fn default() -> Self {
        Self {
            budget_bytes: TERM_BUDGET_BYTES_V1,
            #[cfg(test)]
            panic_at: None,
        }
    }
}

/// The term storage one parallel gradient used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TermUseV1 {
    pub(super) plan: TermPlanV1,
    /// The bytes every slot's buffers held at the end, their peak (no slot
    /// releases room during an update).
    pub(super) capacity_bytes: usize,
}

/// Everything the workers of one update share.
struct PipelineV1<'u, 'a, 'g> {
    model: &'u SModelV1,
    group_count: f32,
    value_coefficient: f32,
    /// The bounded verified update: every recorded substep's forward is
    /// checked against its stored row, on a tape within the cap.
    verified: Option<&'u VerifiedJobsV1<'u>>,
    masked: bool,
    slots: Vec<RwLock<PreparedGaeV1<'a>>>,
    parts: GradientPartsV1<'g>,
    phase: Mutex<PhaseV1>,
    /// Helpers wait here for the next phase or the stop.
    start: Condvar,
    /// The caller waits here for the helpers to finish a phase.
    done: Condvar,
    #[cfg(test)]
    panic_at: Option<(usize, u64)>,
}

/// A helper's end of one phase, also when its work panics.
struct PhaseDoneV1<'p, 'u, 'a, 'g>(&'p PipelineV1<'u, 'a, 'g>);

impl Drop for PhaseDoneV1<'_, '_, '_, '_> {
    fn drop(&mut self) {
        let mut phase = self.0.lock_v1();
        phase.running = phase.running.saturating_sub(1);
        if thread::panicking() {
            phase.panicked = true;
        }
        if phase.running == 0 {
            self.0.done.notify_all();
        }
    }
}

/// Stops the helpers when the caller leaves the update, also by a panic, so
/// that the scope can join them.
struct StopV1<'p, 'u, 'a, 'g>(&'p PipelineV1<'u, 'a, 'g>);

impl Drop for StopV1<'_, '_, '_, '_> {
    fn drop(&mut self) {
        self.0.lock_v1().stop = true;
        self.0.start.notify_all();
    }
}

impl<'a> PipelineV1<'_, 'a, '_> {
    fn lock_v1(&self) -> MutexGuard<'_, PhaseV1> {
        self.phase.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// One worker's part of the current phase: tasks until none is left.
    fn work_v1(&self, worker: usize, work: &mut SWorkV1, census: &profile::PoolCensusV1) {
        if let Err(error) = ensure_mxcsr_v1() {
            self.lock_v1().errors[worker] = Some(error);
            return;
        }
        let mut busy = census.worker_v1(worker);
        loop {
            let (task, replaying) = {
                let mut phase = self.lock_v1();
                #[cfg(test)]
                if self.panic_at == Some((worker, phase.generation)) {
                    drop(phase);
                    panic!("S learner worker {worker} panicked (test)");
                }
                let Some(&task) = phase.tasks.get(phase.next) else {
                    return;
                };
                phase.next += 1;
                (task, phase.replaying.clone())
            };
            let started = busy.item_started_v1();
            let kind = self.run_v1(task, replaying, work);
            busy.item_done_v1(kind, started);
        }
    }

    fn run_v1(&self, task: TaskV1, replaying: Range<usize>, work: &mut SWorkV1) -> DrainV1 {
        match task {
            TaskV1::Record(index) => {
                let mut slot = write_v1(&self.slots[index]);
                if let Some(Ok(job)) = slot.job {
                    let score = self.model.prepare_gae_v1(
                        job,
                        self.group_count,
                        self.value_coefficient,
                        self.verified,
                        work,
                        &mut slot.terms,
                    );
                    slot.score = Some(score);
                }
                DrainV1::Record
            }
            TaskV1::Additions => {
                let _span = profile::span_v1(StageV1::ReplayAdditions);
                let mut rest = self
                    .parts
                    .rest
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                for slot in &self.slots[replaying] {
                    add_terms_v1(&mut rest, &read_v1(slot).terms);
                }
                DrainV1::Additions
            }
            TaskV1::Shard(index) => {
                let mut shard = self.parts.shards[index]
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let mut census = profile::shard_v1(
                    &self.model.layout,
                    shard.offset,
                    shard.start_row,
                    shard.rows,
                    shard.values.len(),
                );
                for slot in &self.slots[replaying] {
                    replay_shard_terms_v1(&mut shard, &read_v1(slot).terms, &mut census);
                }
                DrainV1::Replay
            }
        }
    }

    /// A helper's loop: one turn per phase until the stop.
    fn helper_v1(&self, worker: usize, work: &mut SWorkV1, census: &profile::PoolCensusV1) {
        let mut seen = 0;
        loop {
            {
                let mut phase = self.lock_v1();
                while phase.generation == seen && !phase.stop {
                    phase = self
                        .start
                        .wait(phase)
                        .unwrap_or_else(PoisonError::into_inner);
                }
                if phase.stop {
                    return;
                }
                seen = phase.generation;
            }
            let _done = PhaseDoneV1(self);
            self.work_v1(worker, work, census);
        }
    }

    /// Runs one phase on every worker, the caller included, and waits for
    /// all of them. Reports the helpers' refusals first, in worker order,
    /// then the caller's, as separate drains did.
    fn phase_v1(
        &self,
        tasks: &[TaskV1],
        replaying: Range<usize>,
        helpers: usize,
        work: &mut SWorkV1,
        census: &profile::PoolCensusV1,
    ) -> Result<(), String> {
        let phase_span = profile::span_v1(StageV1::Phase);
        let mark = census.mark_v1();
        {
            let mut phase = self.lock_v1();
            phase.tasks.clear();
            phase.tasks.extend_from_slice(tasks);
            phase.next = 0;
            phase.replaying = replaying;
            for error in &mut phase.errors {
                *error = None;
            }
            phase.running = helpers;
            phase.generation += 1;
        }
        self.start.notify_all();
        self.work_v1(0, work, census);
        let barrier = profile::span_v1(StageV1::Barrier);
        let mut phase = self.lock_v1();
        while phase.running > 0 {
            phase = self
                .done
                .wait(phase)
                .unwrap_or_else(PoisonError::into_inner);
        }
        drop(barrier);
        drop(phase_span);
        census.phase_done_v1(mark);
        if phase.panicked {
            return Err("S learner helper panicked".into());
        }
        let errors = &mut phase.errors;
        let workers = errors.len();
        match (1..workers)
            .chain([0])
            .find_map(|worker| errors[worker].take())
        {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// The caller's side of an update: fills the next set with the next
    /// block of jobs, runs each phase and merges every recorded block's loss
    /// terms in order. Returns the policy and value sums.
    fn phases_v1(
        &self,
        mut jobs: impl Iterator<Item = Result<GaeJobV1<'a>, String>>,
        plan: TermPlanV1,
        helpers: usize,
        work: &mut SWorkV1,
        census: &profile::PoolCensusV1,
    ) -> Result<(f32, f32), String> {
        let mut policy_sum = 0.0f32;
        let mut value_sum = 0.0f32;
        let mut joint = 0.0f32;
        let mut policy_joint: Option<f32> = None;
        let mut value_error = 0.0f32;
        let mut replaying: Option<Range<usize>> = None;
        let mut next_set = 0;
        let mut tasks = Vec::new();
        loop {
            // The next block records in the next set, unless that set is the
            // one replaying (a single set alternates).
            let first = next_set * plan.slots;
            let mut len = 0;
            if replaying.as_ref().is_none_or(|slots| slots.start != first) {
                for (slot, job) in self.slots[first..first + plan.slots].iter().zip(&mut jobs) {
                    let mut slot = write_v1(slot);
                    slot.job = Some(job);
                    slot.terms.reset_v1();
                    slot.terms.reserve_v1(plan.bound);
                    len += 1;
                }
            }
            let recording = (len > 0).then(|| first..first + len);
            if recording.is_some() {
                next_set = (next_set + 1) % plan.sets;
            }
            if recording.is_none() && replaying.is_none() {
                break;
            }
            tasks.clear();
            if let Some(slots) = &recording {
                tasks.extend(slots.clone().map(TaskV1::Record));
            }
            if replaying.is_some() {
                tasks.push(TaskV1::Additions);
                tasks.extend((0..self.parts.shards.len()).map(TaskV1::Shard));
            }
            census.phase_kinds_v1(len, replaying.is_some());
            self.phase_v1(
                &tasks,
                replaying.clone().unwrap_or(0..0),
                helpers,
                work,
                census,
            )?;
            if let Some(slots) = &recording {
                profile::block_v1(len, len.min(helpers + 1), || {
                    term_block_bytes_v1(&self.slots, slots.clone())
                });
                let _merge = profile::span_v1(StageV1::ScoreMerge);
                for slot in &self.slots[slots.clone()] {
                    let mut slot = write_v1(slot);
                    let job = slot.job.take().expect("S learner scheduled job")?;
                    let (selected, error) = slot.score.take().expect("S learner prepared job")?;
                    if job.sub == 0 {
                        joint = selected;
                        policy_joint = (!job.is_masked).then_some(selected);
                        value_error = error;
                    } else {
                        joint += selected;
                        if !job.is_masked {
                            policy_joint = Some(policy_joint.map_or(selected, |j| j + selected));
                        }
                    }
                    if job.last {
                        policy_sum += if self.masked {
                            -policy_joint.unwrap_or(0.0) * job.advantage
                        } else {
                            -joint * job.advantage
                        };
                        value_sum += value_error * value_error;
                    }
                }
            }
            replaying = recording;
        }
        Ok((policy_sum, value_sum))
    }
}

impl SModelV1 {
    pub(super) fn gae_loss_gradient_parallel_v1(
        &self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        gradient: &mut [f32],
        workers: usize,
        caller_work: &mut SWorkV1,
    ) -> Result<SLossV1, String> {
        #[cfg(feature = "gpu-exact-v1")]
        if let Some(loss) =
            self.try_gpu_gae_loss_gradient_masked_v1(groups, value_coefficient, None, gradient)?
        {
            return Ok(loss);
        }
        self.gae_loss_gradient_planned_v1(
            groups,
            value_coefficient,
            gradient,
            workers,
            caller_work,
            PipelineOptionsV1::default(),
        )
        .map(|(loss, _)| loss)
    }

    pub(super) fn gae_loss_gradient_masked_parallel_v1(
        &self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        mask: Option<&super::auxiliary::SPolicyMaskV1>,
        gradient: &mut [f32],
        workers: usize,
        caller_work: &mut SWorkV1,
    ) -> Result<SLossV1, String> {
        #[cfg(feature = "gpu-exact-v1")]
        if let Some(loss) =
            self.try_gpu_gae_loss_gradient_masked_v1(groups, value_coefficient, mask, gradient)?
        {
            return Ok(loss);
        }
        self.gae_loss_gradient_pipelined_v1(
            groups,
            value_coefficient,
            mask,
            gradient,
            workers,
            caller_work,
            PipelineOptionsV1::default(),
            None,
        )
        .map(|(loss, _)| loss)
    }

    /// The parallel GAE gradient under `options`, with the term storage it
    /// used. `workers` counts the caller.
    pub(super) fn gae_loss_gradient_planned_v1(
        &self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        gradient: &mut [f32],
        workers: usize,
        caller_work: &mut SWorkV1,
        options: PipelineOptionsV1,
    ) -> Result<(SLossV1, TermUseV1), String> {
        self.gae_loss_gradient_pipelined_v1(
            groups,
            value_coefficient,
            None,
            gradient,
            workers,
            caller_work,
            options,
            None,
        )
    }

    /// [`Self::gae_loss_gradient_planned_v1`] whose every substep's forward
    /// is first checked against its stored row (`verified`). The first
    /// failure in substep order is reported, a tape over the cap as
    /// `verified_row_v1::VERIFIED_TAPE_CAPACITY_V1`; `verified` records the
    /// work done and the tapes retained, also on failure.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn gae_loss_gradient_verified_v1(
        &self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        gradient: &mut [f32],
        workers: usize,
        caller_work: &mut SWorkV1,
        verified: &VerifiedJobsV1<'_>,
        options: PipelineOptionsV1,
    ) -> Result<(SLossV1, TermUseV1), String> {
        // Each slot's peak is this update's, whatever the caller's tape held.
        caller_work.verified_tape_peak = 0;
        self.gae_loss_gradient_pipelined_v1(
            groups,
            value_coefficient,
            None,
            gradient,
            workers,
            caller_work,
            options,
            Some(verified),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn gae_loss_gradient_pipelined_v1(
        &self,
        groups: &[SGroupV1<'_>],
        value_coefficient: f32,
        mask: Option<&super::auxiliary::SPolicyMaskV1>,
        gradient: &mut [f32],
        workers: usize,
        caller_work: &mut SWorkV1,
        options: PipelineOptionsV1,
        verified: Option<&VerifiedJobsV1<'_>>,
    ) -> Result<(SLossV1, TermUseV1), String> {
        ensure_mxcsr_v1()?;
        if let Some(mask) = mask {
            if mask.masked.len() != groups.len()
                || mask
                    .masked
                    .iter()
                    .zip(groups)
                    .any(|(m, g)| m.len() != g.substeps.len())
            {
                return Err("S policy mask does not match the batch".into());
            }
        }
        if groups.is_empty() {
            return Err("S update has no groups".into());
        }
        if !value_coefficient.is_finite() || value_coefficient <= 0.0 {
            return Err("S value coefficient must be positive".into());
        }
        let bound = groups
            .iter()
            .flat_map(|group| &group.substeps)
            .map(|substep| TermBoundV1::of_input_v1(&self.layout, substep.input))
            .fold(TermBoundV1::default(), TermBoundV1::max_v1);
        let plan = TermPlanV1::new_v1(bound, workers, options.budget_bytes);
        // The worker census ends with this block, after every helper has
        // joined and the pipeline has released the gradient.
        let (sums, capacity_bytes) = {
            let census = profile::pool_v1(workers);
            // One tape and backward workspace per helper; the caller brings
            // its own.
            let mut helper_work = vec![SWorkV1::default(); workers - 1];
            let pipeline = PipelineV1 {
                model: self,
                group_count: groups.len() as f32,
                value_coefficient,
                verified,
                masked: mask.is_some(),
                slots: (0..plan.sets * plan.slots)
                    .map(|_| RwLock::default())
                    .collect(),
                parts: GradientPartsV1::new_v1(&self.layout, gradient, workers),
                phase: Mutex::new(PhaseV1 {
                    errors: vec![None; workers],
                    ..PhaseV1::default()
                }),
                start: Condvar::new(),
                done: Condvar::new(),
                #[cfg(test)]
                panic_at: options.panic_at,
            };
            let update_profile = super::update_profile_v1::current_v1();
            let sums = thread::scope(|scope| {
                // Stops every helper however the caller leaves, so that the
                // scope can join them.
                let stop = StopV1(&pipeline);
                let spawning = census.mark_v1();
                let helpers: Vec<_> = helper_work
                    .iter_mut()
                    .enumerate()
                    .filter_map(|(index, work)| {
                        let (pipeline, census, update_profile) =
                            (&pipeline, &census, &update_profile);
                        let helper = thread::Builder::new()
                            .spawn_scoped(scope, move || {
                                super::update_profile_v1::adopt_v1(update_profile);
                                pipeline.helper_v1(1 + index, work, census);
                                profile::helper_done_v1();
                            })
                            .ok();
                        census.spawned_v1(helper.is_some());
                        helper
                    })
                    .collect();
                census.spawn_done_v1(spawning);
                let sums = pipeline.phases_v1(
                    jobs_v1(groups, mask),
                    plan,
                    helpers.len(),
                    caller_work,
                    &census,
                );
                drop(stop);
                let joining = census.mark_v1();
                let joins = helpers.len();
                let mut panic = None;
                for helper in helpers {
                    if let Err(payload) = helper.join() {
                        panic.get_or_insert(payload);
                    }
                }
                census.join_done_v1(joining, joins);
                if let Some(payload) = panic {
                    std::panic::resume_unwind(payload);
                }
                sums
            });
            let capacity_bytes: usize = pipeline
                .slots
                .iter()
                .map(|slot| read_v1(slot).terms.bytes_v1().2)
                .sum();
            if let Some(verified) = verified {
                verified.slots_done_v1(
                    helper_work
                        .iter()
                        .chain([&*caller_work])
                        .map(|work| work.verified_tape_peak),
                );
            }
            (sums, capacity_bytes)
        };
        let (policy_sum, value_sum) = sums?;
        for range in self.layout.gauge_ranges_v1() {
            gradient[range].fill(0.0);
        }
        let group_count = groups.len() as f32;
        let (loss, policy_term) = match mask {
            None => (
                (policy_sum + value_coefficient * value_sum) / group_count,
                policy_sum / group_count,
            ),
            Some(m) => (
                policy_sum / m.policy_count + value_coefficient * value_sum / group_count,
                policy_sum / m.policy_count,
            ),
        };
        if !loss.is_finite() || gradient.iter().any(|g| !g.is_finite()) {
            return Err("S loss or gradient is not finite".into());
        }
        let loss = SLossV1 {
            group_count: groups.len(),
            policy_sum,
            value_sum,
            policy_term,
            value_term: value_sum / group_count,
            loss,
        };
        Ok((
            loss,
            TermUseV1 {
                plan,
                capacity_bytes,
            },
        ))
    }

    fn prepare_gae_v1(
        &self,
        job: GaeJobV1<'_>,
        group_count: f32,
        value_coefficient: f32,
        verified: Option<&VerifiedJobsV1<'_>>,
        work: &mut SWorkV1,
        terms: &mut SGradientTermsV1,
    ) -> Result<(f32, f32), String> {
        if let Some(verified) = verified {
            // The replay's forward of a learner row, which this update does
            // not repeat: its outputs must be the stored row's.
            let _forward = profile::span_v1(StageV1::Forward);
            verified.forward_v1(
                self,
                (job.index, job.sub),
                job.substep.input,
                &mut work.tape,
                &mut work.verified_tape_peak,
            )?;
        } else {
            let update_forward = super::update_profile_v1::span_v1(
                super::update_profile_v1::StageV1::LearnerForward,
            );
            let forward = profile::span_v1(StageV1::Forward);
            self.forward_into_v1(job.substep.input, &mut work.tape)?;
            drop(forward);
            update_forward.finish_v1(&work.tape.logits, work.tape.value);
        }
        let policy_row = profile::span_v1(StageV1::PolicyRow);
        let selected = gae_policy_row_into_v1(
            &work.tape.logits,
            job.substep.selected,
            job.d_joint,
            job.index,
            job.sub,
            &mut work.dlogits,
        )?;
        drop(policy_row);
        let (value_error, dvalue) = if job.sub == 0 {
            let error = work.tape.value - job.target;
            (error, (value_coefficient / group_count) * (2.0 * error))
        } else {
            (0.0, 0.0)
        };
        let update_backward =
            super::update_profile_v1::span_v1(super::update_profile_v1::StageV1::LearnerBackward);
        let backward = profile::span_v1(StageV1::Backward);
        self.backward_into_v1(
            job.substep.input,
            &work.tape,
            &work.dlogits,
            dvalue,
            &mut SGradientSinkV1::Record(terms),
            &mut work.back,
        )?;
        drop(backward);
        update_backward.finish_plain_v1();
        if let Some(verified) = verified {
            verified.backward_done_v1();
        }
        Ok((selected, value_error))
    }
}
