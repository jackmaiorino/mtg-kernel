//! Exact parallel CPU backward for the ordinary GAE update.
//!
//! The sequential backward (`gae_v1`'s `Sequential` arm) visits groups in
//! reverse and each group's substeps in reverse. For each substep it computes
//! the activation gradients layer by layer and adds every layer's
//! parameter-gradient terms to the shared gradient tensors as it goes. The
//! activation gradients depend only on the parameters, the substep's tape and
//! its logit and value gradients, never on the gradient tensors, so phase A
//! computes them for a block of substeps at once and keeps each layer's output
//! gradient. Phase B then gives every gradient element exactly the additions
//! the sequential pass gives it, in the same order: substeps in visiting order
//! and, within a substep, the layer's rows in order (the scorer bias's rows in
//! reverse, the card embedding's objects in order). Each element's sum is
//! unchanged and elements are independent, so phase B hands disjoint row
//! blocks of the gradient tensors to different threads.
//!
//! Every gradient bit, the gauge evidence and the first failure reported equal
//! the sequential pass's: the gauge is observed in the sequential order before
//! the blocks run, and failures are merged in visiting order, a substep's
//! gauge observation ahead of its backward.

use super::*;
use std::ops::Range;
use std::sync::{Mutex, PoisonError};

/// Fewest substeps per backward worker before another thread is worth
/// spawning; one substep's backward costs about a millisecond.
const EXACT_BACKWARD_MIN_SUBSTEPS_PER_WORKER_V1: usize = 32;

/// Substeps whose layer output gradients are held at once (tens of kilobytes
/// each).
const EXACT_BACKWARD_BLOCK_SUBSTEPS_V1: usize = 512;

/// Phase A chunks and phase B gradient blocks per worker, for load balance.
const EXACT_BACKWARD_ITEMS_PER_WORKER_V1: usize = 8;

/// One linear layer's backward step for one substep: its input (borrowed from
/// the tape) and its output gradient (in `SubstepTermsV1::d_outputs`).
struct LinearTermsV1<'t> {
    weight_index: usize,
    bias_index: usize,
    input: &'t [f32],
    rows: usize,
    input_dim: usize,
    output_dim: usize,
    d_output: Range<usize>,
}

/// Everything phase B needs from one substep's backward. Buffers are cleared
/// before each use and reused across blocks.
#[derive(Default)]
struct SubstepTermsV1<'t> {
    d_outputs: Vec<f32>,
    linears: Vec<LinearTermsV1<'t>>,
    /// Non-padding card tokens in object order, each with its embedding
    /// gradient row in `d_card_embeddings`.
    card_tokens: Vec<usize>,
    d_card_embeddings: Vec<f32>,
}

/// Phase A's sink: computes each layer's input gradient exactly as
/// `linear_backward_into` does and keeps the terms it would have added.
struct RecordTermsV1<'r, 't> {
    terms: &'r mut SubstepTermsV1<'t>,
    card_embedding_len: usize,
}

impl<'t> ReverseGradientSinkV1<'t> for RecordTermsV1<'_, 't> {
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
        let weight = &parameters[weight_index].values;
        // `linear_backward_into`'s checks. The gradient tensors it also checks
        // are allocated with the parameters' shapes.
        if input.len() != rows * input_dim
            || d_output.len() != rows * output_dim
            || weight.len() != output_dim * input_dim
            || parameters[bias_index].values.len() != output_dim
        {
            return Err(NativePolicyTrainErrorV1::ParameterManifest);
        }
        resize_zeroed_v1(d_input, rows * input_dim);
        input_gradient_rows_v1(weight, input_dim, output_dim, d_output, d_input);
        let terms = &mut *self.terms;
        let start = terms.d_outputs.len();
        terms.d_outputs.extend_from_slice(d_output);
        terms.linears.push(LinearTermsV1 {
            weight_index,
            bias_index,
            input,
            rows,
            input_dim,
            output_dim,
            d_output: start..terms.d_outputs.len(),
        });
        Ok(())
    }

    fn card_embedding_v1(&mut self, object_card_ids: &'t [usize], d_object_input: &[f32]) {
        let terms = &mut *self.terms;
        for (object, token) in object_card_ids.iter().copied().enumerate() {
            if token == 0 {
                continue;
            }
            // The sequential sink panics here, indexing past the embedding.
            assert!(
                token * CARD_EMBEDDING_DIM_V1 + CARD_EMBEDDING_DIM_V1 <= self.card_embedding_len,
                "card token {token} is outside the card embedding"
            );
            let source = object * OBJECT_ENCODER_INPUT + OBJECT_FEATURE_DIM_V1;
            terms.card_tokens.push(token);
            terms
                .d_card_embeddings
                .extend_from_slice(&d_object_input[source..source + CARD_EMBEDDING_DIM_V1]);
        }
    }
}

