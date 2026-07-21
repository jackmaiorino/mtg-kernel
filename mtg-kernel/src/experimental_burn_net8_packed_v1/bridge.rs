//! Production trainer bridge for the CudaBurnDense backend.
//!
//! One update: snapshot the CPU train state, import it to the device, pack
//! the update's encoded decision views, run the dense group loss step, read
//! back logits/values, recompute every evidence field host-side in the exact
//! CPU f32 fold order over the CUDA outputs, tolerance-check the transported
//! scorer bits (the CUDA identity's semantic difference from the CPU
//! backends' bit-exact revalidation), build the gauge record through the
//! production accumulator, export the device state, and replace the CPU
//! state. Transactional semantics stay CPU-owned.

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
    let plan: DenseGroupLossPlanV1 = build_dense_group_loss_plan_v1(
        &workspace,
        &selected_action_indices,
        &substep_group_indices,
        &group_first_substeps,
        &terminal_returns,
        &device,
    )
    .map_err(bridge_error_v1)?;
    let batch = DevicePackedBatch::upload(&device, &workspace);
    let (logit_outputs, value_outputs) = device_state
        .forward_outputs_v1(&batch)
        .map_err(bridge_error_v1)?;
    let raw_residual = device_state
        .train_one_step_bridge_v1(&batch, &plan, value_coefficient, learning_rate)
        .map_err(bridge_error_v1)?;

    // Host recomputation of every evidence field from the CUDA outputs, in
    // the exact CPU f32 fold order, plus the transported-bits tolerance gate.
    let mut selected_outputs = Vec::with_capacity(selected_action_indices.len());
    let mut physical_terms = Vec::with_capacity(groups.len());
    let mut gauge_accumulator = ScorerBiasGaugeAccumulatorV1::default();
    let mut policy_sum = 0.0_f32;
    let mut value_sum = 0.0_f32;
    let group_count = groups.len() as f32;
    let mut flat_substep = 0_usize;
    for (group_index, group) in groups.iter().enumerate() {
        let mut joint_log_probability: Option<f32> = None;
        let group_first_value = value_outputs
            .get(group_first_substeps[group_index])
            .copied()
            .ok_or(NativePolicyTrainErrorV1::CudaBackend {
                code: "cuda-burn-dense-bridge-value-cardinality",
            })?;
        let target = f32::from(group.terminal_return);
        let advantage = target - group_first_value;
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
            let (selected_log_probability, _log_probabilities) =
                selected_log_softmax(row, substep.selected_action_index)?;
            joint_log_probability = Some(match joint_log_probability {
                None => selected_log_probability,
                Some(active) => active + selected_log_probability,
            });
            selected_outputs.push(NativeSelectedOutputV1 {
                group_index,
                substep_index,
                selected_action_index: substep.selected_action_index,
                selected_logit: row[substep.selected_action_index],
                value: substep_value,
                selected_log_probability,
            });
            gauge_accumulator.observe(
                row,
                substep.selected_action_index,
                -advantage / group_count,
            )?;
            flat_substep += 1;
        }
        let joint_log_probability = joint_log_probability.expect("nonempty group checked above");
        let substep_count = u32::try_from(group.substeps.len()).map_err(|_| {
            NativePolicyTrainErrorV1::PhysicalSubstepCountOverflow {
                group_index,
                substep_count: group.substeps.len(),
            }
        })?;
        let policy_term = -joint_log_probability * advantage;
        let value_error = group_first_value - target;
        let value_term = value_error * value_error;
        policy_sum += policy_term;
        value_sum += value_term;
        physical_terms.push(NativePhysicalLossTermV1 {
            joint_log_probability,
            value: group_first_value,
            terminal_return: group.terminal_return,
            substep_count,
        });
    }
    let loss = (policy_sum + value_coefficient * value_sum) / group_count;
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
