//! The pregame announcement step scale
//! (`loss_selection.pregame_announcement_step_scale`, plain CPU GAE update
//! with the terminal pregame advantage only).
//!
//! A scale applied to the announcement rows' advantage before the optimizer
//! does not scale their step: on weights only those rows reach, Adam's second
//! moment is made of the scaled gradient itself, so `m / sqrt(v)` does not
//! depend on the scale. This mechanism scales after the normalisation
//! instead. The backward keeps two gradient buffers:
//!
//! * `a`: the policy term of the announcement rows (a pregame decision with a
//!   mulligan row, at every own mulligan count; put-back decisions are not
//!   announcement rows),
//! * `r`: everything else, the announcement rows' value term included.
//!
//! and Adam takes, per weight,
//!
//! ```text
//! m = beta1 m + (1 - beta1) (r + s a)
//! v = beta2 v + (1 - beta2) (r + a)^2
//! step = -lr m_hat / (sqrt(v_hat) + eps)
//! ```
//!
//! so the second moment is the unscaled update's and the announcement rows'
//! part of every weight's step is `s` times what the unscaled update gives at
//! the same state and batch. Nothing before the optimizer changes (advantages,
//! their normalisation, value targets, the loss scalars), the optimizer state
//! keeps its shape and meaning, and a step without the scale runs none of
//! this code.
//!
//! Both buffers come from the real backward: each pass visits groups in
//! reverse and their substeps in reverse, exactly as the ordinary pass, and
//! differs only in the two loss-root coefficients of a group
//! ([`BackwardPartV1`]). The exact parallel backward takes the same part
//! selector and keeps its bit-for-bit equality with the sequential loop for
//! each part. The fixed-partition backward and the exact GPU learner do not
//! route the second buffer and refuse the scale.

use super::*;

/// Which terms of the ordinary GAE loss one backward pass differentiates.
/// `marks[g]` is whether group `g` is an announcement row.
#[derive(Clone, Copy)]
pub(super) enum BackwardPartV1<'a> {
    /// Every term of every group: every update without the scale.
    All,
    /// Every term but the policy term of the marked groups (`r`).
    Rest(&'a [bool]),
    /// Only the policy term of the marked groups (`a`).
    Announcement(&'a [bool]),
}

impl BackwardPartV1<'_> {
    /// Whether this pass visits the group's substeps at all.
    pub(super) fn visits_v1(self, group_index: usize) -> bool {
        match self {
            Self::Announcement(marks) => marks[group_index],
            Self::All | Self::Rest(_) => true,
        }
    }

    /// The gradient this pass gives the group's joint log-probability at
    /// `substep_index`: the ordinary one, or zero where the policy term
    /// belongs to the other buffer.
    pub(super) fn d_joint_log_probability_v1(
        self,
        group_index: usize,
        group: &GroupTapeV1<'_>,
        substep_index: usize,
        group_count: f32,
    ) -> f32 {
        match self {
            Self::Rest(marks) if marks[group_index] => 0.0,
            Self::All | Self::Rest(_) | Self::Announcement(_) => {
                group.d_joint_log_probability_v1(substep_index, group_count)
            }
        }
    }

    /// The gradient this pass gives the group's value: the ordinary one, or
    /// zero in the announcement pass (the value term stays in `r`).
    pub(super) fn d_value_v1(
        self,
        group: &GroupTapeV1<'_>,
        value_coefficient: f32,
        group_count: f32,
    ) -> f32 {
        match self {
            Self::Announcement(_) => 0.0,
            Self::All | Self::Rest(_) => {
                (value_coefficient / group_count) * (2.0 * group.value_error)
            }
        }
    }
}

/// What one scaled step did, for the update receipt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PregameAnnouncementStepRecordV1 {
    pub(crate) scale: f32,
    /// Announcement rows whose policy term went to the second buffer.
    pub(crate) routed_groups: usize,
    /// L2 norms of the two buffers as Adam saw them (after the gauge
    /// projection of the scorer bias).
    pub(crate) announcement_gradient_l2: f64,
    pub(crate) rest_gradient_l2: f64,
}

pub(super) fn gradient_l2_v1(gradients: &[Vec<f32>]) -> f64 {
    gradients
        .iter()
        .flat_map(|tensor| tensor.iter())
        .map(|value| f64::from(*value) * f64::from(*value))
        .sum::<f64>()
        .sqrt()
}

