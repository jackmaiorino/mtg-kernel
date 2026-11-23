//! Fixed-tree parallel CPU backward for the ordinary GAE update: the opt-in
//! `update_backward_execution` value `fixed_tree_v1`. A prototype. Nothing
//! selects it unless a config names it.
//!
//! The sequential backward gives every gradient element its terms in one
//! serial order over the whole batch, so a parallel form of it has to replay
//! that order element by element (`exact_parallel_backward_v1`). This
//! backward fixes another order instead, one that no thread count changes:
//!
//! 1. **Rows.** The learner substeps the pass visits, in the sequential
//!    pass's visiting order: groups from the last to the first, and each
//!    group's substeps from the last to the first.
//! 2. **Chunks.** Consecutive runs of [`FIXED_TREE_CHUNK_ROWS_V1`] rows; the
//!    last chunk takes the remainder. The size is a constant of the contract.
//!    It is never derived from the thread count, the machine or the batch.
//! 3. **Chunk sums.** A chunk's sum starts at +0.0 and takes the chunk's rows
//!    in row order through the sequential backward's own sink and kernels
//!    (`AccumulateTransposedV1`). Within a chunk every element therefore
//!    receives exactly the additions the sequential pass gives it for those
//!    rows, in its order, and a batch of one chunk is the sequential pass bit
//!    for bit.
//! 4. **Tree.** Chunk sums are combined pairwise, level by level. Node `2j`
//!    plus node `2j + 1` of one level is node `j` of the next, the lower
//!    index on the left; a last node without a partner moves up unchanged.
//!    The shape depends only on the number of chunks. For five chunks the
//!    result is `((c0 + c1) + (c2 + c3)) + c4`.
//!
//! A combination is one binary32 addition per element. There is no product in
//! it, so nothing can be fused, and it is written as a plain loop with no
//! target feature or intrinsic. Which thread computes a chunk or performs a
//! combination changes no operand of any addition, so the gradients are the
//! same bits for every worker count, including one.
//!
//! **Card embedding.** A chunk reaches a few dozen of the card table's rows,
//! so a node holds only the rows its chunks named. Combining two nodes adds
//! the rows both hold and carries over a row only one holds. In the pinned
//! floating-point state this equals the dense elementwise sum: the missing
//! row would be all +0.0, and `x + (+0.0)` is `x` for every `x` that is not a
//! negative zero, which no sum that started at +0.0 is under round to
//! nearest. A row no chunk named stays +0.0 in the result, as the sequential
//! pass leaves it.
//!
//! **Everything else is the sequential pass's.** The loss scalars are summed
//! before any backward runs and are not touched. The scorer-bias gauge is
//! observed in the visiting order before the chunks run, so its bound and its
//! high-precision residual are the sequential pass's; only the raw binary32
//! residual it is checked against comes from the tree. That bound allows
//! `gamma(m - 1)` for the cross-substep accumulation of `m` substeps, which
//! covers any order of those additions, the tree's included. The first
//! failure reported is the one the sequential loop would have reached first:
//! a substep's gauge observation ahead of its backward, earlier substeps
//! ahead of later ones.
//!
//! **Not the sequential bits.** With more than one chunk an element's sum is
//! associated differently, so its last bits may differ from the sequential
//! pass's. A lineage trains under one of the two orders, never a mix.

use super::accumulate_transposed_v1::AccumulateTransposedV1;
use super::exact_parallel_backward_v1::{drain_in_parallel_v1, GradientTensorsV1};
use super::pregame_announcement_step_v1::BackwardPartV1;
use super::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Mutex, PoisonError};

/// Rows per chunk: a constant of the `fixed_tree_v1` contract. Another value
/// is another reduction order and needs another identity.
pub(crate) const FIXED_TREE_CHUNK_ROWS_V1: usize = 64;

/// The numerical-backend identity of this backward, beside
/// `NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1` and
/// `FIXED_PARTITION_PARALLEL_BACKWARD_NUMERICAL_BACKEND_IDENTITY_V1`.
pub(crate) const FIXED_TREE_BACKWARD_NUMERICAL_BACKEND_IDENTITY_V1: &str =
    "rust-production-native-policy-train-step-v1-cpu-ieee754-binary32-fixed-tree-rows64-v1";

const _: () = assert!(FIXED_TREE_CHUNK_ROWS_V1 == 64);

