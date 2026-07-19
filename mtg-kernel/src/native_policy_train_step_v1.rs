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
    CARD_EMBEDDING_DIM_V1, CARD_VOCAB_SIZE_V1, EDGE_FEATURE_DIM_V1, HIDDEN_DIM_V1,
    OBJECT_FEATURE_DIM_V1, OBJECT_GROUP_COUNT_V1, PARAMETER_COUNT_V1, STATE_DIM_V1,
};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::cell::Cell;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(test)]
thread_local! {
    static FORWARD_WITH_TAPE_CALL_COUNT_V1: Cell<u64> = const { Cell::new(0) };
    static PACKED_INDEPENDENT_RECOMPUTE_CALL_COUNT_V1: Cell<u64> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn forward_with_tape_call_count_for_test_v1() -> u64 {
    FORWARD_WITH_TAPE_CALL_COUNT_V1.with(Cell::get)
}

#[cfg(test)]
pub(crate) fn packed_independent_recompute_call_count_for_test_v1() -> u64 {
    PACKED_INDEPENDENT_RECOMPUTE_CALL_COUNT_V1.with(Cell::get)
}

pub(crate) const TRAINER_ALGORITHM_V1: &str = "terminal_reinforce_value/v3";
pub(crate) const TRAIN_STEP_IDENTITY_V1: &str = "native-policy-value-cpu-train-step-v1";
pub const NATIVE_POLICY_TRAIN_STEP_NUMERICAL_BACKEND_IDENTITY_V1: &str =
    "rust-production-native-policy-train-step-v1-cpu-ieee754-binary32-sequential";
pub(crate) const NATIVE_OPTIMIZER_IDENTITY_V1: &str = "native-adam-canonical-scorer-bias-gauge-v1";
pub(crate) const NATIVE_TRAIN_STATE_SHA256_IDENTITY_V1: &str =
    "mtg-kernel-native-policy-value-train-state-sha256-v1";
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

const EXPECTED_PARAMETER_SHAPES: [&[usize]; PARAMETER_TENSOR_COUNT] = [
    &[CARD_VOCAB_SIZE_V1, CARD_EMBEDDING_DIM_V1],
    &[HIDDEN_DIM_V1, OBJECT_ENCODER_INPUT],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, EDGE_ENCODER_INPUT],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, NODE_UPDATE_INPUT],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, STATE_ENCODER_INPUT],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, ACTION_REF_ENCODER_INPUT],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, ACTION_ENCODER_INPUT],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1, SCORER_INPUT],
    &[HIDDEN_DIM_V1],
    &[1, HIDDEN_DIM_V1],
    &[1],
    &[HIDDEN_DIM_V1, HIDDEN_DIM_V1],
    &[HIDDEN_DIM_V1],
    &[1, HIDDEN_DIM_V1],
    &[1],
];

/// Frozen named-parameter order and shapes used by both the in-memory train
/// state validator and the headerless persisted payload codec.
pub(crate) fn native_train_state_parameter_layout_v1(
) -> impl ExactSizeIterator<Item = (&'static str, &'static [usize])> {
    EXPECTED_PARAMETER_NAMES
        .into_iter()
        .zip(EXPECTED_PARAMETER_SHAPES)
}