/// One substep in the sequential pass's visiting order, with the gradients
/// that pass gives its joint log-probability and value.
struct VisitV1<'g, 'a> {
    index: usize,
    selected: &'g SelectedDecisionTapeV1<'a>,
    d_joint_log_probability: f32,
    d_value: f32,
}

enum PhaseAOutcomeV1 {
    Completed(Result<(), NativePolicyTrainErrorV1>),
    Panicked(Box<dyn std::any::Any + Send + 'static>),
}

/// A row range of one gradient tensor, owned by one phase B task.
struct GradientBlockV1<'v> {
    tensor: usize,
    rows: Range<usize>,
    width: usize,
    values: &'v mut [f32],
    cost: u64,
}

type BackwardResultV1 =
    Result<(Vec<Vec<f32>>, ScorerBiasGaugeAccumulatorV1), NativePolicyTrainErrorV1>;

/// The ordinary GAE backward's gradients and gauge evidence on up to
/// `workers` threads, or `None` when too few substeps would share them (the
/// caller then runs its sequential loop). `group_count` and
/// `value_coefficient` are the caller's, and `groups` are in forward order.
pub(super) fn exact_parallel_backward_v1(
    parameters: &[NativeNamedParameterV1],
    groups: &[GroupTapeV1<'_>],
    group_count: f32,
    value_coefficient: f32,
    workers: usize,
    math: TrainingMathV1,
    mxcsr: ThreadMxcsrCheckV1,
) -> Option<BackwardResultV1> {
    let substeps = groups.iter().map(|group| group.tapes.len()).sum::<usize>();
    let workers = workers.min(substeps / EXACT_BACKWARD_MIN_SUBSTEPS_PER_WORKER_V1);
    if workers < 2 {
        return None;
    }
    #[cfg(test)]
    EXACT_BACKWARD_RUNS_FOR_TEST_V1.with(|runs| runs.set(runs.get() + 1));
    Some(backward_v1(
        parameters,
        groups,
        group_count,
        value_coefficient,
        workers,
        math,
        mxcsr,
    ))
}

fn backward_v1(
    parameters: &[NativeNamedParameterV1],
    groups: &[GroupTapeV1<'_>],
    group_count: f32,
    value_coefficient: f32,
    workers: usize,
    math: TrainingMathV1,
    mxcsr: ThreadMxcsrCheckV1,
) -> BackwardResultV1 {
    let controls = ControlsV1::current_v1();
    // The sequential arm's coefficients and visiting order.
    let mut visits = Vec::new();
    for group in groups.iter().rev() {
        let d_joint_log_probability = -group.advantage / group_count;
        let d_value = (value_coefficient / group_count) * (2.0 * group.value_error);
        for (substep_index, selected) in group.tapes.iter().enumerate().rev() {
            visits.push(VisitV1 {
                index: visits.len(),
                selected,
                d_joint_log_probability,
                d_value: if substep_index == 0 { d_value } else { 0.0 },
            });
        }
    }
    let mut gauge = ScorerBiasGaugeAccumulatorV1::for_math_v1(math);
    let mut gauge_failure = None;
    for visit in &visits {
        let observed = controls.gauge_v1(visit.index).and_then(|()| {
            gauge.observe(
                visit.selected.tape.logits_v1(),
                visit.selected.selected_action_index,
                visit.d_joint_log_probability,
            )
        });
        if let Err(error) = observed {
            gauge_failure = Some((visit.index, error));
            break;
        }
    }
    // The sequential arm observes a substep's gauge just before its backward,
    // so a gauge failure stops it before that substep's backward.
    let backward_end = gauge_failure
        .as_ref()
        .map_or(visits.len(), |(index, _)| *index);

    let mut gradients = parameters
        .iter()
        .map(|parameter| vec![0.0; parameter.values.len()])
        .collect::<Vec<_>>();
    let mut terms = Vec::new();
    for block in visits[..backward_end].chunks(controls.block_substeps_v1()) {
        terms.resize_with(block.len(), SubstepTermsV1::default);
        let outcomes = record_block_v1(
            parameters, block, &mut terms, workers, controls, math, mxcsr,
        )?;
        for outcome in outcomes {
            match outcome {
                PhaseAOutcomeV1::Completed(result) => result?,
                PhaseAOutcomeV1::Panicked(payload) => resume_unwind(payload),
            }
        }
        let blocks = gradient_blocks_v1(parameters, &mut gradients, &terms, workers);
        drain_in_parallel_v1(
            blocks,
            workers,
            mxcsr,
            || (),
            |_, block| {
                replay_block_v1(block, &terms);
            },
        )?;
    }
    if let Some((_, error)) = gauge_failure {
        return Err(error);
    }
    Ok((gradients, gauge))
}

/// Phase A: every substep's activation gradients and kept terms, with one
/// outcome per substep in visiting order.
fn record_block_v1<'g>(
    parameters: &[NativeNamedParameterV1],
    visits: &[VisitV1<'g, '_>],
    terms: &mut [SubstepTermsV1<'g>],
    workers: usize,
    controls: ControlsV1,
    math: TrainingMathV1,
    mxcsr: ThreadMxcsrCheckV1,
) -> Result<Vec<PhaseAOutcomeV1>, NativePolicyTrainErrorV1> {
    let mut outcomes = Vec::with_capacity(visits.len());
    outcomes.resize_with(visits.len(), || PhaseAOutcomeV1::Completed(Ok(())));
    let chunk = visits
        .len()
        .div_ceil(workers * EXACT_BACKWARD_ITEMS_PER_WORKER_V1)
        .max(1);
    let items = visits
        .chunks(chunk)
        .zip(terms.chunks_mut(chunk))
        .zip(outcomes.chunks_mut(chunk))
        .collect::<Vec<_>>();
    drain_in_parallel_v1(
        items,
        workers,
        mxcsr,
        ReverseWorkspaceV1::default,
        |workspace, ((visits, terms), outcomes)| {
            for ((visit, terms), outcome) in visits.iter().zip(terms).zip(outcomes) {
                *outcome = match catch_unwind(AssertUnwindSafe(|| {
                    controls.backward_v1(visit.index)?;
                    record_substep_v1(parameters, visit, terms, workspace, math)
                })) {
                    Ok(result) => PhaseAOutcomeV1::Completed(result),
                    Err(payload) => PhaseAOutcomeV1::Panicked(payload),
                };
            }
        },
    )?;
    Ok(outcomes)
}

fn record_substep_v1<'g>(
    parameters: &[NativeNamedParameterV1],
    visit: &VisitV1<'g, '_>,
    terms: &mut SubstepTermsV1<'g>,
    workspace: &mut ReverseWorkspaceV1,
    math: TrainingMathV1,
) -> Result<(), NativePolicyTrainErrorV1> {
    terms.d_outputs.clear();
    terms.linears.clear();
    terms.card_tokens.clear();
    terms.d_card_embeddings.clear();
    prepare_reverse_logits_v1(
        visit.selected,
        visit.d_joint_log_probability,
        workspace,
        math,
    );
    let tape: &'g DecisionTapeV1 = &visit.selected.tape;
    reverse_decision_into_v1(
        parameters,
        &mut RecordTermsV1 {
            terms,
            card_embedding_len: parameters[CARD_EMBEDDING].values.len(),
        },
        tape,
        &workspace.d_logits,
        visit.d_value,
        &mut workspace.decision,
    )
}

/// Splits every gradient tensor this block touches into row blocks of about
/// equal work (one multiply-add per element and layer row), largest first.
fn gradient_blocks_v1<'v>(
    parameters: &[NativeNamedParameterV1],
    gradients: &'v mut [Vec<f32>],
    terms: &[SubstepTermsV1<'_>],
    workers: usize,
) -> Vec<GradientBlockV1<'v>> {
    // Elements per row: the product of the dimensions after the first.
    let widths = parameters
        .iter()
        .map(|parameter| parameter.shape.iter().skip(1).product::<usize>().max(1))
        .collect::<Vec<_>>();
    let mut costs = vec![0u64; gradients.len()];
    for substep in terms {
        for linear in &substep.linears {
            // Replay steps through each tensor by its shape's rows, which
            // `validate_parameter_manifest` ties to the layer's dimensions.
            assert!(
                widths[linear.weight_index] == linear.input_dim && widths[linear.bias_index] == 1,
                "the exact parallel backward needs validated parameter shapes"
            );
            let outputs = (linear.rows * linear.output_dim) as u64;
            costs[linear.weight_index] += outputs * linear.input_dim as u64;
            costs[linear.bias_index] += outputs;
        }
        assert!(
            substep.card_tokens.is_empty() || widths[CARD_EMBEDDING] == CARD_EMBEDDING_DIM_V1,
            "the exact parallel backward needs validated parameter shapes"
        );
        costs[CARD_EMBEDDING] += substep.d_card_embeddings.len() as u64;
    }
    let target =
        (costs.iter().sum::<u64>() / (workers * EXACT_BACKWARD_ITEMS_PER_WORKER_V1) as u64).max(1);
    let mut blocks = Vec::new();
    for (tensor, ((values, width), cost)) in gradients.iter_mut().zip(widths).zip(costs).enumerate()
    {
        let rows = values.len() / width;
        if cost == 0 || rows == 0 {
            continue;
        }
        let pieces = usize::try_from(cost.div_ceil(target))
            .unwrap_or(rows)
            .clamp(1, rows);
        let rows_per_piece = rows.div_ceil(pieces);
        for (piece, values) in values.chunks_mut(rows_per_piece * width).enumerate() {
            let first = piece * rows_per_piece;
            let block_rows = values.len() / width;
            blocks.push(GradientBlockV1 {
                tensor,
                rows: first..first + block_rows,
                width,
                values,
                cost: cost * block_rows as u64 / rows as u64,
            });
        }
    }
    blocks.sort_by_key(|block| std::cmp::Reverse(block.cost));
    blocks
}