/// How many chunks `rows` rows make.
pub(crate) fn fixed_tree_chunk_count_v1(rows: usize) -> usize {
    rows.div_ceil(FIXED_TREE_CHUNK_ROWS_V1)
}

/// Nodes on `level` of the tree over `chunks` chunks (level 0 is the chunks).
fn level_width_v1(chunks: usize, level: u32) -> usize {
    chunks.div_ceil(1usize << level)
}

/// One row: a substep in the sequential pass's visiting order, with the
/// gradients that pass gives its joint log-probability and value.
struct RowV1<'g, 'a> {
    index: usize,
    selected: &'g SelectedDecisionTapeV1<'a>,
    d_joint_log_probability: f32,
    d_value: f32,
}

/// The lengths of a node's tensors, taken once from the sequential
/// backward's own layout.
struct NodeShapeV1 {
    /// By parameter index, in the parameter's shape. The card embedding's
    /// entry is zero: a node holds its rows sparsely.
    gradient_lens: Vec<usize>,
    /// By parameter index: the transposed twin of a hidden-width weight, or
    /// zero for a tensor that accumulates in its own shape.
    input_major_lens: Vec<usize>,
    hidden: usize,
    card_width: usize,
}

impl NodeShapeV1 {
    fn node_v1(&self) -> NodeSumV1 {
        NodeSumV1 {
            tensors: GradientTensorsV1 {
                gradients: self
                    .gradient_lens
                    .iter()
                    .map(|len| vec![0.0; *len])
                    .collect(),
                input_major: self
                    .input_major_lens
                    .iter()
                    .map(|len| vec![0.0; *len])
                    .collect(),
                hidden: self.hidden,
            },
            card_slots: BTreeMap::new(),
            card_rows: Vec::new(),
            card_width: self.card_width,
        }
    }
}

/// The gradient sum of one chunk, or of the chunks under one tree node.
///
/// `tensors` is laid out as the sequential backward's: a hidden-width weight
/// accumulates in its transposed twin and its own-shape tensor stays +0.0
/// until the root is written back, every other tensor accumulates in its own
/// shape, and the card embedding's tensor is empty.
struct NodeSumV1 {
    tensors: GradientTensorsV1,
    /// Card token to its row's position in `card_rows`.
    card_slots: BTreeMap<usize, usize>,
    /// The held card-embedding rows, `card_width` values each.
    card_rows: Vec<f32>,
    card_width: usize,
}

impl NodeSumV1 {
    /// Every sum back to +0.0 and no card row held.
    fn reset_v1(&mut self) {
        let GradientTensorsV1 {
            gradients,
            input_major,
            ..
        } = &mut self.tensors;
        for (gradient, transposed) in gradients.iter_mut().zip(input_major.iter_mut()) {
            if transposed.is_empty() {
                gradient.fill(0.0);
            } else {
                // Only the twin was written; the own-shape tensor is still +0.0.
                transposed.fill(0.0);
            }
        }
        self.card_slots.clear();
        self.card_rows.clear();
    }

    /// `self + right`, element by element, into `self`: one tree combination
    /// with `self` the left child.
    fn add_v1(&mut self, right: &NodeSumV1) {
        let width = self.card_width;
        let GradientTensorsV1 {
            gradients,
            input_major,
            ..
        } = &mut self.tensors;
        for ((gradient, transposed), (right_gradient, right_transposed)) in
            gradients.iter_mut().zip(input_major.iter_mut()).zip(
                right
                    .tensors
                    .gradients
                    .iter()
                    .zip(&right.tensors.input_major),
            )
        {
            if transposed.is_empty() {
                add_elements_v1(gradient, right_gradient);
            } else {
                add_elements_v1(transposed, right_transposed);
            }
        }
        for (token, right_slot) in &right.card_slots {
            let right_row = &right.card_rows[right_slot * width..(right_slot + 1) * width];
            match self.card_slots.get(token) {
                Some(slot) => {
                    add_elements_v1(
                        &mut self.card_rows[slot * width..(slot + 1) * width],
                        right_row,
                    );
                }
                None => {
                    let slot = self.card_rows.len() / width;
                    self.card_rows.extend_from_slice(right_row);
                    self.card_slots.insert(*token, slot);
                }
            }
        }
    }
}

