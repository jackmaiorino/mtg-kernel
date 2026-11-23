//! The fixed-tree diagnostic: one existing update's learner step measured
//! both ways on the same batch (`native_expanded_training_run_v1
//! --fixed-tree-diagnostic UPDATE_COMMAND.json`).
//!
//! The update command of a completed iteration names a checkpoint and a
//! trajectory batch. [`diagnose_fixed_tree_v1`] takes it through the update's
//! own loading, validation, replay and target computation
//! (`execute_update_inner_v1`), and where the update would train it runs the
//! plain GAE step on copies of the loaded state instead:
//!
//! - under the sequential order (today's backward), at the given thread
//!   count, where the trainer replays that order on its preparation workers,
//!   and on one thread;
//! - under the fixed tree (`fixed_tree_backward_v1`), at the given thread
//!   count and on one thread.
//!
//! Each is repeated and timed by phase, and the first result of the two
//! threaded runs is compared element by element: the gradients, the loss and
//! the train state after the optimizer step. Nothing is published and no
//! file is written: the command's output directory is never read or created,
//! and the loaded state is only copied.
//!
//! **Relative differences.** `|a - b| / max(|a|, |b|, floor)`, with `floor`
//! the stated `relative_floor` (default [`DEFAULT_RELATIVE_FLOOR_V1`]). Two
//! values that are both smaller in magnitude than the floor are compared
//! against the floor, so sums that cancel to nearly zero do not dominate.
//!
//! **Whole update.** `update_seconds` is this invocation's input read and
//! behavior replay (measured once, the same work both ways) plus the median
//! learner step. Checkpoint publication and readback are not performed and
//! are the same both ways.

use super::*;
use crate::native_policy_train_step_v1::NativePolicyTrainStepResultV1;
use crate::native_training_phase_diagnostic_v1::NativeTrainingPhaseV1;

/// The relative difference's default floor. Adam's epsilon is 1e-8; a
/// gradient element below a tenth of it moves no parameter visibly.
pub const DEFAULT_RELATIVE_FLOOR_V1: f64 = 1.0e-9;
/// The default number of repeats of each measurement.
pub const DEFAULT_REPEATS_V1: usize = 5;
/// The largest thread count the diagnostic takes (the preparation workers'
/// bound).
pub const MAX_THREADS_V1: usize = 32;
/// The largest repeat count the diagnostic takes.
pub const MAX_REPEATS_V1: usize = 1_000;

/// What the diagnostic measures with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedTreeDiagnosticOptionsV1 {
    /// Threads of the threaded measurements; `None` is the command's own
    /// preparation workers (one for a serial update).
    pub threads: Option<usize>,
    /// Repeats of each measurement; medians are reported.
    pub repeats: usize,
    /// The floor of the relative difference's denominator.
    pub relative_floor: f64,
}

impl Default for FixedTreeDiagnosticOptionsV1 {
    fn default() -> Self {
        Self {
            threads: None,
            repeats: DEFAULT_REPEATS_V1,
            relative_floor: DEFAULT_RELATIVE_FLOOR_V1,
        }
    }
}

impl FixedTreeDiagnosticOptionsV1 {
    pub fn validate_v1(&self) -> Result<(), String> {
        ensure(
            self.threads
                .is_none_or(|threads| (1..=MAX_THREADS_V1).contains(&threads)),
            "fixed-tree diagnostic threads must be in 1..=32",
        )?;
        ensure(
            (1..=MAX_REPEATS_V1).contains(&self.repeats),
            "fixed-tree diagnostic repeats must be in 1..=1000",
        )?;
        ensure(
            self.relative_floor.is_finite() && self.relative_floor > 0.0,
            "fixed-tree diagnostic relative floor must be positive",
        )
    }
}