#[derive(Clone, Debug)]
pub(crate) enum NativePolicyForwardInputV1<'a> {
    Encoded(Box<NativeEncodedDecisionViewV1<'a>>),
    Packed {
        encoded: Box<NativeEncodedDecisionViewV1<'a>>,
        tape: &'a NativePolicyPackedForwardTapeV1,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct NativePolicySubstepV1<'a> {
    pub(crate) forward: NativePolicyForwardInputV1<'a>,
    pub(crate) selected_action_index: usize,
    /// Complete scorer output captured when this exact encoded decision was
    /// sampled. The train-time forward must independently reproduce every row
    /// bit-exactly. A packed backward tape, when present, must separately match
    /// the same transported bits before backward or optimizer work.
    pub(crate) expected_raw_action_logit_bits: &'a [u32],
    pub(crate) expected_value_bits: u32,
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
pub struct NativeScorerBiasGaugeRecordV1 {
    pub parameter_name: &'static str,
    pub substep_count: usize,
    pub total_action_count: usize,
    pub max_action_count: usize,
    pub sum_abs_policy_coefficients: f64,
    pub substep_bounds: Vec<NativeGaugeSubstepBoundV1>,
    pub per_substep_bound_sum: f64,
    pub cross_substep_bound: f64,
    pub raw_gradient_residual: f32,
    pub derived_absolute_bound: f64,
    pub high_precision_residual: f64,
    pub canonical_gradient: f32,
    pub parameter_before_bits: u32,
    pub parameter_after_bits: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeGaugeSubstepBoundV1 {
    pub action_count: usize,
    pub abs_policy_coefficient: f64,
    pub gamma_operation_count: usize,
    pub gamma: f64,
    pub bound_component: f64,
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
    ExpectedLogitCountMismatch {
        group_index: usize,
        substep_index: usize,
        expected: usize,
        actual: usize,
    },
    RecomputedLogitBitsMismatch {
        group_index: usize,
        substep_index: usize,
        action_index: usize,
        selected_action_index: usize,
        expected_bits: u32,
        actual_bits: u32,
    },
    RecomputedValueBitsMismatch {
        group_index: usize,
        substep_index: usize,
        expected_bits: u32,
        actual_bits: u32,
    },
    PackedForwardModelGenerationMismatch {
        group_index: usize,
        substep_index: usize,
    },
    PackedForwardLogitCountMismatch {
        group_index: usize,
        substep_index: usize,
        expected: usize,
        actual: usize,
    },
    PackedForwardLogitBitsMismatch {
        group_index: usize,
        substep_index: usize,
        action_index: usize,
        selected_action_index: usize,
        expected_bits: u32,
        actual_bits: u32,
    },
    PackedForwardValueBitsMismatch {
        group_index: usize,
        substep_index: usize,
        expected_bits: u32,
        actual_bits: u32,
    },
    InvalidValueCoefficient,
    InvalidLearningRate,
    ParameterManifest,
    OptimizerState,
    GaugeAnchor,
    AdamStepOverflow,
    GaugeBoundOverflow,
    GaugeResidualExceeded {
        residual_bits: u32,
        bound_bits: u64,
    },
    GaugeHighPrecisionResidualExceeded {
        residual_bits: u64,
        bound_bits: u64,
    },
    GroupCountNotExactlyRepresentable {
        group_count: usize,
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

/// Complete owned in-memory state of the native model and Adam optimizer.
///
/// This is deliberately not a persisted checkpoint or record schema.  Its
/// ordered named tensors make the otherwise-private optimizer state movable
/// without weakening the model manifest, padding-row, or gauge invariants.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativePolicyValueTrainSnapshotV1 {
    pub(crate) adam_step: u64,
    pub(crate) scorer_bias_anchor_bits: u32,
    pub(crate) parameters: Vec<NativeNamedParameterV1>,
    pub(crate) first_moments: Vec<NativeNamedParameterV1>,
    pub(crate) second_moments: Vec<NativeNamedParameterV1>,
}

impl NativePolicyValueTrainSnapshotV1 {
    pub(crate) fn state_sha256_v1(&self) -> Result<[u8; 32], NativePolicyTrainErrorV1> {
        validate_owned_train_snapshot_v1(self, None)?;
        Ok(hash_owned_train_snapshot_v1(self))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NativePolicyValueTrainStateV1 {
    model: NativePolicyValueNetV1,
    adam_step: u64,
    first_moments: Vec<Vec<f32>>,
    second_moments: Vec<Vec<f32>>,
    scorer_bias_anchor_bits: u32,
}

impl NativePolicyValueTrainStateV1 {
    pub(crate) fn new_v1(model: NativePolicyValueNetV1) -> Result<Self, NativePolicyTrainErrorV1> {
        let parameters = model.parameter_snapshot_v1();
        validate_parameter_manifest(&parameters)?;
        let scorer_bias_anchor_bits = parameters[SCORER_SECOND_BIAS].values[0].to_bits();
        let first_moments = parameters
            .iter()
            .map(|parameter| vec![0.0; parameter.values.len()])
            .collect();
        let second_moments = parameters
            .iter()
            .map(|parameter| vec![0.0; parameter.values.len()])
            .collect();
        let state = Self {
            model,
            adam_step: 0,
            first_moments,
            second_moments,
            scorer_bias_anchor_bits,
        };
        state.validate_state_v1()?;
        Ok(state)
    }

    /// Reconstructs a complete train state against a caller-provided model
    /// template. The template supplies the frozen model contract and gauge
    /// anchor; all snapshot data is validated before a candidate is returned.
    pub(crate) fn from_snapshot_v1(
        model: NativePolicyValueNetV1,
        snapshot: &NativePolicyValueTrainSnapshotV1,
    ) -> Result<Self, NativePolicyTrainErrorV1> {
        let template_parameters = model.parameter_snapshot_v1();
        validate_parameter_manifest(&template_parameters)?;
        let scorer_bias_anchor_bits = template_parameters[SCORER_SECOND_BIAS].values[0].to_bits();
        validate_owned_train_snapshot_v1(snapshot, Some(scorer_bias_anchor_bits))?;

        let mut candidate_model = model;
        candidate_model.replace_parameter_snapshot_v1(&snapshot.parameters)?;
        let candidate = Self {
            model: candidate_model,
            adam_step: snapshot.adam_step,
            first_moments: snapshot
                .first_moments
                .iter()
                .map(|parameter| parameter.values.clone())
                .collect(),
            second_moments: snapshot
                .second_moments
                .iter()
                .map(|parameter| parameter.values.clone())
                .collect(),
            scorer_bias_anchor_bits,
        };
        candidate.validate_state_v1()?;
        Ok(candidate)
    }

    pub(crate) fn model_v1(&self) -> &NativePolicyValueNetV1 {
        &self.model
    }

    pub(crate) fn adam_step_v1(&self) -> u64 {
        self.adam_step
    }

    pub(crate) fn scorer_bias_anchor_f32_bits_v1(&self) -> u32 {
        self.scorer_bias_anchor_bits
    }

    pub(crate) fn first_moment_snapshot_v1(&self) -> Vec<NativeNamedParameterV1> {
        named_state_snapshot(&self.model.parameter_snapshot_v1(), &self.first_moments)
    }

    pub(crate) fn second_moment_snapshot_v1(&self) -> Vec<NativeNamedParameterV1> {
        named_state_snapshot(&self.model.parameter_snapshot_v1(), &self.second_moments)
    }

    /// Returns a complete owned snapshot only after validating the live state.
    pub(crate) fn snapshot_v1(
        &self,
    ) -> Result<NativePolicyValueTrainSnapshotV1, NativePolicyTrainErrorV1> {
        self.validate_state_v1()?;
        let parameters = self.model.parameter_snapshot_v1();
        Ok(NativePolicyValueTrainSnapshotV1 {
            adam_step: self.adam_step,
            scorer_bias_anchor_bits: self.scorer_bias_anchor_bits,
            first_moments: named_state_snapshot(&parameters, &self.first_moments),
            second_moments: named_state_snapshot(&parameters, &self.second_moments),
            parameters,
        })
    }

    /// Domain-separated digest of every ordered parameter and moment bit plus
    /// the Adam step. Invalid live state never receives a digest.
    pub(crate) fn state_sha256_v1(&self) -> Result<[u8; 32], NativePolicyTrainErrorV1> {
        let snapshot = self.snapshot_v1()?;
        Ok(hash_owned_train_snapshot_v1(&snapshot))
    }

    /// Atomically replaces model parameters, moments, and Adam step. The
    /// existing state's separately remembered gauge anchor is authoritative.
    pub(crate) fn replace_snapshot_v1(
        &mut self,
        snapshot: &NativePolicyValueTrainSnapshotV1,
    ) -> Result<(), NativePolicyTrainErrorV1> {
        self.validate_state_v1()?;
        let candidate = Self::from_snapshot_v1(self.model.clone(), snapshot)?;
        *self = candidate;
        Ok(())
    }

    fn validate_state_v1(&self) -> Result<(), NativePolicyTrainErrorV1> {
        i32::try_from(self.adam_step)
            .map(|_| ())
            .map_err(|_| NativePolicyTrainErrorV1::AdamStepOverflow)?;
        let parameters = self.model.parameter_snapshot_v1();
        validate_parameter_manifest(&parameters)?;
        validate_optimizer_state(&parameters, &self.first_moments, &self.second_moments)?;
        validate_canonical_gauge_state(
            &parameters,
            &self.first_moments,
            &self.second_moments,
            self.scorer_bias_anchor_bits,
        )
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
        validate_canonical_gauge_state(
            &parameters,
            &self.first_moments,
            &self.second_moments,
            self.scorer_bias_anchor_bits,
        )?;
        let packed_model_generation_sha256 = groups
            .iter()
            .flat_map(|group| group.substeps)
            .any(|substep| matches!(&substep.forward, NativePolicyForwardInputV1::Packed { .. }))
            .then(|| self.model.parameter_manifest_sha256_v1());
        if let Some(expected_generation) = packed_model_generation_sha256.as_deref() {
            for (group_index, group) in groups.iter().enumerate() {
                for (substep_index, substep) in group.substeps.iter().enumerate() {
                    if let NativePolicyForwardInputV1::Packed { tape, .. } = &substep.forward {
                        if tape.model_generation_sha256.as_ref() != expected_generation {
                            return Err(
                                NativePolicyTrainErrorV1::PackedForwardModelGenerationMismatch {
                                    group_index,
                                    substep_index,
                                },
                            );
                        }
                    }
                }
            }
        }

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
                let tape = match &substep.forward {
                    NativePolicyForwardInputV1::Encoded(encoded) => {
                        let recomputed = Box::new(forward_with_tape(
                            &parameters,
                            self.model.config_v1(),
                            **encoded,
                        )?);
                        validate_forward_output_bits_v1(
                            &recomputed,
                            substep,
                            group_index,
                            substep_index,
                            ForwardOutputSourceV1::IndependentRecompute,
                        )?;
                        DecisionTapeSourceV1::Owned(recomputed)
                    }
                    NativePolicyForwardInputV1::Packed { encoded, tape } => {
                        #[cfg(test)]
                        PACKED_INDEPENDENT_RECOMPUTE_CALL_COUNT_V1
                            .with(|count| count.set(count.get() + 1));
                        let independently_recomputed =
                            forward_with_tape(&parameters, self.model.config_v1(), **encoded)?;
                        validate_forward_output_bits_v1(
                            &independently_recomputed,
                            substep,
                            group_index,
                            substep_index,
                            ForwardOutputSourceV1::IndependentRecompute,
                        )?;
                        validate_forward_output_bits_v1(
                            &tape.tape,
                            substep,
                            group_index,
                            substep_index,
                            ForwardOutputSourceV1::PackedBackwardTape,
                        )?;
                        // The independent tape is intentionally dropped at
                        // this branch boundary. Only the rollout-time packed
                        // tape can reach the backward work staged below.
                        drop(independently_recomputed);
                        DecisionTapeSourceV1::Packed(&tape.tape)
                    }
                };
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

        let group_count = exact_group_count_f32(groups.len())?;
        let loss = (policy_sum + value_coefficient * value_sum) / group_count;
        finite_scalar("loss", 0, policy_sum)?;
        finite_scalar("loss", 1, value_sum)?;
        finite_scalar("loss", 2, loss)?;

        let mut gauge_accumulator = ScorerBiasGaugeAccumulatorV1::default();
        let mut reverse_workspace = ReverseWorkspaceV1::default();
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
                    &selected.tape.logits,
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

#[derive(Clone, Copy)]
enum ForwardOutputSourceV1 {
    IndependentRecompute,
    PackedBackwardTape,
}

fn validate_forward_output_bits_v1(
    tape: &DecisionTapeV1,
    substep: &NativePolicySubstepV1<'_>,
    group_index: usize,
    substep_index: usize,
    source: ForwardOutputSourceV1,
) -> Result<(), NativePolicyTrainErrorV1> {
    let expected_count = substep.expected_raw_action_logit_bits.len();
    if expected_count != tape.logits.len() {
        return Err(match source {
            ForwardOutputSourceV1::IndependentRecompute => {
                NativePolicyTrainErrorV1::ExpectedLogitCountMismatch {
                    group_index,
                    substep_index,
                    expected: expected_count,
                    actual: tape.logits.len(),
                }
            }
            ForwardOutputSourceV1::PackedBackwardTape => {
                NativePolicyTrainErrorV1::PackedForwardLogitCountMismatch {
                    group_index,
                    substep_index,
                    expected: expected_count,
                    actual: tape.logits.len(),
                }
            }
        });
    }
    for (action_index, (&expected_bits, actual)) in substep
        .expected_raw_action_logit_bits
        .iter()
        .zip(&tape.logits)
        .enumerate()
    {
        let actual_bits = actual.to_bits();
        if actual_bits != expected_bits {
            return Err(match source {
                ForwardOutputSourceV1::IndependentRecompute => {
                    NativePolicyTrainErrorV1::RecomputedLogitBitsMismatch {
                        group_index,
                        substep_index,
                        action_index,
                        selected_action_index: substep.selected_action_index,
                        expected_bits,
                        actual_bits,
                    }
                }
                ForwardOutputSourceV1::PackedBackwardTape => {
                    NativePolicyTrainErrorV1::PackedForwardLogitBitsMismatch {
                        group_index,
                        substep_index,
                        action_index,
                        selected_action_index: substep.selected_action_index,
                        expected_bits,
                        actual_bits,
                    }
                }
            });
        }
    }
    let actual_value_bits = tape.value.to_bits();
    if actual_value_bits != substep.expected_value_bits {
        return Err(match source {
            ForwardOutputSourceV1::IndependentRecompute => {
                NativePolicyTrainErrorV1::RecomputedValueBitsMismatch {
                    group_index,
                    substep_index,
                    expected_bits: substep.expected_value_bits,
                    actual_bits: actual_value_bits,
                }
            }
            ForwardOutputSourceV1::PackedBackwardTape => {
                NativePolicyTrainErrorV1::PackedForwardValueBitsMismatch {
                    group_index,
                    substep_index,
                    expected_bits: substep.expected_value_bits,
                    actual_bits: actual_value_bits,
                }
            }
        });
    }
    Ok(())
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
        if !self.high_precision_residual.is_finite()
            || self.high_precision_residual.abs() > derived_absolute_bound
        {
            return Err(
                NativePolicyTrainErrorV1::GaugeHighPrecisionResidualExceeded {
                    residual_bits: self.high_precision_residual.to_bits(),
                    bound_bits: derived_absolute_bound.to_bits(),
                },
            );
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

fn exact_group_count_f32(group_count: usize) -> Result<f32, NativePolicyTrainErrorV1> {
    let widened = u64::try_from(group_count)
        .map_err(|_| NativePolicyTrainErrorV1::GroupCountNotExactlyRepresentable { group_count })?;
    let represented = widened as f32;
    if !represented.is_finite() || represented as u128 != u128::from(widened) {
        return Err(NativePolicyTrainErrorV1::GroupCountNotExactlyRepresentable { group_count });
    }
    Ok(represented)
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

/// Scorer-owned, update-local forward state.  The type is deliberately
/// opaque outside this module: callers may move and borrow it, but cannot
/// rewrite activations or detach it from the model generation that created it.
pub(crate) struct NativePolicyPackedForwardTapeV1 {
    model_generation_sha256: Arc<str>,
    tape: DecisionTapeV1,
    #[cfg(test)]
    lifetime_probe: Option<Arc<()>>,
}

impl std::fmt::Debug for NativePolicyPackedForwardTapeV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativePolicyPackedForwardTapeV1")
            .field("model_generation_sha256", &self.model_generation_sha256)
            .field("action_count", &self.tape.logits.len())
            .finish_non_exhaustive()
    }
}

impl NativePolicyPackedForwardTapeV1 {
    pub(crate) fn logits_v1(&self) -> &[f32] {
        &self.tape.logits
    }

    pub(crate) fn value_v1(&self) -> f32 {
        self.tape.value
    }

    #[cfg(test)]
    pub(crate) fn corrupt_model_generation_for_test_v1(&mut self) {
        let mut corrupted = self.model_generation_sha256.to_string();
        let replacement = if corrupted.starts_with('0') { "1" } else { "0" };
        corrupted.replace_range(0..1, replacement);
        self.model_generation_sha256 = Arc::from(corrupted);
    }

    #[cfg(test)]
    pub(crate) fn corrupt_logit_for_test_v1(&mut self, index: usize) -> Result<(), ()> {
        let logit = self.tape.logits.get_mut(index).ok_or(())?;
        *logit = f32::from_bits(logit.to_bits() ^ 1);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn corrupt_non_selected_logit_for_test_v1(&mut self, selected: usize) -> bool {
        let index = match (0..self.tape.logits.len()).find(|index| *index != selected) {
            Some(index) => index,
            None => return false,
        };
        self.corrupt_logit_for_test_v1(index).is_ok()
    }

    #[cfg(test)]
    pub(crate) fn corrupt_value_for_test_v1(&mut self) {
        self.tape.value = f32::from_bits(self.tape.value.to_bits() ^ 1);
    }
}

/// One immutable parameter snapshot is shared by every scorer forward in an
/// update. This avoids a parameter clone per scored decision and retains the
/// exact rollout-time activations for backward. Learning still performs its
/// separately owned output-validation forward from canonical encoded input.
pub(crate) struct NativePolicyPackedForwardBuilderV1 {
    parameters: Arc<[NativeNamedParameterV1]>,
    config: crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1,
    model_generation_sha256: Arc<str>,
    #[cfg(test)]
    lifetime_probe: Option<Arc<()>>,
    #[cfg(test)]
    forward_call_count: Arc<AtomicU64>,
}

impl NativePolicyPackedForwardBuilderV1 {
    pub(crate) fn from_model_v1(
        model: &NativePolicyValueNetV1,
    ) -> Result<Self, NativePolicyTrainErrorV1> {
        let parameters = model.parameter_snapshot_v1();
        validate_parameter_manifest(&parameters)?;
        Ok(Self {
            parameters: Arc::from(parameters),
            config: model.config_v1(),
            model_generation_sha256: Arc::from(model.parameter_manifest_sha256_v1()),
            #[cfg(test)]
            lifetime_probe: None,
            #[cfg(test)]
            forward_call_count: Arc::new(AtomicU64::new(0)),
        })
    }

    pub(crate) fn forward_v1(
        &self,
        encoded: NativeEncodedDecisionViewV1<'_>,
    ) -> Result<NativePolicyPackedForwardTapeV1, NativePolicyTrainErrorV1> {
        let tape = forward_with_tape(self.parameters.as_ref(), self.config, encoded)?;
        #[cfg(test)]
        self.forward_call_count.fetch_add(1, Ordering::SeqCst);
        Ok(NativePolicyPackedForwardTapeV1 {
            model_generation_sha256: self.model_generation_sha256.clone(),
            tape,
            #[cfg(test)]
            lifetime_probe: self.lifetime_probe.clone(),
        })
    }

    #[cfg(test)]
    fn with_lifetime_probe_for_test_v1(mut self, probe: Arc<()>) -> Self {
        self.lifetime_probe = Some(probe);
        self
    }

    #[cfg(test)]
    pub(crate) fn forward_call_count_for_test_v1(&self) -> u64 {
        self.forward_call_count.load(Ordering::SeqCst)
    }
}

enum DecisionTapeSourceV1<'a> {
    Owned(Box<DecisionTapeV1>),
    Packed(&'a DecisionTapeV1),
}

impl Deref for DecisionTapeSourceV1<'_> {
    type Target = DecisionTapeV1;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(tape) => tape,
            Self::Packed(tape) => tape,
        }
    }
}

struct SelectedDecisionTapeV1<'a> {
    tape: DecisionTapeSourceV1<'a>,
    selected_action_index: usize,
    log_probabilities: Vec<f32>,
}

struct GroupTapeV1<'a> {
    tapes: Vec<SelectedDecisionTapeV1<'a>>,
    advantage: f32,
    value_error: f32,
}

/// Update-local scratch only. Buffers are zeroed or overwritten before every
/// use; no activation, gradient contribution, or optimizer state crosses a
/// decision. Reuse removes allocator traffic without changing any arithmetic
/// loop or the pinned reverse group/substep/action accumulation order.
#[derive(Default)]
struct ReverseWorkspaceV1 {
    grad_output: Vec<f32>,
    d_logits: Vec<f32>,
    decision: ReverseDecisionWorkspaceV1,
}

#[derive(Default)]
struct ReverseDecisionWorkspaceV1 {
    d_scorer_hidden: Vec<f32>,
    d_scorer_input: Vec<f32>,
    d_state_hidden: Vec<f32>,
    d_action_hidden: Vec<f32>,
    d_value_hidden: Vec<f32>,
    d_state_value: Vec<f32>,
    d_action_input: Vec<f32>,
    d_action_ref_pooled: Vec<f32>,
    d_object_hidden: Vec<f32>,
    d_action_ref_hidden: Vec<f32>,
    d_action_ref_input: Vec<f32>,
    d_state_input: Vec<f32>,
    d_node_input: Vec<f32>,
    d_object_base: Vec<f32>,
    d_edge_pooled: Vec<f32>,
    d_edge_hidden: Vec<f32>,
    d_edge_input: Vec<f32>,
    d_object_input: Vec<f32>,
    two_layer_second_pre: Vec<f32>,
    two_layer_first_output: Vec<f32>,
}

fn forward_with_tape(
    parameters: &[NativeNamedParameterV1],
    config: crate::native_policy_value_net_v1::NativePolicyValueModelConfigV1,
    encoded: NativeEncodedDecisionViewV1<'_>,
) -> Result<DecisionTapeV1, NativePolicyTrainErrorV1> {
    #[cfg(test)]
    FORWARD_WITH_TAPE_CALL_COUNT_V1.with(|count| count.set(count.get() + 1));
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
    workspace: &mut ReverseDecisionWorkspaceV1,
) -> Result<(), NativePolicyTrainErrorV1> {
    let action_count = tape.logits.len();
    if d_logits.len() != action_count {
        return Err(NativePolicyTrainErrorV1::ParameterManifest);
    }
    let object_count = tape.object_card_ids.len();
    let ReverseDecisionWorkspaceV1 {
        d_scorer_hidden,
        d_scorer_input,
        d_state_hidden,
        d_action_hidden,
        d_value_hidden,
        d_state_value,
        d_action_input,
        d_action_ref_pooled,
        d_object_hidden,
        d_action_ref_hidden,
        d_action_ref_input,
        d_state_input,
        d_node_input,
        d_object_base,
        d_edge_pooled,
        d_edge_hidden,
        d_edge_input,
        d_object_input,
        two_layer_second_pre,
        two_layer_first_output,
    } = workspace;

    linear_backward_into(
        parameters,
        gradients,
        SCORER_SECOND_WEIGHT,
        SCORER_SECOND_BIAS,
        &tape.scorer_hidden,
        action_count,
        HIDDEN_DIM_V1,
        1,
        d_logits,
        d_scorer_hidden,
    )?;
    apply_tanh_gradient(&tape.scorer_hidden, d_scorer_hidden)?;
    linear_backward_into(
        parameters,
        gradients,
        SCORER_FIRST_WEIGHT,
        SCORER_FIRST_BIAS,
        &tape.scorer_input,
        action_count,
        SCORER_INPUT,
        HIDDEN_DIM_V1,
        d_scorer_hidden,
        d_scorer_input,
    )?;
    resize_zeroed_v1(d_state_hidden, HIDDEN_DIM_V1);
    resize_zeroed_v1(d_action_hidden, action_count * HIDDEN_DIM_V1);
    for action in 0..action_count {
        let source = action * SCORER_INPUT;
        for hidden in 0..HIDDEN_DIM_V1 {
            d_state_hidden[hidden] += d_scorer_input[source + hidden];
            d_action_hidden[action * HIDDEN_DIM_V1 + hidden] =
                d_scorer_input[source + HIDDEN_DIM_V1 + hidden];
        }
    }

    let d_value_output = [d_value];
    linear_backward_into(
        parameters,
        gradients,
        VALUE_SECOND_WEIGHT,
        VALUE_SECOND_BIAS,
        &tape.value_hidden,
        1,
        HIDDEN_DIM_V1,
        1,
        &d_value_output,
        d_value_hidden,
    )?;
    apply_tanh_gradient(&tape.value_hidden, d_value_hidden)?;
    linear_backward_into(
        parameters,
        gradients,
        VALUE_FIRST_WEIGHT,
        VALUE_FIRST_BIAS,
        &tape.state_encoder.second_output,
        1,
        HIDDEN_DIM_V1,
        HIDDEN_DIM_V1,
        d_value_hidden,
        d_state_value,
    )?;
    add_slices(d_state_hidden, d_state_value)?;

    two_layer_backward_into(
        parameters,
        gradients,
        ACTION_SPEC,
        &tape.action_encoder,
        d_action_hidden,
        two_layer_second_pre,
        two_layer_first_output,
        d_action_input,
    )?;
    resize_zeroed_v1(d_action_ref_pooled, action_count * HIDDEN_DIM_V1);
    for action in 0..action_count {
        let source = action * ACTION_ENCODER_INPUT + ACTION_FEATURE_DIM_V1;
        let destination = action * HIDDEN_DIM_V1;
        d_action_ref_pooled[destination..destination + HIDDEN_DIM_V1]
            .copy_from_slice(&d_action_input[source..source + HIDDEN_DIM_V1]);
    }

    resize_zeroed_v1(d_object_hidden, object_count * HIDDEN_DIM_V1);
    if let Some(action_ref_tape) = &tape.action_ref_encoder {
        let action_ref_count = tape.action_ref_action_indices.len();
        resize_zeroed_v1(d_action_ref_hidden, action_ref_count * HIDDEN_DIM_V1);
        for action_ref in 0..action_ref_count {
            let source = tape.action_ref_action_indices[action_ref] * HIDDEN_DIM_V1;
            let destination = action_ref * HIDDEN_DIM_V1;
            d_action_ref_hidden[destination..destination + HIDDEN_DIM_V1]
                .copy_from_slice(&d_action_ref_pooled[source..source + HIDDEN_DIM_V1]);
        }
        two_layer_backward_into(
            parameters,
            gradients,
            ACTION_REF_SPEC,
            action_ref_tape,
            d_action_ref_hidden,
            two_layer_second_pre,
            two_layer_first_output,
            d_action_ref_input,
        )?;
        for action_ref in 0..action_ref_count {
            let source = action_ref * ACTION_REF_ENCODER_INPUT + ACTION_REF_FEATURE_DIM_V1;
            let destination = tape.action_ref_node_indices[action_ref] * HIDDEN_DIM_V1;
            for hidden in 0..HIDDEN_DIM_V1 {
                d_object_hidden[destination + hidden] += d_action_ref_input[source + hidden];
            }
        }
    }

    two_layer_backward_into(
        parameters,
        gradients,
        STATE_SPEC,
        &tape.state_encoder,
        d_state_hidden,
        two_layer_second_pre,
        two_layer_first_output,
        d_state_input,
    )?;
    for (object, group) in tape.object_groups.iter().copied().enumerate() {
        let source = STATE_DIM_V1 + group * HIDDEN_DIM_V1;
        let destination = object * HIDDEN_DIM_V1;
        for hidden in 0..HIDDEN_DIM_V1 {
            d_object_hidden[destination + hidden] += d_state_input[source + hidden];
        }
    }

    two_layer_backward_into(
        parameters,
        gradients,
        NODE_SPEC,
        &tape.node_update,
        d_object_hidden,
        two_layer_second_pre,
        two_layer_first_output,
        d_node_input,
    )?;
    resize_zeroed_v1(d_object_base, object_count * HIDDEN_DIM_V1);
    resize_zeroed_v1(d_edge_pooled, object_count * HIDDEN_DIM_V1);
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
        resize_zeroed_v1(d_edge_hidden, edge_count * HIDDEN_DIM_V1);
        for edge in 0..edge_count {
            let destination = edge * HIDDEN_DIM_V1;
            let source_row = tape.edge_source_indices[edge] * HIDDEN_DIM_V1;
            let target_row = tape.edge_target_indices[edge] * HIDDEN_DIM_V1;
            for hidden in 0..HIDDEN_DIM_V1 {
                d_edge_hidden[destination + hidden] =
                    d_edge_pooled[source_row + hidden] + d_edge_pooled[target_row + hidden];
            }
        }
        two_layer_backward_into(
            parameters,
            gradients,
            EDGE_SPEC,
            edge_tape,
            d_edge_hidden,
            two_layer_second_pre,
            two_layer_first_output,
            d_edge_input,
        )?;
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

    two_layer_backward_into(
        parameters,
        gradients,
        OBJECT_SPEC,
        &tape.object_encoder,
        d_object_base,
        two_layer_second_pre,
        two_layer_first_output,
        d_object_input,
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

#[allow(clippy::too_many_arguments)]
fn two_layer_backward_into(
    parameters: &[NativeNamedParameterV1],
    gradients: &mut [Vec<f32>],
    spec: TwoLayerSpecV1,
    tape: &TwoLayerTapeV1,
    d_output: &[f32],
    d_second_pre: &mut Vec<f32>,
    d_first_output: &mut Vec<f32>,
    d_input: &mut Vec<f32>,
) -> Result<(), NativePolicyTrainErrorV1> {
    d_second_pre.clear();
    d_second_pre.extend_from_slice(d_output);
    apply_tanh_gradient(&tape.second_output, d_second_pre)?;
    linear_backward_into(
        parameters,
        gradients,
        spec.second_weight,
        spec.second_bias,
        &tape.first_output,
        tape.rows,
        HIDDEN_DIM_V1,
        HIDDEN_DIM_V1,
        d_second_pre,
        d_first_output,
    )?;
    apply_tanh_gradient(&tape.first_output, d_first_output)?;
    linear_backward_into(
        parameters,
        gradients,
        spec.first_weight,
        spec.first_bias,
        &tape.input,
        tape.rows,
        spec.input_dim,
        HIDDEN_DIM_V1,
        d_first_output,
        d_input,
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
fn linear_backward_into(
    parameters: &[NativeNamedParameterV1],
    gradients: &mut [Vec<f32>],
    weight_index: usize,
    bias_index: usize,
    input: &[f32],
    rows: usize,
    input_dim: usize,
    output_dim: usize,
    d_output: &[f32],
    d_input: &mut Vec<f32>,
) -> Result<(), NativePolicyTrainErrorV1> {
    let weight = &parameters[weight_index].values;
    if input.len() != rows * input_dim
        || d_output.len() != rows * output_dim
        || weight.len() != output_dim * input_dim
        || gradients[weight_index].len() != weight.len()
        || gradients[bias_index].len() != output_dim
    {
        return Err(NativePolicyTrainErrorV1::ParameterManifest);
    }
    resize_zeroed_v1(d_input, rows * input_dim);
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
    Ok(())
}

fn resize_zeroed_v1(values: &mut Vec<f32>, len: usize) {
    values.clear();
    values.resize(len, 0.0);
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
    adam_update_impl(
        parameters,
        gradients,
        first_moments,
        second_moments,
        step,
        learning_rate,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn adam_update_impl(
    parameters: &[NativeNamedParameterV1],
    gradients: &[Vec<f32>],
    first_moments: &[Vec<f32>],
    second_moments: &[Vec<f32>],
    step: u64,
    learning_rate: f32,
    skip_exact_zero_state: bool,
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
            let previous_second = second_moments[parameter_index][value_index];
            // The dense Adam equations map this exact (+0,+0,+0) triple back
            // to the already-cloned parameter and moment bits. This dominates
            // untouched embedding rows; signed zero and every active state
            // deliberately retain the dense arithmetic path.
            if skip_exact_zero_state
                && gradient.to_bits() == 0
                && previous_first.to_bits() == 0
                && previous_second.to_bits() == 0
            {
                continue;
            }
            let first = previous_first + (gradient - previous_first) * (1.0 - ADAM_BETA1_V1);
            let second =
                previous_second * ADAM_BETA2_V1 + gradient * gradient * (1.0 - ADAM_BETA2_V1);
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
            .zip(
                EXPECTED_PARAMETER_NAMES
                    .iter()
                    .zip(EXPECTED_PARAMETER_SHAPES),
            )
            .any(|(parameter, (expected_name, expected_shape))| {
                parameter.name != *expected_name
                    || parameter.shape.as_slice() != expected_shape
                    || expected_shape.iter().product::<usize>() != parameter.values.len()
                    || parameter.values.iter().any(|value| !value.is_finite())
            })
        || parameters
            .iter()
            .map(|parameter| parameter.values.len())
            .sum::<usize>()
            != PARAMETER_COUNT_V1
        || parameters[CARD_EMBEDDING].values[..CARD_EMBEDDING_DIM_V1]
            .iter()
            .any(|value| value.to_bits() != 0)
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
        || first_moments[CARD_EMBEDDING][..CARD_EMBEDDING_DIM_V1]
            .iter()
            .any(|value| value.to_bits() != 0)
        || second_moments[CARD_EMBEDDING][..CARD_EMBEDDING_DIM_V1]
            .iter()
            .any(|value| value.to_bits() != 0)
    {
        return Err(NativePolicyTrainErrorV1::OptimizerState);
    }
    Ok(())
}

fn validate_canonical_gauge_state(
    parameters: &[NativeNamedParameterV1],
    first_moments: &[Vec<f32>],
    second_moments: &[Vec<f32>],
    scorer_bias_anchor_bits: u32,
) -> Result<(), NativePolicyTrainErrorV1> {
    if parameters[SCORER_SECOND_BIAS].values[0].to_bits() != scorer_bias_anchor_bits {
        return Err(NativePolicyTrainErrorV1::GaugeAnchor);
    }
    if parameters[SCORER_SECOND_BIAS].name != CANONICAL_GAUGE_PARAMETERS_V1[0]
        || parameters[SCORER_SECOND_BIAS].shape.as_slice() != [1]
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

fn validate_owned_train_snapshot_v1(
    snapshot: &NativePolicyValueTrainSnapshotV1,
    expected_scorer_bias_anchor_bits: Option<u32>,
) -> Result<(), NativePolicyTrainErrorV1> {
    i32::try_from(snapshot.adam_step)
        .map(|_| ())
        .map_err(|_| NativePolicyTrainErrorV1::AdamStepOverflow)?;
    validate_parameter_manifest(&snapshot.parameters)?;

    if snapshot.first_moments.len() != snapshot.parameters.len()
        || snapshot.second_moments.len() != snapshot.parameters.len()
        || snapshot
            .parameters
            .iter()
            .zip(&snapshot.first_moments)
            .zip(&snapshot.second_moments)
            .any(|((parameter, first), second)| {
                first.name != parameter.name
                    || second.name != parameter.name
                    || first.shape != parameter.shape
                    || second.shape != parameter.shape
                    || first.values.len() != parameter.values.len()
                    || second.values.len() != parameter.values.len()
                    || first.values.iter().any(|value| !value.is_finite())
                    || second
                        .values
                        .iter()
                        .any(|value| !value.is_finite() || *value < 0.0)
            })
        || snapshot.first_moments[CARD_EMBEDDING].values[..CARD_EMBEDDING_DIM_V1]
            .iter()
            .any(|value| value.to_bits() != 0)
        || snapshot.second_moments[CARD_EMBEDDING].values[..CARD_EMBEDDING_DIM_V1]
            .iter()
            .any(|value| value.to_bits() != 0)
        || snapshot.first_moments[SCORER_SECOND_BIAS].values[0].to_bits() != 0
        || snapshot.second_moments[SCORER_SECOND_BIAS].values[0].to_bits() != 0
    {
        return Err(NativePolicyTrainErrorV1::OptimizerState);
    }

    let parameter_anchor_bits = snapshot.parameters[SCORER_SECOND_BIAS].values[0].to_bits();
    if parameter_anchor_bits != snapshot.scorer_bias_anchor_bits
        || expected_scorer_bias_anchor_bits
            .is_some_and(|expected| snapshot.scorer_bias_anchor_bits != expected)
    {
        return Err(NativePolicyTrainErrorV1::GaugeAnchor);
    }
    Ok(())
}

fn hash_owned_train_snapshot_v1(snapshot: &NativePolicyValueTrainSnapshotV1) -> [u8; 32] {
    fn atom(hasher: &mut Sha256, tag: &[u8], payload: &[u8]) {
        hasher.update((tag.len() as u32).to_be_bytes());
        hasher.update(tag);
        hasher.update((payload.len() as u64).to_be_bytes());
        hasher.update(payload);
    }

    fn tensor_section(
        hasher: &mut Sha256,
        section: &'static [u8],
        tensors: &[NativeNamedParameterV1],
    ) {
        atom(hasher, b"section", section);
        atom(
            hasher,
            b"tensor_count",
            &(tensors.len() as u64).to_be_bytes(),
        );
        for (ordinal, tensor) in tensors.iter().enumerate() {
            atom(hasher, b"tensor_ordinal", &(ordinal as u64).to_be_bytes());
            atom(hasher, b"tensor_name", tensor.name.as_bytes());
            atom(
                hasher,
                b"tensor_rank",
                &(tensor.shape.len() as u64).to_be_bytes(),
            );
            let mut shape_bytes = Vec::with_capacity(tensor.shape.len() * 8);
            for dimension in &tensor.shape {
                shape_bytes.extend_from_slice(&(*dimension as u64).to_be_bytes());
            }
            atom(hasher, b"tensor_shape_u64be", &shape_bytes);
            atom(
                hasher,
                b"tensor_element_count",
                &(tensor.values.len() as u64).to_be_bytes(),
            );
            let mut value_bytes = Vec::with_capacity(tensor.values.len() * 4);
            for value in &tensor.values {
                value_bytes.extend_from_slice(&value.to_bits().to_le_bytes());
            }
            atom(hasher, b"tensor_f32le", &value_bytes);
        }
    }

    let mut hasher = Sha256::new();
    atom(
        &mut hasher,
        b"domain",
        NATIVE_TRAIN_STATE_SHA256_IDENTITY_V1.as_bytes(),
    );
    atom(
        &mut hasher,
        b"adam_step_u64be",
        &snapshot.adam_step.to_be_bytes(),
    );
    atom(
        &mut hasher,
        b"scorer_bias_anchor_f32le",
        &snapshot.scorer_bias_anchor_bits.to_le_bytes(),
    );
    tensor_section(&mut hasher, b"parameters", &snapshot.parameters);
    tensor_section(&mut hasher, b"first_moments", &snapshot.first_moments);
    tensor_section(&mut hasher, b"second_moments", &snapshot.second_moments);
    let digest = hasher.finalize();
    let mut output = [0u8; 32];
    output.copy_from_slice(&digest);
    output
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
        "7672c87912b6015f393d66921a3e78cb5623dd76582a9513f2d87c560c0f4aa7";
    const LOSS_REDUCTION_JSON: &str = include_str!(
        "../../data/native_policy_train_step_v1/loss_reduction_intermediate_rung_v1.json"
    );
    const LOSS_REDUCTION_FIXTURE_SHA256: &str =
        "537f86c8f09b3529fb985efc46306dc139b9ce1cfee1fb32515886d6a7fe2cd7";
    const LOSS_REDUCTION_GENERATOR: &[u8] =
        include_bytes!("../../python/tools/generate_native_policy_loss_reduction_rung_v1.py");
    const LOSS_REDUCTION_GENERATOR_SHA256: &str =
        "ad73d06792605703a071dda5fb6366fbc0f4f866841faec12480dc9f7a4a787b";
    const TRAIN_FIXTURE_GENERATOR_AUTHORITY: &[u8] =
        include_bytes!("../../python/tools/generate_native_policy_train_step_v1_goldens.py");
    const FORWARD_FIXTURE_GENERATOR_AUTHORITY: &[u8] =
        include_bytes!("../../python/tools/generate_native_policy_value_net_v1_goldens.py");
    const MODEL_AUTHORITY: &[u8] = include_bytes!("../../python/mtg_kernel_rl/model.py");
    const TRAINER_AUTHORITY: &[u8] = include_bytes!("../../python/mtg_kernel_rl/trainer.py");
    const FORWARD_AUTHORITY: &[u8] = include_bytes!(
        "../../data/native_policy_value_net_v1/runner_fixed_forward_goldens_v1.json"
    );

    #[derive(Debug, Deserialize)]
    struct LossReductionFixture {
        schema: String,
        identity: String,
        authority: LossReductionAuthority,
        provenance: LossReductionProvenance,
        model_state: LossReductionModelState,
        term_stream: LossReductionTermStream,
        reduction: LossReductionRecord,
    }

    #[derive(Debug, Deserialize)]
    struct LossReductionAuthority {
        generator_path: String,
        generator_sha256: String,
        train_fixture_generator_path: String,
        train_fixture_generator_sha256: String,
        forward_fixture_generator_path: String,
        forward_fixture_generator_sha256: String,
        base_artifact_path: String,
        base_artifact_sha256: String,
        model_path: String,
        model_sha256: String,
        trainer_path: String,
        trainer_sha256: String,
        forward_fixture_path: String,
        forward_fixture_sha256: String,
        platform_system: String,
        platform_machine: String,
        python_version: String,
        torch_version: String,
        torch_num_threads: usize,
        torch_num_interop_threads: usize,
        torch_deterministic_algorithms: bool,
        torch_default_dtype: String,
    }

    #[derive(Debug, Deserialize)]
    struct LossReductionProvenance {
        base_program_json_pointer: String,
        cycle_rule: String,
        base_group_count: usize,
        base_substep_count: usize,
        cycle_count: usize,
        learner_physical_decision_group_count: usize,
        policy_substep_count: usize,
        terminal_return_counts: std::collections::BTreeMap<String, usize>,
    }

    #[derive(Debug, Deserialize)]
    struct LossReductionModelState {
        trainer_algorithm: String,
        initializer: String,
        adam_step_before: u64,
        reconstruction: String,
        parameters_sha256: String,
        first_moments_sha256: String,
        second_moments_sha256: String,
    }

    #[derive(Debug, Deserialize)]
    struct LossReductionTermStream {
        framing: String,
        sha256: String,
        base_cycle_terms: Vec<LossReductionBaseTerm>,
        policy_nonzero_count: usize,
        value_nonzero_count: usize,
        policy_positive_count: usize,
        policy_negative_count: usize,
        value_positive_count: usize,
    }

    #[derive(Debug, Deserialize)]
    struct LossReductionBaseTerm {
        base_group_index: usize,
        policy_term_f32_bits: String,
        value_term_f32_bits: String,
    }

    #[derive(Debug, Deserialize)]
    struct LossReductionRecord {
        torch_operation: String,
        rust_operation: String,
        torch_stack: LossReductionScalars,
        sequential_f32_over_same_torch_term_bits: LossReductionScalars,
        frozen_tolerance: LossReductionTolerance,
        same_term_sequential_vs_torch_stack:
            std::collections::BTreeMap<String, LossReductionComparison>,
        all_same_term_comparisons_hold: bool,
    }

    #[derive(Debug, Deserialize)]
    struct LossReductionScalars {
        policy_sum: LossReductionScalar,
        value_sum: LossReductionScalar,
        loss: LossReductionScalar,
    }

    #[derive(Debug, Deserialize)]
    struct LossReductionScalar {
        value: f32,
        f32_bits: String,
    }

    #[derive(Debug, Deserialize)]
    struct LossReductionTolerance {
        absolute: f64,
        relative: f64,
        comparison_rule: String,
    }

    #[derive(Debug, Deserialize)]
    struct LossReductionComparison {
        absolute_delta_f64: f64,
        allowed_delta_f64: f64,
        holds: bool,
    }

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

    struct ExpectedOutputBits {
        raw_action_logit_bits: Vec<u32>,
        value_bits: u32,
    }

    fn expected_outputs_for_step(
        model: &NativePolicyValueNetV1,
        forward: &ForwardFixture,
        step: &GoldenStep,
    ) -> Vec<Vec<ExpectedOutputBits>> {
        step.groups
            .iter()
            .map(|group| {
                group
                    .substeps
                    .iter()
                    .map(|substep| {
                        let output = model
                            .forward_v1(encoded(case_by_name(forward, &substep.case)))
                            .expect("authority-test scorer forward succeeds");
                        ExpectedOutputBits {
                            raw_action_logit_bits: output
                                .logits
                                .iter()
                                .map(|value| value.to_bits())
                                .collect(),
                            value_bits: output.value.to_bits(),
                        }
                    })
                    .collect()
            })
            .collect()
    }

    fn substeps_for_step<'a>(
        forward: &'a ForwardFixture,
        step: &GoldenStep,
        expected_outputs: &'a [Vec<ExpectedOutputBits>],
    ) -> Vec<Vec<NativePolicySubstepV1<'a>>> {
        assert_eq!(step.groups.len(), expected_outputs.len());
        step.groups
            .iter()
            .zip(expected_outputs)
            .map(|(group, expected_group)| {
                assert_eq!(group.substeps.len(), expected_group.len());
                group
                    .substeps
                    .iter()
                    .zip(expected_group)
                    .map(|(substep, expected)| NativePolicySubstepV1 {
                        forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded(
                            case_by_name(forward, &substep.case),
                        ))),
                        selected_action_index: substep.selected_action_index,
                        expected_raw_action_logit_bits: &expected.raw_action_logit_bits,
                        expected_value_bits: expected.value_bits,
                    })
                    .collect()
            })
            .collect()
    }

    fn packed_tapes_for_step(
        builder: &NativePolicyPackedForwardBuilderV1,
        forward: &ForwardFixture,
        step: &GoldenStep,
    ) -> Vec<Vec<NativePolicyPackedForwardTapeV1>> {
        step.groups
            .iter()
            .map(|group| {
                group
                    .substeps
                    .iter()
                    .map(|substep| {
                        builder
                            .forward_v1(encoded(case_by_name(forward, &substep.case)))
                            .expect("packed scorer forward succeeds")
                    })
                    .collect()
            })
            .collect()
    }

    fn packed_substeps_for_step<'a>(
        forward: &'a ForwardFixture,
        step: &GoldenStep,
        expected_outputs: &'a [Vec<ExpectedOutputBits>],
        packed_tapes: &'a [Vec<NativePolicyPackedForwardTapeV1>],
    ) -> Vec<Vec<NativePolicySubstepV1<'a>>> {
        assert_eq!(step.groups.len(), expected_outputs.len());
        assert_eq!(step.groups.len(), packed_tapes.len());
        step.groups
            .iter()
            .zip(expected_outputs)
            .zip(packed_tapes)
            .map(|((group, expected_group), packed_group)| {
                assert_eq!(group.substeps.len(), expected_group.len());
                assert_eq!(group.substeps.len(), packed_group.len());
                group
                    .substeps
                    .iter()
                    .zip(expected_group)
                    .zip(packed_group)
                    .map(|((substep, expected), packed)| NativePolicySubstepV1 {
                        forward: NativePolicyForwardInputV1::Packed {
                            encoded: Box::new(encoded(case_by_name(forward, &substep.case))),
                            tape: packed,
                        },
                        selected_action_index: substep.selected_action_index,
                        expected_raw_action_logit_bits: &expected.raw_action_logit_bits,
                        expected_value_bits: expected.value_bits,
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

    fn reduction_f32_bits(value: &str) -> u32 {
        let digits = value
            .strip_prefix("0x")
            .filter(|digits| digits.len() == 8)
            .expect("loss-reduction f32 bits use exact 0x plus eight-hex framing");
        u32::from_str_radix(digits, 16).expect("loss-reduction f32 bits parse")
    }

    fn reduction_scalar_bits(scalar: &LossReductionScalar) -> u32 {
        let bits = reduction_f32_bits(&scalar.f32_bits);
        assert_eq!(scalar.value.to_bits(), bits);
        bits
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
    fn scorer_packed_tapes_are_exact_with_independent_recompute_and_drop_with_the_update() {
        let (forward, golden) = fixtures();
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut reference_state = NativePolicyValueTrainStateV1::new_v1(model.clone()).unwrap();
        let mut packed_state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();

        for step in &golden.steps {
            let expected_outputs =
                expected_outputs_for_step(reference_state.model_v1(), &forward, step);
            let reference_substeps = substeps_for_step(&forward, step, &expected_outputs);
            let reference_groups = groups_for_step(step, &reference_substeps);
            let lifetime_probe = Arc::new(());
            let packed_before = packed_state.snapshot_v1().unwrap();

            let packed_result = {
                let builder =
                    NativePolicyPackedForwardBuilderV1::from_model_v1(packed_state.model_v1())
                        .unwrap()
                        .with_lifetime_probe_for_test_v1(Arc::clone(&lifetime_probe));
                let calls_before_scorer = builder.forward_call_count_for_test_v1();
                let recompute_calls_before = packed_independent_recompute_call_count_for_test_v1();
                let packed_tapes = packed_tapes_for_step(&builder, &forward, step);
                let expected_forward_count = step
                    .groups
                    .iter()
                    .map(|group| group.substeps.len() as u64)
                    .sum::<u64>();
                let calls_after_scorer = builder.forward_call_count_for_test_v1();
                assert_eq!(
                    calls_after_scorer - calls_before_scorer,
                    expected_forward_count
                );
                let recompute_calls_before_train =
                    packed_independent_recompute_call_count_for_test_v1();
                assert_eq!(recompute_calls_before_train, recompute_calls_before);
                assert_eq!(
                    Arc::strong_count(&lifetime_probe),
                    2 + expected_forward_count as usize,
                    "one builder and one owner per packed tape"
                );
                let packed_substeps =
                    packed_substeps_for_step(&forward, step, &expected_outputs, &packed_tapes);
                let packed_groups = groups_for_step(step, &packed_substeps);
                let result = packed_state
                    .train_step_v1(
                        &packed_groups,
                        golden.value_coefficient,
                        golden.optimizer.learning_rate,
                    )
                    .unwrap();
                assert_eq!(
                    builder.forward_call_count_for_test_v1(),
                    calls_after_scorer,
                    "learning must not use the scorer-owned forward builder"
                );
                assert_eq!(
                    packed_independent_recompute_call_count_for_test_v1()
                        - recompute_calls_before_train,
                    expected_forward_count,
                    "packed learning must independently recompute every substep exactly once"
                );
                result
            };
            assert_eq!(
                Arc::strong_count(&lifetime_probe),
                1,
                "packed tapes must not escape their update"
            );
            let dense_gradients = packed_result
                .gradients
                .iter()
                .map(|parameter| parameter.values.clone())
                .collect::<Vec<_>>();
            let dense_first_before = packed_before
                .first_moments
                .iter()
                .map(|parameter| parameter.values.clone())
                .collect::<Vec<_>>();
            let dense_second_before = packed_before
                .second_moments
                .iter()
                .map(|parameter| parameter.values.clone())
                .collect::<Vec<_>>();
            let (mut dense_parameters, mut dense_first, mut dense_second) = adam_update_impl(
                &packed_before.parameters,
                &dense_gradients,
                &dense_first_before,
                &dense_second_before,
                packed_result.adam_step,
                golden.optimizer.learning_rate,
                false,
            )
            .unwrap();
            dense_parameters[SCORER_SECOND_BIAS].values[0] =
                packed_before.parameters[SCORER_SECOND_BIAS].values[0];
            dense_first[SCORER_SECOND_BIAS][0] = 0.0;
            dense_second[SCORER_SECOND_BIAS][0] = 0.0;
            let dense_expected = NativePolicyValueTrainSnapshotV1 {
                adam_step: packed_result.adam_step,
                scorer_bias_anchor_bits: packed_before.scorer_bias_anchor_bits,
                first_moments: named_state_snapshot(&dense_parameters, &dense_first),
                second_moments: named_state_snapshot(&dense_parameters, &dense_second),
                parameters: dense_parameters,
            };
            assert_eq!(packed_state.snapshot_v1().unwrap(), dense_expected);

            let calls_before_reference = forward_with_tape_call_count_for_test_v1();
            let reference_result = reference_state
                .train_step_v1(
                    &reference_groups,
                    golden.value_coefficient,
                    golden.optimizer.learning_rate,
                )
                .unwrap();
            let expected_reference_forwards = step
                .groups
                .iter()
                .map(|group| group.substeps.len() as u64)
                .sum::<u64>();
            assert_eq!(
                forward_with_tape_call_count_for_test_v1() - calls_before_reference,
                expected_reference_forwards
            );
            assert_eq!(packed_result, reference_result);
            assert_eq!(
                packed_state.snapshot_v1().unwrap(),
                reference_state.snapshot_v1().unwrap()
            );
        }
    }

    #[test]
    fn corrupt_packed_tape_and_model_generation_fail_before_state_mutation() {
        let (forward, golden) = fixtures();
        let step = &golden.steps[0];
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let baseline = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        let before = baseline.snapshot_v1().unwrap();
        let expected_outputs = expected_outputs_for_step(baseline.model_v1(), &forward, step);

        let mut corrupt_tape_state = baseline.clone();
        let builder =
            NativePolicyPackedForwardBuilderV1::from_model_v1(baseline.model_v1()).unwrap();
        let mut corrupt_tapes = packed_tapes_for_step(&builder, &forward, step);
        let mut corruption = None;
        for (group_index, (golden_group, tape_group)) in
            step.groups.iter().zip(&mut corrupt_tapes).enumerate()
        {
            for (substep_index, (golden_substep, tape)) in
                golden_group.substeps.iter().zip(tape_group).enumerate()
            {
                if tape.corrupt_non_selected_logit_for_test_v1(golden_substep.selected_action_index)
                {
                    corruption = Some((
                        group_index,
                        substep_index,
                        golden_substep.selected_action_index,
                    ));
                    break;
                }
            }
            if corruption.is_some() {
                break;
            }
        }
        let (corrupt_group, corrupt_substep, selected_action_index) =
            corruption.expect("fixture contains a multi-action packed tape");
        let corrupt_substeps =
            packed_substeps_for_step(&forward, step, &expected_outputs, &corrupt_tapes);
        let corrupt_groups = groups_for_step(step, &corrupt_substeps);
        let error = corrupt_tape_state
            .train_step_v1(
                &corrupt_groups,
                golden.value_coefficient,
                golden.optimizer.learning_rate,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            NativePolicyTrainErrorV1::PackedForwardLogitBitsMismatch {
                group_index,
                substep_index,
                action_index,
                selected_action_index: actual_selected,
                expected_bits,
                actual_bits,
            } if group_index == corrupt_group
                && substep_index == corrupt_substep
                && action_index != selected_action_index
                && actual_selected == selected_action_index
                && expected_bits != actual_bits
        ));
        assert_eq!(corrupt_tape_state.snapshot_v1().unwrap(), before);

        let mut stale_generation_state = baseline;
        let mut stale_tapes = packed_tapes_for_step(&builder, &forward, step);
        stale_tapes[0][0].corrupt_model_generation_for_test_v1();
        let stale_substeps =
            packed_substeps_for_step(&forward, step, &expected_outputs, &stale_tapes);
        let stale_groups = groups_for_step(step, &stale_substeps);
        assert_eq!(
            stale_generation_state.train_step_v1(
                &stale_groups,
                golden.value_coefficient,
                golden.optimizer.learning_rate,
            ),
            Err(
                NativePolicyTrainErrorV1::PackedForwardModelGenerationMismatch {
                    group_index: 0,
                    substep_index: 0,
                }
            )
        );
        assert_eq!(stale_generation_state.snapshot_v1().unwrap(), before);
    }

    #[test]
    fn exact_zero_adam_shortcut_matches_dense_bits_and_excludes_signed_zero() {
        let parameters = vec![NativeNamedParameterV1 {
            name: "shortcut.test",
            shape: vec![8],
            values: vec![1.0, -2.0, 3.0, 4.0, 5.0, 6.0, 0.0, -0.0],
        }];
        let gradients = vec![vec![0.0, -0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0]];
        let first = vec![vec![0.0, 0.0, -0.0, 0.0, 0.0, 0.5, 0.0, 0.0]];
        let second = vec![vec![0.0, 0.0, 0.0, -0.0, 0.0, 0.0, 0.5, 0.0]];
        for step in [1, 2, 4_096] {
            let dense =
                adam_update_impl(&parameters, &gradients, &first, &second, step, 0.001, false)
                    .unwrap();
            let shortcut =
                adam_update_impl(&parameters, &gradients, &first, &second, step, 0.001, true)
                    .unwrap();
            for (dense_values, shortcut_values) in [
                (&dense.0[0].values, &shortcut.0[0].values),
                (&dense.1[0], &shortcut.1[0]),
                (&dense.2[0], &shortcut.2[0]),
            ] {
                assert_eq!(
                    shortcut_values
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
                    dense_values
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn intermediate_torch_stack_vs_production_sequential_loss_reduction_holds() {
        let (forward, golden) = fixtures();
        let rung: LossReductionFixture =
            serde_json::from_str(LOSS_REDUCTION_JSON).expect("loss-reduction rung parses");

        assert_eq!(
            rung.schema,
            "native-policy-loss-reduction-intermediate-rung-v1"
        );
        assert_eq!(
            rung.identity,
            "torch-stack-vs-rust-sequential-loss-reduction-v1"
        );
        assert_eq!(
            digest_bytes(LOSS_REDUCTION_JSON.as_bytes()),
            LOSS_REDUCTION_FIXTURE_SHA256
        );
        assert_eq!(
            digest_bytes(LOSS_REDUCTION_GENERATOR),
            LOSS_REDUCTION_GENERATOR_SHA256
        );
        assert_eq!(
            rung.authority.generator_path,
            "python/tools/generate_native_policy_loss_reduction_rung_v1.py"
        );
        assert_eq!(
            rung.authority.generator_sha256,
            LOSS_REDUCTION_GENERATOR_SHA256
        );
        assert_eq!(
            rung.authority.train_fixture_generator_path,
            "python/tools/generate_native_policy_train_step_v1_goldens.py"
        );
        assert_eq!(
            rung.authority.train_fixture_generator_sha256,
            digest_bytes(TRAIN_FIXTURE_GENERATOR_AUTHORITY)
        );
        assert_eq!(
            rung.authority.forward_fixture_generator_path,
            "python/tools/generate_native_policy_value_net_v1_goldens.py"
        );
        assert_eq!(
            rung.authority.forward_fixture_generator_sha256,
            digest_bytes(FORWARD_FIXTURE_GENERATOR_AUTHORITY)
        );
        assert_eq!(
            rung.authority.base_artifact_path,
            "data/native_policy_train_step_v1/runner_fixed_train_step_goldens_v1.json"
        );
        assert_eq!(rung.authority.base_artifact_sha256, TRAIN_FIXTURE_SHA256);
        assert_eq!(rung.authority.model_path, "python/mtg_kernel_rl/model.py");
        assert_eq!(rung.authority.model_sha256, golden.authority.model_sha256);
        assert_eq!(
            rung.authority.trainer_path,
            "python/mtg_kernel_rl/trainer.py"
        );
        assert_eq!(
            rung.authority.trainer_sha256,
            golden.authority.trainer_sha256
        );
        assert_eq!(
            rung.authority.forward_fixture_path,
            "data/native_policy_value_net_v1/runner_fixed_forward_goldens_v1.json"
        );
        assert_eq!(
            rung.authority.forward_fixture_sha256,
            golden.authority.forward_fixture_sha256
        );
        assert_eq!(rung.authority.platform_system, "Windows");
        assert_eq!(rung.authority.platform_machine, "AMD64");
        assert_eq!(rung.authority.python_version, "3.13.14");
        assert_eq!(rung.authority.torch_version, "2.13.0+cpu");
        assert_eq!(rung.authority.torch_num_threads, 1);
        assert_eq!(rung.authority.torch_num_interop_threads, 1);
        assert!(rung.authority.torch_deterministic_algorithms);
        assert_eq!(rung.authority.torch_default_dtype, "torch.float32");

        assert_eq!(rung.provenance.base_program_json_pointer, "/steps/2/groups");
        assert_eq!(
            rung.provenance.cycle_rule,
            "rung_group[i] = base_group[i % 32] for i in 0..1024"
        );
        assert_eq!(rung.provenance.base_group_count, 32);
        assert_eq!(rung.provenance.base_substep_count, 40);
        assert_eq!(rung.provenance.cycle_count, 32);
        assert_eq!(rung.provenance.learner_physical_decision_group_count, 1_024);
        assert_eq!(rung.provenance.policy_substep_count, 1_280);
        assert_eq!(golden.steps.len(), 3);
        let base_step = &golden.steps[2];
        assert_eq!(base_step.step, 3);
        assert_eq!(base_step.groups.len(), rung.provenance.base_group_count);
        assert_eq!(
            base_step
                .groups
                .iter()
                .map(|group| group.substeps.len())
                .sum::<usize>(),
            rung.provenance.base_substep_count
        );
        let mut terminal_counts = std::collections::BTreeMap::new();
        for terminal_return in [-1i8, 0, 1] {
            terminal_counts.insert(
                terminal_return.to_string(),
                base_step
                    .groups
                    .iter()
                    .filter(|group| group.terminal_return == terminal_return)
                    .count()
                    * rung.provenance.cycle_count,
            );
        }
        assert_eq!(terminal_counts, rung.provenance.terminal_return_counts);

        assert_eq!(rung.model_state.trainer_algorithm, TRAINER_ALGORITHM_V1);
        assert_eq!(rung.model_state.initializer, "runner-fixed-v1");
        assert_eq!(rung.model_state.adam_step_before, 2);
        assert!(rung
            .model_state
            .reconstruction
            .contains("execute base artifact steps 1 and 2"));
        let authority_state = &golden.steps[1];
        assert_eq!(
            rung.model_state.parameters_sha256,
            authority_state.parameters_after_adam.sha256
        );
        assert_eq!(
            rung.model_state.first_moments_sha256,
            authority_state.first_moments_after_adam.sha256
        );
        assert_eq!(
            rung.model_state.second_moments_sha256,
            authority_state.second_moments_after_adam.sha256
        );

        assert_eq!(
            rung.term_stream.framing,
            "for group_index in 0..1024: u32_le(group_index)||u32_le(policy_term_f32_bits)||u32_le(value_term_f32_bits)"
        );
        assert_eq!(rung.term_stream.base_cycle_terms.len(), 32);
        let mut stream_hasher = Sha256::new();
        let mut sequential_policy_sum = 0.0f32;
        let mut sequential_value_sum = 0.0f32;
        let mut policy_nonzero_count = 0usize;
        let mut value_nonzero_count = 0usize;
        let mut policy_positive_count = 0usize;
        let mut policy_negative_count = 0usize;
        let mut value_positive_count = 0usize;
        for group_index in 0..rung.provenance.learner_physical_decision_group_count {
            let base_group_index = group_index % rung.provenance.base_group_count;
            let term = &rung.term_stream.base_cycle_terms[base_group_index];
            assert_eq!(term.base_group_index, base_group_index);
            let policy_bits = reduction_f32_bits(&term.policy_term_f32_bits);
            let value_bits = reduction_f32_bits(&term.value_term_f32_bits);
            stream_hasher.update((group_index as u32).to_le_bytes());
            stream_hasher.update(policy_bits.to_le_bytes());
            stream_hasher.update(value_bits.to_le_bytes());
            let policy_term = f32::from_bits(policy_bits);
            let value_term = f32::from_bits(value_bits);
            sequential_policy_sum += policy_term;
            sequential_value_sum += value_term;
            policy_nonzero_count += usize::from(policy_bits & 0x7fff_ffff != 0);
            value_nonzero_count += usize::from(value_bits & 0x7fff_ffff != 0);
            policy_positive_count += usize::from(policy_term > 0.0);
            policy_negative_count += usize::from(policy_term < 0.0);
            value_positive_count += usize::from(value_term > 0.0);
        }
        assert_eq!(
            format!("{:x}", stream_hasher.finalize()),
            rung.term_stream.sha256
        );
        assert_eq!(policy_nonzero_count, rung.term_stream.policy_nonzero_count);
        assert_eq!(value_nonzero_count, rung.term_stream.value_nonzero_count);
        assert_eq!(
            policy_positive_count,
            rung.term_stream.policy_positive_count
        );
        assert_eq!(
            policy_negative_count,
            rung.term_stream.policy_negative_count
        );
        assert_eq!(value_positive_count, rung.term_stream.value_positive_count);
        assert!(policy_positive_count > 0 && policy_negative_count > 0);
        assert_eq!(value_nonzero_count, 1_024);

        let sequential_loss = (sequential_policy_sum
            + golden.value_coefficient * sequential_value_sum)
            / rung.provenance.learner_physical_decision_group_count as f32;
        assert_eq!(
            sequential_policy_sum.to_bits(),
            reduction_scalar_bits(
                &rung
                    .reduction
                    .sequential_f32_over_same_torch_term_bits
                    .policy_sum
            )
        );
        assert_eq!(
            sequential_value_sum.to_bits(),
            reduction_scalar_bits(
                &rung
                    .reduction
                    .sequential_f32_over_same_torch_term_bits
                    .value_sum
            )
        );
        assert_eq!(
            sequential_loss.to_bits(),
            reduction_scalar_bits(&rung.reduction.sequential_f32_over_same_torch_term_bits.loss)
        );
        assert!(rung.reduction.torch_operation.contains("torch.stack"));
        assert!(rung.reduction.rust_operation.contains("policy_sum +="));
        assert_eq!(rung.reduction.frozen_tolerance.absolute, 5.0e-5f64);
        assert_eq!(rung.reduction.frozen_tolerance.relative, 5.0e-5f64);
        assert_eq!(
            rung.reduction.frozen_tolerance.comparison_rule,
            "abs(actual-expected) <= absolute + relative*abs(expected)"
        );
        assert!(rung.reduction.all_same_term_comparisons_hold);
        for (name, expected, actual) in [
            (
                "policy_sum",
                rung.reduction.torch_stack.policy_sum.value,
                sequential_policy_sum,
            ),
            (
                "value_sum",
                rung.reduction.torch_stack.value_sum.value,
                sequential_value_sum,
            ),
            (
                "loss",
                rung.reduction.torch_stack.loss.value,
                sequential_loss,
            ),
        ] {
            let comparison = &rung.reduction.same_term_sequential_vs_torch_stack[name];
            let delta = (f64::from(actual) - f64::from(expected)).abs();
            let allowed = rung.reduction.frozen_tolerance.absolute
                + rung.reduction.frozen_tolerance.relative * f64::from(expected).abs();
            assert_eq!(comparison.absolute_delta_f64, delta);
            assert_eq!(comparison.allowed_delta_f64, allowed);
            assert_eq!(comparison.holds, delta <= allowed);
            assert!(
                comparison.holds,
                "same-term reduction exceeds tolerance for {name}"
            );
        }
        reduction_scalar_bits(&rung.reduction.torch_stack.policy_sum);
        reduction_scalar_bits(&rung.reduction.torch_stack.value_sum);
        reduction_scalar_bits(&rung.reduction.torch_stack.loss);

        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        for step in golden
            .steps
            .iter()
            .take(rung.model_state.adam_step_before as usize)
        {
            let expected_outputs = expected_outputs_for_step(state.model_v1(), &forward, step);
            let substeps = substeps_for_step(&forward, step, &expected_outputs);
            let groups = groups_for_step(step, &substeps);
            let result = state
                .train_step_v1(
                    &groups,
                    golden.value_coefficient,
                    golden.optimizer.learning_rate,
                )
                .unwrap();
            assert_eq!(result.adam_step, step.step);
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
                &state.model_v1().parameter_snapshot_v1(),
                &step.parameters_after_adam,
                golden.authority.optimizer_absolute_tolerance,
                golden.authority.optimizer_relative_tolerance,
            );
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
        }
        assert_eq!(state.adam_step_v1(), rung.model_state.adam_step_before);

        let expected_outputs = expected_outputs_for_step(state.model_v1(), &forward, base_step);
        let base_substeps = substeps_for_step(&forward, base_step, &expected_outputs);
        let expanded_substeps = (0..rung.provenance.learner_physical_decision_group_count)
            .map(|group_index| {
                base_substeps[group_index % rung.provenance.base_group_count].clone()
            })
            .collect::<Vec<_>>();
        let expanded_groups = expanded_substeps
            .iter()
            .enumerate()
            .map(|(group_index, substeps)| NativePolicyPhysicalDecisionV1 {
                substeps,
                terminal_return: base_step.groups[group_index % rung.provenance.base_group_count]
                    .terminal_return,
            })
            .collect::<Vec<_>>();
        let result = state
            .train_step_v1(
                &expanded_groups,
                golden.value_coefficient,
                golden.optimizer.learning_rate,
            )
            .unwrap();
        assert_eq!(result.adam_step, rung.model_state.adam_step_before + 1);
        assert_eq!(state.adam_step_v1(), result.adam_step);
        assert_eq!(
            result.physical_terms.len(),
            rung.provenance.learner_physical_decision_group_count
        );
        assert_eq!(
            result.selected_outputs.len(),
            rung.provenance.policy_substep_count
        );

        let mut reproduced_policy_sum = 0.0f32;
        let mut reproduced_value_sum = 0.0f32;
        let mut actual_policy_positive = 0usize;
        let mut actual_policy_negative = 0usize;
        let mut actual_value_nonzero = 0usize;
        for term in &result.physical_terms {
            let target = f32::from(term.terminal_return);
            let advantage = target - term.value;
            let policy_term = -term.joint_log_probability * advantage;
            let value_error = term.value - target;
            let value_term = value_error * value_error;
            reproduced_policy_sum += policy_term;
            reproduced_value_sum += value_term;
            actual_policy_positive += usize::from(policy_term > 0.0);
            actual_policy_negative += usize::from(policy_term < 0.0);
            actual_value_nonzero += usize::from(value_term.to_bits() & 0x7fff_ffff != 0);
        }
        let reproduced_loss = (reproduced_policy_sum
            + golden.value_coefficient * reproduced_value_sum)
            / result.physical_terms.len() as f32;
        assert_eq!(result.policy_sum.to_bits(), reproduced_policy_sum.to_bits());
        assert_eq!(result.value_sum.to_bits(), reproduced_value_sum.to_bits());
        assert_eq!(result.loss.to_bits(), reproduced_loss.to_bits());
        assert!(actual_policy_positive > 0 && actual_policy_negative > 0);
        assert_eq!(actual_value_nonzero, result.physical_terms.len());

        assert_close(
            result.policy_sum,
            rung.reduction.torch_stack.policy_sum.value,
            rung.reduction.frozen_tolerance.absolute as f32,
            rung.reduction.frozen_tolerance.relative as f32,
        );
        assert_close(
            result.value_sum,
            rung.reduction.torch_stack.value_sum.value,
            rung.reduction.frozen_tolerance.absolute as f32,
            rung.reduction.frozen_tolerance.relative as f32,
        );
        assert_close(
            result.loss,
            rung.reduction.torch_stack.loss.value,
            rung.reduction.frozen_tolerance.absolute as f32,
            rung.reduction.frozen_tolerance.relative as f32,
        );
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
        assert!(golden.steps.iter().any(|step| step.groups.len() == 32));

        for step in &golden.steps {
            let expected_outputs = expected_outputs_for_step(state.model_v1(), &forward, step);
            let substeps = substeps_for_step(&forward, step, &expected_outputs);
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
                let NativePolicyForwardInputV1::Encoded(encoded) = &substep.forward else {
                    panic!("finite-difference oracle requires encoded reference input");
                };
                let output = model.forward_v1(**encoded).unwrap();
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
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let expected_outputs = expected_outputs_for_step(&model, &forward, step);
        let substeps = substeps_for_step(&forward, step, &expected_outputs);
        let groups = groups_for_step(step, &substeps);
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
                let NativePolicyForwardInputV1::Encoded(encoded) = &group.substeps[0].forward
                else {
                    panic!("finite-difference oracle requires encoded reference input");
                };
                let output = model.forward_v1(**encoded).unwrap();
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

    fn trained_state_after_first_golden_step() -> NativePolicyValueTrainStateV1 {
        let (forward, golden) = fixtures();
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        let step = &golden.steps[0];
        let expected_outputs = expected_outputs_for_step(state.model_v1(), &forward, step);
        let substeps = substeps_for_step(&forward, step, &expected_outputs);
        let groups = groups_for_step(step, &substeps);
        state
            .train_step_v1(
                &groups,
                golden.value_coefficient,
                golden.optimizer.learning_rate,
            )
            .unwrap();
        state
    }

    fn assert_snapshot_rejected_without_drift(
        state: &mut NativePolicyValueTrainStateV1,
        baseline: &NativePolicyValueTrainSnapshotV1,
        standalone_validation_must_reject: bool,
        corrupt: impl FnOnce(&mut NativePolicyValueTrainSnapshotV1),
    ) {
        let before_hash = state.state_sha256_v1().unwrap();
        let mut candidate = baseline.clone();
        corrupt(&mut candidate);
        if standalone_validation_must_reject {
            assert!(candidate.state_sha256_v1().is_err());
        }
        assert!(state.replace_snapshot_v1(&candidate).is_err());
        assert_eq!(state.state_sha256_v1().unwrap(), before_hash);
        assert_eq!(state.snapshot_v1().unwrap(), *baseline);
    }

    fn hex_sha256(digest: [u8; 32]) -> String {
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn complete_train_snapshot_roundtrips_and_continues_bit_exactly() {
        let mut original = trained_state_after_first_golden_step();
        let snapshot = original.snapshot_v1().unwrap();
        let original_hash = original.state_sha256_v1().unwrap();
        assert_eq!(snapshot.state_sha256_v1().unwrap(), original_hash);
        assert_eq!(snapshot.adam_step, 1);
        assert_eq!(snapshot.parameters.len(), PARAMETER_TENSOR_COUNT);
        assert_eq!(snapshot.first_moments.len(), PARAMETER_TENSOR_COUNT);
        assert_eq!(snapshot.second_moments.len(), PARAMETER_TENSOR_COUNT);
        assert_eq!(
            snapshot.scorer_bias_anchor_bits,
            original.scorer_bias_anchor_bits
        );
        assert_eq!(
            snapshot.parameters[SCORER_SECOND_BIAS].values[0].to_bits(),
            snapshot.scorer_bias_anchor_bits
        );
        assert_eq!(
            snapshot.first_moments[SCORER_SECOND_BIAS].values[0].to_bits(),
            0
        );
        assert_eq!(
            snapshot.second_moments[SCORER_SECOND_BIAS].values[0].to_bits(),
            0
        );

        let template =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut restored =
            NativePolicyValueTrainStateV1::from_snapshot_v1(template, &snapshot).unwrap();
        assert_eq!(restored.snapshot_v1().unwrap(), snapshot);
        assert_eq!(restored.state_sha256_v1().unwrap(), original_hash);

        let replacement_model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut replaced = NativePolicyValueTrainStateV1::new_v1(replacement_model).unwrap();
        replaced.replace_snapshot_v1(&snapshot).unwrap();
        assert_eq!(replaced.snapshot_v1().unwrap(), snapshot);
        assert_eq!(replaced.state_sha256_v1().unwrap(), original_hash);

        let (forward, golden) = fixtures();
        let step = &golden.steps[1];
        let expected_outputs = expected_outputs_for_step(original.model_v1(), &forward, step);
        let substeps = substeps_for_step(&forward, step, &expected_outputs);
        let groups = groups_for_step(step, &substeps);
        let original_result = original
            .train_step_v1(
                &groups,
                golden.value_coefficient,
                golden.optimizer.learning_rate,
            )
            .unwrap();
        let restored_result = restored
            .train_step_v1(
                &groups,
                golden.value_coefficient,
                golden.optimizer.learning_rate,
            )
            .unwrap();
        let replaced_result = replaced
            .train_step_v1(
                &groups,
                golden.value_coefficient,
                golden.optimizer.learning_rate,
            )
            .unwrap();
        assert_eq!(restored_result, original_result);
        assert_eq!(replaced_result, original_result);
        assert_eq!(
            restored.snapshot_v1().unwrap(),
            original.snapshot_v1().unwrap()
        );
        assert_eq!(
            replaced.snapshot_v1().unwrap(),
            original.snapshot_v1().unwrap()
        );
        assert_eq!(
            restored.state_sha256_v1().unwrap(),
            original.state_sha256_v1().unwrap()
        );
        assert_eq!(
            replaced.state_sha256_v1().unwrap(),
            original.state_sha256_v1().unwrap()
        );
    }

    #[test]
    fn train_state_hash_is_frozen_complete_and_domain_separated() {
        assert_eq!(
            NATIVE_TRAIN_STATE_SHA256_IDENTITY_V1,
            "mtg-kernel-native-policy-value-train-state-sha256-v1"
        );
        let model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        let baseline = state.snapshot_v1().unwrap();
        let baseline_hash = baseline.state_sha256_v1().unwrap();
        assert_eq!(
            hex_sha256(baseline_hash),
            "02450f0bbfc223c3f3dc925da11ff20190fc86bc9c6ac3f0519aace7706cdf31"
        );

        let mut step = baseline.clone();
        step.adam_step = 1;
        assert_ne!(step.state_sha256_v1().unwrap(), baseline_hash);

        let mut parameter = baseline.clone();
        parameter.parameters[OBJECT_FIRST_WEIGHT].values[0] += f32::EPSILON;
        assert_ne!(parameter.state_sha256_v1().unwrap(), baseline_hash);

        let mut first = baseline.clone();
        first.first_moments[OBJECT_FIRST_WEIGHT].values[0] = 0.25;
        assert_ne!(first.state_sha256_v1().unwrap(), baseline_hash);

        let mut second = baseline.clone();
        second.second_moments[OBJECT_FIRST_WEIGHT].values[0] = 0.5;
        assert_ne!(second.state_sha256_v1().unwrap(), baseline_hash);

        let mut section_a = baseline.clone();
        section_a.first_moments[OBJECT_FIRST_WEIGHT].values[0] = 0.25;
        section_a.second_moments[OBJECT_FIRST_WEIGHT].values[0] = 0.5;
        let mut section_b = baseline;
        section_b.first_moments[OBJECT_FIRST_WEIGHT].values[0] = 0.5;
        section_b.second_moments[OBJECT_FIRST_WEIGHT].values[0] = 0.25;
        assert_ne!(
            section_a.state_sha256_v1().unwrap(),
            section_b.state_sha256_v1().unwrap()
        );
    }

    #[test]
    fn snapshot_corruption_classes_fail_closed_without_state_or_hash_drift() {
        let mut state = trained_state_after_first_golden_step();
        let baseline = state.snapshot_v1().unwrap();

        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.parameters.swap(1, 2);
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.parameters.pop();
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.parameters[1].name = "wrong.weight";
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.parameters[1].shape[0] += 1;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.parameters[1].values.pop();
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.parameters[1].values[0] = f32::NAN;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.parameters[CARD_EMBEDDING].values[0] = -0.0;
        });

        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.first_moments.swap(1, 2);
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.first_moments.pop();
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.first_moments[1].name = "wrong.weight";
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.first_moments[1].shape[0] += 1;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.first_moments[1].values.pop();
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.first_moments[1].values[0] = f32::INFINITY;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.first_moments[CARD_EMBEDDING].values[0] = -0.0;
        });

        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.second_moments.swap(1, 2);
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.second_moments.pop();
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.second_moments[1].name = "wrong.weight";
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.second_moments[1].shape[0] += 1;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.second_moments[1].values.pop();
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.second_moments[1].values[0] = f32::NEG_INFINITY;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.second_moments[1].values[0] = -f32::EPSILON;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.second_moments[CARD_EMBEDDING].values[0] = -0.0;
        });

        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.first_moments[SCORER_SECOND_BIAS].values[0] = -0.0;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.second_moments[SCORER_SECOND_BIAS].values[0] = f32::EPSILON;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.parameters[SCORER_SECOND_BIAS].values[0] += 0.25;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.scorer_bias_anchor_bits ^= 1;
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, false, |snapshot| {
            snapshot.parameters[SCORER_SECOND_BIAS].values[0] += 0.25;
            snapshot.scorer_bias_anchor_bits =
                snapshot.parameters[SCORER_SECOND_BIAS].values[0].to_bits();
        });
        assert_snapshot_rejected_without_drift(&mut state, &baseline, true, |snapshot| {
            snapshot.adam_step = i32::MAX as u64 + 1;
        });

        let template =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        let mut moved_anchor = baseline.clone();
        moved_anchor.parameters[SCORER_SECOND_BIAS].values[0] += 0.25;
        moved_anchor.scorer_bias_anchor_bits =
            moved_anchor.parameters[SCORER_SECOND_BIAS].values[0].to_bits();
        assert_eq!(
            NativePolicyValueTrainStateV1::from_snapshot_v1(template, &moved_anchor).unwrap_err(),
            NativePolicyTrainErrorV1::GaugeAnchor
        );
    }

    #[test]
    fn invalid_live_gauge_anchor_or_padding_state_receives_no_snapshot_or_hash() {
        let state = trained_state_after_first_golden_step();

        let mut moved_anchor = state.clone();
        let mut parameters = moved_anchor.model.parameter_snapshot_v1();
        parameters[SCORER_SECOND_BIAS].values[0] += 0.25;
        moved_anchor
            .model
            .replace_parameter_snapshot_v1(&parameters)
            .unwrap();
        assert_eq!(
            moved_anchor.snapshot_v1().unwrap_err(),
            NativePolicyTrainErrorV1::GaugeAnchor
        );
        assert_eq!(
            moved_anchor.state_sha256_v1().unwrap_err(),
            NativePolicyTrainErrorV1::GaugeAnchor
        );

        let mut bad_padding_moment = state;
        bad_padding_moment.first_moments[CARD_EMBEDDING][0] = -0.0;
        assert_eq!(
            bad_padding_moment.snapshot_v1().unwrap_err(),
            NativePolicyTrainErrorV1::OptimizerState
        );
        assert_eq!(
            bad_padding_moment.state_sha256_v1().unwrap_err(),
            NativePolicyTrainErrorV1::OptimizerState
        );
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

        let mut high_precision_exceeded = ScorerBiasGaugeAccumulatorV1::default();
        high_precision_exceeded.observe(&logits, 2, -0.75).unwrap();
        high_precision_exceeded.high_precision_residual = 1.0;
        assert!(matches!(
            high_precision_exceeded.finish(0.0, (-0.05f32).to_bits()),
            Err(NativePolicyTrainErrorV1::GaugeHighPrecisionResidualExceeded { .. })
        ));

        let mut high_precision_nonfinite = ScorerBiasGaugeAccumulatorV1::default();
        high_precision_nonfinite.observe(&logits, 2, -0.75).unwrap();
        high_precision_nonfinite.high_precision_residual = f64::NAN;
        assert!(matches!(
            high_precision_nonfinite.finish(0.0, (-0.05f32).to_bits()),
            Err(NativePolicyTrainErrorV1::GaugeHighPrecisionResidualExceeded { .. })
        ));
    }

    #[test]
    fn group_count_conversion_is_exact_or_rejected() {
        assert_eq!(
            exact_group_count_f32(1).unwrap().to_bits(),
            1.0f32.to_bits()
        );
        let largest_exact = 1usize << f32::MANTISSA_DIGITS;
        assert_eq!(
            exact_group_count_f32(largest_exact).unwrap().to_bits(),
            (largest_exact as f32).to_bits()
        );
        let first_inexact = largest_exact + 1;
        assert_eq!(
            exact_group_count_f32(first_inexact),
            Err(
                NativePolicyTrainErrorV1::GroupCountNotExactlyRepresentable {
                    group_count: first_inexact,
                }
            )
        );
        let larger_exact = largest_exact << 1;
        assert_eq!(
            exact_group_count_f32(larger_exact).unwrap().to_bits(),
            (larger_exact as f32).to_bits()
        );
        let saturating_roundtrip_trap = u32::MAX as usize;
        assert_eq!(
            exact_group_count_f32(saturating_roundtrip_trap),
            Err(
                NativePolicyTrainErrorV1::GroupCountNotExactlyRepresentable {
                    group_count: saturating_roundtrip_trap,
                }
            )
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
        let case_output = state.model_v1().forward_v1(encoded(case)).unwrap();
        let case_logit_bits = case_output
            .logits
            .iter()
            .map(|logit| logit.to_bits())
            .collect::<Vec<_>>();
        let bad_selected = [NativePolicySubstepV1 {
            forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded(case))),
            selected_action_index: usize::MAX,
            expected_raw_action_logit_bits: &case_logit_bits,
            expected_value_bits: case_output.value.to_bits(),
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
            forward: NativePolicyForwardInputV1::Encoded(Box::new(malformed)),
            selected_action_index: 0,
            expected_raw_action_logit_bits: &[],
            expected_value_bits: 0,
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
            forward: NativePolicyForwardInputV1::Encoded(Box::new(encoded(case))),
            selected_action_index: 1,
            expected_raw_action_logit_bits: &case_logit_bits,
            expected_value_bits: case_output.value.to_bits(),
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