/// `left[i] + right[i]` into `left[i]` for every element: the tree's only
/// arithmetic. One rounded binary32 addition per element, with no product to
/// fuse and no reduction across elements, so the vector width a target picks
/// changes no bit.
fn add_elements_v1(left: &mut [f32], right: &[f32]) {
    assert_eq!(
        left.len(),
        right.len(),
        "fixed-tree nodes share one tensor layout"
    );
    for (left, right) in left.iter_mut().zip(right) {
        *left += *right;
    }
}

/// A chunk's sink: the sequential backward's sink for every layer, and the
/// node's own sparse rows for the card embedding.
struct ChunkSinkV1<'n> {
    dense: AccumulateTransposedV1<'n>,
    card_slots: &'n mut BTreeMap<usize, usize>,
    card_rows: &'n mut Vec<f32>,
    card_embedding_len: usize,
}

impl<'t> ReverseGradientSinkV1<'t> for ChunkSinkV1<'_> {
    fn linear_v1(
        &mut self,
        parameters: &[NativeNamedParameterV1],
        weight_index: usize,
        bias_index: usize,
        input: &'t [f32],
        rows: usize,
        input_dim: usize,
        output_dim: usize,
        d_output: &[f32],
        d_input: &mut Vec<f32>,
    ) -> Result<(), NativePolicyTrainErrorV1> {
        self.dense.linear_v1(
            parameters,
            weight_index,
            bias_index,
            input,
            rows,
            input_dim,
            output_dim,
            d_output,
            d_input,
        )
    }

    fn linear_read_from_v1(
        &mut self,
        parameters: &[NativeNamedParameterV1],
        weight_index: usize,
        bias_index: usize,
        input: &'t [f32],
        rows: usize,
        input_dim: usize,
        output_dim: usize,
        d_output: &[f32],
        first_read: usize,
        d_input: &mut Vec<f32>,
    ) -> Result<(), NativePolicyTrainErrorV1> {
        self.dense.linear_read_from_v1(
            parameters,
            weight_index,
            bias_index,
            input,
            rows,
            input_dim,
            output_dim,
            d_output,
            first_read,
            d_input,
        )
    }

    /// The sequential sink's additions, in its order (objects in order,
    /// padding token 0 skipped), into the node's row for each card instead
    /// of the card table's.
    fn card_embedding_dims_v1<D: NetDimsV1>(
        &mut self,
        object_card_ids: &'t [usize],
        d_object_input: &[f32],
    ) {
        let width = D::CARD_EMBEDDING;
        for (object, token) in object_card_ids.iter().copied().enumerate() {
            if token == 0 {
                continue;
            }
            // The sequential sink panics here, indexing past the embedding.
            assert!(
                token * width + width <= self.card_embedding_len,
                "card token {token} is outside the card embedding"
            );
            let slot = match self.card_slots.get(&token) {
                Some(slot) => *slot,
                None => {
                    let slot = self.card_rows.len() / width;
                    self.card_rows.resize(self.card_rows.len() + width, 0.0);
                    self.card_slots.insert(token, slot);
                    slot
                }
            };
            let source = object * D::OBJECT_ENCODER_INPUT + OBJECT_FEATURE_DIM_V1;
            for (sum, d_value) in self.card_rows[slot * width..(slot + 1) * width]
                .iter_mut()
                .zip(&d_object_input[source..source + width])
            {
                *sum += *d_value;
            }
        }
    }
}

/// One chunk's sum: its rows in order, each through the sequential arm's own
/// two steps (the logit gradients, then the reverse decision), into `sum`,
/// which is all +0.0 with no card row.
fn accumulate_chunk_dims_v1<D: NetDimsV1>(
    parameters: &[NativeNamedParameterV1],
    rows: &[RowV1<'_, '_>],
    sum: &mut NodeSumV1,
    workspace: &mut ReverseWorkspaceV1,
    controls: ControlsV1,
    math: TrainingMathV1,
) -> Result<(), NativePolicyTrainErrorV1> {
    let NodeSumV1 {
        tensors,
        card_slots,
        card_rows,
        ..
    } = sum;
    let mut sink = ChunkSinkV1 {
        dense: AccumulateTransposedV1::new_v1(tensors),
        card_slots,
        card_rows,
        card_embedding_len: parameters[CARD_EMBEDDING].values.len(),
    };
    for row in rows {
        controls.backward_v1(row.index)?;
        prepare_reverse_logits_v1(row.selected, row.d_joint_log_probability, workspace, math);
        let tape: &DecisionTapeV1 = &row.selected.tape;
        reverse_decision_into_dims_v1::<D>(
            parameters,
            &mut sink,
            tape,
            &workspace.d_logits,
            row.d_value,
            &mut workspace.decision,
        )?;
    }
    Ok(())
}

