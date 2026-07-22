//! Production trainer bridge for the CudaBurnDense backend.
//!
//! One update: snapshot the CPU train state, import it to the device, pack
//! the update's encoded decision views, run the dense group loss step, read
//! back logits/values, tolerance-check the CUDA outputs against the
//! transported scorer bits (the CUDA identity's semantic difference from the
//! CPU backends' bit-exact revalidation), then build every evidence field
//! from the transported bits in the exact CPU f32 fold order so the trainer's
//! bit-exact evidence revalidation holds unchanged. The gauge record observes
//! the transported rows and coefficients in the CPU backward traversal order,
//! with the device's raw scorer-bias gradient as the residual witness. The
//! device state is exported and replaces the CPU state; transactional
//! semantics stay CPU-owned.

use super::training::{
    build_dense_group_loss_plan_v1, DenseGroupLossPlanV1, ExperimentalDeviceTrainStateV1,
};
use super::{DevicePackedBatch, HostPackingWorkspace};
use crate::native_policy_train_step_v1::{
    selected_log_softmax, NativePhysicalLossTermV1, NativePolicyForwardInputV1,
    NativePolicyPhysicalDecisionV1, NativePolicyTrainErrorV1, NativePolicyTrainStepResultV1,
    NativePolicyValueTrainStateV1, NativeSelectedOutputV1, ScorerBiasGaugeAccumulatorV1,
};
use std::error::Error;

const TRANSPORTED_OUTPUT_ABSOLUTE_TOLERANCE_V1: f32 = 5.0e-3;
const TRANSPORTED_OUTPUT_RELATIVE_TOLERANCE_V1: f32 = 5.0e-3;
const SCORER_SECOND_BIAS_ORDINAL_V1: usize = 28;
/// Group-aligned training chunk size in substeps. Bounds peak backward
/// activation memory; the value keeps a chunk comfortably inside the device
/// while staying large enough that dense-kernel launch overhead is amortized.
const BRIDGE_CHUNK_SUBSTEP_TARGET_V1: usize = 8_192;

fn bridge_error_v1(_error: Box<dyn Error>) -> NativePolicyTrainErrorV1 {
    NativePolicyTrainErrorV1::CudaBackend {
        code: "cuda-burn-dense-bridge-device-failure",
    }
}

fn tolerance_ok_v1(actual: f32, expected: f32) -> bool {
    let difference = (actual - expected).abs();
    difference <= TRANSPORTED_OUTPUT_ABSOLUTE_TOLERANCE_V1
        || difference
            <= TRANSPORTED_OUTPUT_RELATIVE_TOLERANCE_V1 * expected.abs().max(f32::MIN_POSITIVE)
}