/// Measures the learner step of `command` (an `update` or `update_prepared`
/// command of the plain CPU GAE update of a net-8 learner) both ways; see
/// the module documentation. The command's files must exist as it pins them.
pub fn diagnose_fixed_tree_v1(
    command: ExpandedTrainingCommandV1,
    options: FixedTreeDiagnosticOptionsV1,
) -> Result<Value, String> {
    options.validate_v1()?;
    match command {
        ExpandedTrainingCommandV1::Update {
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            update_backward_execution,
            loss_selection,
            line_b,
            training_math,
            record_format,
            output_directory,
        } => execute_update_inner_v1(
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            update_backward_execution,
            loss_selection,
            output_directory,
            None,
            DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
            MAX_BATCH_BYTES,
            line_b,
            None,
            None,
            None,
            None,
            training_math,
            record_format,
            RecordEnvelopeV1::None,
            Some(&options),
        ),
        ExpandedTrainingCommandV1::UpdatePrepared {
            source,
            trajectories,
            learning_rate,
            value_coefficient,
            update_backend,
            update_backward_execution,
            loss_selection,
            preparation_workers,
            max_prepared_tensor_mebibytes,
            max_trajectory_batch_mebibytes,
            line_b,
            training_math,
            record_format,
            record_envelope,
            output_directory,
        } => {
            validate_preparation_workers_v1(preparation_workers)?;
            validate_max_prepared_tensor_mebibytes_v1(max_prepared_tensor_mebibytes)?;
            validate_max_trajectory_batch_mebibytes_v1(max_trajectory_batch_mebibytes)?;
            execute_update_inner_v1(
                source,
                trajectories,
                learning_rate,
                value_coefficient,
                update_backend,
                update_backward_execution,
                loss_selection,
                output_directory,
                Some(preparation_workers),
                max_prepared_tensor_mebibytes,
                trajectory_batch_bytes_v1(max_trajectory_batch_mebibytes),
                line_b,
                None,
                None,
                None,
                None,
                training_math,
                record_format,
                record_envelope,
                Some(&options),
            )
        }
        _ => Err(
            "the fixed-tree diagnostic takes an update or update_prepared command \
             (an iteration attempt's update-command.json)"
                .into(),
        ),
    }
}

/// What the update had done when it reached its learner step.
pub(super) struct PreparedBatchV1 {
    pub(super) episodes: usize,
    pub(super) before_state_sha256: String,
    pub(super) preparation_workers: Option<usize>,
    pub(super) input_read_seconds: f64,
    pub(super) behavior_replay_seconds: f64,
}

/// One way of running the learner step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModeV1 {
    /// Today's order, on the given threads (the exact parallel replay of the
    /// sequential order where the batch is large enough).
    Sequential,
    /// The fixed tree, on the given threads.
    FixedTree,
}

/// One learner step's wall seconds, whole and by phase.
#[derive(Clone, Copy, Default)]
struct StepSecondsV1 {
    step: f64,
    setup_validation: f64,
    forward_loss: f64,
    backward_gauge: f64,
    adam_math: f64,
    finalization: f64,
}

/// The first repeat's result and state of one measurement, and every
/// repeat's seconds.
struct MeasuredV1 {
    result: NativePolicyTrainStepResultV1,
    state: NativePolicyValueTrainStateV1,
    after_state_sha256: String,
    /// Whether every repeat reached the first repeat's state.
    repeats_agree: bool,
    seconds: Vec<StepSecondsV1>,
}

#[allow(clippy::too_many_arguments)]
fn step_v1(
    mode: ModeV1,
    threads: usize,
    state: &NativePolicyValueTrainStateV1,
    generation: FreshLineageGenerationV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_targets: &[f32],
    advantages: &[f32],
    value_coefficient: f32,
    learning_rate: f32,
) -> Result<
    (
        NativePolicyTrainStepResultV1,
        NativePolicyValueTrainStateV1,
        StepSecondsV1,
    ),
    String,
> {
    // The copy carries the update's one-step settings (the London keep
    // offset, the priority pass offset, the pregame announcement step
    // scale), which the step takes exactly as the update's own step would.
    let mut copy = state.clone();
    let mut profile = NativeTrainingPhaseProfileV1::default();
    let started = std::time::Instant::now();
    let stepped = {
        let mut recorder = NativeTrainingPhaseRecorderV1::enabled_v1(&mut profile);
        match mode {
            ModeV1::Sequential => copy.train_step_gae_feature_transfer_profiled_v1(
                generation,
                groups,
                value_targets,
                advantages,
                value_coefficient,
                learning_rate,
                None,
                threads,
                &mut recorder,
            ),
            ModeV1::FixedTree => copy.train_step_gae_feature_transfer_fixed_tree_dims_v1(
                generation,
                groups,
                value_targets,
                advantages,
                value_coefficient,
                learning_rate,
                threads,
                &mut recorder,
            ),
        }
    };
    let step = started.elapsed().as_secs_f64();
    let result = stepped.map_err(err)?;
    let phase = |phase: NativeTrainingPhaseV1| profile.phase_elapsed_ns_v1(phase) as f64 / 1.0e9;
    Ok((
        result,
        copy,
        StepSecondsV1 {
            step,
            setup_validation: phase(NativeTrainingPhaseV1::SetupValidation),
            forward_loss: phase(NativeTrainingPhaseV1::ForwardLoss),
            backward_gauge: phase(NativeTrainingPhaseV1::BackwardGauge),
            adam_math: phase(NativeTrainingPhaseV1::AdamMath),
            finalization: phase(NativeTrainingPhaseV1::FinalizationCloning),
        },
    ))
}