/// The tree over one backward's chunks. Threads hand it finished nodes in
/// any order; each combination's operands are fixed by the nodes' positions.
struct TreeV1<'s> {
    shape: &'s NodeShapeV1,
    chunks: usize,
    /// Finished nodes whose partner is not finished yet, by level and index.
    waiting: Mutex<BTreeMap<(u32, usize), NodeSumV1>>,
    /// Nodes whose sums moved into their left sibling, kept for reuse.
    spare: Mutex<Vec<NodeSumV1>>,
    root: Mutex<Option<NodeSumV1>>,
}

impl<'s> TreeV1<'s> {
    fn new_v1(shape: &'s NodeShapeV1, chunks: usize) -> Self {
        Self {
            shape,
            chunks,
            waiting: Mutex::new(BTreeMap::new()),
            spare: Mutex::new(Vec::new()),
            root: Mutex::new(None),
        }
    }

    /// A node with every sum at +0.0 and no card row, for one chunk.
    fn take_v1(&self) -> NodeSumV1 {
        let spare = self
            .spare
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();
        match spare {
            Some(mut node) => {
                node.reset_v1();
                node
            }
            None => self.shape.node_v1(),
        }
    }

    /// Takes the finished node `index` of `level` and performs every
    /// combination that is now possible on the way up. Of two partners, the
    /// one that arrives second combines them; the left one is always the
    /// left operand.
    fn complete_v1(&self, mut level: u32, mut index: usize, mut sum: NodeSumV1) {
        loop {
            let width = level_width_v1(self.chunks, level);
            if width == 1 {
                *self.root.lock().unwrap_or_else(PoisonError::into_inner) = Some(sum);
                return;
            }
            let partner = index ^ 1;
            if partner < width {
                let other = {
                    let mut waiting = self.waiting.lock().unwrap_or_else(PoisonError::into_inner);
                    match waiting.remove(&(level, partner)) {
                        Some(other) => other,
                        None => {
                            waiting.insert((level, index), sum);
                            return;
                        }
                    }
                };
                let (mut left, right) = if index & 1 == 0 {
                    (sum, other)
                } else {
                    (other, sum)
                };
                left.add_v1(&right);
                self.spare
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(right);
                sum = left;
            }
            // Combined, or the level's last node without a partner: either
            // way this is node `index / 2` of the next level.
            level += 1;
            index >>= 1;
        }
    }
}