/// `linear_backward_into`'s input gradient: for each row, the outputs' terms
/// in output order. Elements are independent, so the vector width changes
/// no bit (AVX2 does not enable FMA).
fn input_gradient_rows_v1(
    weight: &[f32],
    input_dim: usize,
    output_dim: usize,
    d_output: &[f32],
    d_input: &mut [f32],
) {
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: AVX2 support was detected at run time just above.
        unsafe { input_gradient_rows_avx2_v1(weight, input_dim, output_dim, d_output, d_input) };
        return;
    }
    input_gradient_rows_body_v1(weight, input_dim, output_dim, d_output, d_input);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn input_gradient_rows_avx2_v1(
    weight: &[f32],
    input_dim: usize,
    output_dim: usize,
    d_output: &[f32],
    d_input: &mut [f32],
) {
    input_gradient_rows_body_v1(weight, input_dim, output_dim, d_output, d_input);
}

#[inline(always)]
fn input_gradient_rows_body_v1(
    weight: &[f32],
    input_dim: usize,
    output_dim: usize,
    d_output: &[f32],
    d_input: &mut [f32],
) {
    if input_dim == 0 || output_dim == 0 {
        return;
    }
    for (d_input_row, d_output_row) in d_input
        .chunks_exact_mut(input_dim)
        .zip(d_output.chunks_exact(output_dim))
    {
        for (d_value, weight_row) in d_output_row.iter().zip(weight.chunks_exact(input_dim)) {
            for (d_input, weight) in d_input_row.iter_mut().zip(weight_row) {
                *d_input += *d_value * *weight;
            }
        }
    }
}