fn median_v1(values: impl Iterator<Item = f64>) -> f64 {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    match values.len() {
        0 => 0.0,
        len if len % 2 == 1 => values[len / 2],
        len => (values[len / 2 - 1] + values[len / 2]) / 2.0,
    }
}

/// Differences of two equally shaped tensors, element by element.
#[derive(Clone, Copy, Default)]
struct DifferenceV1 {
    elements: usize,
    differing_elements: usize,
    max_abs: f64,
    max_rel: f64,
    max_magnitude: f64,
}

impl DifferenceV1 {
    fn of_v1(left: &[f32], right: &[f32], floor: f64) -> Result<Self, String> {
        ensure(
            left.len() == right.len(),
            "compared tensors differ in length",
        )?;
        let mut difference = Self {
            elements: left.len(),
            ..Self::default()
        };
        for (left, right) in left.iter().zip(right) {
            difference.differing_elements += usize::from(left.to_bits() != right.to_bits());
            let (left, right) = (f64::from(*left), f64::from(*right));
            let magnitude = left.abs().max(right.abs());
            let abs = (left - right).abs();
            difference.max_abs = difference.max_abs.max(abs);
            difference.max_rel = difference.max_rel.max(abs / magnitude.max(floor));
            difference.max_magnitude = difference.max_magnitude.max(magnitude);
        }
        Ok(difference)
    }

    fn merge_v1(&mut self, other: &Self) {
        self.elements += other.elements;
        self.differing_elements += other.differing_elements;
        self.max_abs = self.max_abs.max(other.max_abs);
        self.max_rel = self.max_rel.max(other.max_rel);
        self.max_magnitude = self.max_magnitude.max(other.max_magnitude);
    }

    fn json_v1(&self) -> Value {
        json!({
            "elements": self.elements,
            "differing_elements": self.differing_elements,
            "max_abs": self.max_abs,
            "max_rel": self.max_rel,
            "max_magnitude": self.max_magnitude,
        })
    }
}

/// The differences of two lists of named tensors: the whole list and each
/// tensor that differs anywhere.
fn tensor_differences_v1<'a>(
    left: impl Iterator<Item = (&'static str, &'a [f32])>,
    right: impl Iterator<Item = (&'static str, &'a [f32])>,
    floor: f64,
) -> Result<Value, String> {
    let mut total = DifferenceV1::default();
    let mut differing = Vec::new();
    let (mut left, mut right) = (left, right);
    loop {
        match (left.next(), right.next()) {
            (None, None) => break,
            (Some((name, left)), Some((other, right))) => {
                ensure(name == other, "compared tensors differ in name")?;
                let difference = DifferenceV1::of_v1(left, right, floor)?;
                total.merge_v1(&difference);
                if difference.differing_elements > 0 {
                    let mut entry = difference.json_v1();
                    entry["name"] = json!(name);
                    differing.push(entry);
                }
            }
            _ => return Err("compared tensor lists differ in length".into()),
        }
    }
    let mut document = total.json_v1();
    document["differing_tensors"] = json!(differing);
    Ok(document)
}

fn scalar_difference_v1(left: f32, right: f32, floor: f64) -> Value {
    let (a, b) = (f64::from(left), f64::from(right));
    let abs = (a - b).abs();
    json!({
        "sequential": left,
        "fixed_tree": right,
        "same_bits": left.to_bits() == right.to_bits(),
        "abs": abs,
        "rel": abs / a.abs().max(b.abs()).max(floor),
    })
}

