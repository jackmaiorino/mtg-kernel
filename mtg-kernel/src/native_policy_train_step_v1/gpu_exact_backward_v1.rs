//! The ordinary GAE backward's gradients from an exact GPU learner
//! (`gpu_exact_v1::net8_learner`), when this process installed one.
//!
//! Everything that decides a bit outside the network stays here, on the
//! CPU, exactly as `exact_parallel_backward_v1` does it: the visiting order
//! (groups and their substeps in reverse), each visit's joint log-probability
//! and value coefficients, the scorer-bias gauge observed in that order, and
//! each substep's logits gradient (`prepare_reverse_logits_v1`, which uses
//! the platform `exp`). The device runs the forward (its outputs must equal
//! the CPU tape's bits), the per-substep reverse pass and every gradient
//! element's ordered sum. The gradients are therefore the CPU backward's, and
//! the caller's gauge, Adam and finalization are unchanged.

use super::*;
use crate::gpu_exact_v1::net8_learner::LearnerSubstepV1;

type BackwardResultV1 =
    Result<(Vec<Vec<f32>>, ScorerBiasGaugeAccumulatorV1), NativePolicyTrainErrorV1>;

/// The gradients and gauge evidence, or None when no exact GPU learner is
/// installed or this update is outside what it runs (another width, the V8
/// pairing net, packed inputs or the portable math); the caller then runs
/// its CPU backward, with the same bits.
#[allow(clippy::too_many_arguments)]
pub(super) fn gpu_exact_backward_dims_v1<D: NetDimsV1>(
    parameters: &[NativeNamedParameterV1],
    group_tapes: &[GroupTapeV1<'_>],
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    group_count: f32,
    value_coefficient: f32,
    input_config: NativePolicyValueModelConfigV1,
    math: TrainingMathV1,
) -> Option<BackwardResultV1> {
    if !is_net8_dims_v1::<D>()
        || parameters.len() != PARAMETER_TENSOR_COUNT
        || !math.is_platform_libm()
        || !crate::vector_tanh_v1::default_tanhf_environment_v1()
        || !crate::gpu_exact_v1::learner_installed_v1()
    {
        return None;
    }
    let mut views = Vec::with_capacity(group_tapes.iter().map(|g| g.tapes.len()).sum());
    for (group, tapes) in groups.iter().zip(group_tapes) {
        if group.substeps.len() != tapes.tapes.len() {
            return None;
        }
        let mut row = Vec::with_capacity(group.substeps.len());
        for substep in group.substeps {
            let NativePolicyForwardInputV1::Encoded(view) = &substep.forward else {
                return None;
            };
            row.push(**view);
        }
        views.push(row);
    }
    Some(backward_v1(parameters, group_tapes, &views, group_count, value_coefficient, input_config, math))
}

fn backward_v1(
    parameters: &[NativeNamedParameterV1],
    group_tapes: &[GroupTapeV1<'_>],
    views: &[Vec<crate::native_policy_value_net_v1::NativeEncodedDecisionViewV1<'_>>],
    group_count: f32,
    value_coefficient: f32,
    input_config: NativePolicyValueModelConfigV1,
    math: TrainingMathV1,
) -> BackwardResultV1 {
    // The sequential arm's coefficients, visiting order and gauge, as
    // `exact_parallel_backward_v1::backward_dims_v1` computes them.
    let mut gauge = ScorerBiasGaugeAccumulatorV1::for_math_v1(math);
    let mut workspace = ReverseWorkspaceV1::default();
    let mut d_logits: Vec<Vec<f32>> = Vec::new();
    let mut visits = Vec::new();
    for (group_index, group) in group_tapes.iter().enumerate().rev() {
        let d_joint_log_probability = -group.advantage / group_count;
        let d_value = (value_coefficient / group_count) * (2.0 * group.value_error);
        for (substep_index, selected) in group.tapes.iter().enumerate().rev() {
            // A gauge failure stops the sequential arm before this
            // substep's backward and is the update's error.
            gauge.observe(
                selected.tape.logits_v1(),
                selected.selected_action_index,
                d_joint_log_probability,
            )?;
            prepare_reverse_logits_v1(selected, d_joint_log_probability, &mut workspace, math);
            d_logits.push(workspace.d_logits.clone());
            visits.push((
                group_index,
                substep_index,
                if substep_index == 0 { d_value } else { 0.0 },
            ));
        }
    }
    let substeps: Vec<LearnerSubstepV1<'_>> = visits
        .iter()
        .zip(&d_logits)
        .map(|(&(group, substep, d_value), d_logits)| {
            let tape = &group_tapes[group].tapes[substep].tape;
            LearnerSubstepV1 {
                view: views[group][substep],
                logits: tape.logits_v1(),
                value: tape.value_v1(),
                d_logits,
                d_value,
            }
        })
        .collect();
    let tensors: Vec<&[f32]> = parameters.iter().map(|p| p.values.as_slice()).collect();
    let gradients = crate::gpu_exact_v1::learner_gradients_v1(input_config, &tensors, &substeps)
        .map_err(|message| NativePolicyTrainErrorV1::GpuExactBackend { message })?;
    Ok((gradients, gauge))
}
