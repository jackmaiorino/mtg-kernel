//! E's supported executed-choice CE. Separate from line-b's sampler and mask.
use super::*;
use crate::line_b_teacher_target_v1::{LineBDivergenceV1, LineBSoftmaxV1};

pub(crate) struct ESupportedPolicyRowV1 {
    pub(crate) group_index: usize,
    pub(crate) substep_index: usize,
    pub(crate) legal_width: usize,
    pub(crate) target_index: usize,
}
pub(crate) struct ESupportedPolicyInputV1 {
    pub(crate) scheduled: usize,
    pub(crate) rows: Vec<ESupportedPolicyRowV1>,
    pub(crate) treatment: bool,
}

impl NativePolicyValueTrainStateV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn train_step_gae_e_supported_v1(
        &mut self,
        generation: crate::sideboard_play_policy_v1::FreshLineageGenerationV1,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: Option<usize>,
        input: &ESupportedPolicyInputV1,
    ) -> Result<(NativePolicyTrainStepResultV1, Option<serde_json::Value>), NativePolicyTrainErrorV1>
    {
        self.require_platform_math_v1("E-supported")?;
        let invalid = |code| NativePolicyTrainErrorV1::LineBAuxiliary { code };
        if input.rows.len() > input.scheduled {
            return Err(invalid("e-supported-scheduled-count"));
        }
        let mut seen = std::collections::BTreeSet::new();
        let roots = input
            .rows
            .iter()
            .map(|row| {
                let step = groups
                    .get(row.group_index)
                    .and_then(|g| g.substeps.get(row.substep_index))
                    .ok_or(invalid("e-supported-row-outside-batch"))?;
                if !seen.insert((row.group_index, row.substep_index))
                    || row.legal_width < 2
                    || row.target_index >= row.legal_width
                    || step.expected_raw_action_logit_bits.len() != row.legal_width
                {
                    return Err(invalid("e-supported-action-binding"));
                }
                let mut probabilities = vec![0.0; row.legal_width];
                probabilities[row.target_index] = 1.0;
                let mut log_probabilities = vec![f64::NEG_INFINITY; row.legal_width];
                log_probabilities[row.target_index] = 0.0;
                Ok(LineBAuxiliaryRootV1 {
                    group_index: row.group_index,
                    substep_index: row.substep_index,
                    target: Some(LineBSoftmaxV1 {
                        probabilities,
                        log_probabilities,
                        weight_sum: 1.0,
                    }),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Reuse the verified forward-KL reverse pass, never line-b's collection
        // sampler or post-update mask. Normalize by every scheduled slot.
        if roots.is_empty() || !input.treatment {
            let config = match generation {
                crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V3 => {
                    self.model.feature_transfer_config_v3()
                }
                crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V4 => {
                    self.model.feature_transfer_config_v4()
                }
            };
            let result = self.train_step_gae_with_e_config_v1(
                groups,
                value_targets,
                advantages,
                value_coefficient,
                learning_rate,
                backward_worker_limit,
                config,
            )?;
            return Ok((result, None));
        }
        let auxiliary = LineBAuxiliaryInputV1 {
            scheduled_count: Some(input.scheduled),
            direction: LineBDivergenceV1::Forward,
            coefficient: 0.1,
            roots,
        };
        let (result, dose) = self.train_step_gae_feature_transfer_line_b_v1(
            generation,
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_worker_limit,
            &auxiliary,
        )?;
        // Existing loss is preserved in policy_sum/value_sum; total reports CE.

        let total_loss = f64::from(result.loss) + dose.auxiliary_loss;
        Ok((
            result,
            Some(
                serde_json::json!({"method":"e-supported-choice-ce-v1-20261012",
            "scheduled":input.scheduled,"valid":input.rows.len(),"coefficient":0.1,
            "ce":dose.divergences_before,"auxiliary_loss":dose.auxiliary_loss,
            "total_loss":total_loss,
            "gradient_components":dose.gradient_components,
            "auxiliary_bias_residual":dose.auxiliary_bias_residual}),
            ),
        ))
    }
}

/// SQ1's soft rollout-Q target at one learner row (collab
/// LANES/soft-q-teaching-20261018).
pub(crate) struct SoftTargetPolicyRowV1 {
    pub(crate) group_index: usize,
    pub(crate) substep_index: usize,
    pub(crate) target: LineBSoftmaxV1,
}
/// Every scheduled slot counts in the mean; unavailable slots add nothing.
pub(crate) struct SoftTargetPolicyInputV1 {
    pub(crate) scheduled: usize,
    pub(crate) rows: Vec<SoftTargetPolicyRowV1>,
    pub(crate) coefficient: f64,
}

impl NativePolicyValueTrainStateV1 {
    /// Ordinary GAE plus `coefficient / scheduled` times the sum of forward
    /// KL(q || p) over the rows, through E's full-Adam path (no mask, no
    /// line (b) sampler). Without rows it is the ordinary update.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn train_step_gae_soft_targets_v1(
        &mut self,
        generation: crate::sideboard_play_policy_v1::FreshLineageGenerationV1,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: Option<usize>,
        input: &SoftTargetPolicyInputV1,
    ) -> Result<(NativePolicyTrainStepResultV1, Option<serde_json::Value>), NativePolicyTrainErrorV1>
    {
        self.require_platform_math_v1("SQ1 soft targets")?;
        let invalid = |code| NativePolicyTrainErrorV1::LineBAuxiliary { code };
        if input.rows.len() > input.scheduled
            || input.scheduled == 0
            || !(input.coefficient.is_finite() && input.coefficient > 0.0)
        {
            return Err(invalid("soft-target-schedule"));
        }
        let mut seen = std::collections::BTreeSet::new();
        let roots = input
            .rows
            .iter()
            .map(|row| {
                let step = groups
                    .get(row.group_index)
                    .and_then(|g| g.substeps.get(row.substep_index))
                    .ok_or(invalid("soft-target-row-outside-batch"))?;
                let width = row.target.probabilities.len();
                let mass: f64 = row.target.probabilities.iter().sum();
                if !seen.insert((row.group_index, row.substep_index))
                    || width < 2
                    || row.target.log_probabilities.len() != width
                    || step.expected_raw_action_logit_bits.len() != width
                    || row
                        .target
                        .probabilities
                        .iter()
                        .any(|p| !(p.is_finite() && *p >= 0.0))
                    || (mass - 1.0).abs() > 1e-9
                {
                    return Err(invalid("soft-target-binding"));
                }
                Ok(LineBAuxiliaryRootV1 {
                    group_index: row.group_index,
                    substep_index: row.substep_index,
                    target: Some(LineBSoftmaxV1 {
                        probabilities: row.target.probabilities.clone(),
                        log_probabilities: row.target.log_probabilities.clone(),
                        weight_sum: row.target.weight_sum,
                    }),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if roots.is_empty() {
            let config = match generation {
                crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V3 => {
                    self.model.feature_transfer_config_v3()
                }
                crate::sideboard_play_policy_v1::FreshLineageGenerationV1::V4 => {
                    self.model.feature_transfer_config_v4()
                }
            };
            let result = self.train_step_gae_with_e_config_v1(
                groups,
                value_targets,
                advantages,
                value_coefficient,
                learning_rate,
                backward_worker_limit,
                config,
            )?;
            return Ok((result, None));
        }
        let auxiliary = LineBAuxiliaryInputV1 {
            scheduled_count: Some(input.scheduled),
            direction: LineBDivergenceV1::Forward,
            coefficient: input.coefficient,
            roots,
        };
        let (result, dose) = self.train_step_gae_feature_transfer_line_b_v1(
            generation,
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            backward_worker_limit,
            &auxiliary,
        )?;
        let total_loss = f64::from(result.loss) + dose.auxiliary_loss;
        Ok((
            result,
            Some(
                serde_json::json!({"scheduled":input.scheduled,"valid":input.rows.len(),
                "coefficient":input.coefficient,"kl_before":dose.divergences_before,
                "auxiliary_loss":dose.auxiliary_loss,"total_loss":total_loss,
                "gradient_components":dose.gradient_components,
                "auxiliary_head_l2":dose.auxiliary_head_l2,"ordinary_head_l2":dose.ordinary_head_l2,
                "auxiliary_bias_residual":dose.auxiliary_bias_residual}),
            ),
        ))
    }
}