/// Run one production training update on the CudaBurnDense backend.
pub(crate) fn train_step_cuda_burn_dense_v1(
    state: &mut NativePolicyValueTrainStateV1,
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_coefficient: f32,
    learning_rate: f32,
) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
    if groups.is_empty() {
        return Err(NativePolicyTrainErrorV1::EmptyBatch);
    }
    // Flatten every substep in order, retaining group structure.
    let mut views = Vec::new();
    let mut selected_action_indices = Vec::new();
    let mut substep_group_indices = Vec::new();
    let mut group_first_substeps = Vec::with_capacity(groups.len());
    let mut terminal_returns = Vec::with_capacity(groups.len());
    for (group_index, group) in groups.iter().enumerate() {
        if group.substeps.is_empty() {
            return Err(NativePolicyTrainErrorV1::EmptyPhysicalDecision { group_index });
        }
        if !matches!(group.terminal_return, -1..=1) {
            return Err(NativePolicyTrainErrorV1::InvalidTerminalReturn {
                group_index,
                value: group.terminal_return,
            });
        }
        group_first_substeps.push(views.len());
        terminal_returns.push(group.terminal_return);
        for substep in group.substeps.iter() {
            let encoded = match &substep.forward {
                NativePolicyForwardInputV1::Encoded(encoded) => **encoded,
                NativePolicyForwardInputV1::Packed { encoded, .. } => **encoded,
            };
            views.push(encoded);
            selected_action_indices.push(substep.selected_action_index);
            substep_group_indices.push(group_index);
        }
    }

    // Snapshot, import to device, pack, plan, step.
    let snapshot = state
        .snapshot_v1()
        .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
            code: "cuda-burn-dense-bridge-snapshot-failure",
        })?;
    let parameter_before_bits =
        snapshot.parameters[SCORER_SECOND_BIAS_ORDINAL_V1].values[0].to_bits();
    let device = burn_cuda::CudaDevice::new(0);
    let mut device_state = ExperimentalDeviceTrainStateV1::import_snapshot_v1(&snapshot, &device)
        .map_err(bridge_error_v1)?;
    let mut workspace = HostPackingWorkspace::default();
    workspace.pack_views(&views).map_err(bridge_error_v1)?;
    let batch = DevicePackedBatch::upload(&device, &workspace);
    let (logit_outputs, value_outputs) = device_state
        .forward_outputs_v1(&batch)
        .map_err(bridge_error_v1)?;
    drop(batch);

    // The training step runs in group-aligned chunks with device-side
    // gradient accumulation and a single Adam application: peak activation
    // memory stays bounded by one chunk regardless of the update's substep
    // count, and each chunk's loss divides by the whole update's group count
    // so the accumulated gradient is the full-batch gradient.
    let mut chunk_group_starts = vec![0_usize];
    for (group_index, group_first) in group_first_substeps.iter().enumerate().skip(1) {
        let chunk_start_group = *chunk_group_starts.last().expect("nonempty starts");
        let chunk_first = group_first_substeps[chunk_start_group];
        if group_first - chunk_first >= BRIDGE_CHUNK_SUBSTEP_TARGET_V1 {
            chunk_group_starts.push(group_index);
        }
    }
    let mut accumulator = burn::optim::GradientsAccumulator::new();
    let mut raw_residual = 0.0_f32;
    let total_group_count = groups.len() as f32;
    for (ordinal, chunk_start_group) in chunk_group_starts.iter().copied().enumerate() {
        let chunk_end_group = chunk_group_starts
            .get(ordinal + 1)
            .copied()
            .unwrap_or(groups.len());
        let substep_begin = group_first_substeps[chunk_start_group];
        let substep_end = group_first_substeps
            .get(chunk_end_group)
            .copied()
            .unwrap_or(views.len());
        let chunk_group_first_substeps = group_first_substeps[chunk_start_group..chunk_end_group]
            .iter()
            .map(|first| first - substep_begin)
            .collect::<Vec<_>>();
        let chunk_substep_group_indices = substep_group_indices[substep_begin..substep_end]
            .iter()
            .map(|group| group - chunk_start_group)
            .collect::<Vec<_>>();
        let mut chunk_workspace = HostPackingWorkspace::default();
        chunk_workspace
            .pack_views(&views[substep_begin..substep_end])
            .map_err(bridge_error_v1)?;
        let chunk_plan: DenseGroupLossPlanV1 = build_dense_group_loss_plan_v1(
            &chunk_workspace,
            &selected_action_indices[substep_begin..substep_end],
            &chunk_substep_group_indices,
            &chunk_group_first_substeps,
            &terminal_returns[chunk_start_group..chunk_end_group],
            &device,
        )
        .map_err(bridge_error_v1)?;
        let chunk_batch = DevicePackedBatch::upload(&device, &chunk_workspace);
        raw_residual += device_state
            .chunk_backward_v1(
                &mut accumulator,
                &chunk_batch,
                &chunk_plan,
                value_coefficient,
                total_group_count,
            )
            .map_err(bridge_error_v1)?;
    }
    device_state
        .apply_accumulated_v1(accumulator, learning_rate)
        .map_err(bridge_error_v1)?;

    // Tolerance-gate the CUDA outputs against the transported scorer bits,
    // then build every evidence field from the transported bits in the exact
    // CPU f32 fold order: the trainer revalidates evidence bit-exactly against
    // the rollout transport, so evidence stays CPU-canonical while the device
    // update itself uses the CUDA outputs.
    let mut selected_outputs = Vec::with_capacity(selected_action_indices.len());
    let mut physical_terms = Vec::with_capacity(groups.len());
    let mut transported_advantages = Vec::with_capacity(groups.len());
    let mut policy_sum = 0.0_f32;
    let mut value_sum = 0.0_f32;
    let group_count = groups.len() as f32;
    let mut flat_substep = 0_usize;
    for (group_index, group) in groups.iter().enumerate() {
        let mut joint_log_probability: Option<f32> = None;
        value_outputs.get(group_first_substeps[group_index]).ok_or(
            NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-value-cardinality",
            },
        )?;
        let transported_first_value = f32::from_bits(group.substeps[0].expected_value_bits);
        let target = f32::from(group.terminal_return);
        let advantage = target - transported_first_value;
        for (substep_index, substep) in group.substeps.iter().enumerate() {
            let begin = workspace.action_offsets[flat_substep];
            let end = workspace.action_offsets[flat_substep + 1];
            let row = &logit_outputs[begin..end];
            if substep.selected_action_index >= row.len() {
                return Err(NativePolicyTrainErrorV1::SelectedActionOutOfRange {
                    group_index,
                    substep_index,
                    selected: substep.selected_action_index,
                    action_count: row.len(),
                });
            }
            if substep.expected_raw_action_logit_bits.len() != row.len() {
                return Err(NativePolicyTrainErrorV1::ExpectedLogitCountMismatch {
                    group_index,
                    substep_index,
                    expected: substep.expected_raw_action_logit_bits.len(),
                    actual: row.len(),
                });
            }
            for (actual, expected_bits) in row.iter().zip(substep.expected_raw_action_logit_bits) {
                if !tolerance_ok_v1(*actual, f32::from_bits(*expected_bits)) {
                    return Err(NativePolicyTrainErrorV1::CudaBackend {
                        code: "cuda-burn-dense-bridge-transported-logit-tolerance",
                    });
                }
            }
            let substep_value = value_outputs[flat_substep];
            if !tolerance_ok_v1(substep_value, f32::from_bits(substep.expected_value_bits)) {
                return Err(NativePolicyTrainErrorV1::CudaBackend {
                    code: "cuda-burn-dense-bridge-transported-value-tolerance",
                });
            }
            let transported_row = substep
                .expected_raw_action_logit_bits
                .iter()
                .map(|bits| f32::from_bits(*bits))
                .collect::<Vec<f32>>();
            let (selected_log_probability, _log_probabilities) =
                selected_log_softmax(&transported_row, substep.selected_action_index)?;
            joint_log_probability = Some(match joint_log_probability {
                None => selected_log_probability,
                Some(active) => active + selected_log_probability,
            });
            selected_outputs.push(NativeSelectedOutputV1 {
                group_index,
                substep_index,
                selected_action_index: substep.selected_action_index,
                selected_logit: transported_row[substep.selected_action_index],
                value: f32::from_bits(substep.expected_value_bits),
                selected_log_probability,
            });
            flat_substep += 1;
        }
        transported_advantages.push(advantage);
        let joint_log_probability = joint_log_probability.expect("nonempty group checked above");
        let substep_count = u32::try_from(group.substeps.len()).map_err(|_| {
            NativePolicyTrainErrorV1::PhysicalSubstepCountOverflow {
                group_index,
                substep_count: group.substeps.len(),
            }
        })?;
        let policy_term = -joint_log_probability * advantage;
        let value_error = transported_first_value - target;
        let value_term = value_error * value_error;
        policy_sum += policy_term;
        value_sum += value_term;
        physical_terms.push(NativePhysicalLossTermV1 {
            joint_log_probability,
            value: transported_first_value,
            terminal_return: group.terminal_return,
            substep_count,
        });
    }
    let loss = (policy_sum + value_coefficient * value_sum) / group_count;

    // The gauge accumulator observes substeps in the CPU backward traversal
    // order (groups reversed, substeps reversed within each group) with the
    // transported rows and coefficients: the store lattice validation binds
    // the recorded bounds to that order and rederives every coefficient
    // bit-exactly from the evidence terms. The device's raw scorer-bias
    // gradient stays the residual witness of the softmax gauge identity.
    let mut gauge_accumulator = ScorerBiasGaugeAccumulatorV1::default();
    for group_index in (0..groups.len()).rev() {
        let group = &groups[group_index];
        let coefficient = -transported_advantages[group_index] / group_count;
        for substep_index in (0..group.substeps.len()).rev() {
            let substep = &group.substeps[substep_index];
            let transported_row = substep
                .expected_raw_action_logit_bits
                .iter()
                .map(|bits| f32::from_bits(*bits))
                .collect::<Vec<f32>>();
            gauge_accumulator.observe(
                &transported_row,
                substep.selected_action_index,
                coefficient,
            )?;
        }
    }
    let scorer_bias_gauge = gauge_accumulator.finish(raw_residual, parameter_before_bits)?;

    // Export the device state and replace the CPU state through the
    // validating snapshot constructor.
    let updated_snapshot = device_state.export_snapshot_v1().map_err(bridge_error_v1)?;
    let adam_step = updated_snapshot.adam_step;
    *state = NativePolicyValueTrainStateV1::from_snapshot_v1(
        state.model_v1().clone(),
        &updated_snapshot,
    )
    .map_err(|_| NativePolicyTrainErrorV1::CudaBackend {
        code: "cuda-burn-dense-bridge-state-reimport-failure",
    })?;

    Ok(NativePolicyTrainStepResultV1 {
        policy_sum,
        value_sum,
        loss,
        adam_step,
        selected_outputs,
        physical_terms,
        gradients: Vec::new(),
        scorer_bias_gauge,
    })
}
