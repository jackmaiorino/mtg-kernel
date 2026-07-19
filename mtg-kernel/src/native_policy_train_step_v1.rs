//! Auditable CPU backward and Adam reference for `KernelPolicyValueNet`.
//!
//! This is the numerical reference for the frozen
//! `terminal_reinforce_value/v3` loss.  It deliberately does not provide a
//! runtime scheduler, checkpoint format, seeded initializer, CUDA backend, or
//! performance claim.
//!
//! The scorer's final scalar bias is an exact gauge: adding it to every legal
//! action logit leaves log-softmax unchanged, so in exact arithmetic
//! `dL/db = c * (1 - sum(softmax)) = 0` for every policy substep. CPU f32
//! reduction residuals are retained, bounded, and recorded before the native
//! optimizer canonicalizes exactly this one gauge. The value-head bias is not
//! shift-invariant and is explicitly outside the gauge set.
//!
//! The fail-closed bound is scale-derived, not an observed-residual literal.
//! With binary32 unit roundoff `u = EPSILON/2` and
//! `gamma(k) = k*u/(1-k*u)`, substep `j` with `n_j` actions and absolute
//! policy coefficient `|c_j|` contributes `|c_j|*gamma(8*n_j+8)`. For `m`
//! substeps, the shared-bias accumulation contributes another
//! `gamma(m-1)*2*sum_j(|c_j|)`. Their sum is recorded beside the raw residual;
//! exceeding it aborts before model or optimizer mutation.

use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionViewV1, NativeNamedParameterV1, NativePolicyValueErrorV1,
    NativePolicyValueNetV1, ACTION_FEATURE_DIM_V1, ACTION_REF_FEATURE_DIM_V1,
    CARD_EMBEDDING_DIM_V1, EDGE_FEATURE_DIM_V1, HIDDEN_DIM_V1, OBJECT_FEATURE_DIM_V1,
    OBJECT_GROUP_COUNT_V1, PARAMETER_COUNT_V1, STATE_DIM_V1,
};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub(crate) const TRAINER_ALGORITHM_V1: &str = "terminal_reinforce_value/v3";
pub(crate) const TRAIN_STEP_IDENTITY_V1: &str = "native-policy-value-cpu-train-step-v1";
pub(crate) const NATIVE_OPTIMIZER_IDENTITY_V1: &str = "native-adam-canonical-scorer-bias-gauge-v1";
pub(crate) const LEGACY_GAUGE_NONCLAIM_V1: &str =
    "no exact optimizer-state parity claim for legacy terminal_reinforce_value/v3 scorer.2.bias f32 gauge drift";
pub(crate) const CANONICAL_GAUGE_PARAMETERS_V1: [&str; 1] = ["scorer.2.bias"];
pub(crate) const ADAM_BETA1_V1: f32 = 0.9;
pub(crate) const ADAM_BETA2_V1: f32 = 0.999;
pub(crate) const ADAM_EPSILON_V1: f32 = 1.0e-8;
pub(crate) const ADAM_WEIGHT_DECAY_V1: f32 = 0.0;

const OBJECT_ENCODER_INPUT: usize = OBJECT_FEATURE_DIM_V1 + CARD_EMBEDDING_DIM_V1;
const EDGE_ENCODER_INPUT: usize = EDGE_FEATURE_DIM_V1 + HIDDEN_DIM_V1 * 2;
const NODE_UPDATE_INPUT: usize = HIDDEN_DIM_V1 * 2;
const STATE_ENCODER_INPUT: usize = STATE_DIM_V1 + HIDDEN_DIM_V1 * OBJECT_GROUP_COUNT_V1;
const ACTION_REF_ENCODER_INPUT: usize = ACTION_REF_FEATURE_DIM_V1 + HIDDEN_DIM_V1;
const ACTION_ENCODER_INPUT: usize = ACTION_FEATURE_DIM_V1 + HIDDEN_DIM_V1;
const SCORER_INPUT: usize = HIDDEN_DIM_V1 * 2;

const CARD_EMBEDDING: usize = 0;
const OBJECT_FIRST_WEIGHT: usize = 1;
const OBJECT_FIRST_BIAS: usize = 2;
const OBJECT_SECOND_WEIGHT: usize = 3;
const OBJECT_SECOND_BIAS: usize = 4;
const EDGE_FIRST_WEIGHT: usize = 5;
const EDGE_FIRST_BIAS: usize = 6;
const EDGE_SECOND_WEIGHT: usize = 7;
const EDGE_SECOND_BIAS: usize = 8;
const NODE_FIRST_WEIGHT: usize = 9;
const NODE_FIRST_BIAS: usize = 10;
const NODE_SECOND_WEIGHT: usize = 11;
const NODE_SECOND_BIAS: usize = 12;
const STATE_FIRST_WEIGHT: usize = 13;
const STATE_FIRST_BIAS: usize = 14;
const STATE_SECOND_WEIGHT: usize = 15;
const STATE_SECOND_BIAS: usize = 16;
const ACTION_REF_FIRST_WEIGHT: usize = 17;
const ACTION_REF_FIRST_BIAS: usize = 18;
const ACTION_REF_SECOND_WEIGHT: usize = 19;
const ACTION_REF_SECOND_BIAS: usize = 20;
const ACTION_FIRST_WEIGHT: usize = 21;
const ACTION_FIRST_BIAS: usize = 22;
const ACTION_SECOND_WEIGHT: usize = 23;
const ACTION_SECOND_BIAS: usize = 24;
const SCORER_FIRST_WEIGHT: usize = 25;
const SCORER_FIRST_BIAS: usize = 26;
const SCORER_SECOND_WEIGHT: usize = 27;
const SCORER_SECOND_BIAS: usize = 28;
const VALUE_FIRST_WEIGHT: usize = 29;
const VALUE_FIRST_BIAS: usize = 30;
const VALUE_SECOND_WEIGHT: usize = 31;
const VALUE_SECOND_BIAS: usize = 32;
const PARAMETER_TENSOR_COUNT: usize = 33;

