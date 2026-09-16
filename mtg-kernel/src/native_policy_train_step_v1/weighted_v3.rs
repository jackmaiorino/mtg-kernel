//! Explicit weighted V3 CPU objective for a separately validated BO3 caller.
//!
//! For group g: w_g * [-stop_gradient(R_g-v_g0) * sum(log p_gs)
//!                    + lambda * (v_g0-R_g)^2]. Weights are positive finite f32
//! values supplied by the caller, with no normalization or group-count divisor.
//! A BO3 caller owns 1/(eligible_matches * learner_groups_in_match), fresh-data
//! identities and complete-match admission. This primitive cannot infer them.
//!
//! The existing unweighted entry points do not call this module. Forward
//! kernels, reverse group/substep order, gauge checks and Adam are reused;
//! this objective has its own explicit arithmetic and must have a distinct
//! checkpoint/loss identity at the caller. Scalar outputs policy_sum/value_sum
//! are weighted sums here. physical_terms and selected_outputs are unweighted
//! diagnostics; they do not independently encode the supplied weights.
//!
//! Bounds: 65,536 physical groups and 100,000 total substeps. Counts do not bound
//! decoded payload or total RSS; caller ingestion limits and the execution
//! supervisor's memory reserve remain necessary. No parallel or GPU backend.
//! Source tests are engineering evidence, never playing-strength evidence.

use super::*;

const MAX_GROUPS: usize = 65_536;
const MAX_SUBSTEPS: usize = 100_000;