/// Phase B: the block's elements receive the sequential pass's additions in
/// its order: substeps in visiting order, then rows in order, except the
/// scorer bias's reverse rows. Elements are independent, so the vector width
/// changes no bit (AVX2 does not enable FMA).
fn replay_block_v1(block: GradientBlockV1<'_>, terms: &[SubstepTermsV1<'_>]) {
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: AVX2 support was detected at run time just above.
        unsafe { replay_block_avx2_v1(block, terms) };
        return;
    }
    replay_block_body_v1(block, terms);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn replay_block_avx2_v1(block: GradientBlockV1<'_>, terms: &[SubstepTermsV1<'_>]) {
    replay_block_body_v1(block, terms);
}

#[inline(always)]
fn replay_block_body_v1(block: GradientBlockV1<'_>, terms: &[SubstepTermsV1<'_>]) {
    let GradientBlockV1 {
        tensor,
        rows: block_rows,
        width,
        values,
        ..
    } = block;
    let first = block_rows.start;
    for substep in terms {
        if tensor == CARD_EMBEDDING {
            for (token, d_embedding) in substep.card_tokens.iter().zip(
                substep
                    .d_card_embeddings
                    .chunks_exact(CARD_EMBEDDING_DIM_V1),
            ) {
                if block_rows.contains(token) {
                    let offset = (token - first) * width;
                    for (gradient, d_value) in
                        values[offset..offset + width].iter_mut().zip(d_embedding)
                    {
                        *gradient += *d_value;
                    }
                }
            }
            continue;
        }
        for linear in &substep.linears {
            let d_output = &substep.d_outputs[linear.d_output.clone()];
            if linear.weight_index == tensor {
                for row in 0..linear.rows {
                    let input_row =
                        &linear.input[row * linear.input_dim..(row + 1) * linear.input_dim];
                    let d_row = &d_output[row * linear.output_dim..(row + 1) * linear.output_dim];
                    for output_index in block_rows.clone() {
                        let d_value = d_row[output_index];
                        let offset = (output_index - first) * linear.input_dim;
                        for (gradient, input) in values[offset..offset + linear.input_dim]
                            .iter_mut()
                            .zip(input_row)
                        {
                            *gradient += d_value * *input;
                        }
                    }
                }
            }
            if linear.bias_index == tensor {
                if linear.weight_index == SCORER_SECOND_WEIGHT {
                    for output_index in block_rows.clone() {
                        for row in (0..linear.rows).rev() {
                            values[output_index - first] +=
                                d_output[row * linear.output_dim + output_index];
                        }
                    }
                } else {
                    for row in 0..linear.rows {
                        for output_index in block_rows.clone() {
                            values[output_index - first] +=
                                d_output[row * linear.output_dim + output_index];
                        }
                    }
                }
            }
        }
    }
}