fn timing_json_v1(measured: &MeasuredV1, batch: &PreparedBatchV1) -> Value {
    let median = |pick: fn(&StepSecondsV1) -> f64| median_v1(measured.seconds.iter().map(pick));
    let step = median(|seconds| seconds.step);
    json!({
        "gradient_pass_seconds": median(|seconds| seconds.backward_gauge),
        "forward_and_loss_seconds": median(|seconds| seconds.forward_loss),
        "setup_validation_seconds": median(|seconds| seconds.setup_validation),
        "adam_seconds": median(|seconds| seconds.adam_math),
        "finalization_seconds": median(|seconds| seconds.finalization),
        "learner_step_seconds": step,
        "update_seconds": batch.input_read_seconds + batch.behavior_replay_seconds + step,
        "after_state_sha256": measured.after_state_sha256,
        "repeats_agree": measured.repeats_agree,
        "samples": {
            "gradient_pass_seconds": measured
                .seconds
                .iter()
                .map(|seconds| seconds.backward_gauge)
                .collect::<Vec<_>>(),
            "learner_step_seconds": measured
                .seconds
                .iter()
                .map(|seconds| seconds.step)
                .collect::<Vec<_>>(),
        },
    })
}

/// The measurement, at the point where the update would train: `state` is
/// the loaded state with the update's one-step settings installed, and
/// `groups` with their targets are exactly the step's inputs. See the module
/// documentation.
#[allow(clippy::too_many_arguments)]
pub(super) fn measure_v1(
    options: &FixedTreeDiagnosticOptionsV1,
    state: &NativePolicyValueTrainStateV1,
    generation: FreshLineageGenerationV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_targets: &[f32],
    advantages: &[f32],
    value_coefficient: f32,
    learning_rate: f32,
    batch: PreparedBatchV1,
) -> Result<Value, String> {
    options.validate_v1()?;
    let threads = options
        .threads
        .unwrap_or(batch.preparation_workers.unwrap_or(1));
    ensure(
        (1..=MAX_THREADS_V1).contains(&threads),
        "fixed-tree diagnostic threads must be in 1..=32",
    )?;
    let floor = options.relative_floor;
    // Interleaved, so drift in the machine's load reaches every measurement.
    let mut plan = vec![(ModeV1::Sequential, threads), (ModeV1::FixedTree, threads)];
    if threads > 1 {
        plan.push((ModeV1::Sequential, 1));
        plan.push((ModeV1::FixedTree, 1));
    }
    let mut measured: Vec<Option<MeasuredV1>> = plan.iter().map(|_| None).collect();
    for _ in 0..options.repeats {
        for ((mode, threads), slot) in plan.iter().zip(&mut measured) {
            let (result, after, seconds) = step_v1(
                *mode,
                *threads,
                state,
                generation,
                groups,
                value_targets,
                advantages,
                value_coefficient,
                learning_rate,
            )?;
            let sha256 = hex(&after.state_sha256_v1().map_err(err)?);
            match slot {
                None => {
                    *slot = Some(MeasuredV1 {
                        result,
                        state: after,
                        after_state_sha256: sha256,
                        repeats_agree: true,
                        seconds: vec![seconds],
                    });
                }
                Some(first) => {
                    first.repeats_agree &= first.after_state_sha256 == sha256;
                    first.seconds.push(seconds);
                }
            }
        }
    }
    let mut measured = measured
        .into_iter()
        .map(|slot| slot.ok_or_else(|| "the fixed-tree diagnostic measured nothing".to_string()));
    let sequential = measured.next().transpose()?.ok_or("no sequential step")?;
    let fixed_tree = measured.next().transpose()?.ok_or("no fixed-tree step")?;
    let sequential_single = measured.next().transpose()?;
    let fixed_tree_single = measured.next().transpose()?;

    let rows = sequential.result.selected_outputs.len();
    let chunk_rows = crate::native_policy_train_step_v1::FIXED_TREE_CHUNK_ROWS_V1;
    let chunks = crate::native_policy_train_step_v1::fixed_tree_chunk_count_v1(rows);
    let gradient = tensor_differences_v1(
        sequential
            .result
            .gradients
            .iter()
            .map(|tensor| (tensor.name, tensor.values.as_slice())),
        fixed_tree
            .result
            .gradients
            .iter()
            .map(|tensor| (tensor.name, tensor.values.as_slice())),
        floor,
    )?;
    let sequential_after = sequential.state.tensors_v1().map_err(err)?;
    let fixed_tree_after = fixed_tree.state.tensors_v1().map_err(err)?;
    let section =
        |left: &[crate::native_policy_value_net_v1::NativeNamedTensorRefV1<'_>],
         right: &[crate::native_policy_value_net_v1::NativeNamedTensorRefV1<'_>]| {
            tensor_differences_v1(
                left.iter().map(|tensor| (tensor.name, tensor.values)),
                right.iter().map(|tensor| (tensor.name, tensor.values)),
                floor,
            )
        };
    let gauge = |measured: &MeasuredV1| {
        let gauge = &measured.result.scorer_bias_gauge;
        json!({
            "raw_gradient_residual": gauge.raw_gradient_residual,
            "high_precision_residual": gauge.high_precision_residual,
            "derived_absolute_bound": gauge.derived_absolute_bound,
        })
    };
    let ratio = |numerator: &MeasuredV1, denominator: &MeasuredV1| {
        let pass = |measured: &MeasuredV1| {
            median_v1(
                measured
                    .seconds
                    .iter()
                    .map(|seconds| seconds.backward_gauge),
            )
        };
        let (numerator, denominator) = (pass(numerator), pass(denominator));
        (denominator > 0.0).then(|| numerator / denominator)
    };

    let mut sequential_timing = json!({ "threads": threads });
    sequential_timing["at_threads"] = timing_json_v1(&sequential, &batch);
    let mut fixed_tree_timing = json!({ "threads": threads });
    fixed_tree_timing["at_threads"] = timing_json_v1(&fixed_tree, &batch);
    // On one thread the threaded measurement is the single-thread one.
    let sequential_single_ref = sequential_single.as_ref().unwrap_or(&sequential);
    let fixed_tree_single_ref = fixed_tree_single.as_ref().unwrap_or(&fixed_tree);
    sequential_timing["at_one_thread"] = timing_json_v1(sequential_single_ref, &batch);
    fixed_tree_timing["at_one_thread"] = timing_json_v1(fixed_tree_single_ref, &batch);

    Ok(json!({
        "schema": "mtg-kernel-fixed-tree-diagnostic/v1",
        "claim": "engineering measurement of one update's learner step; nothing was published \
                  and no playing-strength claim is made",
        "threads": threads,
        "repeats": options.repeats,
        "command_preparation_workers": batch.preparation_workers,
        "before_state_sha256": batch.before_state_sha256,
        "batch": {
            "episodes": batch.episodes,
            "physical_decisions": groups.len(),
            "rows": rows,
            "chunk_rows": chunk_rows,
            "chunks": chunks,
        },
        "input_read_seconds": batch.input_read_seconds,
        "behavior_replay_seconds": batch.behavior_replay_seconds,
        "update_seconds_definition": "input_read_seconds + behavior_replay_seconds (this \
            invocation's, the same work both ways) + the median learner_step_seconds; checkpoint \
            publication and readback are not performed",
        "sequential": sequential_timing,
        "fixed_tree": fixed_tree_timing,
        "gradient_pass_ratio_fixed_tree_over_sequential": {
            "at_threads": ratio(&fixed_tree, &sequential),
            "at_one_thread": ratio(fixed_tree_single_ref, sequential_single_ref),
        },
        "invariance": {
            "sequential_threads_agree":
                sequential.after_state_sha256 == sequential_single_ref.after_state_sha256,
            "fixed_tree_threads_agree":
                fixed_tree.after_state_sha256 == fixed_tree_single_ref.after_state_sha256,
            "repeats_agree": sequential.repeats_agree
                && fixed_tree.repeats_agree
                && sequential_single_ref.repeats_agree
                && fixed_tree_single_ref.repeats_agree,
        },
        "difference": {
            "between": "the sequential order and the fixed tree, both at `threads` threads",
            "relative_definition": "|a - b| / max(|a|, |b|, relative_floor)",
            "relative_floor": floor,
            "gradient": gradient,
            "scorer_bias_gauge": {
                "sequential": gauge(&sequential),
                "fixed_tree": gauge(&fixed_tree),
            },
            "loss": scalar_difference_v1(sequential.result.loss, fixed_tree.result.loss, floor),
            "policy_sum": scalar_difference_v1(
                sequential.result.policy_sum,
                fixed_tree.result.policy_sum,
                floor,
            ),
            "value_sum": scalar_difference_v1(
                sequential.result.value_sum,
                fixed_tree.result.value_sum,
                floor,
            ),
            "parameters_after_step": section(
                &sequential_after.parameters,
                &fixed_tree_after.parameters,
            )?,
            "first_moments_after_step": section(
                &sequential_after.first_moments,
                &fixed_tree_after.first_moments,
            )?,
            "second_moments_after_step": section(
                &sequential_after.second_moments,
                &fixed_tree_after.second_moments,
            )?,
        },
    }))
}

#[cfg(test)]
mod tests;
