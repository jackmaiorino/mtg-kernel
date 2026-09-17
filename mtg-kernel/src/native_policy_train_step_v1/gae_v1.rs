//! CPU `"gae_advantage_value/v1"` objective (design section 2): sibling to
//! `train_step_with_input_config_v1`, reusing its exact forward loop,
//! backward (Sequential and FixedPartitions), and Adam machinery. The only
//! change is the source of the per-group advantage/value-target: instead of
//! deriving them in-loop from `terminal_return`, this takes them as
//! precomputed, already-normalized inputs (`value_targets`, `advantages`),
//! one pair per group in the same order, computed once per update by
//! `expanded_deck_training_v1::compute_gae_targets_v1` from every group's
//! bit-exact `expected_value_bits` and the episode boundary both
//! preparation paths now carry. `terminal_return` itself is not read for
//! arithmetic here; it still flows into `physical_terms` unchanged, exactly
//! as diagnostic evidence.
//!
//! Like `weighted_v3`, this module supports only
//! `NativePolicyForwardInputV1::Encoded` (`expanded_deck_training_v1`'s CPU
//! dispatch never constructs `Packed`), so there is no parallel independent
//! packed recompute here, unlike the unweighted path's full generality.
//! Unlike `weighted_v3`, the loss divides by `group_count` (design section
//! 2, "the same group_count denominator idiom already used at :1833"), not
//! a per-group weight product, so `advantages` must already be normalized
//! by the caller.

use super::*;

fn validate_gae_inputs_v1(
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    value_targets: &[f32],
    advantages: &[f32],
) -> Result<(), NativePolicyTrainErrorV1> {
    if groups.is_empty() {
        return Err(NativePolicyTrainErrorV1::EmptyBatch);
    }
    if groups.len() != value_targets.len() || groups.len() != advantages.len() {
        return Err(NativePolicyTrainErrorV1::GaeGroupCountMismatch {
            groups: groups.len(),
            value_targets: value_targets.len(),
            advantages: advantages.len(),
        });
    }
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
        if group.baseline_bits != 0 {
            return Err(NativePolicyTrainErrorV1::BaselineUnsupportedBackend { group_index });
        }
        for (substep_index, substep) in group.substeps.iter().enumerate() {
            if !matches!(&substep.forward, NativePolicyForwardInputV1::Encoded(_)) {
                return Err(
                    NativePolicyTrainErrorV1::FeatureTransferRequiresCanonicalInput {
                        group_index,
                        substep_index,
                    },
                );
            }
        }
    }
    for (index, value) in value_targets.iter().enumerate() {
        if !value.is_finite() {
            return Err(NativePolicyTrainErrorV1::NonFinite {
                stage: "gae_value_target",
                index,
            });
        }
    }
    for (index, value) in advantages.iter().enumerate() {
        if !value.is_finite() {
            return Err(NativePolicyTrainErrorV1::NonFinite {
                stage: "gae_advantage",
                index,
            });
        }
    }
    Ok(())
}