/// The announcement buffer `a` and its gauge evidence: the ordinary backward
/// restricted to the policy term of the marked groups, on the exact parallel
/// backward when enough substeps share it and on the sequential loop
/// otherwise (the same bits either way). `groups` are in forward order and
/// are only read.
#[allow(clippy::too_many_arguments)]
pub(super) fn announcement_backward_dims_v1<D: NetDimsV1>(
    parameters: &[NativeNamedParameterV1],
    groups: &[GroupTapeV1<'_>],
    marks: &[bool],
    group_count: f32,
    value_coefficient: f32,
    workers: usize,
    math: TrainingMathV1,
    mxcsr: ThreadMxcsrCheckV1,
) -> Result<(Vec<Vec<f32>>, ScorerBiasGaugeAccumulatorV1), NativePolicyTrainErrorV1> {
    let part = BackwardPartV1::Announcement(marks);
    if let Some(result) = exact_parallel_backward_v1::exact_parallel_backward_dims_v1::<D>(
        parameters,
        groups,
        part,
        group_count,
        value_coefficient,
        workers,
        math,
        mxcsr,
    ) {
        return result;
    }
    let mut gradients = parameters
        .iter()
        .map(|parameter| vec![0.0; parameter.values.len()])
        .collect::<Vec<_>>();
    let mut gauge_accumulator = ScorerBiasGaugeAccumulatorV1::for_math_v1(math);
    let mut reverse_workspace = ReverseWorkspaceV1::default();
    // The ordinary sequential arm's reverse group and substep order.
    for (group_index, group) in groups.iter().enumerate().rev() {
        if !part.visits_v1(group_index) {
            continue;
        }
        let d_value = part.d_value_v1(group, value_coefficient, group_count);
        for (substep_index, selected) in group.tapes.iter().enumerate().rev() {
            let d_joint_log_probability =
                part.d_joint_log_probability_v1(group_index, group, substep_index, group_count);
            prepare_reverse_logits_v1(
                selected,
                d_joint_log_probability,
                &mut reverse_workspace,
                math,
            );
            gauge_accumulator.observe(
                selected.tape.logits_v1(),
                selected.selected_action_index,
                d_joint_log_probability,
            )?;
            reverse_decision_dims_v1::<D>(
                parameters,
                &mut gradients,
                &selected.tape,
                &reverse_workspace.d_logits,
                if substep_index == 0 { d_value } else { 0.0 },
                &mut reverse_workspace.decision,
            )?;
        }
    }
    Ok((gradients, gauge_accumulator))
}

/// Adam with the announcement buffer scaled after the normalisation: the
/// first moment takes `rest + scale * announcement`, the second moment takes
/// `(rest + announcement)^2`. Everything else is `adam_update_impl`'s
/// arithmetic, in its order.
#[allow(clippy::too_many_arguments)]
pub(super) fn adam_update_announcement_v1(
    parameters: &[NativeNamedParameterV1],
    rest: &[Vec<f32>],
    announcement: &[Vec<f32>],
    scale: f32,
    first_moments: &[Vec<f32>],
    second_moments: &[Vec<f32>],
    step: u64,
    learning_rate: f32,
    math: TrainingMathV1,
) -> Result<AdamUpdateV1, NativePolicyTrainErrorV1> {
    let exponent = i32::try_from(step).map_err(|_| NativePolicyTrainErrorV1::AdamStepOverflow)?;
    let bias_correction1 = 1.0f64 - math.powi_f64_v1(f64::from(ADAM_BETA1_V1), exponent);
    let bias_correction2 = 1.0f64 - math.powi_f64_v1(f64::from(ADAM_BETA2_V1), exponent);
    let step_size = (f64::from(learning_rate) / bias_correction1) as f32;
    let bias_correction2_sqrt = bias_correction2.sqrt() as f32;
    finite_scalar("adam_scalar", 0, step_size)?;
    finite_scalar("adam_scalar", 1, bias_correction2_sqrt)?;

    let mut next_parameters = parameters.to_vec();
    let mut next_first = first_moments.to_vec();
    let mut next_second = second_moments.to_vec();
    for parameter_index in 0..parameters.len() {
        for value_index in 0..parameters[parameter_index].values.len() {
            let rest_gradient = rest[parameter_index][value_index];
            let announcement_gradient = announcement[parameter_index][value_index];
            let previous_first = first_moments[parameter_index][value_index];
            let previous_second = second_moments[parameter_index][value_index];
            // As in `adam_update_impl`: the exact all-(+0) case keeps the
            // cloned bits; every other state takes the dense arithmetic.
            if rest_gradient.to_bits() == 0
                && announcement_gradient.to_bits() == 0
                && previous_first.to_bits() == 0
                && previous_second.to_bits() == 0
            {
                continue;
            }
            let first_gradient = rest_gradient + scale * announcement_gradient;
            let second_gradient = rest_gradient + announcement_gradient;
            let first = previous_first + (first_gradient - previous_first) * (1.0 - ADAM_BETA1_V1);
            let second = previous_second * ADAM_BETA2_V1
                + second_gradient * second_gradient * (1.0 - ADAM_BETA2_V1);
            let denominator = second.sqrt() / bias_correction2_sqrt + ADAM_EPSILON_V1;
            let parameter = parameters[parameter_index].values[value_index]
                + (-step_size) * first / denominator;
            if !first.is_finite() || !second.is_finite() || !parameter.is_finite() || second < 0.0 {
                return Err(NativePolicyTrainErrorV1::NonFinite {
                    stage: "adam_update_announcement",
                    index: value_index,
                });
            }
            next_first[parameter_index][value_index] = first;
            next_second[parameter_index][value_index] = second;
            next_parameters[parameter_index].values[value_index] = parameter;
        }
    }
    Ok((next_parameters, next_first, next_second))
}