const EXPECTED_PARAMETER_NAMES: [&str; PARAMETER_TENSOR_COUNT] = [
    "card_embedding.weight",
    "object_encoder.0.weight",
    "object_encoder.0.bias",
    "object_encoder.2.weight",
    "object_encoder.2.bias",
    "edge_encoder.0.weight",
    "edge_encoder.0.bias",
    "edge_encoder.2.weight",
    "edge_encoder.2.bias",
    "node_update.0.weight",
    "node_update.0.bias",
    "node_update.2.weight",
    "node_update.2.bias",
    "state_encoder.0.weight",
    "state_encoder.0.bias",
    "state_encoder.2.weight",
    "state_encoder.2.bias",
    "action_ref_encoder.0.weight",
    "action_ref_encoder.0.bias",
    "action_ref_encoder.2.weight",
    "action_ref_encoder.2.bias",
    "action_encoder.0.weight",
    "action_encoder.0.bias",
    "action_encoder.2.weight",
    "action_encoder.2.bias",
    "scorer.0.weight",
    "scorer.0.bias",
    "scorer.2.weight",
    "scorer.2.bias",
    "value_head.0.weight",
    "value_head.0.bias",
    "value_head.2.weight",
    "value_head.2.bias",
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct NativePolicySubstepV1<'a> {
    pub(crate) encoded: NativeEncodedDecisionViewV1<'a>,
    pub(crate) selected_action_index: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NativePolicyPhysicalDecisionV1<'a> {
    pub(crate) substeps: &'a [NativePolicySubstepV1<'a>],
    pub(crate) terminal_return: i8,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeSelectedOutputV1 {
    pub(crate) group_index: usize,
    pub(crate) substep_index: usize,
    pub(crate) selected_action_index: usize,
    pub(crate) selected_logit: f32,
    pub(crate) value: f32,
    pub(crate) selected_log_probability: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativePhysicalLossTermV1 {
    pub(crate) joint_log_probability: f32,
    pub(crate) value: f32,
    pub(crate) terminal_return: i8,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativePolicyTrainStepResultV1 {
    pub(crate) policy_sum: f32,
    pub(crate) value_sum: f32,
    pub(crate) loss: f32,
    pub(crate) adam_step: u64,
    pub(crate) selected_outputs: Vec<NativeSelectedOutputV1>,
    pub(crate) physical_terms: Vec<NativePhysicalLossTermV1>,
    pub(crate) gradients: Vec<NativeNamedParameterV1>,
    pub(crate) scorer_bias_gauge: NativeScorerBiasGaugeRecordV1,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeScorerBiasGaugeRecordV1 {
    pub(crate) parameter_name: &'static str,
    pub(crate) substep_count: usize,
    pub(crate) total_action_count: usize,
    pub(crate) max_action_count: usize,
    pub(crate) sum_abs_policy_coefficients: f64,
    pub(crate) substep_bounds: Vec<NativeGaugeSubstepBoundV1>,
    pub(crate) per_substep_bound_sum: f64,
    pub(crate) cross_substep_bound: f64,
    pub(crate) raw_gradient_residual: f32,
    pub(crate) derived_absolute_bound: f64,
    pub(crate) high_precision_residual: f64,
    pub(crate) canonical_gradient: f32,
    pub(crate) parameter_before_bits: u32,
    pub(crate) parameter_after_bits: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeGaugeSubstepBoundV1 {
    pub(crate) action_count: usize,
    pub(crate) abs_policy_coefficient: f64,
    pub(crate) gamma_operation_count: usize,
    pub(crate) gamma: f64,
    pub(crate) bound_component: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativePolicyTrainErrorV1 {
    Model(NativePolicyValueErrorV1),
    EmptyBatch,
    EmptyPhysicalDecision {
        group_index: usize,
    },
    InvalidTerminalReturn {
        group_index: usize,
        value: i8,
    },
    SelectedActionOutOfRange {
        group_index: usize,
        substep_index: usize,
        selected: usize,
        action_count: usize,
    },
    InvalidValueCoefficient,
    InvalidLearningRate,
    ParameterManifest,
    OptimizerState,
    AdamStepOverflow,
    GaugeBoundOverflow,
    GaugeResidualExceeded {
        residual_bits: u32,
        bound_bits: u64,
    },
    NonFinite {
        stage: &'static str,
        index: usize,
    },
}

impl Display for NativePolicyTrainErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native policy train step v1 error: {self:?}")
    }
}

impl Error for NativePolicyTrainErrorV1 {}

impl From<NativePolicyValueErrorV1> for NativePolicyTrainErrorV1 {
    fn from(value: NativePolicyValueErrorV1) -> Self {
        Self::Model(value)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NativePolicyValueTrainStateV1 {
    model: NativePolicyValueNetV1,
    adam_step: u64,
    first_moments: Vec<Vec<f32>>,
    second_moments: Vec<Vec<f32>>,
}

impl NativePolicyValueTrainStateV1 {
    pub(crate) fn new_v1(model: NativePolicyValueNetV1) -> Result<Self, NativePolicyTrainErrorV1> {
        let parameters = model.parameter_snapshot_v1();
        validate_parameter_manifest(&parameters)?;
        let first_moments = parameters
            .iter()
            .map(|parameter| vec![0.0; parameter.values.len()])
            .collect();
        let second_moments = parameters
            .iter()
            .map(|parameter| vec![0.0; parameter.values.len()])
            .collect();
        Ok(Self {
            model,
            adam_step: 0,
            first_moments,
            second_moments,
        })
    }

    pub(crate) fn model_v1(&self) -> &NativePolicyValueNetV1 {
        &self.model
    }

    pub(crate) fn adam_step_v1(&self) -> u64 {
        self.adam_step
    }

    pub(crate) fn first_moment_snapshot_v1(&self) -> Vec<NativeNamedParameterV1> {
        named_state_snapshot(&self.model.parameter_snapshot_v1(), &self.first_moments)
    }

    pub(crate) fn second_moment_snapshot_v1(&self) -> Vec<NativeNamedParameterV1> {
        named_state_snapshot(&self.model.parameter_snapshot_v1(), &self.second_moments)
    }

    /// Applies one complete grouped loss/backward/Adam update.  Every input,
    /// gradient, moment, and candidate parameter is validated before the live
    /// model or optimizer state changes.
    pub(crate) fn train_step_v1(
        &mut self,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_coefficient: f32,
        learning_rate: f32,
    ) -> Result<NativePolicyTrainStepResultV1, NativePolicyTrainErrorV1> {
        if groups.is_empty() {
            return Err(NativePolicyTrainErrorV1::EmptyBatch);
        }
        if !value_coefficient.is_finite() || value_coefficient <= 0.0 {
            return Err(NativePolicyTrainErrorV1::InvalidValueCoefficient);
        }
        if !learning_rate.is_finite() || learning_rate <= 0.0 {
            return Err(NativePolicyTrainErrorV1::InvalidLearningRate);
        }

        let parameters = self.model.parameter_snapshot_v1();
        validate_parameter_manifest(&parameters)?;
        validate_optimizer_state(&parameters, &self.first_moments, &self.second_moments)?;
        validate_canonical_gauge_state(&parameters, &self.first_moments, &self.second_moments)?;

        let mut gradients = parameters
            .iter()
            .map(|parameter| vec![0.0; parameter.values.len()])
            .collect::<Vec<_>>();
        let mut selected_outputs = Vec::new();
        let mut physical_terms = Vec::with_capacity(groups.len());
        let mut group_tapes = Vec::with_capacity(groups.len());
        let mut policy_sum = 0.0f32;
        let mut value_sum = 0.0f32;

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

            let mut tapes = Vec::with_capacity(group.substeps.len());
            let mut joint_log_probability = None;
            for (substep_index, substep) in group.substeps.iter().enumerate() {
                let tape = forward_with_tape(&parameters, self.model.config_v1(), substep.encoded)?;
                if substep.selected_action_index >= tape.logits.len() {
                    return Err(NativePolicyTrainErrorV1::SelectedActionOutOfRange {
                        group_index,
                        substep_index,
                        selected: substep.selected_action_index,
                        action_count: tape.logits.len(),
                    });
                }
                let (selected_log_probability, log_probabilities) =
                    selected_log_softmax(&tape.logits, substep.selected_action_index)?;
                joint_log_probability = Some(match joint_log_probability {
                    None => selected_log_probability,
                    Some(active) => active + selected_log_probability,
                });
                selected_outputs.push(NativeSelectedOutputV1 {
                    group_index,
                    substep_index,
                    selected_action_index: substep.selected_action_index,
                    selected_logit: tape.logits[substep.selected_action_index],
                    value: tape.value,
                    selected_log_probability,
                });
                tapes.push(SelectedDecisionTapeV1 {
                    tape,
                    selected_action_index: substep.selected_action_index,
                    log_probabilities,
                });
            }

            let joint_log_probability = joint_log_probability.expect("nonempty group checked");
            let value = tapes[0].tape.value;
            let target = f32::from(group.terminal_return);
            let advantage = target - value;
            let policy_term = -joint_log_probability * advantage;
            let value_error = value - target;
            let value_term = value_error * value_error;
            policy_sum += policy_term;
            value_sum += value_term;
            physical_terms.push(NativePhysicalLossTermV1 {
                joint_log_probability,
                value,
                terminal_return: group.terminal_return,
            });
            group_tapes.push(GroupTapeV1 {
                tapes,
                advantage,
                value_error,
            });
        }

        let group_count = groups.len() as f32;
        let loss = (policy_sum + value_coefficient * value_sum) / group_count;
        finite_scalar("loss", 0, policy_sum)?;
        finite_scalar("loss", 1, value_sum)?;
        finite_scalar("loss", 2, loss)?;

        let mut gauge_accumulator = ScorerBiasGaugeAccumulatorV1::default();
        // Torch autograd walks the independently constructed physical-decision
        // and substep graphs in reverse construction order.  Preserve that
        // accumulation order for shared parameter gradients. The scorer's
        // final scalar-bias reduction also uses Torch's reverse action-row
        // order; all other row traversals retain their established order.
        for group in group_tapes.into_iter().rev() {
            let d_joint_log_probability = -group.advantage / group_count;
            let d_value = (value_coefficient / group_count) * (2.0 * group.value_error);
            for (substep_index, selected) in group.tapes.iter().enumerate().rev() {
                // Torch LogSoftmaxBackward: retain the complete forward
                // log-softmax output, then evaluate grad_output -
                // exp(output) * sum(grad_output).  Both reductions and the
                // following final scalar-bias accumulation use the pinned
                // reverse graph/action order. The raw f32 residual is retained
                // for the fail-closed gauge-bound record below.
                let mut grad_output = vec![0.0; selected.log_probabilities.len()];
                grad_output[selected.selected_action_index] = d_joint_log_probability;
                let grad_output_sum = grad_output
                    .iter()
                    .copied()
                    .fold(0.0f32, |sum, value| sum + value);
                let d_logits = grad_output
                    .iter()
                    .zip(&selected.log_probabilities)
                    .map(|(gradient, log_probability)| {
                        *gradient - log_probability.exp() * grad_output_sum
                    })
                    .collect::<Vec<_>>();
                gauge_accumulator.observe(
                    &selected.tape.logits,
                    selected.selected_action_index,
                    d_joint_log_probability,
                )?;
                reverse_decision(
                    &parameters,
                    &mut gradients,
                    &selected.tape,
                    &d_logits,
                    if substep_index == 0 { d_value } else { 0.0 },
                )?;
            }
        }
        validate_finite_nested("gradient", &gradients)?;
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
        next_parameters[SCORER_SECOND_BIAS].values[0] = parameters[SCORER_SECOND_BIAS].values[0];
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

#[derive(Default)]
struct ScorerBiasGaugeAccumulatorV1 {
    substep_count: usize,
    total_action_count: usize,
    max_action_count: usize,
    sum_abs_policy_coefficients: f64,
    per_substep_bound_sum: f64,
    high_precision_residual: f64,
    substep_bounds: Vec<NativeGaugeSubstepBoundV1>,
}

impl ScorerBiasGaugeAccumulatorV1 {
    fn observe(
        &mut self,
        logits: &[f32],
        selected_action_index: usize,
        policy_coefficient: f32,
    ) -> Result<(), NativePolicyTrainErrorV1> {
        let operation_count = logits
            .len()
            .checked_mul(8)
            .and_then(|value| value.checked_add(8))
            .ok_or(NativePolicyTrainErrorV1::GaugeBoundOverflow)?;
        let coefficient = f64::from(policy_coefficient);
        let abs_policy_coefficient = coefficient.abs();
        let gamma = f32_gamma(operation_count)?;
        let bound_component = abs_policy_coefficient * gamma;
        self.per_substep_bound_sum += bound_component;
        self.sum_abs_policy_coefficients += abs_policy_coefficient;
        self.substep_count = self
            .substep_count
            .checked_add(1)
            .ok_or(NativePolicyTrainErrorV1::GaugeBoundOverflow)?;
        self.total_action_count = self
            .total_action_count
            .checked_add(logits.len())
            .ok_or(NativePolicyTrainErrorV1::GaugeBoundOverflow)?;
        self.max_action_count = self.max_action_count.max(logits.len());
        self.substep_bounds.push(NativeGaugeSubstepBoundV1 {
            action_count: logits.len(),
            abs_policy_coefficient,
            gamma_operation_count: operation_count,
            gamma,
            bound_component,
        });

        let maximum = logits
            .iter()
            .copied()
            .map(f64::from)
            .fold(f64::NEG_INFINITY, f64::max);
        let exponential_sum = logits
            .iter()
            .copied()
            .map(f64::from)
            .map(|logit| (logit - maximum).exp())
            .sum::<f64>();
        let log_sum = exponential_sum.ln();
        // Match the full reverse scorer-bias reduction while recomputing the
        // normalization and backward in f64 from the same stored f32 logits.
        for action in (0..logits.len()).rev() {
            let log_probability = (f64::from(logits[action]) - maximum) - log_sum;
            let grad_output = if action == selected_action_index {
                coefficient
            } else {
                0.0
            };
            self.high_precision_residual += grad_output - log_probability.exp() * coefficient;
        }
        Ok(())
    }

    fn finish(
        self,
        raw_gradient_residual: f32,
        parameter_before_bits: u32,
    ) -> Result<NativeScorerBiasGaugeRecordV1, NativePolicyTrainErrorV1> {
        let cross_substep_bound = f32_gamma(self.substep_count.saturating_sub(1))?
            * 2.0
            * self.sum_abs_policy_coefficients;
        let derived_absolute_bound = self.per_substep_bound_sum + cross_substep_bound;
        if !derived_absolute_bound.is_finite()
            || f64::from(raw_gradient_residual).abs() > derived_absolute_bound
        {
            return Err(NativePolicyTrainErrorV1::GaugeResidualExceeded {
                residual_bits: raw_gradient_residual.to_bits(),
                bound_bits: derived_absolute_bound.to_bits(),
            });
        }
        Ok(NativeScorerBiasGaugeRecordV1 {
            parameter_name: CANONICAL_GAUGE_PARAMETERS_V1[0],
            substep_count: self.substep_count,
            total_action_count: self.total_action_count,
            max_action_count: self.max_action_count,
            sum_abs_policy_coefficients: self.sum_abs_policy_coefficients,
            substep_bounds: self.substep_bounds,
            per_substep_bound_sum: self.per_substep_bound_sum,
            cross_substep_bound,
            raw_gradient_residual,
            derived_absolute_bound,
            high_precision_residual: self.high_precision_residual,
            canonical_gradient: 0.0,
            parameter_before_bits,
            parameter_after_bits: parameter_before_bits,
        })
    }
}

fn f32_gamma(operation_count: usize) -> Result<f64, NativePolicyTrainErrorV1> {
    let unit_roundoff = f64::from(f32::EPSILON) / 2.0;
    let scaled = operation_count as f64 * unit_roundoff;
    if !scaled.is_finite() || scaled >= 1.0 {
        return Err(NativePolicyTrainErrorV1::GaugeBoundOverflow);
    }
    Ok(scaled / (1.0 - scaled))
}

#[derive(Clone, Copy)]
struct TwoLayerSpecV1 {
    first_weight: usize,
    first_bias: usize,
    second_weight: usize,
    second_bias: usize,
    input_dim: usize,
}

const OBJECT_SPEC: TwoLayerSpecV1 = TwoLayerSpecV1 {
    first_weight: OBJECT_FIRST_WEIGHT,
    first_bias: OBJECT_FIRST_BIAS,
    second_weight: OBJECT_SECOND_WEIGHT,
    second_bias: OBJECT_SECOND_BIAS,
    input_dim: OBJECT_ENCODER_INPUT,
};
const EDGE_SPEC: TwoLayerSpecV1 = TwoLayerSpecV1 {
    first_weight: EDGE_FIRST_WEIGHT,
    first_bias: EDGE_FIRST_BIAS,
    second_weight: EDGE_SECOND_WEIGHT,
    second_bias: EDGE_SECOND_BIAS,
    input_dim: EDGE_ENCODER_INPUT,
};
const NODE_SPEC: TwoLayerSpecV1 = TwoLayerSpecV1 {
    first_weight: NODE_FIRST_WEIGHT,
    first_bias: NODE_FIRST_BIAS,
    second_weight: NODE_SECOND_WEIGHT,
    second_bias: NODE_SECOND_BIAS,
    input_dim: NODE_UPDATE_INPUT,
};
const STATE_SPEC: TwoLayerSpecV1 = TwoLayerSpecV1 {
    first_weight: STATE_FIRST_WEIGHT,
    first_bias: STATE_FIRST_BIAS,
    second_weight: STATE_SECOND_WEIGHT,
    second_bias: STATE_SECOND_BIAS,
    input_dim: STATE_ENCODER_INPUT,
};
const ACTION_REF_SPEC: TwoLayerSpecV1 = TwoLayerSpecV1 {
    first_weight: ACTION_REF_FIRST_WEIGHT,
    first_bias: ACTION_REF_FIRST_BIAS,
    second_weight: ACTION_REF_SECOND_WEIGHT,
    second_bias: ACTION_REF_SECOND_BIAS,
    input_dim: ACTION_REF_ENCODER_INPUT,
};
const ACTION_SPEC: TwoLayerSpecV1 = TwoLayerSpecV1 {
    first_weight: ACTION_FIRST_WEIGHT,
    first_bias: ACTION_FIRST_BIAS,
    second_weight: ACTION_SECOND_WEIGHT,
    second_bias: ACTION_SECOND_BIAS,
    input_dim: ACTION_ENCODER_INPUT,
};

struct TwoLayerTapeV1 {
    input: Vec<f32>,
    first_output: Vec<f32>,
    second_output: Vec<f32>,
    rows: usize,
}

struct DecisionTapeV1 {
    object_card_ids: Vec<usize>,
    object_groups: Vec<usize>,
    edge_source_indices: Vec<usize>,
    edge_target_indices: Vec<usize>,
    action_ref_action_indices: Vec<usize>,
    action_ref_node_indices: Vec<usize>,
    object_encoder: TwoLayerTapeV1,
    edge_encoder: Option<TwoLayerTapeV1>,
    node_update: TwoLayerTapeV1,
    state_encoder: TwoLayerTapeV1,
    action_ref_encoder: Option<TwoLayerTapeV1>,
    action_encoder: TwoLayerTapeV1,
    scorer_input: Vec<f32>,
    scorer_hidden: Vec<f32>,
    value_hidden: Vec<f32>,
    logits: Vec<f32>,
    value: f32,
}

struct SelectedDecisionTapeV1 {
    tape: DecisionTapeV1,
    selected_action_index: usize,
    log_probabilities: Vec<f32>,
}

struct GroupTapeV1 {
    tapes: Vec<SelectedDecisionTapeV1>,
    advantage: f32,
    value_error: f32,
}

fn forward_with_tape(
    parameters: &[NativeNamedParameterV1],
    config: crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1,
    encoded: NativeEncodedDecisionViewV1<'_>,
) -> Result<DecisionTapeV1, NativePolicyTrainErrorV1> {
    let counts = encoded.validate(config)?;
    let object_card_ids = encoded
        .object_card_ids
        .iter()
        .map(|value| *value as usize)
        .collect::<Vec<_>>();
    let object_groups = encoded
        .object_groups
        .iter()
        .map(|value| *value as usize)
        .collect::<Vec<_>>();
    let edge_source_indices = encoded
        .edge_source_indices
        .iter()
        .map(|value| *value as usize)
        .collect::<Vec<_>>();
    let edge_target_indices = encoded
        .edge_target_indices
        .iter()
        .map(|value| *value as usize)
        .collect::<Vec<_>>();
    let action_ref_action_indices = encoded
        .action_ref_action_indices
        .iter()
        .map(|value| *value as usize)
        .collect::<Vec<_>>();
    let action_ref_node_indices = encoded
        .action_ref_node_indices
        .iter()
        .map(|value| *value as usize)
        .collect::<Vec<_>>();

    let embedding = &parameters[CARD_EMBEDDING].values;
    let mut object_input = Vec::with_capacity(counts.object_count * OBJECT_ENCODER_INPUT);
    for (object, token) in object_card_ids.iter().copied().enumerate() {
        let feature_begin = object * OBJECT_FEATURE_DIM_V1;
        object_input.extend_from_slice(
            &encoded.object_features[feature_begin..feature_begin + OBJECT_FEATURE_DIM_V1],
        );
        let embedding_begin = token * CARD_EMBEDDING_DIM_V1;
        object_input.extend_from_slice(
            &embedding[embedding_begin..embedding_begin + CARD_EMBEDDING_DIM_V1],
        );
    }
    let object_encoder =
        two_layer_forward(parameters, OBJECT_SPEC, object_input, counts.object_count)?;

    let mut edge_pooled = vec![0.0; counts.object_count * HIDDEN_DIM_V1];
    let edge_encoder = if counts.edge_count == 0 {
        None
    } else {
        let mut edge_input = Vec::with_capacity(counts.edge_count * EDGE_ENCODER_INPUT);
        for edge in 0..counts.edge_count {
            let feature_begin = edge * EDGE_FEATURE_DIM_V1;
            edge_input.extend_from_slice(
                &encoded.edge_features[feature_begin..feature_begin + EDGE_FEATURE_DIM_V1],
            );
            let source_begin = edge_source_indices[edge] * HIDDEN_DIM_V1;
            edge_input.extend_from_slice(
                &object_encoder.second_output[source_begin..source_begin + HIDDEN_DIM_V1],
            );
            let target_begin = edge_target_indices[edge] * HIDDEN_DIM_V1;
            edge_input.extend_from_slice(
                &object_encoder.second_output[target_begin..target_begin + HIDDEN_DIM_V1],
            );
        }
        let tape = two_layer_forward(parameters, EDGE_SPEC, edge_input, counts.edge_count)?;
        add_indexed_rows(&mut edge_pooled, &tape.second_output, &edge_source_indices);
        add_indexed_rows(&mut edge_pooled, &tape.second_output, &edge_target_indices);
        Some(tape)
    };

    let mut node_input = Vec::with_capacity(counts.object_count * NODE_UPDATE_INPUT);
    for object in 0..counts.object_count {
        let begin = object * HIDDEN_DIM_V1;
        node_input.extend_from_slice(&object_encoder.second_output[begin..begin + HIDDEN_DIM_V1]);
        node_input.extend_from_slice(&edge_pooled[begin..begin + HIDDEN_DIM_V1]);
    }
    let node_update = two_layer_forward(parameters, NODE_SPEC, node_input, counts.object_count)?;

    let mut pooled_objects = vec![0.0; OBJECT_GROUP_COUNT_V1 * HIDDEN_DIM_V1];
    add_indexed_rows(
        &mut pooled_objects,
        &node_update.second_output,
        &object_groups,
    );
    let mut state_input = Vec::with_capacity(STATE_ENCODER_INPUT);
    state_input.extend_from_slice(encoded.state);
    state_input.extend_from_slice(&pooled_objects);
    let state_encoder = two_layer_forward(parameters, STATE_SPEC, state_input, 1)?;

    let mut action_ref_pooled = vec![0.0; counts.action_count * HIDDEN_DIM_V1];
    let action_ref_encoder = if counts.action_ref_count == 0 {
        None
    } else {
        let mut action_ref_input =
            Vec::with_capacity(counts.action_ref_count * ACTION_REF_ENCODER_INPUT);
        for (action_ref, node_index) in action_ref_node_indices.iter().copied().enumerate() {
            let feature_begin = action_ref * ACTION_REF_FEATURE_DIM_V1;
            action_ref_input.extend_from_slice(
                &encoded.action_ref_features
                    [feature_begin..feature_begin + ACTION_REF_FEATURE_DIM_V1],
            );
            let node_begin = node_index * HIDDEN_DIM_V1;
            action_ref_input.extend_from_slice(
                &node_update.second_output[node_begin..node_begin + HIDDEN_DIM_V1],
            );
        }
        let tape = two_layer_forward(
            parameters,
            ACTION_REF_SPEC,
            action_ref_input,
            counts.action_ref_count,
        )?;
        add_indexed_rows(
            &mut action_ref_pooled,
            &tape.second_output,
            &action_ref_action_indices,
        );
        Some(tape)
    };

    let mut action_input = Vec::with_capacity(counts.action_count * ACTION_ENCODER_INPUT);
    for action in 0..counts.action_count {
        let feature_begin = action * ACTION_FEATURE_DIM_V1;
        action_input.extend_from_slice(
            &encoded.action_features[feature_begin..feature_begin + ACTION_FEATURE_DIM_V1],
        );
        let pooled_begin = action * HIDDEN_DIM_V1;
        action_input
            .extend_from_slice(&action_ref_pooled[pooled_begin..pooled_begin + HIDDEN_DIM_V1]);
    }
    let action_encoder =
        two_layer_forward(parameters, ACTION_SPEC, action_input, counts.action_count)?;

    let mut scorer_input = Vec::with_capacity(counts.action_count * SCORER_INPUT);
    for action in 0..counts.action_count {
        scorer_input.extend_from_slice(&state_encoder.second_output);
        let action_begin = action * HIDDEN_DIM_V1;
        scorer_input.extend_from_slice(
            &action_encoder.second_output[action_begin..action_begin + HIDDEN_DIM_V1],
        );
    }
    let mut scorer_hidden = linear_forward(
        parameters,
        SCORER_FIRST_WEIGHT,
        SCORER_FIRST_BIAS,
        &scorer_input,
        counts.action_count,
        SCORER_INPUT,
        HIDDEN_DIM_V1,
    )?;
    tanh_in_place(&mut scorer_hidden);
    let logits = linear_forward(
        parameters,
        SCORER_SECOND_WEIGHT,
        SCORER_SECOND_BIAS,
        &scorer_hidden,
        counts.action_count,
        HIDDEN_DIM_V1,
        1,
    )?;

    let mut value_hidden = linear_forward(
        parameters,
        VALUE_FIRST_WEIGHT,
        VALUE_FIRST_BIAS,
        &state_encoder.second_output,
        1,
        HIDDEN_DIM_V1,
        HIDDEN_DIM_V1,
    )?;
    tanh_in_place(&mut value_hidden);
    let value = linear_forward(
        parameters,
        VALUE_SECOND_WEIGHT,
        VALUE_SECOND_BIAS,
        &value_hidden,
        1,
        HIDDEN_DIM_V1,
        1,
    )?[0];
    validate_finite_slice("forward_logits", &logits)?;
    finite_scalar("forward_value", 0, value)?;

    Ok(DecisionTapeV1 {
        object_card_ids,
        object_groups,
        edge_source_indices,
        edge_target_indices,
        action_ref_action_indices,
        action_ref_node_indices,
        object_encoder,
        edge_encoder,
        node_update,
        state_encoder,
        action_ref_encoder,
        action_encoder,
        scorer_input,
        scorer_hidden,
        value_hidden,
        logits,
        value,
    })
}

fn reverse_decision(
    parameters: &[NativeNamedParameterV1],
    gradients: &mut [Vec<f32>],
    tape: &DecisionTapeV1,
    d_logits: &[f32],
    d_value: f32,
) -> Result<(), NativePolicyTrainErrorV1> {
    let action_count = tape.logits.len();
    if d_logits.len() != action_count {
        return Err(NativePolicyTrainErrorV1::ParameterManifest);
    }
    let object_count = tape.object_card_ids.len();

    let mut d_scorer_hidden = linear_backward(
        parameters,
        gradients,
        SCORER_SECOND_WEIGHT,
        SCORER_SECOND_BIAS,
        &tape.scorer_hidden,
        action_count,
        HIDDEN_DIM_V1,
        1,
        d_logits,
    )?;
    apply_tanh_gradient(&tape.scorer_hidden, &mut d_scorer_hidden)?;
    let d_scorer_input = linear_backward(
        parameters,
        gradients,
        SCORER_FIRST_WEIGHT,
        SCORER_FIRST_BIAS,
        &tape.scorer_input,
        action_count,
        SCORER_INPUT,
        HIDDEN_DIM_V1,
        &d_scorer_hidden,
    )?;
    let mut d_state_hidden = vec![0.0; HIDDEN_DIM_V1];
    let mut d_action_hidden = vec![0.0; action_count * HIDDEN_DIM_V1];
    for action in 0..action_count {
        let source = action * SCORER_INPUT;
        for hidden in 0..HIDDEN_DIM_V1 {
            d_state_hidden[hidden] += d_scorer_input[source + hidden];
            d_action_hidden[action * HIDDEN_DIM_V1 + hidden] =
                d_scorer_input[source + HIDDEN_DIM_V1 + hidden];
        }
    }

    let d_value_output = [d_value];
    let mut d_value_hidden = linear_backward(
        parameters,
        gradients,
        VALUE_SECOND_WEIGHT,
        VALUE_SECOND_BIAS,
        &tape.value_hidden,
        1,
        HIDDEN_DIM_V1,
        1,
        &d_value_output,
    )?;
    apply_tanh_gradient(&tape.value_hidden, &mut d_value_hidden)?;
    let d_state_value = linear_backward(
        parameters,
        gradients,
        VALUE_FIRST_WEIGHT,
        VALUE_FIRST_BIAS,
        &tape.state_encoder.second_output,
        1,
        HIDDEN_DIM_V1,
        HIDDEN_DIM_V1,
        &d_value_hidden,
    )?;
    add_slices(&mut d_state_hidden, &d_state_value)?;

    let d_action_input = two_layer_backward(
        parameters,
        gradients,
        ACTION_SPEC,
        &tape.action_encoder,
        &d_action_hidden,
    )?;
    let mut d_action_ref_pooled = vec![0.0; action_count * HIDDEN_DIM_V1];
    for action in 0..action_count {
        let source = action * ACTION_ENCODER_INPUT + ACTION_FEATURE_DIM_V1;
        let destination = action * HIDDEN_DIM_V1;
        d_action_ref_pooled[destination..destination + HIDDEN_DIM_V1]
            .copy_from_slice(&d_action_input[source..source + HIDDEN_DIM_V1]);
    }

    let mut d_object_hidden = vec![0.0; object_count * HIDDEN_DIM_V1];
    if let Some(action_ref_tape) = &tape.action_ref_encoder {
        let action_ref_count = tape.action_ref_action_indices.len();
        let mut d_action_ref_hidden = vec![0.0; action_ref_count * HIDDEN_DIM_V1];
        for action_ref in 0..action_ref_count {
            let source = tape.action_ref_action_indices[action_ref] * HIDDEN_DIM_V1;
            let destination = action_ref * HIDDEN_DIM_V1;
            d_action_ref_hidden[destination..destination + HIDDEN_DIM_V1]
                .copy_from_slice(&d_action_ref_pooled[source..source + HIDDEN_DIM_V1]);
        }
        let d_action_ref_input = two_layer_backward(
            parameters,
            gradients,
            ACTION_REF_SPEC,
            action_ref_tape,
            &d_action_ref_hidden,
        )?;
        for action_ref in 0..action_ref_count {
            let source = action_ref * ACTION_REF_ENCODER_INPUT + ACTION_REF_FEATURE_DIM_V1;
            let destination = tape.action_ref_node_indices[action_ref] * HIDDEN_DIM_V1;
            for hidden in 0..HIDDEN_DIM_V1 {
                d_object_hidden[destination + hidden] += d_action_ref_input[source + hidden];
            }
        }
    }

    let d_state_input = two_layer_backward(
        parameters,
        gradients,
        STATE_SPEC,
        &tape.state_encoder,
        &d_state_hidden,
    )?;
    for (object, group) in tape.object_groups.iter().copied().enumerate() {
        let source = STATE_DIM_V1 + group * HIDDEN_DIM_V1;
        let destination = object * HIDDEN_DIM_V1;
        for hidden in 0..HIDDEN_DIM_V1 {
            d_object_hidden[destination + hidden] += d_state_input[source + hidden];
        }
    }

    let d_node_input = two_layer_backward(
        parameters,
        gradients,
        NODE_SPEC,
        &tape.node_update,
        &d_object_hidden,
    )?;
    let mut d_object_base = vec![0.0; object_count * HIDDEN_DIM_V1];
    let mut d_edge_pooled = vec![0.0; object_count * HIDDEN_DIM_V1];
    for object in 0..object_count {
        let source = object * NODE_UPDATE_INPUT;
        let destination = object * HIDDEN_DIM_V1;
        d_object_base[destination..destination + HIDDEN_DIM_V1]
            .copy_from_slice(&d_node_input[source..source + HIDDEN_DIM_V1]);
        d_edge_pooled[destination..destination + HIDDEN_DIM_V1]
            .copy_from_slice(&d_node_input[source + HIDDEN_DIM_V1..source + NODE_UPDATE_INPUT]);
    }

    if let Some(edge_tape) = &tape.edge_encoder {
        let edge_count = tape.edge_source_indices.len();
        let mut d_edge_hidden = vec![0.0; edge_count * HIDDEN_DIM_V1];
        for edge in 0..edge_count {
            let destination = edge * HIDDEN_DIM_V1;
            let source_row = tape.edge_source_indices[edge] * HIDDEN_DIM_V1;
            let target_row = tape.edge_target_indices[edge] * HIDDEN_DIM_V1;
            for hidden in 0..HIDDEN_DIM_V1 {
                d_edge_hidden[destination + hidden] =
                    d_edge_pooled[source_row + hidden] + d_edge_pooled[target_row + hidden];
            }
        }
        let d_edge_input =
            two_layer_backward(parameters, gradients, EDGE_SPEC, edge_tape, &d_edge_hidden)?;
        for edge in 0..edge_count {
            let source = edge * EDGE_ENCODER_INPUT + EDGE_FEATURE_DIM_V1;
            let source_destination = tape.edge_source_indices[edge] * HIDDEN_DIM_V1;
            let target_destination = tape.edge_target_indices[edge] * HIDDEN_DIM_V1;
            for hidden in 0..HIDDEN_DIM_V1 {
                d_object_base[source_destination + hidden] += d_edge_input[source + hidden];
                d_object_base[target_destination + hidden] +=
                    d_edge_input[source + HIDDEN_DIM_V1 + hidden];
            }
        }
    }

    let d_object_input = two_layer_backward(
        parameters,
        gradients,
        OBJECT_SPEC,
        &tape.object_encoder,
        &d_object_base,
    )?;
    for (object, token) in tape.object_card_ids.iter().copied().enumerate() {
        if token == 0 {
            continue;
        }
        let source = object * OBJECT_ENCODER_INPUT + OBJECT_FEATURE_DIM_V1;
        let destination = token * CARD_EMBEDDING_DIM_V1;
        for embedding in 0..CARD_EMBEDDING_DIM_V1 {
            gradients[CARD_EMBEDDING][destination + embedding] +=
                d_object_input[source + embedding];
        }
    }
    Ok(())
}

fn two_layer_forward(
    parameters: &[NativeNamedParameterV1],
    spec: TwoLayerSpecV1,
    input: Vec<f32>,
    rows: usize,
) -> Result<TwoLayerTapeV1, NativePolicyTrainErrorV1> {
    let mut first_output = linear_forward(
        parameters,
        spec.first_weight,
        spec.first_bias,
        &input,
        rows,
        spec.input_dim,
        HIDDEN_DIM_V1,
    )?;
    tanh_in_place(&mut first_output);
    let mut second_output = linear_forward(
        parameters,
        spec.second_weight,
        spec.second_bias,
        &first_output,
        rows,
        HIDDEN_DIM_V1,
        HIDDEN_DIM_V1,
    )?;
    tanh_in_place(&mut second_output);
    Ok(TwoLayerTapeV1 {
        input,
        first_output,
        second_output,
        rows,
    })
}

fn two_layer_backward(
    parameters: &[NativeNamedParameterV1],
    gradients: &mut [Vec<f32>],
    spec: TwoLayerSpecV1,
    tape: &TwoLayerTapeV1,
    d_output: &[f32],
) -> Result<Vec<f32>, NativePolicyTrainErrorV1> {
    let mut d_second_pre = d_output.to_vec();
    apply_tanh_gradient(&tape.second_output, &mut d_second_pre)?;
    let mut d_first_output = linear_backward(
        parameters,
        gradients,
        spec.second_weight,
        spec.second_bias,
        &tape.first_output,
        tape.rows,
        HIDDEN_DIM_V1,
        HIDDEN_DIM_V1,
        &d_second_pre,
    )?;
    apply_tanh_gradient(&tape.first_output, &mut d_first_output)?;
    linear_backward(
        parameters,
        gradients,
        spec.first_weight,
        spec.first_bias,
        &tape.input,
        tape.rows,
        spec.input_dim,
        HIDDEN_DIM_V1,
        &d_first_output,
    )
}

#[allow(clippy::too_many_arguments)]
fn linear_forward(
    parameters: &[NativeNamedParameterV1],
    weight_index: usize,
    bias_index: usize,
    input: &[f32],
    rows: usize,
    input_dim: usize,
    output_dim: usize,
) -> Result<Vec<f32>, NativePolicyTrainErrorV1> {
    let weight = &parameters[weight_index].values;
    let bias = &parameters[bias_index].values;
    if input.len() != rows * input_dim
        || weight.len() != output_dim * input_dim
        || bias.len() != output_dim
    {
        return Err(NativePolicyTrainErrorV1::ParameterManifest);
    }
    let mut output = Vec::with_capacity(rows * output_dim);
    for row in 0..rows {
        let input_begin = row * input_dim;
        for (output_index, bias_value) in bias.iter().copied().enumerate() {
            let weight_begin = output_index * input_dim;
            let mut value = bias_value;
            for input_index in 0..input_dim {
                value += input[input_begin + input_index] * weight[weight_begin + input_index];
            }
            output.push(value);
        }
    }
    validate_finite_slice("linear_forward", &output)?;
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn linear_backward(
    parameters: &[NativeNamedParameterV1],
    gradients: &mut [Vec<f32>],
    weight_index: usize,
    bias_index: usize,
    input: &[f32],
    rows: usize,
    input_dim: usize,
    output_dim: usize,
    d_output: &[f32],
) -> Result<Vec<f32>, NativePolicyTrainErrorV1> {
    let weight = &parameters[weight_index].values;
    if input.len() != rows * input_dim
        || d_output.len() != rows * output_dim
        || weight.len() != output_dim * input_dim
        || gradients[weight_index].len() != weight.len()
        || gradients[bias_index].len() != output_dim
    {
        return Err(NativePolicyTrainErrorV1::ParameterManifest);
    }
    let mut d_input = vec![0.0; rows * input_dim];
    for row in 0..rows {
        let input_begin = row * input_dim;
        let output_begin = row * output_dim;
        for output_index in 0..output_dim {
            let d_value = d_output[output_begin + output_index];
            if weight_index != SCORER_SECOND_WEIGHT {
                gradients[bias_index][output_index] += d_value;
            }
            let weight_begin = output_index * input_dim;
            for input_index in 0..input_dim {
                gradients[weight_index][weight_begin + input_index] +=
                    d_value * input[input_begin + input_index];
                d_input[input_begin + input_index] += d_value * weight[weight_begin + input_index];
            }
        }
    }
    if weight_index == SCORER_SECOND_WEIGHT {
        // Torch's pinned Windows CPU reduction visits the scorer's legal-action
        // rows in reverse storage order for this final scalar bias sum.
        // Combined with reverse autograd graph traversal above, this is full
        // reverse group/substep/action order and retains the observed residual
        // that becomes material under Adam eps=1e-8.  Other linear biases keep
        // their established accumulation path.
        for output_index in 0..output_dim {
            for row in (0..rows).rev() {
                gradients[bias_index][output_index] += d_output[row * output_dim + output_index];
            }
        }
    }
    Ok(d_input)
}

fn selected_log_softmax(
    logits: &[f32],
    selected: usize,
) -> Result<(f32, Vec<f32>), NativePolicyTrainErrorV1> {
    let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut exponential_sum = 0.0f32;
    for logit in logits {
        let exponential = (*logit - maximum).exp();
        exponential_sum += exponential;
    }
    let log_sum = exponential_sum.ln();
    let log_probabilities = logits
        .iter()
        .map(|logit| (*logit - maximum) - log_sum)
        .collect::<Vec<_>>();
    let log_probability = log_probabilities[selected];
    finite_scalar("selected_log_probability", selected, log_probability)?;
    validate_finite_slice("log_probabilities", &log_probabilities)?;
    Ok((log_probability, log_probabilities))
}

type AdamUpdateV1 = (Vec<NativeNamedParameterV1>, Vec<Vec<f32>>, Vec<Vec<f32>>);

fn adam_update(
    parameters: &[NativeNamedParameterV1],
    gradients: &[Vec<f32>],
    first_moments: &[Vec<f32>],
    second_moments: &[Vec<f32>],
    step: u64,
    learning_rate: f32,
) -> Result<AdamUpdateV1, NativePolicyTrainErrorV1> {
    let exponent = i32::try_from(step).map_err(|_| NativePolicyTrainErrorV1::AdamStepOverflow)?;
    let bias_correction1 = 1.0f64 - f64::from(ADAM_BETA1_V1).powi(exponent);
    let bias_correction2 = 1.0f64 - f64::from(ADAM_BETA2_V1).powi(exponent);
    let step_size = (f64::from(learning_rate) / bias_correction1) as f32;
    let bias_correction2_sqrt = bias_correction2.sqrt() as f32;
    finite_scalar("adam_scalar", 0, step_size)?;
    finite_scalar("adam_scalar", 1, bias_correction2_sqrt)?;

    let mut next_parameters = parameters.to_vec();
    let mut next_first = first_moments.to_vec();
    let mut next_second = second_moments.to_vec();
    for parameter_index in 0..parameters.len() {
        for value_index in 0..parameters[parameter_index].values.len() {
            let gradient = gradients[parameter_index][value_index];
            let previous_first = first_moments[parameter_index][value_index];
            let first = previous_first + (gradient - previous_first) * (1.0 - ADAM_BETA1_V1);
            let second = second_moments[parameter_index][value_index] * ADAM_BETA2_V1
                + gradient * gradient * (1.0 - ADAM_BETA2_V1);
            let denominator = second.sqrt() / bias_correction2_sqrt + ADAM_EPSILON_V1;
            let parameter = parameters[parameter_index].values[value_index]
                + (-step_size) * first / denominator;
            if !first.is_finite() || !second.is_finite() || !parameter.is_finite() || second < 0.0 {
                return Err(NativePolicyTrainErrorV1::NonFinite {
                    stage: "adam_update",
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

fn validate_parameter_manifest(
    parameters: &[NativeNamedParameterV1],
) -> Result<(), NativePolicyTrainErrorV1> {
    if parameters.len() != PARAMETER_TENSOR_COUNT
        || parameters
            .iter()
            .zip(EXPECTED_PARAMETER_NAMES)
            .any(|(parameter, expected)| {
                parameter.name != expected
                    || parameter.shape.iter().product::<usize>() != parameter.values.len()
                    || parameter.values.iter().any(|value| !value.is_finite())
            })
        || parameters
            .iter()
            .map(|parameter| parameter.values.len())
            .sum::<usize>()
            != PARAMETER_COUNT_V1
    {
        return Err(NativePolicyTrainErrorV1::ParameterManifest);
    }
    Ok(())
}

fn validate_optimizer_state(
    parameters: &[NativeNamedParameterV1],
    first_moments: &[Vec<f32>],
    second_moments: &[Vec<f32>],
) -> Result<(), NativePolicyTrainErrorV1> {
    if first_moments.len() != parameters.len()
        || second_moments.len() != parameters.len()
        || parameters
            .iter()
            .zip(first_moments)
            .zip(second_moments)
            .any(|((parameter, first), second)| {
                first.len() != parameter.values.len()
                    || second.len() != parameter.values.len()
                    || first.iter().any(|value| !value.is_finite())
                    || second
                        .iter()
                        .any(|value| !value.is_finite() || *value < 0.0)
            })
    {
        return Err(NativePolicyTrainErrorV1::OptimizerState);
    }
    Ok(())
}

fn validate_canonical_gauge_state(
    parameters: &[NativeNamedParameterV1],
    first_moments: &[Vec<f32>],
    second_moments: &[Vec<f32>],
) -> Result<(), NativePolicyTrainErrorV1> {
    if parameters[SCORER_SECOND_BIAS].name != CANONICAL_GAUGE_PARAMETERS_V1[0]
        || parameters[SCORER_SECOND_BIAS].values.len() != 1
        || first_moments[SCORER_SECOND_BIAS].len() != 1
        || second_moments[SCORER_SECOND_BIAS].len() != 1
        || first_moments[SCORER_SECOND_BIAS][0].to_bits() != 0
        || second_moments[SCORER_SECOND_BIAS][0].to_bits() != 0
    {
        return Err(NativePolicyTrainErrorV1::OptimizerState);
    }
    Ok(())
}

fn named_state_snapshot(
    parameters: &[NativeNamedParameterV1],
    values: &[Vec<f32>],
) -> Vec<NativeNamedParameterV1> {
    parameters
        .iter()
        .zip(values)
        .map(|(parameter, values)| NativeNamedParameterV1 {
            name: parameter.name,
            shape: parameter.shape.clone(),
            values: values.clone(),
        })
        .collect()
}

fn add_indexed_rows(destination: &mut [f32], source: &[f32], indices: &[usize]) {
    for (row, index) in indices.iter().copied().enumerate() {
        let source_begin = row * HIDDEN_DIM_V1;
        let destination_begin = index * HIDDEN_DIM_V1;
        for hidden in 0..HIDDEN_DIM_V1 {
            destination[destination_begin + hidden] += source[source_begin + hidden];
        }
    }
}

fn tanh_in_place(values: &mut [f32]) {
    for value in values {
        *value = value.tanh();
    }
}

fn apply_tanh_gradient(
    activation: &[f32],
    gradient: &mut [f32],
) -> Result<(), NativePolicyTrainErrorV1> {
    if activation.len() != gradient.len() {
        return Err(NativePolicyTrainErrorV1::ParameterManifest);
    }
    for (activation, gradient) in activation.iter().zip(gradient) {
        *gradient *= 1.0 - activation * activation;
    }
    Ok(())
}

fn add_slices(destination: &mut [f32], source: &[f32]) -> Result<(), NativePolicyTrainErrorV1> {
    if destination.len() != source.len() {
        return Err(NativePolicyTrainErrorV1::ParameterManifest);
    }
    for (destination, source) in destination.iter_mut().zip(source) {
        *destination += source;
    }
    Ok(())
}

fn validate_finite_slice(
    stage: &'static str,
    values: &[f32],
) -> Result<(), NativePolicyTrainErrorV1> {
    if let Some((index, _)) = values
        .iter()
        .enumerate()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(NativePolicyTrainErrorV1::NonFinite { stage, index });
    }
    Ok(())
}

fn validate_finite_nested(
    stage: &'static str,
    values: &[Vec<f32>],
) -> Result<(), NativePolicyTrainErrorV1> {
    let mut offset = 0usize;
    for value in values {
        if let Some((index, _)) = value
            .iter()
            .enumerate()
            .find(|(_, value)| !value.is_finite())
        {
            return Err(NativePolicyTrainErrorV1::NonFinite {
                stage,
                index: offset + index,
            });
        }
        offset += value.len();
    }
    Ok(())
}

fn finite_scalar(
    stage: &'static str,
    index: usize,
    value: f32,
) -> Result<(), NativePolicyTrainErrorV1> {
    if !value.is_finite() {
        return Err(NativePolicyTrainErrorV1::NonFinite { stage, index });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_policy_value_net_v1::{
        NativeEncodedDecisionSchemaV1, NativePolicyValueModelConfigV1,
    };
    use serde::Deserialize;
    use sha2::{Digest, Sha256};

    const FORWARD_JSON: &str =
        include_str!("../../data/native_policy_value_net_v1/runner_fixed_forward_goldens_v1.json");
    const TRAIN_JSON: &str = include_str!(
        "../../data/native_policy_train_step_v1/runner_fixed_train_step_goldens_v1.json"
    );
    const TRAIN_FIXTURE_SHA256: &str =
        "6b7444a3b9640e943d6127ecadaa71be8ac72b7d134cdcd24709db8578ff1769";
    const MODEL_AUTHORITY: &[u8] = include_bytes!("../../python/mtg_kernel_rl/model.py");
    const TRAINER_AUTHORITY: &[u8] = include_bytes!("../../python/mtg_kernel_rl/trainer.py");
    const FORWARD_AUTHORITY: &[u8] = include_bytes!(
        "../../data/native_policy_value_net_v1/runner_fixed_forward_goldens_v1.json"
    );

    #[derive(Debug, Deserialize)]
    struct ForwardFixture {
        cases: Vec<ForwardCase>,
    }

    #[derive(Debug, Deserialize)]
    struct ForwardCase {
        name: String,
        state: Vec<f32>,
        object_features: Vec<f32>,
        object_card_ids: Vec<i64>,
        object_groups: Vec<i64>,
        object_node_ids: Vec<i64>,
        edge_features: Vec<f32>,
        edge_source_indices: Vec<i64>,
        edge_target_indices: Vec<i64>,
        action_features: Vec<f32>,
        action_ref_features: Vec<f32>,
        action_ref_card_ids: Vec<i64>,
        action_ref_action_indices: Vec<i64>,
        action_ref_node_indices: Vec<i64>,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenFixture {
        schema: String,
        identity: String,
        authority: GoldenAuthority,
        optimizer: GoldenOptimizer,
        value_coefficient: f32,
        initial_parameters: GoldenTensorState,
        steps: Vec<GoldenStep>,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenAuthority {
        model_sha256: String,
        trainer_sha256: String,
        forward_fixture_sha256: String,
        numerical_claim: String,
        exact_authority_scope: String,
        portable_check_scope: String,
        authority_platform_system: String,
        authority_platform_machine: String,
        authority_python_version: String,
        selected_output_absolute_tolerance: f32,
        selected_output_relative_tolerance: f32,
        loss_absolute_tolerance: f32,
        loss_relative_tolerance: f32,
        gradient_absolute_tolerance: f32,
        gradient_relative_tolerance: f32,
        optimizer_absolute_tolerance: f32,
        optimizer_relative_tolerance: f32,
        torch_num_threads: usize,
        torch_num_interop_threads: usize,
        torch_deterministic_algorithms: bool,
        torch_default_dtype: String,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenOptimizer {
        name: String,
        identity: String,
        trainer_algorithm: String,
        canonical_gauge_parameters: Vec<String>,
        legacy_gauge_nonclaim: String,
        exact_arithmetic_proof: String,
        value_head_gauge: String,
        learning_rate: f32,
        betas: [f32; 2],
        epsilon: f32,
        weight_decay: f32,
        amsgrad: bool,
        foreach: bool,
        maximize: bool,
        capturable: bool,
        differentiable: bool,
        fused: bool,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenStep {
        step: u64,
        groups: Vec<GoldenGroup>,
        selected_outputs: Vec<GoldenSelectedOutput>,
        loss: GoldenLoss,
        scorer_bias_gauge: GoldenScorerBiasGauge,
        gradients_before_adam: GoldenTensorState,
        first_moments_after_adam: GoldenTensorState,
        second_moments_after_adam: GoldenTensorState,
        parameters_after_adam: GoldenTensorState,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenGroup {
        terminal_return: i8,
        substeps: Vec<GoldenSubstep>,
        joint_log_probability: GoldenScalar,
        value_from_substep_zero: GoldenScalar,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenSubstep {
        case: String,
        selected_action_index: usize,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenSelectedOutput {
        group_index: usize,
        substep_index: usize,
        selected_action_index: usize,
        selected_logit: GoldenScalar,
        value: GoldenScalar,
        selected_log_probability: GoldenScalar,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenLoss {
        policy_sum: GoldenScalar,
        value_sum: GoldenScalar,
        loss: GoldenScalar,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenScalar {
        value: f32,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenTensorState {
        sha256: String,
        tensor_count: usize,
        element_count: usize,
        ordered: Vec<GoldenTensor>,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenTensor {
        name: String,
        shape: Vec<usize>,
        count: usize,
        sha256_f32_le: String,
        statistics: GoldenStatistics,
        probes: Vec<GoldenProbe>,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenStatistics {
        sum_f64: f64,
        sum_abs_f64: f64,
        sum_squares_f64: f64,
        minimum_f32: f32,
        maximum_f32: f32,
        nonzero_count: usize,
        nonzero_witness_floor: f32,
        nonzero_witness_count: usize,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenProbe {
        index: usize,
        value: f32,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenScorerBiasGauge {
        parameter_name: String,
        substep_count: usize,
        total_action_count: usize,
        max_action_count: usize,
        sum_abs_policy_coefficients_f64: f64,
        substep_bounds: Vec<GoldenGaugeSubstepBound>,
        per_substep_bound_sum_f64: f64,
        cross_substep_bound_f64: f64,
        derived_absolute_bound_f64: f64,
        raw_gradient_residual: GoldenScalar,
        high_precision_residual_f64: f64,
        canonical_gradient: GoldenScalar,
        parameter_before_bits: String,
        parameter_after_bits: String,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenGaugeSubstepBound {
        action_count: usize,
        abs_policy_coefficient_f64: f64,
        gamma_operation_count: usize,
        gamma_f64: f64,
        bound_component_f64: f64,
    }

    fn fixtures() -> (ForwardFixture, GoldenFixture) {
        (
            serde_json::from_str(FORWARD_JSON).expect("forward fixture parses"),
            serde_json::from_str(TRAIN_JSON).expect("training fixture parses"),
        )
    }

    fn encoded(case: &ForwardCase) -> NativeEncodedDecisionViewV1<'_> {
        NativeEncodedDecisionViewV1::from_slices_unvalidated(
            NativeEncodedDecisionSchemaV1::contract_v1(),
            &case.state,
            &case.object_features,
            &case.object_card_ids,
            &case.object_groups,
            &case.object_node_ids,
            &case.edge_features,
            &case.edge_source_indices,
            &case.edge_target_indices,
            &case.action_features,
            &case.action_ref_features,
            &case.action_ref_card_ids,
            &case.action_ref_action_indices,
            &case.action_ref_node_indices,
        )
    }

    fn case_by_name<'a>(fixture: &'a ForwardFixture, name: &str) -> &'a ForwardCase {
        fixture
            .cases
            .iter()
            .find(|case| case.name == name)
            .expect("golden case exists")
    }

    fn substeps_for_step<'a>(
        forward: &'a ForwardFixture,
        step: &'a GoldenStep,
    ) -> Vec<Vec<NativePolicySubstepV1<'a>>> {
        step.groups
            .iter()
            .map(|group| {
                group
                    .substeps
                    .iter()
                    .map(|substep| NativePolicySubstepV1 {
                        encoded: encoded(case_by_name(forward, &substep.case)),
                        selected_action_index: substep.selected_action_index,
                    })
                    .collect()
            })
            .collect()
    }

    fn groups_for_step<'a>(
        step: &'a GoldenStep,
        substeps: &'a [Vec<NativePolicySubstepV1<'a>>],
    ) -> Vec<NativePolicyPhysicalDecisionV1<'a>> {
        step.groups
            .iter()
            .zip(substeps)
            .map(|(group, substeps)| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return: group.terminal_return,
            })
            .collect()
    }

    fn assert_close(actual: f32, expected: f32, absolute: f32, relative: f32) {
        let tolerance = absolute + relative * expected.abs();
        assert!(
            (actual - expected).abs() <= tolerance,
            "actual={actual:?} expected={expected:?} delta={:?} tolerance={tolerance:?}",
            (actual - expected).abs()
        );
    }

    fn assert_close_f64(actual: f64, expected: f64, absolute: f64, relative: f64) {
        let tolerance = absolute + relative * expected.abs();
        assert!(
            (actual - expected).abs() <= tolerance,
            "actual={actual:?} expected={expected:?} delta={:?} tolerance={tolerance:?}",
            (actual - expected).abs()
        );
    }

    fn is_sha256(value: &str) -> bool {
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    fn assert_tensor_state(
        actual: &[NativeNamedParameterV1],
        expected: &GoldenTensorState,
        absolute: f32,
        relative: f32,
    ) {
        assert!(is_sha256(&expected.sha256));
        assert_eq!(actual.len(), expected.tensor_count);
        assert_eq!(
            actual
                .iter()
                .map(|tensor| tensor.values.len())
                .sum::<usize>(),
            expected.element_count
        );
        assert_eq!(actual.len(), expected.ordered.len());
        for (actual, expected) in actual.iter().zip(&expected.ordered) {
            assert_eq!(actual.name, expected.name);
            assert_eq!(actual.shape, expected.shape);
            assert_eq!(actual.values.len(), expected.count);
            assert!(is_sha256(&expected.sha256_f32_le));
            let nonzero_count = actual
                .values
                .iter()
                .filter(|value| value.to_bits() & 0x7fff_ffff != 0)
                .count();
            assert!(expected.statistics.nonzero_count <= expected.count);
            let witness_nonzero_count = actual
                .values
                .iter()
                .filter(|value| value.abs() > expected.statistics.nonzero_witness_floor)
                .count();
            assert!(witness_nonzero_count <= nonzero_count);
            assert!(expected.statistics.nonzero_witness_count <= expected.statistics.nonzero_count);
            assert_eq!(
                nonzero_count,
                expected.statistics.nonzero_count,
                "raw nonzero-count mismatch for {}: Rust first={:?}, Torch min={:?} max={:?}",
                actual.name,
                actual.values.first(),
                expected.statistics.minimum_f32,
                expected.statistics.maximum_f32,
            );
            let minimum = actual.values.iter().copied().fold(f32::INFINITY, f32::min);
            let maximum = actual
                .values
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            assert_close(minimum, expected.statistics.minimum_f32, absolute, relative);
            assert_close(maximum, expected.statistics.maximum_f32, absolute, relative);
            let sum = actual
                .values
                .iter()
                .map(|value| f64::from(*value))
                .sum::<f64>();
            let sum_abs = actual
                .values
                .iter()
                .map(|value| f64::from(*value).abs())
                .sum::<f64>();
            let sum_squares = actual
                .values
                .iter()
                .map(|value| f64::from(*value) * f64::from(*value))
                .sum::<f64>();
            let count = actual.values.len() as f64;
            assert_close_f64(
                sum / count,
                expected.statistics.sum_f64 / count,
                f64::from(absolute),
                f64::from(relative),
            );
            assert_close_f64(
                sum_abs / count,
                expected.statistics.sum_abs_f64 / count,
                f64::from(absolute),
                f64::from(relative),
            );
            assert_close_f64(
                sum_squares / count,
                expected.statistics.sum_squares_f64 / count,
                f64::from(absolute),
                f64::from(relative),
            );
            for probe in &expected.probes {
                assert!(probe.index < actual.values.len());
                assert_close(actual.values[probe.index], probe.value, absolute, relative);
            }
        }
    }

    fn digest_bytes(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn assert_gauge_record(
        actual: &NativeScorerBiasGaugeRecordV1,
        expected: &GoldenScorerBiasGauge,
    ) -> bool {
        assert_eq!(actual.parameter_name, expected.parameter_name);
        assert_eq!(actual.parameter_name, CANONICAL_GAUGE_PARAMETERS_V1[0]);
        assert_eq!(actual.substep_count, expected.substep_count);
        assert_eq!(actual.substep_bounds.len(), actual.substep_count);
        assert_eq!(expected.substep_bounds.len(), expected.substep_count);

        let mut reconstructed_total_action_count = 0usize;
        let mut reconstructed_max_action_count = 0usize;
        let mut reconstructed_sum_abs_coefficients = 0.0f64;
        let mut reconstructed_per_substep_bound = 0.0f64;
        for (actual_bound, expected_bound) in
            actual.substep_bounds.iter().zip(&expected.substep_bounds)
        {
            assert_eq!(actual_bound.action_count, expected_bound.action_count);
            let expected_operation_count = actual_bound
                .action_count
                .checked_mul(8)
                .and_then(|value| value.checked_add(8))
                .unwrap();
            assert_eq!(actual_bound.gamma_operation_count, expected_operation_count);
            assert_eq!(
                expected_bound.gamma_operation_count,
                expected_operation_count
            );
            let reconstructed_gamma = f32_gamma(expected_operation_count).unwrap();
            assert_eq!(actual_bound.gamma.to_bits(), reconstructed_gamma.to_bits());
            assert_close_f64(
                expected_bound.gamma_f64,
                reconstructed_gamma,
                1.0e-18,
                1.0e-15,
            );
            assert_close_f64(
                actual_bound.abs_policy_coefficient,
                expected_bound.abs_policy_coefficient_f64,
                5.0e-5,
                5.0e-5,
            );
            assert_eq!(
                actual_bound.bound_component.to_bits(),
                (actual_bound.abs_policy_coefficient * reconstructed_gamma).to_bits()
            );
            assert_close_f64(
                expected_bound.bound_component_f64,
                expected_bound.abs_policy_coefficient_f64 * expected_bound.gamma_f64,
                1.0e-18,
                1.0e-15,
            );

            reconstructed_total_action_count = reconstructed_total_action_count
                .checked_add(actual_bound.action_count)
                .unwrap();
            reconstructed_max_action_count =
                reconstructed_max_action_count.max(actual_bound.action_count);
            reconstructed_sum_abs_coefficients += actual_bound.abs_policy_coefficient;
            reconstructed_per_substep_bound += actual_bound.bound_component;
        }
        assert_eq!(actual.total_action_count, reconstructed_total_action_count);
        assert_eq!(actual.total_action_count, expected.total_action_count);
        assert_eq!(actual.max_action_count, reconstructed_max_action_count);
        assert_eq!(actual.max_action_count, expected.max_action_count);
        assert_eq!(
            actual.sum_abs_policy_coefficients.to_bits(),
            reconstructed_sum_abs_coefficients.to_bits()
        );
        assert_close_f64(
            actual.sum_abs_policy_coefficients,
            expected.sum_abs_policy_coefficients_f64,
            5.0e-5,
            5.0e-5,
        );
        assert_eq!(
            actual.per_substep_bound_sum.to_bits(),
            reconstructed_per_substep_bound.to_bits()
        );
        assert_close_f64(
            expected.per_substep_bound_sum_f64,
            expected
                .substep_bounds
                .iter()
                .map(|bound| bound.bound_component_f64)
                .sum::<f64>(),
            1.0e-18,
            1.0e-15,
        );

        let cross_gamma = f32_gamma(actual.substep_count.saturating_sub(1)).unwrap();
        let reconstructed_cross_bound = cross_gamma * 2.0 * actual.sum_abs_policy_coefficients;
        assert_eq!(
            actual.cross_substep_bound.to_bits(),
            reconstructed_cross_bound.to_bits()
        );
        let expected_cross_bound = cross_gamma * 2.0 * expected.sum_abs_policy_coefficients_f64;
        assert_close_f64(
            expected.cross_substep_bound_f64,
            expected_cross_bound,
            1.0e-18,
            1.0e-15,
        );
        assert_eq!(
            actual.derived_absolute_bound.to_bits(),
            (actual.per_substep_bound_sum + actual.cross_substep_bound).to_bits()
        );
        assert_close_f64(
            expected.derived_absolute_bound_f64,
            expected.per_substep_bound_sum_f64 + expected.cross_substep_bound_f64,
            1.0e-18,
            1.0e-15,
        );
        assert!(f64::from(actual.raw_gradient_residual).abs() <= actual.derived_absolute_bound);
        assert!(
            f64::from(expected.raw_gradient_residual.value).abs()
                <= expected.derived_absolute_bound_f64
        );
        assert!(
            f64::from(actual.raw_gradient_residual - expected.raw_gradient_residual.value).abs()
                <= actual.derived_absolute_bound + expected.derived_absolute_bound_f64,
            "cross-lane scorer-bias residuals exceed the sum of their independently derived bounds"
        );
        assert_eq!(actual.canonical_gradient.to_bits(), 0);
        assert_eq!(expected.canonical_gradient.value.to_bits(), 0);
        assert_eq!(actual.parameter_before_bits, actual.parameter_after_bits);
        assert_eq!(
            expected.parameter_before_bits,
            expected.parameter_after_bits
        );
        assert_eq!(
            format!("0x{:08x}", actual.parameter_before_bits),
            expected.parameter_before_bits
        );
        assert!(actual.high_precision_residual.is_finite());
        assert!(expected.high_precision_residual_f64.is_finite());
        let actual_shrinks = actual.raw_gradient_residual.to_bits() & 0x7fff_ffff != 0
            && actual.high_precision_residual.abs() < f64::from(actual.raw_gradient_residual).abs();
        if actual.raw_gradient_residual.to_bits() & 0x7fff_ffff != 0 {
            assert!(actual_shrinks);
        }
        if expected.raw_gradient_residual.value.to_bits() & 0x7fff_ffff != 0 {
            assert!(
                expected.high_precision_residual_f64.abs()
                    < f64::from(expected.raw_gradient_residual.value).abs()
            );
        }
        actual_shrinks
    }

    #[test]
    fn torch_authority_grouped_loss_backward_and_multistep_adam_match_declared_witnesses() {
        let (forward, golden) = fixtures();
        assert_eq!(
            golden.schema,
            "native-policy-value-cpu-train-step-v1-torch-goldens-v1"
        );
        assert_eq!(digest_bytes(TRAIN_JSON.as_bytes()), TRAIN_FIXTURE_SHA256);
        assert_eq!(golden.identity, TRAIN_STEP_IDENTITY_V1);
        assert_eq!(TRAINER_ALGORITHM_V1, "terminal_reinforce_value/v3");
        assert_eq!(digest_bytes(MODEL_AUTHORITY), golden.authority.model_sha256);
        assert_eq!(
            digest_bytes(TRAINER_AUTHORITY),
            golden.authority.trainer_sha256
        );
        assert_eq!(
            digest_bytes(FORWARD_AUTHORITY),
            golden.authority.forward_fixture_sha256
        );
        assert!(golden
            .authority
            .numerical_claim
            .contains("no every-Rust-element"));
        assert!(golden.authority.numerical_claim.contains("SHA-256 bound"));
        assert!(golden
            .authority
            .numerical_claim
            .contains("canonical scorer-bias gauge"));
        assert!(golden
            .authority
            .exact_authority_scope
            .starts_with("--authority-check"));
        assert!(golden
            .authority
            .portable_check_scope
            .contains("without regenerating host-sensitive numerical bytes"));
        assert_eq!(golden.authority.authority_platform_system, "Windows");
        assert_eq!(golden.authority.authority_platform_machine, "AMD64");
        assert_eq!(golden.authority.authority_python_version, "3.13.14");
        assert_eq!(golden.authority.torch_num_threads, 1);
        assert_eq!(golden.authority.torch_num_interop_threads, 1);
        assert!(golden.authority.torch_deterministic_algorithms);
        assert_eq!(golden.authority.torch_default_dtype, "torch.float32");

        let optimizer = &golden.optimizer;
        assert_eq!(optimizer.name, "Adam");
        assert_eq!(optimizer.identity, NATIVE_OPTIMIZER_IDENTITY_V1);
        assert_eq!(optimizer.trainer_algorithm, TRAINER_ALGORITHM_V1);
        assert_eq!(
            optimizer.canonical_gauge_parameters,
            CANONICAL_GAUGE_PARAMETERS_V1
        );
        assert_eq!(optimizer.legacy_gauge_nonclaim, LEGACY_GAUGE_NONCLAIM_V1);
        assert!(optimizer
            .exact_arithmetic_proof
            .contains("translation-invariant"));
        assert!(optimizer.value_head_gauge.starts_with("none;"));
        assert!(!optimizer
            .canonical_gauge_parameters
            .iter()
            .any(|name| name.contains("value_head")));
        assert_eq!(optimizer.betas[0].to_bits(), ADAM_BETA1_V1.to_bits());
        assert_eq!(optimizer.betas[1].to_bits(), ADAM_BETA2_V1.to_bits());
        assert_eq!(optimizer.epsilon.to_bits(), ADAM_EPSILON_V1.to_bits());
        assert_eq!(
            optimizer.weight_decay.to_bits(),
            ADAM_WEIGHT_DECAY_V1.to_bits()
        );
        assert!(!optimizer.amsgrad);
        assert!(!optimizer.foreach);
        assert!(!optimizer.maximize);
        assert!(!optimizer.capturable);
        assert!(!optimizer.differentiable);
        assert!(!optimizer.fused);

        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        assert_eq!(
            model.parameter_manifest_sha256_v1(),
            golden.initial_parameters.sha256
        );
        assert_tensor_state(
            &model.parameter_snapshot_v1(),
            &golden.initial_parameters,
            f32::EPSILON,
            f32::EPSILON,
        );
        let initial_parameter_snapshot = model.parameter_snapshot_v1();
        let initial_scorer_bias_bits =
            initial_parameter_snapshot[SCORER_SECOND_BIAS].values[0].to_bits();
        let initial_value_bias_bits =
            initial_parameter_snapshot[VALUE_SECOND_BIAS].values[0].to_bits();
        let mut state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        let mut high_precision_shrink_witness = false;
        let mut ordinary_value_head_update_witness = false;

        let selected_indices = golden
            .steps
            .iter()
            .flat_map(|step| &step.selected_outputs)
            .map(|selected| selected.selected_action_index)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(selected_indices, [0, 1, 2].into_iter().collect());
        assert!(golden
            .steps
            .iter()
            .flat_map(|step| &step.groups)
            .any(|group| group.substeps.len() > 1));

        for step in &golden.steps {
            let substeps = substeps_for_step(&forward, step);
            let groups = groups_for_step(step, &substeps);
            let result = state
                .train_step_v1(&groups, golden.value_coefficient, optimizer.learning_rate)
                .unwrap();
            assert_eq!(result.adam_step, step.step);
            assert_eq!(state.adam_step_v1(), step.step);
            assert_eq!(result.selected_outputs.len(), step.selected_outputs.len());
            for (actual, expected) in result.selected_outputs.iter().zip(&step.selected_outputs) {
                assert_eq!(actual.group_index, expected.group_index);
                assert_eq!(actual.substep_index, expected.substep_index);
                assert_eq!(actual.selected_action_index, expected.selected_action_index);
                assert_close(
                    actual.selected_logit,
                    expected.selected_logit.value,
                    golden.authority.selected_output_absolute_tolerance,
                    golden.authority.selected_output_relative_tolerance,
                );
                assert_close(
                    actual.value,
                    expected.value.value,
                    golden.authority.selected_output_absolute_tolerance,
                    golden.authority.selected_output_relative_tolerance,
                );
                assert_close(
                    actual.selected_log_probability,
                    expected.selected_log_probability.value,
                    golden.authority.selected_output_absolute_tolerance,
                    golden.authority.selected_output_relative_tolerance,
                );
            }
            assert_eq!(result.physical_terms.len(), step.groups.len());
            for (actual, expected) in result.physical_terms.iter().zip(&step.groups) {
                assert_eq!(actual.terminal_return, expected.terminal_return);
                assert_close(
                    actual.joint_log_probability,
                    expected.joint_log_probability.value,
                    golden.authority.loss_absolute_tolerance,
                    golden.authority.loss_relative_tolerance,
                );
                assert_close(
                    actual.value,
                    expected.value_from_substep_zero.value,
                    golden.authority.selected_output_absolute_tolerance,
                    golden.authority.selected_output_relative_tolerance,
                );
            }
            assert_close(
                result.policy_sum,
                step.loss.policy_sum.value,
                golden.authority.loss_absolute_tolerance,
                golden.authority.loss_relative_tolerance,
            );
            assert_close(
                result.value_sum,
                step.loss.value_sum.value,
                golden.authority.loss_absolute_tolerance,
                golden.authority.loss_relative_tolerance,
            );
            assert_close(
                result.loss,
                step.loss.loss.value,
                golden.authority.loss_absolute_tolerance,
                golden.authority.loss_relative_tolerance,
            );
            assert_tensor_state(
                &result.gradients,
                &step.gradients_before_adam,
                golden.authority.gradient_absolute_tolerance,
                golden.authority.gradient_relative_tolerance,
            );
            high_precision_shrink_witness |=
                assert_gauge_record(&result.scorer_bias_gauge, &step.scorer_bias_gauge);
            assert_tensor_state(
                &state.first_moment_snapshot_v1(),
                &step.first_moments_after_adam,
                golden.authority.optimizer_absolute_tolerance,
                golden.authority.optimizer_relative_tolerance,
            );
            assert_tensor_state(
                &state.second_moment_snapshot_v1(),
                &step.second_moments_after_adam,
                golden.authority.optimizer_absolute_tolerance,
                golden.authority.optimizer_relative_tolerance,
            );
            assert_tensor_state(
                &state.model_v1().parameter_snapshot_v1(),
                &step.parameters_after_adam,
                golden.authority.optimizer_absolute_tolerance,
                golden.authority.optimizer_relative_tolerance,
            );

            assert_eq!(result.gradients[SCORER_SECOND_BIAS].values[0].to_bits(), 0);
            assert_eq!(state.first_moments[SCORER_SECOND_BIAS][0].to_bits(), 0);
            assert_eq!(state.second_moments[SCORER_SECOND_BIAS][0].to_bits(), 0);
            assert_eq!(
                state.model_v1().parameter_snapshot_v1()[SCORER_SECOND_BIAS].values[0].to_bits(),
                initial_scorer_bias_bits
            );
            ordinary_value_head_update_witness |=
                result.gradients[VALUE_SECOND_BIAS].values[0].to_bits() & 0x7fff_ffff != 0
                    && state.first_moments[VALUE_SECOND_BIAS][0].to_bits() & 0x7fff_ffff != 0
                    && state.model_v1().parameter_snapshot_v1()[VALUE_SECOND_BIAS].values[0]
                        .to_bits()
                        != initial_value_bias_bits;

            assert!(
                result.gradients[CARD_EMBEDDING].values[..CARD_EMBEDDING_DIM_V1]
                    .iter()
                    .all(|value| value.to_bits() == 0)
            );
            assert!(state.first_moments[CARD_EMBEDDING][..CARD_EMBEDDING_DIM_V1]
                .iter()
                .all(|value| value.to_bits() == 0));
            assert!(
                state.second_moments[CARD_EMBEDDING][..CARD_EMBEDDING_DIM_V1]
                    .iter()
                    .all(|value| value.to_bits() == 0)
            );
            assert!(
                state.model_v1().parameter_snapshot_v1()[CARD_EMBEDDING].values
                    [..CARD_EMBEDDING_DIM_V1]
                    .iter()
                    .all(|value| value.to_bits() == 0)
            );
        }
        assert!(high_precision_shrink_witness);
        assert!(ordinary_value_head_update_witness);
    }

    fn loss_only_with_frozen_advantages(
        model: &NativePolicyValueNetV1,
        groups: &[NativePolicyPhysicalDecisionV1<'_>],
        value_coefficient: f32,
        frozen_advantages: &[f32],
    ) -> f32 {
        assert_eq!(groups.len(), frozen_advantages.len());
        let mut policy_sum = 0.0f32;
        let mut value_sum = 0.0f32;
        for (group, frozen_advantage) in groups.iter().zip(frozen_advantages) {
            let mut joint = None;
            let mut first_value = None;
            for substep in group.substeps {
                let output = model.forward_v1(substep.encoded).unwrap();
                let (log_probability, _) =
                    selected_log_softmax(&output.logits, substep.selected_action_index).unwrap();
                joint = Some(match joint {
                    None => log_probability,
                    Some(active) => active + log_probability,
                });
                first_value.get_or_insert(output.value);
            }
            let value = first_value.unwrap();
            let target = f32::from(group.terminal_return);
            policy_sum += -joint.unwrap() * frozen_advantage;
            let error = value - target;
            value_sum += error * error;
        }
        (policy_sum + value_coefficient * value_sum) / groups.len() as f32
    }

    fn perturbed_model(
        model: &NativePolicyValueNetV1,
        parameter_name: &str,
        value_index: usize,
        delta: f32,
    ) -> NativePolicyValueNetV1 {
        let mut candidate = model.clone();
        let mut parameters = candidate.parameter_snapshot_v1();
        let parameter = parameters
            .iter_mut()
            .find(|parameter| parameter.name == parameter_name)
            .unwrap();
        parameter.values[value_index] += delta;
        candidate
            .replace_parameter_snapshot_v1(&parameters)
            .unwrap();
        candidate
    }

    #[test]
    fn sampled_central_differences_cover_embedding_scatter_and_both_heads() {
        let (forward, golden) = fixtures();
        let step = &golden.steps[0];
        let substeps = substeps_for_step(&forward, step);
        let groups = groups_for_step(step, &substeps);
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
        let result = state
            .train_step_v1(
                &groups,
                golden.value_coefficient,
                golden.optimizer.learning_rate,
            )
            .unwrap();
        let frozen_advantages = groups
            .iter()
            .map(|group| {
                let output = model.forward_v1(group.substeps[0].encoded).unwrap();
                f32::from(group.terminal_return) - output.value
            })
            .collect::<Vec<_>>();
        let targets = [
            "card_embedding.weight",
            "edge_encoder.0.weight",
            "action_ref_encoder.0.weight",
            "scorer.2.weight",
            "value_head.2.weight",
        ];
        let epsilon = 2.0e-3f32;
        for target in targets {
            let gradient = result
                .gradients
                .iter()
                .find(|parameter| parameter.name == target)
                .unwrap();
            let (index, analytic) = gradient
                .values
                .iter()
                .copied()
                .enumerate()
                .max_by(|left, right| left.1.abs().total_cmp(&right.1.abs()))
                .unwrap();
            assert!(analytic.abs() > 1.0e-7, "{target} witness gradient is zero");
            let positive = perturbed_model(&model, target, index, epsilon);
            let negative = perturbed_model(&model, target, index, -epsilon);
            let numerical = (loss_only_with_frozen_advantages(
                &positive,
                &groups,
                golden.value_coefficient,
                &frozen_advantages,
            ) - loss_only_with_frozen_advantages(
                &negative,
                &groups,
                golden.value_coefficient,
                &frozen_advantages,
            )) / (2.0 * epsilon);
            let tolerance = 8.0e-3 + 6.0e-2 * analytic.abs();
            assert!(
                (numerical - analytic).abs() <= tolerance,
                "central difference mismatch for {target}[{index}]: numerical={numerical:?} analytic={analytic:?} tolerance={tolerance:?}"
            );
        }
    }

    fn assert_state_unchanged(
        state: &NativePolicyValueTrainStateV1,
        parameters: &[NativeNamedParameterV1],
        first: &[NativeNamedParameterV1],
        second: &[NativeNamedParameterV1],
        step: u64,
    ) {
        assert_eq!(state.model_v1().parameter_snapshot_v1(), parameters);
        assert_eq!(state.first_moment_snapshot_v1(), first);
        assert_eq!(state.second_moment_snapshot_v1(), second);
        assert_eq!(state.adam_step_v1(), step);
    }

    #[test]
    fn scorer_bias_gauge_bound_is_derived_and_fail_closed() {
        let logits = [0.25f32, -0.5, 1.0];
        let mut valid = ScorerBiasGaugeAccumulatorV1::default();
        valid.observe(&logits, 2, -0.75).unwrap();
        let record = valid.finish(0.0, (-0.05f32).to_bits()).unwrap();
        assert_eq!(record.substep_count, 1);
        assert_eq!(record.total_action_count, logits.len());
        assert_eq!(record.max_action_count, logits.len());
        assert_eq!(record.substep_bounds.len(), 1);
        let component = &record.substep_bounds[0];
        assert_eq!(component.gamma_operation_count, 8 * logits.len() + 8);
        assert_eq!(
            component.gamma.to_bits(),
            f32_gamma(component.gamma_operation_count)
                .unwrap()
                .to_bits()
        );
        assert_eq!(
            component.bound_component.to_bits(),
            (component.abs_policy_coefficient * component.gamma).to_bits()
        );
        assert_eq!(record.cross_substep_bound.to_bits(), 0);
        assert_eq!(
            record.derived_absolute_bound.to_bits(),
            record.per_substep_bound_sum.to_bits()
        );

        let mut exceeded = ScorerBiasGaugeAccumulatorV1::default();
        exceeded.observe(&logits, 2, -0.75).unwrap();
        assert!(matches!(
            exceeded.finish(1.0, (-0.05f32).to_bits()),
            Err(NativePolicyTrainErrorV1::GaugeResidualExceeded { .. })
        ));
        assert_eq!(
            f32_gamma(1usize << f32::MANTISSA_DIGITS),
            Err(NativePolicyTrainErrorV1::GaugeBoundOverflow)
        );
    }

    #[test]
    fn malformed_groups_shapes_and_nonfinite_updates_are_transactional() {
        let (forward, golden) = fixtures();
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        let parameters = state.model_v1().parameter_snapshot_v1();
        let first = state.first_moment_snapshot_v1();
        let second = state.second_moment_snapshot_v1();

        assert_eq!(
            state.train_step_v1(
                &[],
                golden.value_coefficient,
                golden.optimizer.learning_rate
            ),
            Err(NativePolicyTrainErrorV1::EmptyBatch)
        );
        assert_state_unchanged(&state, &parameters, &first, &second, 0);

        let empty = NativePolicyPhysicalDecisionV1 {
            substeps: &[],
            terminal_return: 0,
        };
        assert_eq!(
            state.train_step_v1(
                &[empty],
                golden.value_coefficient,
                golden.optimizer.learning_rate
            ),
            Err(NativePolicyTrainErrorV1::EmptyPhysicalDecision { group_index: 0 })
        );
        assert_state_unchanged(&state, &parameters, &first, &second, 0);

        let case = case_by_name(&forward, "ordered_edges_and_action_refs");
        let bad_selected = [NativePolicySubstepV1 {
            encoded: encoded(case),
            selected_action_index: usize::MAX,
        }];
        let bad_group = [NativePolicyPhysicalDecisionV1 {
            substeps: &bad_selected,
            terminal_return: 1,
        }];
        assert!(matches!(
            state.train_step_v1(
                &bad_group,
                golden.value_coefficient,
                golden.optimizer.learning_rate
            ),
            Err(NativePolicyTrainErrorV1::SelectedActionOutOfRange { .. })
        ));
        assert_state_unchanged(&state, &parameters, &first, &second, 0);

        let malformed = NativeEncodedDecisionViewV1::from_slices_unvalidated(
            NativeEncodedDecisionSchemaV1::contract_v1(),
            &case.state[..case.state.len() - 1],
            &case.object_features,
            &case.object_card_ids,
            &case.object_groups,
            &case.object_node_ids,
            &case.edge_features,
            &case.edge_source_indices,
            &case.edge_target_indices,
            &case.action_features,
            &case.action_ref_features,
            &case.action_ref_card_ids,
            &case.action_ref_action_indices,
            &case.action_ref_node_indices,
        );
        let malformed_substeps = [NativePolicySubstepV1 {
            encoded: malformed,
            selected_action_index: 0,
        }];
        let malformed_groups = [NativePolicyPhysicalDecisionV1 {
            substeps: &malformed_substeps,
            terminal_return: 0,
        }];
        assert!(matches!(
            state.train_step_v1(
                &malformed_groups,
                golden.value_coefficient,
                golden.optimizer.learning_rate
            ),
            Err(NativePolicyTrainErrorV1::Model(_))
        ));
        assert_state_unchanged(&state, &parameters, &first, &second, 0);

        let valid_substeps = [NativePolicySubstepV1 {
            encoded: encoded(case),
            selected_action_index: 1,
        }];
        let invalid_return = [NativePolicyPhysicalDecisionV1 {
            substeps: &valid_substeps,
            terminal_return: 2,
        }];
        assert_eq!(
            state.train_step_v1(
                &invalid_return,
                golden.value_coefficient,
                golden.optimizer.learning_rate
            ),
            Err(NativePolicyTrainErrorV1::InvalidTerminalReturn {
                group_index: 0,
                value: 2
            })
        );
        assert_state_unchanged(&state, &parameters, &first, &second, 0);

        let valid_groups = [NativePolicyPhysicalDecisionV1 {
            substeps: &valid_substeps,
            terminal_return: 1,
        }];
        assert_eq!(
            state.train_step_v1(&valid_groups, 0.0, golden.optimizer.learning_rate),
            Err(NativePolicyTrainErrorV1::InvalidValueCoefficient)
        );
        assert_eq!(
            state.train_step_v1(&valid_groups, golden.value_coefficient, f32::NAN),
            Err(NativePolicyTrainErrorV1::InvalidLearningRate)
        );
        assert_state_unchanged(&state, &parameters, &first, &second, 0);

        assert!(matches!(
            state.train_step_v1(&valid_groups, golden.value_coefficient, f32::MAX),
            Err(NativePolicyTrainErrorV1::NonFinite {
                stage: "adam_scalar",
                ..
            })
        ));
        assert_state_unchanged(&state, &parameters, &first, &second, 0);

        let mut invalid_gauge_state = NativePolicyValueTrainStateV1::new_v1(
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap(),
        )
        .unwrap();
        invalid_gauge_state.first_moments[SCORER_SECOND_BIAS][0] = f32::EPSILON;
        let invalid_parameters = invalid_gauge_state.model_v1().parameter_snapshot_v1();
        let invalid_first = invalid_gauge_state.first_moment_snapshot_v1();
        let invalid_second = invalid_gauge_state.second_moment_snapshot_v1();
        assert_eq!(
            invalid_gauge_state.train_step_v1(
                &valid_groups,
                golden.value_coefficient,
                golden.optimizer.learning_rate,
            ),
            Err(NativePolicyTrainErrorV1::OptimizerState)
        );
        assert_state_unchanged(
            &invalid_gauge_state,
            &invalid_parameters,
            &invalid_first,
            &invalid_second,
            0,
        );
    }
}