enum ChunkOutcomeV1 {
    /// Not started: an earlier chunk had already failed.
    Skipped,
    Completed,
    Failed(NativePolicyTrainErrorV1),
    Panicked(Box<dyn std::any::Any + Send + 'static>),
}

type BackwardResultV1 =
    Result<(Vec<Vec<f32>>, ScorerBiasGaugeAccumulatorV1), NativePolicyTrainErrorV1>;

/// The ordinary GAE backward's gradients and gauge evidence under the fixed
/// tree (module documentation), on up to `workers` threads: the calling
/// thread and scoped helpers, as the exact parallel backward runs. `part`
/// selects the visited groups and the two loss-root coefficients of a visit,
/// exactly as in the sequential loop (`BackwardPartV1::All` is every update
/// without the pregame announcement step scale). `group_count` and
/// `value_coefficient` are the caller's, and `groups` are in forward order.
#[allow(clippy::too_many_arguments)]
pub(super) fn fixed_tree_backward_dims_v1<D: NetDimsV1>(
    parameters: &[NativeNamedParameterV1],
    groups: &[GroupTapeV1<'_>],
    part: BackwardPartV1<'_>,
    group_count: f32,
    value_coefficient: f32,
    workers: usize,
    math: TrainingMathV1,
    mxcsr: ThreadMxcsrCheckV1,
) -> BackwardResultV1 {
    let controls = ControlsV1::current_v1();
    // The sequential arm's coefficients and visiting order, for this part.
    let mut rows = Vec::new();
    for (group_index, group) in groups.iter().enumerate().rev() {
        if !part.visits_v1(group_index) {
            continue;
        }
        let d_value = part.d_value_v1(group, value_coefficient, group_count);
        for (substep_index, selected) in group.tapes.iter().enumerate().rev() {
            rows.push(RowV1 {
                index: rows.len(),
                selected,
                d_joint_log_probability: part.d_joint_log_probability_v1(
                    group_index,
                    group,
                    substep_index,
                    group_count,
                ),
                d_value: if substep_index == 0 { d_value } else { 0.0 },
            });
        }
    }
    // The gauge in the sequential order, ahead of the chunks.
    let mut gauge = ScorerBiasGaugeAccumulatorV1::for_math_v1(math);
    let mut gauge_failure = None;
    for row in &rows {
        let observed = controls.gauge_v1(row.index).and_then(|()| {
            gauge.observe(
                row.selected.tape.logits_v1(),
                row.selected.selected_action_index,
                row.d_joint_log_probability,
            )
        });
        if let Err(error) = observed {
            gauge_failure = Some((row.index, error));
            break;
        }
    }
    // The sequential arm observes a substep's gauge just before its backward,
    // so a gauge failure stops it before that substep's backward.
    let backward_end = gauge_failure
        .as_ref()
        .map_or(rows.len(), |(index, _)| *index);
    let rows = &rows[..backward_end];

    // The sequential backward's tensor layout, and its card table for the
    // result.
    let GradientTensorsV1 {
        gradients: mut layout,
        input_major,
        hidden,
    } = GradientTensorsV1::zeroed_dims_v1::<D>(parameters);
    let mut card_table = std::mem::take(&mut layout[CARD_EMBEDDING]);
    let shape = NodeShapeV1 {
        gradient_lens: layout.iter().map(Vec::len).collect(),
        input_major_lens: input_major.iter().map(Vec::len).collect(),
        hidden,
        card_width: D::CARD_EMBEDDING,
    };
    drop((layout, input_major));

    let chunk_rows = controls.chunk_rows_v1();
    let chunks = rows.len().div_ceil(chunk_rows);
    let workers = workers.clamp(1, chunks.max(1));
    #[cfg(test)]
    {
        record_effective_backward_workers_for_test_v1(workers);
        FIXED_TREE_RUNS_FOR_TEST_V1.with(|runs| runs.set(runs.get() + 1));
    }
    let tree = TreeV1::new_v1(&shape, chunks);
    let first_failure = AtomicUsize::new(usize::MAX);
    let mut outcomes = Vec::with_capacity(chunks);
    outcomes.resize_with(chunks, || ChunkOutcomeV1::Skipped);
    let items = rows
        .chunks(chunk_rows)
        .zip(outcomes.iter_mut())
        .enumerate()
        .collect::<Vec<_>>();
    drain_in_parallel_v1(
        items,
        workers,
        mxcsr,
        ReverseWorkspaceV1::default,
        |workspace, (chunk, (chunk_slice, outcome))| {
            // Chunks are taken in order, so every chunk before a failed one
            // has been taken already and the first failure is kept.
            if chunk > first_failure.load(AtomicOrdering::SeqCst) {
                return;
            }
            let computed = catch_unwind(AssertUnwindSafe(|| {
                let mut sum = tree.take_v1();
                accumulate_chunk_dims_v1::<D>(
                    parameters,
                    chunk_slice,
                    &mut sum,
                    workspace,
                    controls,
                    math,
                )
                .map(|()| sum)
            }));
            *outcome = match computed {
                Ok(Ok(sum)) => {
                    tree.complete_v1(0, chunk, sum);
                    ChunkOutcomeV1::Completed
                }
                Ok(Err(error)) => {
                    first_failure.fetch_min(chunk, AtomicOrdering::SeqCst);
                    ChunkOutcomeV1::Failed(error)
                }
                Err(payload) => {
                    first_failure.fetch_min(chunk, AtomicOrdering::SeqCst);
                    ChunkOutcomeV1::Panicked(payload)
                }
            };
        },
    )?;
    // The first failure in visiting order: a chunk stops at its first failing
    // row, and no chunk before the first failed one was skipped.
    for outcome in outcomes {
        match outcome {
            ChunkOutcomeV1::Completed | ChunkOutcomeV1::Skipped => {}
            ChunkOutcomeV1::Failed(error) => return Err(error),
            ChunkOutcomeV1::Panicked(payload) => resume_unwind(payload),
        }
    }
    if let Some((_, error)) = gauge_failure {
        return Err(error);
    }

    let TreeV1 { root, .. } = tree;
    let root = root.into_inner().unwrap_or_else(PoisonError::into_inner);
    let mut tensors = match (root, chunks) {
        (Some(root), _) => {
            let NodeSumV1 {
                tensors,
                card_slots,
                card_rows,
                card_width,
            } = root;
            for (token, slot) in card_slots {
                card_table[token * card_width..(token + 1) * card_width]
                    .copy_from_slice(&card_rows[slot * card_width..(slot + 1) * card_width]);
            }
            tensors
        }
        // No row at all: every sum is +0.0.
        (None, 0) => shape.node_v1().tensors,
        // Every chunk completed, so the tree has its root.
        (None, _) => return Err(NativePolicyTrainErrorV1::ParameterManifest),
    };
    tensors.gradients[CARD_EMBEDDING] = card_table;
    Ok((tensors.into_gradients_v1(), gauge))
}

/// Test-only failure injection and chunk size, read on the calling thread;
/// empty outside tests.
#[derive(Clone, Copy)]
struct ControlsV1 {
    #[cfg(test)]
    hooks: FixedTreeHooksForTestV1,
}

impl ControlsV1 {
    fn current_v1() -> Self {
        Self {
            #[cfg(test)]
            hooks: FIXED_TREE_HOOKS_FOR_TEST_V1.with(Cell::get),
        }
    }

    /// The contract's constant. A test may ask for another size to show that
    /// the size is part of the result.
    fn chunk_rows_v1(self) -> usize {
        #[cfg(test)]
        if let Some(chunk_rows) = self.hooks.chunk_rows {
            return chunk_rows;
        }
        FIXED_TREE_CHUNK_ROWS_V1
    }

    #[cfg_attr(not(test), allow(unused_variables))]
    fn gauge_v1(self, index: usize) -> Result<(), NativePolicyTrainErrorV1> {
        #[cfg(test)]
        if self.hooks.gauge_error_at == Some(index) {
            return Err(NativePolicyTrainErrorV1::GaugeBoundOverflow);
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(unused_variables))]
    fn backward_v1(self, index: usize) -> Result<(), NativePolicyTrainErrorV1> {
        #[cfg(test)]
        if self.hooks.backward_panic_at == Some(index) {
            panic!("injected fixed-tree backward panic at row {index}");
        }
        #[cfg(test)]
        if self.hooks.backward_error_at == Some(index) {
            return Err(NativePolicyTrainErrorV1::NonFinite {
                stage: "injected_fixed_tree_backward",
                index,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
thread_local! {
    static FIXED_TREE_RUNS_FOR_TEST_V1: Cell<u64> = const { Cell::new(0) };
    static FIXED_TREE_HOOKS_FOR_TEST_V1: Cell<FixedTreeHooksForTestV1> =
        const { Cell::new(FixedTreeHooksForTestV1::NONE) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(super) struct FixedTreeHooksForTestV1 {
    /// Rows per chunk instead of the contract's constant.
    pub(super) chunk_rows: Option<usize>,
    pub(super) gauge_error_at: Option<usize>,
    pub(super) backward_error_at: Option<usize>,
    pub(super) backward_panic_at: Option<usize>,
}

#[cfg(test)]
impl FixedTreeHooksForTestV1 {
    pub(super) const NONE: Self = Self {
        chunk_rows: None,
        gauge_error_at: None,
        backward_error_at: None,
        backward_panic_at: None,
    };
}

/// Runs `body` with `hooks` installed on this thread; returns the result and
/// how many fixed-tree backwards ran.
#[cfg(test)]
pub(super) fn with_fixed_tree_hooks_for_test_v1<R>(
    hooks: FixedTreeHooksForTestV1,
    body: impl FnOnce() -> R,
) -> (R, u64) {
    struct Restore(FixedTreeHooksForTestV1);
    impl Drop for Restore {
        fn drop(&mut self) {
            FIXED_TREE_HOOKS_FOR_TEST_V1.with(|active| active.set(self.0));
        }
    }
    let _restore = Restore(FIXED_TREE_HOOKS_FOR_TEST_V1.with(|active| active.replace(hooks)));
    let runs = FIXED_TREE_RUNS_FOR_TEST_V1.with(Cell::get);
    let result = body();
    (result, FIXED_TREE_RUNS_FOR_TEST_V1.with(Cell::get) - runs)
}

#[cfg(test)]
mod tests;
