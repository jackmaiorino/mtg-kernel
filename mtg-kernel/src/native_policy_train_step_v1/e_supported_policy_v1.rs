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
            direction: LineBDivergenceV1::Forward,
            coefficient: 0.1 * roots.len() as f64 / input.scheduled as f64,
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