impl NativePolicyValueTrainStateV1 {
    /// Canonical CPU GAE update under the flat-V3 input contract. V3
    /// sibling of `train_step_gae_feature_transfer_v4`.
    pub(crate) fn train_step_gae_feature_transfer_v3(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let input_config = self.model.feature_transfer_config_v3();
        self.train_step_gae_with_input_config_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            BackwardExecutionV1::Sequential,
            input_config,
        )
    }

    /// V4 sibling of `train_step_gae_feature_transfer_v3`, for the
    /// fresh-lineage (V7 observation-schema) successor contract.
    pub(crate) fn train_step_gae_feature_transfer_v4(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let input_config = self.model.feature_transfer_config_v4();
        self.train_step_gae_with_input_config_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            BackwardExecutionV1::Sequential,
            input_config,
        )
    }

    /// Config-driven fixed-partition sibling of
    /// `train_step_gae_feature_transfer_v4`, mirroring
    /// `train_step_feature_transfer_v4_fixed_partition_v1`. No V3 sibling
    /// exists; callers must reject any non-sequential value for a
    /// V3-generation batch before reaching this function, same as the
    /// unweighted and weighted paths.
    pub(crate) fn train_step_gae_feature_transfer_v4_fixed_partition_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: usize,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let input_config = self.model.feature_transfer_config_v4();
        self.train_step_gae_with_input_config_v1(
            groups,
            value_targets,
            advantages,
            value_coefficient,
            learning_rate,
            BackwardExecutionV1::FixedPartitions {
                worker_limit: backward_worker_limit,
            },
            input_config,
        )
    }

    fn train_step_gae_with_input_config_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_targets: &[f32],
        advantages: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_execution: BackwardExecutionV1,
        input_config: NativePolicyValueModelConfigV1,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        validate_gae_inputs_v1(groups, value_targets, advantages)?;
        if !value_coefficient.is_finite() || value_coefficient <= 0.0 {
            return Err(NativePolicyTrainErrorV1::InvalidValueCoefficient);
        }
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err(NativePolicyTrainErrorV1::InvalidLearningRate);
        }
        let parameters = self.model.parameter_snapshot_v1();
        validate_parameter_manifest(&parameters)?;
        validate_optimizer_state(&parameters, &self.first_moments, &self.second_moments)?;
        validate_canonical_gauge_state(
            &parameters,
            &self.first_moments,
            &self.second_moments,
            self.scorer_bias_anchor_bits,
        )?;
        let mut selected_outputs = Vec::new();
        let mut physical_terms = Vec::with_capacity(groups.len());
        let mut group_tapes = Vec::with_capacity(groups.len());
        let mut policy_sum = 0.0f32;
        let mut value_sum = 0.0f32;
        for (group_index, group) in groups.iter().enumerate() {
            let substep_count = physical_substep_count_u32_v1(group_index, group.substeps.len())?;
            let mut tapes = Vec::with_capacity(group.substeps.len());
            let mut joint_log_probability = None;
            for (substep_index, substep) in group.substeps.iter().enumerate() {
                let NativePolicyForwardInputV1::Encoded(encoded) = &substep.forward else {
                    unreachable!("canonical input checked before forward");
                };
                let recomputed = Box::new(forward_with_tape(&parameters, input_config, **encoded)?);
                validate_forward_output_bits_v1(
                    &recomputed,
                    substep,
                    group_index,
                    substep_index,
                    ForwardOutputSourceV1::IndependentRecompute,
                )?;
                let tape = DecisionTapeSourceV1::Owned(recomputed);
                if substep.selected_action_index >= tape.logits_v1().len() {
                    return Err(NativePolicyTrainErrorV1::SelectedActionOutOfRange {
                        group_index,
                        substep_index,
                        selected: substep.selected_action_index,
                        action_count: tape.logits_v1().len(),
                    });
                }
                let (selected_log_probability, log_probabilities) =
                    selected_log_softmax(tape.logits_v1(), substep.selected_action_index)?;
                joint_log_probability = Some(match joint_log_probability {
                    None => selected_log_probability,
                    Some(active) => active + selected_log_probability,
                });
                selected_outputs.push(NativeSelectedOutputV1 {
                    group_index,
                    substep_index,
                    selected_action_index: substep.selected_action_index,
                    selected_logit: tape.logits_v1()[substep.selected_action_index],
                    value: tape.value_v1(),
                    selected_log_probability,
                });
                tapes.push(SelectedDecisionTapeV1 {
                    tape,
                    selected_action_index: substep.selected_action_index,
                    log_probabilities,
                });
            }
            let joint_log_probability = joint_log_probability.expect("nonempty group checked");
            let value = tapes[0].tape.value_v1();
            let advantage = advantages[group_index];
            let value_target = value_targets[group_index];
            let policy_term = -joint_log_probability * advantage;
            let value_error = value - value_target;
            let value_term = value_error * value_error;
            policy_sum += policy_term;
            value_sum += value_term;
            physical_terms.push(NativePhysicalLossTermV1 {
                joint_log_probability,
                value,
                terminal_return: group.terminal_return,
                substep_count,
            });
            group_tapes.push(GroupTapeV1 {
                tapes,
                advantage,
                value_error,
            });
        }
        let group_count = exact_group_count_f32(groups.len())?;
        let loss = (policy_sum + value_coefficient * value_sum) / group_count;
        finite_scalar("gae_loss", 0, policy_sum)?;
        finite_scalar("gae_loss", 1, value_sum)?;
        finite_scalar("gae_loss", 2, loss)?;

        let (mut gradients, gauge_accumulator) = match backward_execution {
            BackwardExecutionV1::Sequential => {
                let mut gradients = parameters
                    .iter()
                    .map(|parameter| vec![0.0; parameter.values.len()])
                    .collect::<Vec<_>>();
                let mut gauge_accumulator = ScorerBiasGaugeAccumulatorV1::default();
                let mut reverse_workspace = ReverseWorkspaceV1::default();
                // Same reverse group/substep/action order as
                // `train_step_with_input_config_v1`'s Sequential arm.
                for group in group_tapes.into_iter().rev() {
                    let d_joint_log_probability = -group.advantage / group_count;
                    let d_value = (value_coefficient / group_count) * (2.0 * group.value_error);
                    for (substep_index, selected) in group.tapes.iter().enumerate().rev() {
                        resize_zeroed_v1(
                            &mut reverse_workspace.grad_output,
                            selected.log_probabilities.len(),
                        );
                        reverse_workspace.grad_output[selected.selected_action_index] =
                            d_joint_log_probability;
                        let grad_output_sum = reverse_workspace
                            .grad_output
                            .iter()
                            .copied()
                            .fold(0.0f32, |sum, value| sum + value);
                        reverse_workspace.d_logits.clear();
                        reverse_workspace
                            .d_logits
                            .reserve(selected.log_probabilities.len());
                        for (gradient, log_probability) in reverse_workspace
                            .grad_output
                            .iter()
                            .zip(&selected.log_probabilities)
                        {
                            reverse_workspace
                                .d_logits
                                .push(*gradient - log_probability.exp() * grad_output_sum);
                        }
                        gauge_accumulator.observe(
                            selected.tape.logits_v1(),
                            selected.selected_action_index,
                            d_joint_log_probability,
                        )?;
                        reverse_decision(
                            &parameters,
                            &mut gradients,
                            &selected.tape,
                            &reverse_workspace.d_logits,
                            if substep_index == 0 { d_value } else { 0.0 },
                            &mut reverse_workspace.decision,
                        )?;
                    }
                }
                (gradients, gauge_accumulator)
            }
            BackwardExecutionV1::FixedPartitions { worker_limit } => {
                let coefficients: Vec<(f32, f32)> = group_tapes
                    .iter()
                    .map(|group| {
                        (
                            -group.advantage / group_count,
                            (value_coefficient / group_count) * (2.0 * group.value_error),
                        )
                    })
                    .collect();
                let gauge_accumulator = observe_scorer_bias_gauge_v1(&group_tapes, &coefficients)?;
                let execution_context = FixedPartitionBackwardExecutionContextV1::current_v1();
                let gradients = fixed_partition_backward_gradients_v1(
                    &parameters,
                    &group_tapes,
                    &coefficients,
                    worker_limit,
                    parameters
                        .iter()
                        .map(|parameter| vec![0.0; parameter.values.len()])
                        .collect::<Vec<_>>(),
                    &execution_context,
                )?;
                drop(group_tapes);
                (gradients, gauge_accumulator)
            }
        };
        validate_finite_nested("gae_gradient", &gradients)?;
        let raw_scorer_bias_residual = gradients[SCORER_SECOND_BIAS][0];
        let scorer_bias_before_bits = parameters[SCORER_SECOND_BIAS].values[0].to_bits();
        let mut scorer_bias_gauge =
            gauge_accumulator.finish(raw_scorer_bias_residual, scorer_bias_before_bits)?;
        gradients[SCORER_SECOND_BIAS][0] = 0.0;

        let next_step = self
            .adam_step
            .checked_add(1)
            .ok_or(NativePolicyTrainErrorV1::AdamStepOverflow)?;
        let (mut next_parameters, mut next_first_moments, mut next_second_moments) = adam_update(
            &parameters,
            &gradients,
            &self.first_moments,
            &self.second_moments,
            next_step,
            learning_rate,
        )?;
        next_parameters[SCORER_SECOND_BIAS].values[0] =
            f32::from_bits(self.scorer_bias_anchor_bits);
        next_first_moments[SCORER_SECOND_BIAS][0] = 0.0;
        next_second_moments[SCORER_SECOND_BIAS][0] = 0.0;
        scorer_bias_gauge.parameter_after_bits =
            next_parameters[SCORER_SECOND_BIAS].values[0].to_bits();

        let mut candidate_model = self.model.clone();
        candidate_model.replace_parameter_snapshot_v1(&next_parameters)?;
        validate_optimizer_state(&next_parameters, &next_first_moments, &next_second_moments)?;
        validate_canonical_gauge_state(
            &next_parameters,
            &next_first_moments,
            &next_second_moments,
            self.scorer_bias_anchor_bits,
        )?;

        let gradient_snapshot = named_state_snapshot(&parameters, &gradients);
        self.model = candidate_model;
        self.adam_step = next_step;
        self.first_moments = next_first_moments;
        self.second_moments = next_second_moments;
        Ok(NativePolicyTrainStepResultV1 {
            policy_sum,
            value_sum,
            loss,
            adam_step: next_step,
            selected_outputs,
            physical_terms,
            gradients: gradient_snapshot,
            scorer_bias_gauge,
        })
    }
}

#[cfg(test)]
mod tests;