/// Runs `work` on every item, on the calling thread and up to `workers - 1`
/// scoped helpers, each thread with its own `state`. Items are independent,
/// so which thread runs one changes no result. If a helper cannot be spawned
/// the remaining threads take its share; a helper's panic resumes on the
/// calling thread once every thread has stopped. A helper outside the pinned
/// MXCSR state (`mxcsr`) takes no item and its error is returned once every
/// thread has stopped; the calling thread verifies before its own share.
fn drain_in_parallel_v1<T: Send, S>(
    items: Vec<T>,
    workers: usize,
    mxcsr: ThreadMxcsrCheckV1,
    state: impl Fn() -> S + Sync,
    work: impl Fn(&mut S, T) + Sync,
) -> Result<(), NativePolicyTrainErrorV1> {
    mxcsr.verify_v1()?;
    let queue = Mutex::new(items.into_iter());
    let refused = Mutex::new(None);
    let next = || queue.lock().unwrap_or_else(PoisonError::into_inner).next();
    let drain = || {
        let mut state = state();
        while let Some(item) = next() {
            work(&mut state, item);
        }
    };
    let helper = || match mxcsr.verify_worker_v1() {
        Ok(()) => drain(),
        Err(error) => {
            refused
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert(error);
        }
    };
    thread::scope(|scope| {
        let helpers = (1..workers)
            .filter_map(|index| {
                thread::Builder::new()
                    .name(format!("native-gae-backward-{index}"))
                    .spawn_scoped(scope, helper)
                    .ok()
            })
            .collect::<Vec<_>>();
        drain();
        for helper in helpers {
            if let Err(payload) = helper.join() {
                resume_unwind(payload);
            }
        }
    });
    match refused.into_inner().unwrap_or_else(PoisonError::into_inner) {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Test-only failure injection and block size, read on the calling thread;
/// empty outside tests.
#[derive(Clone, Copy)]
struct ControlsV1 {
    #[cfg(test)]
    hooks: ExactBackwardHooksForTestV1,
}

impl ControlsV1 {
    fn current_v1() -> Self {
        Self {
            #[cfg(test)]
            hooks: EXACT_BACKWARD_HOOKS_FOR_TEST_V1.with(Cell::get),
        }
    }

    fn block_substeps_v1(self) -> usize {
        #[cfg(test)]
        if let Some(block_substeps) = self.hooks.block_substeps {
            return block_substeps;
        }
        EXACT_BACKWARD_BLOCK_SUBSTEPS_V1
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
            panic!("injected exact backward panic at substep {index}");
        }
        #[cfg(test)]
        if self.hooks.backward_error_at == Some(index) {
            return Err(NativePolicyTrainErrorV1::NonFinite {
                stage: "injected_exact_backward",
                index,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
thread_local! {
    static EXACT_BACKWARD_RUNS_FOR_TEST_V1: Cell<u64> = const { Cell::new(0) };
    static EXACT_BACKWARD_HOOKS_FOR_TEST_V1: Cell<ExactBackwardHooksForTestV1> =
        const { Cell::new(ExactBackwardHooksForTestV1::NONE) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(super) struct ExactBackwardHooksForTestV1 {
    pub(super) block_substeps: Option<usize>,
    pub(super) gauge_error_at: Option<usize>,
    pub(super) backward_error_at: Option<usize>,
    pub(super) backward_panic_at: Option<usize>,
}

#[cfg(test)]
impl ExactBackwardHooksForTestV1 {
    pub(super) const NONE: Self = Self {
        block_substeps: None,
        gauge_error_at: None,
        backward_error_at: None,
        backward_panic_at: None,
    };
}

/// Runs `body` with `hooks` installed on this thread; returns the result and
/// how many exact parallel backwards ran.
#[cfg(test)]
pub(super) fn with_exact_backward_hooks_for_test_v1<R>(
    hooks: ExactBackwardHooksForTestV1,
    body: impl FnOnce() -> R,
) -> (R, u64) {
    struct Restore(ExactBackwardHooksForTestV1);
    impl Drop for Restore {
        fn drop(&mut self) {
            EXACT_BACKWARD_HOOKS_FOR_TEST_V1.with(|active| active.set(self.0));
        }
    }
    let _restore = Restore(EXACT_BACKWARD_HOOKS_FOR_TEST_V1.with(|active| active.replace(hooks)));
    let runs = EXACT_BACKWARD_RUNS_FOR_TEST_V1.with(Cell::get);
    let result = body();
    (
        result,
        EXACT_BACKWARD_RUNS_FOR_TEST_V1.with(Cell::get) - runs,
    )
}

#[cfg(test)]
mod tests;