fn validate_weighted_inputs_v3(
    groups: &[NativePolicyPhysicalDecisionV1<'_>],
    weights: &[f32],
) -> Result<(), NativePolicyTrainErrorV1> {
    if groups.is_empty() {
        return Err(NativePolicyTrainErrorV1::EmptyBatch);
    }
    if groups.len() != weights.len() {
        return Err(NativePolicyTrainErrorV1::WeightedGroupCountMismatch {
            groups: groups.len(),
            weights: weights.len(),
        });
    }
    if groups.len() > MAX_GROUPS {
        return Err(NativePolicyTrainErrorV1::WeightedBatchLimit {
            groups: groups.len(),
            substeps: 0,
        });
    }
    // Complete weight validation precedes even the first model forward.
    for (group_index, &weight) in weights.iter().enumerate() {
        if !weight.is_finite() || weight <= 0.0 {
            return Err(NativePolicyTrainErrorV1::InvalidPhysicalGroupWeight {
                group_index,
                weight_bits: weight.to_bits(),
            });
        }
    }
    let mut substeps = 0usize;
    for (group_index, group) in groups.iter().enumerate() {
        substeps = substeps.checked_add(group.substeps.len()).ok_or(
            NativePolicyTrainErrorV1::WeightedBatchLimit {
                groups: groups.len(),
                substeps: usize::MAX,
            },
        )?;
        if substeps > MAX_SUBSTEPS {
            return Err(NativePolicyTrainErrorV1::WeightedBatchLimit {
                groups: groups.len(),
                substeps,
            });
        }
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
    Ok(())
}

impl NativePolicyValueTrainStateV1 {
    /// Explicit weighted rich-V6/flat-V3 CPU update. `group_weights` contains
    /// final objective multipliers, not rewards and not probabilities to be
    /// renormalized. Empty inputs and every invalid weight reject unchanged.
    /// The caller must handle an empty eligible-match batch as no-update.
    pub(crate) fn train_step_weighted_feature_transfer_v3(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        group_weights: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let input_config = self.model.feature_transfer_config_v3();
        self.train_step_weighted_with_input_config_v1(
            groups,
            group_weights,
            value_coefficient,
            learning_rate,
            BackwardExecutionV1::Sequential,
            input_config,
        )
    }

    /// V4 sibling of `train_step_weighted_feature_transfer_v3`, for the
    /// fresh-lineage (V7 observation-schema) successor contract. The weighted
    /// grouped loss/backward/Adam arithmetic below is fully shared, unchanged;
    /// only the input-config schema every encoded view is validated against
    /// differs. No CUDA arm exists for the weighted objective at all (V3 or
    /// V4); it is CPU-only, same as before.
    pub(crate) fn train_step_weighted_feature_transfer_v4(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        group_weights: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let input_config = self.model.feature_transfer_config_v4();
        self.train_step_weighted_with_input_config_v1(
            groups,
            group_weights,
            value_coefficient,
            learning_rate,
            BackwardExecutionV1::Sequential,
            input_config,
        )
    }

    /// Config-driven fixed-partition sibling of
    /// `train_step_weighted_feature_transfer_v4`, for
    /// `phase1_bo3_learning_v1::continuation::apply_prepared`'s
    /// `update_backward_execution=fixed_partition_4` V4 arm (see
    /// `native_policy_train_step_v1::train_step_feature_transfer_v4_fixed_partition_v1`,
    /// the unweighted sibling this mirrors). No V3 sibling exists; callers
    /// must reject any non-sequential value for a V3-generation batch before
    /// reaching this function.
    pub(crate) fn train_step_weighted_feature_transfer_v4_fixed_partition_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        group_weights: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_worker_limit: usize,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        let input_config = self.model.feature_transfer_config_v4();
        self.train_step_weighted_with_input_config_v1(
            groups,
            group_weights,
            value_coefficient,
            learning_rate,
            BackwardExecutionV1::FixedPartitions {
                worker_limit: backward_worker_limit,
            },
            input_config,
        )
    }

    fn train_step_weighted_with_input_config_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        group_weights: &[f32],
        value_coefficient: f32,
        learning_rate: f32,
        backward_execution: BackwardExecutionV1,
        input_config: NativePolicyValueModelConfigV1,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        validate_weighted_inputs_v3(groups, group_weights)?;
        if !value_coefficient.is_finite() || value_coefficient <= 0.0 {
            return Err(NativePolicyTrainErrorV1::InvalidValueCoefficient);
        }
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err(NativePolicyTrainErrorV1::InvalidLearningRate);
        }
        let next_step = self
            .adam_step
            .checked_add(1)
            .ok_or(NativePolicyTrainErrorV1::AdamStepOverflow)?;
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
        for (group_index, (group, &weight)) in groups.iter().zip(group_weights).enumerate() {
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
            let target = f32::from(group.terminal_return);
            let advantage = target - value;
            let policy_term = -joint_log_probability * advantage;
            let value_error = value - target;
            let value_term = value_error * value_error;
            policy_sum += weight * policy_term;
            value_sum += weight * value_term;
            physical_terms.push(NativePhysicalLossTermV1 {
                joint_log_probability,
                value,
                terminal_return: group.terminal_return,
                substep_count,
            });
            group_tapes.push((
                GroupTapeV1 {
                    tapes,
                    advantage,
                    value_error,
                },
                weight,
            ));
        }
        let loss = policy_sum + value_coefficient * value_sum;
        finite_scalar("weighted_loss", 0, policy_sum)?;
        finite_scalar("weighted_loss", 1, value_sum)?;
        finite_scalar("weighted_loss", 2, loss)?;

        let zero_gradients = |parameters: &[NativeNamedParameterV1]| -> Vec<Vec<f32>> {
            parameters
                .iter()
                .map(|p| vec![0.0; p.values.len()])
                .collect()
        };
        let (mut gradients, gauge_accumulator) = match backward_execution {
            BackwardExecutionV1::Sequential => {
                let mut gradients = zero_gradients(&parameters);
                let mut gauge_accumulator = ScorerBiasGaugeAccumulatorV1::default();
                let mut reverse_workspace = ReverseWorkspaceV1::default();
                for (group, weight) in group_tapes.into_iter().rev() {
                    let d_joint_log_probability = -group.advantage * weight;
                    let d_value = (value_coefficient * weight) * (2.0 * group.value_error);
                    finite_scalar("weighted_policy_coefficient", 0, d_joint_log_probability)?;
                    finite_scalar("weighted_value_coefficient", 0, d_value)?;
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
                // Same weighted derivative formula as the Sequential arm
                // above (`-advantage * weight`, `(value_coefficient *
                // weight) * 2 * value_error`), precomputed per group so the
                // shared orchestration in `native_policy_train_step_v1`
                // (also used by the unweighted `BackwardExecutionV1::
                // FixedPartitions` arm) can partition and, optionally,
                // parallelize gradient accumulation while gauge evidence
                // keeps its own global reverse observation order.
                let (unweighted_groups, weights): (Vec<GroupTapeV1<'_>>, Vec<f32>) =
                    group_tapes.into_iter().unzip();
                let mut coefficients = Vec::with_capacity(unweighted_groups.len());
                for (index, group) in unweighted_groups.iter().enumerate() {
                    let d_joint_log_probability = -group.advantage * weights[index];
                    let d_value = (value_coefficient * weights[index]) * (2.0 * group.value_error);
                    finite_scalar("weighted_policy_coefficient", index, d_joint_log_probability)?;
                    finite_scalar("weighted_value_coefficient", index, d_value)?;
                    coefficients.push((d_joint_log_probability, d_value));
                }
                let gauge_accumulator =
                    observe_scorer_bias_gauge_v1(&unweighted_groups, &coefficients)?;
                let execution_context = FixedPartitionBackwardExecutionContextV1::current_v1();
                let gradients = fixed_partition_backward_gradients_v1(
                    &parameters,
                    &unweighted_groups,
                    &coefficients,
                    worker_limit,
                    zero_gradients(&parameters),
                    &execution_context,
                )?;
                (gradients, gauge_accumulator)
            }
        };
        validate_finite_nested("weighted_gradient", &gradients)?;
        let mut scorer_bias_gauge = gauge_accumulator.finish(
            gradients[SCORER_SECOND_BIAS][0],
            parameters[SCORER_SECOND_BIAS].values[0].to_bits(),
        )?;
        gradients[SCORER_SECOND_BIAS][0] = 0.0;
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
        // No fallible work follows: commit the whole candidate state together.
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
