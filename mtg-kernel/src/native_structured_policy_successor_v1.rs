//! Strict runtime for the policy-only complete-history structured successor.
//!
//! The structured weights own the policy logits.  The retained XMage CP7
//! outcome parent is loaded solely for its bit-identical value prediction and
//! for the composite identity binding.

use crate::flat_policy_v2::FlatScoringDecisionViewV2;
use crate::native_flat_tensorizer_v2::{
    NativeFlatDecisionTensorV2, NativeFlatTensorizerV2, NATIVE_FLAT_ACTION_FEATURE_DIM_V2,
    NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2, NATIVE_FLAT_EDGE_FEATURE_DIM_V2,
    NATIVE_FLAT_OBJECT_FEATURE_DIM_V2, NATIVE_FLAT_STATE_FEATURE_DIM_V2,
};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1,
};
use crate::native_structured_policy_residual_v1::{
    decode_structured_residual_parameters_v1, expected_history_parameters_v1, lower_hex_v1,
    parse_lower_hex32_v1, raw_sha256_v1, structured_residual_v1, NativeStructuredHistoryEntryV1,
    TensorV1, CARD_EMBEDDING_DIM_V1, CARD_VOCAB_V1, GROUP_EMBEDDING_DIM_V1, HIDDEN_DIM_V1,
    HISTORY_FEATURE_DIM_V1, HISTORY_GROUP_VOCAB_V1, HISTORY_LENGTH_V1, HISTORY_PARAMETER_COUNT_V1,
    HISTORY_ROLE_DIM_V1, PARENT_ADAM_STEP_V1, PARENT_MANIFEST_SHA256_V1,
    PARENT_MODEL_PARAMETER_SHA256_V1, PARENT_NATIVE_STATE_SHA256_V1, PARENT_PAYLOAD_SHA256_V1,
};
use crate::native_xmage_cp7_outcome_reinforce_v1::{
    load_xmage_cp7_outcome_inference_v1, NativeXmageCp7OutcomeInferenceV1,
};
use crate::rl::parse_strict_json_value;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::io;
use std::mem::size_of;
use std::path::Path;

pub(crate) const CANDIDATE_FILENAME_V1: &str = "structured_policy_successor.json";
pub(crate) const REPORT_FILENAME_V1: &str = "report.json";
pub(crate) const WEIGHTS_FILENAME_V1: &str = "weights.f32le";
pub(crate) const PARENT_DIRECTORY_V1: &str = "parent";
pub(crate) const PARENT_MANIFEST_FILENAME_V1: &str = "checkpoint.json";
pub(crate) const PARENT_STATE_FILENAME_V1: &str = "checkpoint.state.f32le";
pub(crate) const CANDIDATE_SCHEMA_V1: &str = "mtg-kernel-structured-policy-successor-candidate/v1";
pub(crate) const REPORT_SCHEMA_V1: &str = "mtg-kernel-structured-policy-successor-fit/v1";
pub(crate) const CANDIDATE_SCHEMA_V2: &str = "mtg-kernel-structured-policy-successor-candidate/v2";
pub(crate) const REPORT_SCHEMA_V2: &str = "mtg-kernel-structured-policy-terminal-rung-report/v1";
pub(crate) const CANDIDATE_SCHEMA_V3: &str = "mtg-kernel-structured-policy-successor-candidate/v3";
pub(crate) const REPORT_SCHEMA_V3: &str =
    "mtg-kernel-structured-policy-terminal-trust-projection-report/v1";
pub(crate) const CANDIDATE_SCHEMA_V4: &str = "mtg-kernel-structured-policy-successor-candidate/v4";
pub(crate) const REPORT_SCHEMA_V4: &str =
    "mtg-kernel-structured-policy-terminal-head-only-report/v1";
pub(crate) const CANDIDATE_SCHEMA_V5: &str = "mtg-kernel-structured-policy-successor-candidate/v5";
pub(crate) const REPORT_SCHEMA_V5: &str =
    "mtg-kernel-structured-policy-space-response-oracle-report/v1";
pub(crate) const CANDIDATE_SCHEMA_V6: &str = "mtg-kernel-structured-policy-successor-candidate/v6";
pub(crate) const REPORT_SCHEMA_V6: &str =
    "mtg-kernel-structured-policy-block-response-oracle-report/v1";
pub(crate) const CANDIDATE_SCHEMA_V7: &str = "mtg-kernel-structured-policy-successor-candidate/v7";
pub(crate) const REPORT_SCHEMA_V7: &str =
    "mtg-kernel-structured-policy-bounded-logit-residual-report/v1";
pub(crate) const PUBLICATION_ENCODING_V1: &str = "json-pretty-sorted-utf8-trailing-lf/v1";
pub(crate) const WEIGHTS_ENCODING_V1: &str = "ordered-row-major-finite-f32-little-endian/v1";
pub(crate) const ARCHITECTURE_V1: &str =
    "complete-public-history-structured-policy-successor-frozen-parent-value/v1";
pub(crate) const ARCHITECTURE_V2: &str =
    "complete-public-history-structured-policy-terminal-rung-frozen-parent-value/v1";
pub(crate) const ARCHITECTURE_V3: &str =
    "complete-public-history-structured-policy-terminal-rung-projected-frozen-parent-value/v1";
pub(crate) const ARCHITECTURE_V4: &str =
    "complete-public-history-structured-policy-terminal-head-only-frozen-parent-value/v1";
pub(crate) const ARCHITECTURE_V5: &str =
    "complete-public-history-structured-policy-space-response-oracle-frozen-parent-value/v1";
pub(crate) const ARCHITECTURE_V6: &str =
    "complete-public-history-structured-policy-block-response-oracle-frozen-parent-value/v1";
pub(crate) const ARCHITECTURE_V7: &str =
    "complete-public-history-structured-policy-bounded-logit-residual-frozen-parent-value/v1";
pub(crate) const VALUE_MODEL_V1: &str = "exact-retained-parent-frozen/v1";
pub(crate) const COMPOSITE_DOMAIN_V1: &[u8] =
    b"mtg-kernel-structured-policy-successor-composite-model/v1";
pub(crate) const COMPOSITE_DOMAIN_V2: &[u8] =
    b"mtg-kernel-structured-policy-terminal-rung-composite-model/v1";
pub(crate) const COMPOSITE_DOMAIN_V3: &[u8] =
    b"mtg-kernel-structured-policy-terminal-trust-projection-composite-model/v1";
pub(crate) const COMPOSITE_DOMAIN_V4: &[u8] =
    b"mtg-kernel-structured-policy-terminal-head-only-composite-model/v1";
pub(crate) const COMPOSITE_DOMAIN_V5: &[u8] =
    b"mtg-kernel-structured-policy-space-response-oracle-composite-model/v1";
pub(crate) const COMPOSITE_DOMAIN_V6: &[u8] =
    b"mtg-kernel-structured-policy-block-response-oracle-composite-model/v1";
pub(crate) const COMPOSITE_DOMAIN_V7: &[u8] =
    b"mtg-kernel-structured-policy-bounded-logit-residual-composite-model/v1";
const SOURCE_CACHE_SHA256_V1: &str =
    "280e34cd7f685beaf52c1cab3b41c53613a5029c063871942f48c063b6f5996f";
const SOURCE_PAIR_COUNT_V1: u64 = 2_048;
const FIT_SEED_V1: u64 = 20_260_804;
const TERMINAL_RUNG_INITIALIZER_CANDIDATE_SHA256_V1: &str =
    "204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72";
const TERMINAL_RUNG_INITIALIZER_REPORT_SHA256_V1: &str =
    "7d854edb46119a611d4283e6cf4630d0207ceb24c12b4089a7d27a43c97fe0b3";
const TERMINAL_RUNG_INITIALIZER_WEIGHTS_SHA256_V1: &str =
    "ca3c45cd69d8d60f1f921bc78c27b098064ef6b16fe7566b84e5045681781b28";
const TERMINAL_RUNG_INITIALIZER_COMPOSITE_SHA256_V1: &str =
    "47b10c1114efc01f9445c71c0c8c4d8cd4a4b89a2154ac68275f3b0c6ebb9ce3";
const TERMINAL_RUNG_INITIALIZER_MODEL_STATE_SHA256_V1: &str =
    "ff2abf50e8760780a9331e53aa7323cb96e3c64edb6e7d89062dbe38bf6a5cc0";
const TERMINAL_RUNG_POOL_SHA256_V1: &str =
    "6c3c8ff09ab519dc9f462b41cbf898da902d230656d14e64d79fc66a19f3bc71";
const TERMINAL_RUNG_BASE_SEED_V1: u64 = 1_660_001;
const TERMINAL_RUNG_FIT_SEED_V1: u64 = 20_260_805;
const AUTHORITY_KIND_V1: &str = "xmage-cp7-outcome-structured-policy-successor-v1";
const AUTHORITY_KIND_V2: &str = "xmage-cp7-outcome-structured-policy-successor-v2";
const AUTHORITY_KIND_V3: &str = "xmage-cp7-outcome-structured-policy-successor-v3";
const AUTHORITY_KIND_V4: &str = "xmage-cp7-outcome-structured-policy-successor-v4";
const AUTHORITY_KIND_V5: &str = "xmage-cp7-outcome-structured-policy-successor-v5";
const AUTHORITY_KIND_V6: &str = "xmage-cp7-outcome-structured-policy-successor-v6";
const AUTHORITY_KIND_V7: &str = "xmage-cp7-outcome-structured-policy-successor-v7";
const TERMINAL_TRUST_SOURCE_FIT_REPORT_SHA256_V1: &str =
    "355c1b179ccd5de5d16f0aeb39dc101ae97a876208a2315358f98b06dcc30a81";
const TERMINAL_TRUST_SOURCE_MODEL_STATE_SHA256_V1: &str =
    "4d1e9853d3472eb8817c10051c5ff779258bc1fc26130e956492ad598c877fe9";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SuccessorVersionV1 {
    DistilledV1,
    TerminalRungV2,
    TerminalTrustProjectionV3,
    TerminalHeadOnlyV4,
    PolicySpaceResponseOracleV5,
    PolicyBlockResponseOracleV6,
    BoundedLogitResidualV7,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateV1 {
    schema: String,
    publication_encoding: String,
    parent: ParentBindingV1,
    architecture: ArchitectureV1,
    weights: WeightsBindingV1,
    report: ReportBindingV1,
    composite_model_parameter_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParentBindingV1 {
    directory: String,
    manifest_sha256: String,
    payload_sha256: String,
    native_state_sha256: String,
    model_parameter_sha256: String,
    adam_step: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchitectureV1 {
    identity: String,
    state_dim: usize,
    object_dim: usize,
    edge_dim: usize,
    action_dim: usize,
    ref_dim: usize,
    hidden_dim: usize,
    card_vocab: usize,
    card_embedding_dim: usize,
    group_vocab: usize,
    group_embedding_dim: usize,
    history_length: usize,
    history_feature_dim: usize,
    history_role_dim: usize,
    value_model: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WeightsBindingV1 {
    filename: String,
    encoding: String,
    sha256: String,
    byte_count: usize,
    parameter_count: usize,
    parameters: Vec<ParameterBindingV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterBindingV1 {
    name: String,
    shape: Vec<usize>,
    offset_f32: usize,
    count_f32: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportBindingV1 {
    filename: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportV1 {
    schema: String,
    source: ReportSourceV1,
    config: ReportConfigV1,
    policy_metrics: ReportPolicyMetricsV1,
    transport: ReportTransportV1,
    weights_sha256: String,
    composite_model_parameter_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportSourceV1 {
    cache_sha256: String,
    pair_count: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportConfigV1 {
    architecture: String,
    value_model: String,
    seed: u64,
    epochs: u64,
    batch_size_physical_decisions: u64,
    learning_rate: f64,
    weight_decay: f64,
    gradient_norm_cap: f64,
    history_length: u64,
    history_feature_dim: u64,
    weighting: String,
    objective: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportPolicyMetricsV1 {
    overall: ReportMetricV1,
    by_candidate_seat: BTreeMap<String, ReportMetricV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportMetricV1 {
    mean_total_variation: f64,
    p90_total_variation: f64,
    top_action_agreement: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportTransportV1 {
    maximum_absolute_logit_error: f64,
    parent_value_bit_exact: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportV2 {
    schema: String,
    initializer: ReportInitializerV2,
    source: ReportSourceV2,
    config: ReportConfigV2,
    movement: ReportMovementV2,
    transport: ReportTransportV1,
    weights_sha256: String,
    composite_model_parameter_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportInitializerV2 {
    candidate_json_sha256: String,
    report_sha256: String,
    weights_sha256: String,
    composite_model_parameter_sha256: String,
    model_state_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportSourceV2 {
    cache_sha256: String,
    pair_count: u64,
    base_seed: u64,
    pool_json_sha256: String,
    source_commit: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportConfigV2 {
    architecture: String,
    value_model: String,
    seed: u64,
    epochs: u64,
    batch_size_physical_decisions: u64,
    learning_rate: f64,
    weight_decay: f64,
    gradient_norm_cap: f64,
    ppo_clip: f64,
    history_length: u64,
    history_feature_dim: u64,
    weighting: String,
    advantage: String,
    objective: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportMovementV2 {
    overall: ReportMovementMetricV2,
    by_candidate_seat: BTreeMap<String, ReportMovementMetricV2>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportMovementMetricV2 {
    mean_total_variation: f64,
    p90_total_variation: f64,
    weighted_mean_kl: f64,
    top_action_agreement: f64,
    maximum_absolute_joint_log_ratio: f64,
    policy_mass: f64,
    policy_rows: u64,
    physical_decisions: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportV3 {
    schema: String,
    initializer: ReportInitializerV2,
    source: ReportSourceV3,
    config: ReportConfigV3,
    movement: ReportMovementV2,
    transport: ReportTransportV1,
    weights_sha256: String,
    composite_model_parameter_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportSourceV3 {
    cache_sha256: String,
    pair_count: u64,
    base_seed: u64,
    pool_json_sha256: String,
    source_commit: String,
    rejected_fit_report_sha256: String,
    rejected_model_state_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportConfigV3 {
    architecture: String,
    value_model: String,
    seed: u64,
    epochs: u64,
    batch_size_physical_decisions: u64,
    learning_rate: f64,
    weight_decay: f64,
    gradient_norm_cap: f64,
    ppo_clip: f64,
    history_length: u64,
    history_feature_dim: u64,
    weighting: String,
    advantage: String,
    objective: String,
    projection_method: String,
    projection_scale: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportV4 {
    schema: String,
    initializer: ReportInitializerV2,
    source: ReportSourceV2,
    config: ReportConfigV4,
    movement: ReportMovementV2,
    transport: ReportTransportV1,
    weights_sha256: String,
    composite_model_parameter_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportConfigV4 {
    architecture: String,
    value_model: String,
    seed: u64,
    epochs: u64,
    batch_size_physical_decisions: u64,
    learning_rate: f64,
    weight_decay: f64,
    gradient_norm_cap: f64,
    ppo_clip: f64,
    history_length: u64,
    history_feature_dim: u64,
    weighting: String,
    advantage: String,
    objective: String,
    trainable_parameter: String,
    trainable_parameter_count: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportV5 {
    schema: String,
    initializer: ReportInitializerV2,
    source: ReportSourceV5,
    config: ReportConfigV5,
    adapter: ReportAdapterV5,
    movement: Option<ReportMovementV2>,
    transport: ReportTransportV1,
    weights_sha256: String,
    composite_model_parameter_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportSourceV5 {
    source_commit: String,
    oracle_seed: u64,
    phase: String,
    generation: u64,
    candidate_index: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportConfigV5 {
    architecture: String,
    value_model: String,
    optimizer: String,
    adapter_parameter: String,
    adapter_parameter_count: u64,
    adapter_maximum_absolute_delta: f64,
    development_only: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportAdapterV5 {
    delta_f32_bits: Vec<String>,
    l2_squared: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportV6 {
    schema: String,
    initializer: ReportInitializerV2,
    source: ReportSourceV6,
    config: ReportConfigV6,
    adapter: ReportAdapterV6,
    movement: Option<ReportMovementV2>,
    transport: ReportTransportV1,
    weights_sha256: String,
    composite_model_parameter_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportSourceV6 {
    source_commit: String,
    oracle_seed: u64,
    phase: String,
    generation: u64,
    candidate_index: u64,
    rejected_fit_report_sha256: String,
    rejected_model_state_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportConfigV6 {
    architecture: String,
    value_model: String,
    optimizer: String,
    adapter: String,
    block_count: u64,
    coefficient_maximum: f64,
    coefficient_l1_budget: f64,
    development_only: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportAdapterV6 {
    block_names: Vec<String>,
    coefficients_f32_bits: Vec<String>,
    coefficient_l1: f64,
    coefficient_l2_squared: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportV7 {
    schema: String,
    initializer: ReportInitializerV2,
    source: ReportSourceV3,
    config: ReportConfigV7,
    movement: ReportMovementV7,
    transport: ReportTransportV1,
    weights_sha256: String,
    composite_model_parameter_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportConfigV7 {
    architecture: String,
    value_model: String,
    projection_method: String,
    logit_residual_clip: f64,
    screen_report_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportMovementV7 {
    overall: ReportMovementMetricV7,
    by_candidate_seat: BTreeMap<String, ReportMovementMetricV7>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportMovementMetricV7 {
    mean_total_variation: f64,
    p90_total_variation: f64,
    top_action_agreement: f64,
    top_action_change_rate: f64,
    maximum_absolute_joint_log_ratio: f64,
    policy_mass: f64,
    policy_rows: u64,
    physical_decisions: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeStructuredPolicySuccessorOutputV1 {
    logits: Vec<f32>,
    value: f32,
}

impl NativeStructuredPolicySuccessorOutputV1 {
    pub(crate) fn logits_v1(&self) -> &[f32] {
        &self.logits
    }

    pub(crate) const fn value_v1(&self) -> f32 {
        self.value
    }
}

/// The successor owns policy logits absolutely.  Parent policy logits must
/// never enter this composition.
pub(crate) fn compose_structured_policy_successor_output_v1(
    structured_logits: Vec<f32>,
    frozen_parent_value: f32,
) -> Result<NativeStructuredPolicySuccessorOutputV1, ()> {
    if structured_logits.is_empty()
        || structured_logits.iter().any(|value| !value.is_finite())
        || !frozen_parent_value.is_finite()
    {
        return Err(());
    }
    Ok(NativeStructuredPolicySuccessorOutputV1 {
        logits: structured_logits,
        value: frozen_parent_value,
    })
}

pub(crate) fn bounded_structured_logit_residual_v1(
    initializer_logits: &[f32],
    trained_logits: &[f32],
    clip: f32,
) -> Result<Vec<f32>, ()> {
    if initializer_logits.is_empty()
        || initializer_logits.len() != trained_logits.len()
        || !clip.is_finite()
        || clip < 0.0
        || initializer_logits.iter().any(|value| !value.is_finite())
        || trained_logits.iter().any(|value| !value.is_finite())
    {
        return Err(());
    }
    let maximum = initializer_logits
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max) as f64;
    let mut probability = initializer_logits
        .iter()
        .map(|value| ((*value as f64) - maximum).exp())
        .collect::<Vec<_>>();
    let normalization = probability.iter().sum::<f64>();
    if !normalization.is_finite() || normalization <= 0.0 {
        return Err(());
    }
    for value in &mut probability {
        *value /= normalization;
    }
    let center = probability
        .iter()
        .zip(initializer_logits.iter().zip(trained_logits))
        .map(|(weight, (initial, trained))| weight * ((*trained as f64) - (*initial as f64)))
        .sum::<f64>();
    let bound = clip as f64;
    let logits = initializer_logits
        .iter()
        .zip(trained_logits)
        .map(|(initial, trained)| {
            ((*initial as f64)
                + (((*trained as f64) - (*initial as f64) - center).clamp(-bound, bound)))
                as f32
        })
        .collect::<Vec<_>>();
    logits
        .iter()
        .all(|value| value.is_finite())
        .then_some(logits)
        .ok_or(())
}

pub(crate) struct NativeStructuredPolicySuccessorInferenceV1 {
    parent: NativeXmageCp7OutcomeInferenceV1,
    parameters: BTreeMap<String, TensorV1>,
    initializer_parameters: Option<BTreeMap<String, TensorV1>>,
    logit_residual_clip: Option<f32>,
    candidate_json_sha256: [u8; 32],
    weights_sha256: [u8; 32],
    report_sha256: [u8; 32],
    composite_model_parameter_sha256: [u8; 32],
    authority_kind: &'static str,
}

impl NativeStructuredPolicySuccessorInferenceV1 {
    pub(crate) const fn candidate_json_sha256_v1(&self) -> [u8; 32] {
        self.candidate_json_sha256
    }

    pub(crate) const fn weights_sha256_v1(&self) -> [u8; 32] {
        self.weights_sha256
    }

    pub(crate) const fn report_sha256_v1(&self) -> [u8; 32] {
        self.report_sha256
    }

    pub(crate) const fn composite_model_parameter_sha256_v1(&self) -> [u8; 32] {
        self.composite_model_parameter_sha256
    }

    pub(crate) const fn parent_adam_step_v1(&self) -> u64 {
        self.parent.adam_step_v1()
    }

    pub(crate) const fn authority_kind_v1(&self) -> &'static str {
        self.authority_kind
    }

    pub(crate) fn score_decision_with_history_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
    ) -> Result<NativeStructuredPolicySuccessorOutputV1, ()> {
        if history.len() > HISTORY_LENGTH_V1 || acting_player > 1 {
            return Err(());
        }
        let action_count = decision.actions().len();
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer.fill(decision, &mut tensor).map_err(|_| ())?;
        let parent = self
            .parent
            .score_encoded_decision_v1(tensor_view_v1(&tensor))?;
        if parent.logits_v1().len() != action_count {
            return Err(());
        }
        let structured = structured_residual_v1(
            &self.parameters,
            &tensor,
            Some((history, acting_player)),
            HISTORY_GROUP_VOCAB_V1,
        )?;
        if structured.logits_v1().len() != action_count {
            return Err(());
        }
        let logits = match (&self.initializer_parameters, self.logit_residual_clip) {
            (Some(initializer_parameters), Some(clip)) => {
                let initializer = structured_residual_v1(
                    initializer_parameters,
                    &tensor,
                    Some((history, acting_player)),
                    HISTORY_GROUP_VOCAB_V1,
                )?;
                bounded_structured_logit_residual_v1(
                    initializer.logits_v1(),
                    structured.logits_v1(),
                    clip,
                )?
            }
            (None, None) => structured.logits_v1().to_vec(),
            _ => return Err(()),
        };
        compose_structured_policy_successor_output_v1(logits, parent.value_v1())
    }
}

fn tensor_view_v1(tensor: &NativeFlatDecisionTensorV2) -> NativeEncodedDecisionViewV1<'_> {
    NativeEncodedDecisionViewV1::from_slices_unvalidated(
        NativeEncodedDecisionSchemaV1::contract_v1(),
        &tensor.state,
        &tensor.object_features,
        &tensor.object_card_ids,
        &tensor.object_groups,
        &tensor.object_node_ids,
        &tensor.edge_features,
        &tensor.edge_source_indices,
        &tensor.edge_target_indices,
        &tensor.action_features,
        &tensor.action_ref_features,
        &tensor.action_ref_card_ids,
        &tensor.action_ref_action_indices,
        &tensor.action_ref_node_indices,
    )
}

fn strict_json_value_v1(bytes: &[u8]) -> Result<serde_json::Value, Box<dyn Error>> {
    if !bytes.ends_with(b"\n") || bytes.contains(&b'\r') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid JSON framing").into());
    }
    Ok(parse_strict_json_value(std::str::from_utf8(bytes)?)?)
}

fn validate_inventory_v1(root: &Path) -> Result<(), Box<dyn Error>> {
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 successor inventory")
        })?;
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            files.insert(name);
        } else if file_type.is_dir() {
            directories.insert(name);
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid successor inventory type",
            )
            .into());
        }
    }
    if files
        != BTreeSet::from([
            CANDIDATE_FILENAME_V1.to_owned(),
            REPORT_FILENAME_V1.to_owned(),
            WEIGHTS_FILENAME_V1.to_owned(),
        ])
        || directories != BTreeSet::from([PARENT_DIRECTORY_V1.to_owned()])
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "successor inventory is not exact",
        )
        .into());
    }
    Ok(())
}

fn validate_parent_inventory_v1(parent: &Path) -> Result<(), Box<dyn Error>> {
    let inventory = fs::read_dir(parent)?
        .map(|entry| {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid successor parent inventory",
                ));
            }
            entry.file_name().into_string().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "non-UTF8 successor parent inventory",
                )
            })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if inventory
        != BTreeSet::from([
            PARENT_MANIFEST_FILENAME_V1.to_owned(),
            PARENT_STATE_FILENAME_V1.to_owned(),
        ])
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "successor parent inventory mismatch",
        )
        .into());
    }
    Ok(())
}

fn validate_parameter_bindings_v1(
    bindings: &[ParameterBindingV1],
    version: SuccessorVersionV1,
) -> Result<(), Box<dyn Error>> {
    let expected = expected_history_parameters_v1();
    let copies = if version == SuccessorVersionV1::BoundedLogitResidualV7 {
        2
    } else {
        1
    };
    if bindings.len() != expected.len() * copies {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "successor parameter list mismatch",
        )
        .into());
    }
    let mut offset = 0usize;
    for (index, binding) in bindings.iter().enumerate() {
        let copy = index / expected.len();
        let (expected_name, expected_shape) = &expected[index % expected.len()];
        let qualified_name = match (version, copy) {
            (SuccessorVersionV1::BoundedLogitResidualV7, 0) => {
                format!("initializer.{expected_name}")
            }
            (SuccessorVersionV1::BoundedLogitResidualV7, 1) => {
                format!("trained.{expected_name}")
            }
            (_, 0) => expected_name.to_string(),
            _ => unreachable!(),
        };
        let count = expected_shape
            .iter()
            .try_fold(1usize, |total, dimension| {
                total.checked_mul(*dimension).ok_or(())
            })
            .map_err(|()| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "successor parameter shape overflow",
                )
            })?;
        if binding.name != qualified_name
            || binding.shape != *expected_shape
            || binding.offset_f32 != offset
            || binding.count_f32 != count
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "successor parameter binding mismatch",
            )
            .into());
        }
        offset = offset.checked_add(count).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "successor parameter offset overflow",
            )
        })?;
    }
    if offset != HISTORY_PARAMETER_COUNT_V1 * copies {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "successor parameter count mismatch",
        )
        .into());
    }
    Ok(())
}

fn finite_metric_v1(metric: &ReportMetricV1) -> bool {
    metric.mean_total_variation.is_finite()
        && metric.p90_total_variation.is_finite()
        && metric.top_action_agreement.is_finite()
        && metric.mean_total_variation >= 0.0
        && metric.p90_total_variation >= 0.0
        && (0.0..=1.0).contains(&metric.top_action_agreement)
}

fn validate_report_v1(
    report: ReportV1,
    weights_sha256: [u8; 32],
    composite_sha256: [u8; 32],
) -> Result<(), Box<dyn Error>> {
    let config = &report.config;
    let exact_float = |actual: f64, expected: f64| actual.to_bits() == expected.to_bits();
    let policy = &report.policy_metrics;
    let seat_0 = policy.by_candidate_seat.get("0");
    let seat_1 = policy.by_candidate_seat.get("1");
    let seats_exact = policy.by_candidate_seat.len() == 2 && seat_0.is_some() && seat_1.is_some();
    let all_metrics = [Some(&policy.overall), seat_0, seat_1]
        .into_iter()
        .flatten()
        .all(finite_metric_v1);
    let gates_pass = [Some(&policy.overall), seat_0, seat_1]
        .into_iter()
        .flatten()
        .all(|metric| {
            metric.mean_total_variation <= 0.015
                && metric.p90_total_variation <= 0.040
                && metric.top_action_agreement >= 0.990
        });
    if report.schema != REPORT_SCHEMA_V1
        || report.source.cache_sha256 != SOURCE_CACHE_SHA256_V1
        || report.source.pair_count != SOURCE_PAIR_COUNT_V1
        || config.architecture != ARCHITECTURE_V1
        || config.value_model != VALUE_MODEL_V1
        || config.seed != FIT_SEED_V1
        || config.epochs != 5
        || config.batch_size_physical_decisions != 64
        || !exact_float(config.learning_rate, 3.0e-4)
        || !exact_float(config.weight_decay, 1.0e-4)
        || !exact_float(config.gradient_norm_cap, 5.0)
        || config.history_length != HISTORY_LENGTH_V1 as u64
        || config.history_feature_dim != HISTORY_FEATURE_DIM_V1 as u64
        || config.weighting != "equal_episode_mass_equal_physical_decision_mass_equal_substep_mass"
        || config.objective != "teacher-to-student-policy-kl-only/v1"
        || !seats_exact
        || !all_metrics
        || !gates_pass
        || !report.transport.maximum_absolute_logit_error.is_finite()
        || report.transport.maximum_absolute_logit_error > 3.0e-5
        || !report.transport.parent_value_bit_exact
        || report.weights_sha256 != lower_hex_v1(weights_sha256)
        || report.composite_model_parameter_sha256 != lower_hex_v1(composite_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "successor report semantic mismatch",
        )
        .into());
    }
    Ok(())
}

fn finite_movement_metric_v2(metric: &ReportMovementMetricV2) -> bool {
    metric.mean_total_variation.is_finite()
        && metric.p90_total_variation.is_finite()
        && metric.weighted_mean_kl.is_finite()
        && metric.top_action_agreement.is_finite()
        && metric.maximum_absolute_joint_log_ratio.is_finite()
        && metric.policy_mass.is_finite()
        && metric.mean_total_variation >= 0.0
        && metric.p90_total_variation >= 0.0
        && metric.weighted_mean_kl >= 0.0
        && (0.0..=1.0).contains(&metric.top_action_agreement)
        && metric.maximum_absolute_joint_log_ratio >= 0.0
        && metric.policy_mass > 0.0
        && metric.policy_rows > 0
        && metric.physical_decisions > 0
}

fn lower_hex_commit_v1(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_report_v2(
    report: ReportV2,
    weights_sha256: [u8; 32],
    composite_sha256: [u8; 32],
) -> Result<(), Box<dyn Error>> {
    let config = &report.config;
    let exact_float = |actual: f64, expected: f64| actual.to_bits() == expected.to_bits();
    let movement = &report.movement;
    let seat_0 = movement.by_candidate_seat.get("0");
    let seat_1 = movement.by_candidate_seat.get("1");
    let seats_exact = movement.by_candidate_seat.len() == 2 && seat_0.is_some() && seat_1.is_some();
    let all_metrics = [Some(&movement.overall), seat_0, seat_1]
        .into_iter()
        .flatten()
        .all(finite_movement_metric_v2);
    let movement_gates_pass = [Some(&movement.overall), seat_0, seat_1]
        .into_iter()
        .flatten()
        .all(|metric| metric.mean_total_variation <= 0.030 && metric.p90_total_variation <= 0.100)
        && movement.overall.maximum_absolute_joint_log_ratio <= 0.50;
    if report.schema != REPORT_SCHEMA_V2
        || report.initializer.candidate_json_sha256 != TERMINAL_RUNG_INITIALIZER_CANDIDATE_SHA256_V1
        || report.initializer.report_sha256 != TERMINAL_RUNG_INITIALIZER_REPORT_SHA256_V1
        || report.initializer.weights_sha256 != TERMINAL_RUNG_INITIALIZER_WEIGHTS_SHA256_V1
        || report.initializer.composite_model_parameter_sha256
            != TERMINAL_RUNG_INITIALIZER_COMPOSITE_SHA256_V1
        || report.initializer.model_state_sha256 != TERMINAL_RUNG_INITIALIZER_MODEL_STATE_SHA256_V1
        || parse_lower_hex32_v1(&report.source.cache_sha256).is_err()
        || report.source.pair_count != 2_048
        || report.source.base_seed != TERMINAL_RUNG_BASE_SEED_V1
        || report.source.pool_json_sha256 != TERMINAL_RUNG_POOL_SHA256_V1
        || !lower_hex_commit_v1(&report.source.source_commit)
        || config.architecture != ARCHITECTURE_V2
        || config.value_model != VALUE_MODEL_V1
        || config.seed != TERMINAL_RUNG_FIT_SEED_V1
        || config.epochs != 5
        || config.batch_size_physical_decisions != 64
        || !exact_float(config.learning_rate, 3.0e-4)
        || !exact_float(config.weight_decay, 0.0)
        || !exact_float(config.gradient_norm_cap, 5.0)
        || !exact_float(config.ppo_clip, 0.10)
        || config.history_length != HISTORY_LENGTH_V1 as u64
        || config.history_feature_dim != HISTORY_FEATURE_DIM_V1 as u64
        || config.weighting != "equal-episode-equal-physical-decision-joint-substep-ratio/v1"
        || config.advantage != "terminal-reward-minus-frozen-parent-value-seat-standardized/v1"
        || config.objective != "terminal-candidate-reward-only-clipped-ppo/v1"
        || !seats_exact
        || !all_metrics
        || !movement_gates_pass
        || !report.transport.maximum_absolute_logit_error.is_finite()
        || report.transport.maximum_absolute_logit_error > 3.0e-5
        || !report.transport.parent_value_bit_exact
        || report.weights_sha256 != lower_hex_v1(weights_sha256)
        || report.composite_model_parameter_sha256 != lower_hex_v1(composite_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "terminal-rung successor report semantic mismatch",
        )
        .into());
    }
    Ok(())
}

fn validate_report_v3(
    report: ReportV3,
    weights_sha256: [u8; 32],
    composite_sha256: [u8; 32],
) -> Result<(), Box<dyn Error>> {
    let config = &report.config;
    let exact_float = |actual: f64, expected: f64| actual.to_bits() == expected.to_bits();
    let movement = &report.movement;
    let seat_0 = movement.by_candidate_seat.get("0");
    let seat_1 = movement.by_candidate_seat.get("1");
    let seats_exact = movement.by_candidate_seat.len() == 2 && seat_0.is_some() && seat_1.is_some();
    let all_metrics = [Some(&movement.overall), seat_0, seat_1]
        .into_iter()
        .flatten()
        .all(finite_movement_metric_v2);
    let movement_gates_pass = [Some(&movement.overall), seat_0, seat_1]
        .into_iter()
        .flatten()
        .all(|metric| metric.mean_total_variation <= 0.030 && metric.p90_total_variation <= 0.100)
        && movement.overall.maximum_absolute_joint_log_ratio <= 0.50;
    if report.schema != REPORT_SCHEMA_V3
        || report.initializer.candidate_json_sha256 != TERMINAL_RUNG_INITIALIZER_CANDIDATE_SHA256_V1
        || report.initializer.report_sha256 != TERMINAL_RUNG_INITIALIZER_REPORT_SHA256_V1
        || report.initializer.weights_sha256 != TERMINAL_RUNG_INITIALIZER_WEIGHTS_SHA256_V1
        || report.initializer.composite_model_parameter_sha256
            != TERMINAL_RUNG_INITIALIZER_COMPOSITE_SHA256_V1
        || report.initializer.model_state_sha256 != TERMINAL_RUNG_INITIALIZER_MODEL_STATE_SHA256_V1
        || parse_lower_hex32_v1(&report.source.cache_sha256).is_err()
        || report.source.pair_count != 2_048
        || report.source.base_seed != TERMINAL_RUNG_BASE_SEED_V1
        || report.source.pool_json_sha256 != TERMINAL_RUNG_POOL_SHA256_V1
        || !lower_hex_commit_v1(&report.source.source_commit)
        || report.source.rejected_fit_report_sha256 != TERMINAL_TRUST_SOURCE_FIT_REPORT_SHA256_V1
        || report.source.rejected_model_state_sha256 != TERMINAL_TRUST_SOURCE_MODEL_STATE_SHA256_V1
        || config.architecture != ARCHITECTURE_V3
        || config.value_model != VALUE_MODEL_V1
        || config.seed != TERMINAL_RUNG_FIT_SEED_V1
        || config.epochs != 5
        || config.batch_size_physical_decisions != 64
        || !exact_float(config.learning_rate, 3.0e-4)
        || !exact_float(config.weight_decay, 0.0)
        || !exact_float(config.gradient_norm_cap, 5.0)
        || !exact_float(config.ppo_clip, 0.10)
        || config.history_length != HISTORY_LENGTH_V1 as u64
        || config.history_feature_dim != HISTORY_FEATURE_DIM_V1 as u64
        || config.weighting != "equal-episode-equal-physical-decision-joint-substep-ratio/v1"
        || config.advantage != "terminal-reward-minus-frozen-parent-value-seat-standardized/v1"
        || config.objective != "terminal-candidate-reward-only-clipped-ppo-trust-projection/v1"
        || config.projection_method != "linear-parameter-displacement-from-qualified-initializer/v1"
        || !exact_float(config.projection_scale, 1.0 / 16.0)
        || !seats_exact
        || !all_metrics
        || !movement_gates_pass
        || !report.transport.maximum_absolute_logit_error.is_finite()
        || report.transport.maximum_absolute_logit_error > 3.0e-5
        || !report.transport.parent_value_bit_exact
        || report.weights_sha256 != lower_hex_v1(weights_sha256)
        || report.composite_model_parameter_sha256 != lower_hex_v1(composite_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "terminal trust-projection successor report semantic mismatch",
        )
        .into());
    }
    Ok(())
}

fn validate_report_v4(
    report: ReportV4,
    weights_sha256: [u8; 32],
    composite_sha256: [u8; 32],
) -> Result<(), Box<dyn Error>> {
    let config = &report.config;
    let exact_float = |actual: f64, expected: f64| actual.to_bits() == expected.to_bits();
    let movement = &report.movement;
    let seat_0 = movement.by_candidate_seat.get("0");
    let seat_1 = movement.by_candidate_seat.get("1");
    let seats_exact = movement.by_candidate_seat.len() == 2 && seat_0.is_some() && seat_1.is_some();
    let all_metrics = [Some(&movement.overall), seat_0, seat_1]
        .into_iter()
        .flatten()
        .all(finite_movement_metric_v2);
    let movement_gates_pass = [Some(&movement.overall), seat_0, seat_1]
        .into_iter()
        .flatten()
        .all(|metric| metric.mean_total_variation <= 0.030 && metric.p90_total_variation <= 0.100)
        && movement.overall.maximum_absolute_joint_log_ratio <= 0.50;
    if report.schema != REPORT_SCHEMA_V4
        || report.initializer.candidate_json_sha256 != TERMINAL_RUNG_INITIALIZER_CANDIDATE_SHA256_V1
        || report.initializer.report_sha256 != TERMINAL_RUNG_INITIALIZER_REPORT_SHA256_V1
        || report.initializer.weights_sha256 != TERMINAL_RUNG_INITIALIZER_WEIGHTS_SHA256_V1
        || report.initializer.composite_model_parameter_sha256
            != TERMINAL_RUNG_INITIALIZER_COMPOSITE_SHA256_V1
        || report.initializer.model_state_sha256 != TERMINAL_RUNG_INITIALIZER_MODEL_STATE_SHA256_V1
        || report.source.cache_sha256
            != "454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d"
        || report.source.pair_count != 2_048
        || report.source.base_seed != TERMINAL_RUNG_BASE_SEED_V1
        || report.source.pool_json_sha256 != TERMINAL_RUNG_POOL_SHA256_V1
        || !lower_hex_commit_v1(&report.source.source_commit)
        || config.architecture != ARCHITECTURE_V4
        || config.value_model != VALUE_MODEL_V1
        || config.seed != 20_260_806
        || config.epochs != 5
        || config.batch_size_physical_decisions != 64
        || !exact_float(config.learning_rate, 3.0e-4)
        || !exact_float(config.weight_decay, 0.0)
        || !exact_float(config.gradient_norm_cap, 5.0)
        || !exact_float(config.ppo_clip, 0.10)
        || config.history_length != HISTORY_LENGTH_V1 as u64
        || config.history_feature_dim != HISTORY_FEATURE_DIM_V1 as u64
        || config.weighting != "equal-episode-equal-physical-decision-joint-substep-ratio/v1"
        || config.advantage != "terminal-reward-minus-frozen-parent-value-seat-standardized/v1"
        || config.objective
            != "terminal-candidate-reward-only-clipped-ppo-policy-head-weight-only/v1"
        || config.trainable_parameter != "policy_head.weight"
        || config.trainable_parameter_count != 48
        || !seats_exact
        || !all_metrics
        || !movement_gates_pass
        || !report.transport.maximum_absolute_logit_error.is_finite()
        || report.transport.maximum_absolute_logit_error > 3.0e-5
        || !report.transport.parent_value_bit_exact
        || report.weights_sha256 != lower_hex_v1(weights_sha256)
        || report.composite_model_parameter_sha256 != lower_hex_v1(composite_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "terminal head-only successor report semantic mismatch",
        )
        .into());
    }
    Ok(())
}

fn parse_f32_bits_hex_v1(value: &str) -> Option<f32> {
    (value.len() == 8
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| u32::from_str_radix(value, 16).ok().map(f32::from_bits))
    .flatten()
}

fn movement_gates_pass_v1(movement: &ReportMovementV2) -> bool {
    let seat_0 = movement.by_candidate_seat.get("0");
    let seat_1 = movement.by_candidate_seat.get("1");
    movement.by_candidate_seat.len() == 2
        && [Some(&movement.overall), seat_0, seat_1]
            .into_iter()
            .flatten()
            .count()
            == 3
        && [Some(&movement.overall), seat_0, seat_1]
            .into_iter()
            .flatten()
            .all(|metric| {
                finite_movement_metric_v2(metric)
                    && metric.mean_total_variation <= 0.030
                    && metric.p90_total_variation <= 0.100
            })
        && movement.overall.maximum_absolute_joint_log_ratio <= 0.50
}

fn validate_report_v5(
    report: ReportV5,
    weights_sha256: [u8; 32],
    composite_sha256: [u8; 32],
) -> Result<(), Box<dyn Error>> {
    let config = &report.config;
    let exact_float = |actual: f64, expected: f64| actual.to_bits() == expected.to_bits();
    let deltas = report
        .adapter
        .delta_f32_bits
        .iter()
        .map(|value| parse_f32_bits_hex_v1(value))
        .collect::<Option<Vec<_>>>();
    let deltas_valid = deltas.as_ref().is_some_and(|values| {
        values.len() == 48
            && values
                .iter()
                .all(|value| value.is_finite() && value.abs() <= 0.050_000_001)
    });
    let phase_valid = matches!(
        report.source.phase.as_str(),
        "development" | "selector" | "selected"
    );
    let publication_valid = if config.development_only {
        report.movement.is_none()
            && exact_float(report.transport.maximum_absolute_logit_error, 1.0)
            && !report.transport.parent_value_bit_exact
            && report.source.phase != "selected"
    } else {
        report.source.phase == "selected"
            && report.movement.as_ref().is_some_and(movement_gates_pass_v1)
            && report.transport.maximum_absolute_logit_error.is_finite()
            && report.transport.maximum_absolute_logit_error <= 3.0e-5
            && report.transport.parent_value_bit_exact
    };
    if report.schema != REPORT_SCHEMA_V5
        || report.initializer.candidate_json_sha256 != TERMINAL_RUNG_INITIALIZER_CANDIDATE_SHA256_V1
        || report.initializer.report_sha256 != TERMINAL_RUNG_INITIALIZER_REPORT_SHA256_V1
        || report.initializer.weights_sha256 != TERMINAL_RUNG_INITIALIZER_WEIGHTS_SHA256_V1
        || report.initializer.composite_model_parameter_sha256
            != TERMINAL_RUNG_INITIALIZER_COMPOSITE_SHA256_V1
        || report.initializer.model_state_sha256 != TERMINAL_RUNG_INITIALIZER_MODEL_STATE_SHA256_V1
        || !lower_hex_commit_v1(&report.source.source_commit)
        || report.source.oracle_seed != 20_260_807
        || !phase_valid
        || report.source.generation > 100
        || report.source.candidate_index > 100
        || config.architecture != ARCHITECTURE_V5
        || config.value_model != VALUE_MODEL_V1
        || config.optimizer != "terminal-outcome-antithetic-cem/v1"
        || config.adapter_parameter != "policy_head.weight"
        || config.adapter_parameter_count != 48
        || !exact_float(config.adapter_maximum_absolute_delta, 0.05)
        || !deltas_valid
        || !report.adapter.l2_squared.is_finite()
        || report.adapter.l2_squared < 0.0
        || !publication_valid
        || report.weights_sha256 != lower_hex_v1(weights_sha256)
        || report.composite_model_parameter_sha256 != lower_hex_v1(composite_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "policy-space response-oracle report semantic mismatch",
        )
        .into());
    }
    Ok(())
}

fn validate_report_v6(
    report: ReportV6,
    weights_sha256: [u8; 32],
    composite_sha256: [u8; 32],
) -> Result<(), Box<dyn Error>> {
    const BLOCK_NAMES: [&str; 10] = [
        "state",
        "history",
        "objects",
        "relations",
        "action",
        "references",
        "query",
        "combine_input",
        "combine_output",
        "policy_head",
    ];
    let config = &report.config;
    let exact_float = |actual: f64, expected: f64| actual.to_bits() == expected.to_bits();
    let coefficients = report
        .adapter
        .coefficients_f32_bits
        .iter()
        .map(|value| parse_f32_bits_hex_v1(value))
        .collect::<Option<Vec<_>>>();
    let coefficient_l1 = coefficients.as_ref().map(|values| {
        values
            .iter()
            .map(|value| f64::from(*value).abs())
            .sum::<f64>()
    });
    let coefficient_l2 = coefficients.as_ref().map(|values| {
        values
            .iter()
            .map(|value| f64::from(*value).powi(2))
            .sum::<f64>()
    });
    let coefficients_valid = coefficients.as_ref().is_some_and(|values| {
        values.len() == BLOCK_NAMES.len()
            && values
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0 && *value <= 0.125_000_001)
            && coefficient_l1.is_some_and(|value| value <= 0.625_001)
    });
    let phase_valid = matches!(
        report.source.phase.as_str(),
        "development" | "selector" | "selected"
    );
    let publication_valid = if config.development_only {
        report.movement.is_none()
            && exact_float(report.transport.maximum_absolute_logit_error, 1.0)
            && !report.transport.parent_value_bit_exact
            && report.source.phase != "selected"
    } else {
        report.source.phase == "selected"
            && report.movement.as_ref().is_some_and(movement_gates_pass_v1)
            && report.transport.maximum_absolute_logit_error.is_finite()
            && report.transport.maximum_absolute_logit_error <= 3.0e-5
            && report.transport.parent_value_bit_exact
    };
    if report.schema != REPORT_SCHEMA_V6
        || report.initializer.candidate_json_sha256 != TERMINAL_RUNG_INITIALIZER_CANDIDATE_SHA256_V1
        || report.initializer.report_sha256 != TERMINAL_RUNG_INITIALIZER_REPORT_SHA256_V1
        || report.initializer.weights_sha256 != TERMINAL_RUNG_INITIALIZER_WEIGHTS_SHA256_V1
        || report.initializer.composite_model_parameter_sha256
            != TERMINAL_RUNG_INITIALIZER_COMPOSITE_SHA256_V1
        || report.initializer.model_state_sha256 != TERMINAL_RUNG_INITIALIZER_MODEL_STATE_SHA256_V1
        || !lower_hex_commit_v1(&report.source.source_commit)
        || report.source.oracle_seed != 20_260_808
        || !phase_valid
        || report.source.generation > 100
        || report.source.candidate_index > 100
        || report.source.rejected_fit_report_sha256 != TERMINAL_TRUST_SOURCE_FIT_REPORT_SHA256_V1
        || report.source.rejected_model_state_sha256 != TERMINAL_TRUST_SOURCE_MODEL_STATE_SHA256_V1
        || config.architecture != ARCHITECTURE_V6
        || config.value_model != VALUE_MODEL_V1
        || config.optimizer != "terminal-outcome-projected-antithetic-cem/v1"
        || config.adapter != "semantic-terminal-ppo-policy-block-displacement/v1"
        || config.block_count != BLOCK_NAMES.len() as u64
        || !exact_float(config.coefficient_maximum, 0.125)
        || !exact_float(config.coefficient_l1_budget, 0.625)
        || report
            .adapter
            .block_names
            .iter()
            .map(String::as_str)
            .ne(BLOCK_NAMES)
        || !coefficients_valid
        || !report.adapter.coefficient_l1.is_finite()
        || !report.adapter.coefficient_l2_squared.is_finite()
        || coefficient_l1.is_none_or(|value| (report.adapter.coefficient_l1 - value).abs() > 1.0e-9)
        || coefficient_l2
            .is_none_or(|value| (report.adapter.coefficient_l2_squared - value).abs() > 1.0e-9)
        || !publication_valid
        || report.weights_sha256 != lower_hex_v1(weights_sha256)
        || report.composite_model_parameter_sha256 != lower_hex_v1(composite_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "policy-block response-oracle report semantic mismatch",
        )
        .into());
    }
    Ok(())
}

fn finite_movement_metric_v7(metric: &ReportMovementMetricV7) -> bool {
    metric.mean_total_variation.is_finite()
        && metric.p90_total_variation.is_finite()
        && metric.top_action_agreement.is_finite()
        && metric.top_action_change_rate.is_finite()
        && metric.maximum_absolute_joint_log_ratio.is_finite()
        && metric.policy_mass.is_finite()
        && metric.mean_total_variation >= 0.0
        && metric.p90_total_variation >= 0.0
        && (0.0..=1.0).contains(&metric.top_action_agreement)
        && (0.0..=1.0).contains(&metric.top_action_change_rate)
        && ((1.0 - metric.top_action_agreement) - metric.top_action_change_rate).abs() <= 1.0e-12
        && metric.maximum_absolute_joint_log_ratio >= 0.0
        && metric.policy_mass > 0.0
        && metric.policy_rows > 0
        && metric.physical_decisions > 0
}

fn movement_gates_pass_v7(movement: &ReportMovementV7) -> bool {
    let seat_0 = movement.by_candidate_seat.get("0");
    let seat_1 = movement.by_candidate_seat.get("1");
    movement.by_candidate_seat.len() == 2
        && [Some(&movement.overall), seat_0, seat_1]
            .into_iter()
            .flatten()
            .count()
            == 3
        && [Some(&movement.overall), seat_0, seat_1]
            .into_iter()
            .flatten()
            .all(|metric| {
                finite_movement_metric_v7(metric)
                    && metric.mean_total_variation <= 0.030
                    && metric.p90_total_variation <= 0.100
            })
        && movement.overall.maximum_absolute_joint_log_ratio <= 0.50
}

fn validate_report_v7(
    report: ReportV7,
    weights_sha256: [u8; 32],
    composite_sha256: [u8; 32],
) -> Result<f32, Box<dyn Error>> {
    let config = &report.config;
    let exact_float = |actual: f64, expected: f64| actual.to_bits() == expected.to_bits();
    let clip_valid = [
        0.03, 0.04, 0.05, 0.06, 0.08, 0.10, 0.12, 0.16, 0.20, 0.24, 0.28, 0.32, 0.40,
    ]
    .into_iter()
    .any(|expected| exact_float(config.logit_residual_clip, expected));
    let movement = &report.movement;
    let mechanism_gates_pass = movement_gates_pass_v7(movement)
        && movement.overall.mean_total_variation >= 4.0 * 0.0017975835928040842
        && 1.0 - movement.overall.top_action_agreement >= 4.0 * (1.0 - 0.9994097787903164);
    if report.schema != REPORT_SCHEMA_V7
        || report.initializer.candidate_json_sha256 != TERMINAL_RUNG_INITIALIZER_CANDIDATE_SHA256_V1
        || report.initializer.report_sha256 != TERMINAL_RUNG_INITIALIZER_REPORT_SHA256_V1
        || report.initializer.weights_sha256 != TERMINAL_RUNG_INITIALIZER_WEIGHTS_SHA256_V1
        || report.initializer.composite_model_parameter_sha256
            != TERMINAL_RUNG_INITIALIZER_COMPOSITE_SHA256_V1
        || report.initializer.model_state_sha256 != TERMINAL_RUNG_INITIALIZER_MODEL_STATE_SHA256_V1
        || report.source.cache_sha256
            != "454e4ce1b8f7413839a36c8e2731fc0cb65581ce13e593634bffa70013a6f16d"
        || report.source.pair_count != 2_048
        || report.source.base_seed != TERMINAL_RUNG_BASE_SEED_V1
        || report.source.pool_json_sha256 != TERMINAL_RUNG_POOL_SHA256_V1
        || !lower_hex_commit_v1(&report.source.source_commit)
        || report.source.rejected_fit_report_sha256 != TERMINAL_TRUST_SOURCE_FIT_REPORT_SHA256_V1
        || report.source.rejected_model_state_sha256 != TERMINAL_TRUST_SOURCE_MODEL_STATE_SHA256_V1
        || config.architecture != ARCHITECTURE_V7
        || config.value_model != VALUE_MODEL_V1
        || config.projection_method != "initializer-weighted-centered-logit-residual-clamp/v1"
        || !clip_valid
        || parse_lower_hex32_v1(&config.screen_report_sha256).is_err()
        || !mechanism_gates_pass
        || !report.transport.maximum_absolute_logit_error.is_finite()
        || report.transport.maximum_absolute_logit_error > 3.0e-5
        || !report.transport.parent_value_bit_exact
        || report.weights_sha256 != lower_hex_v1(weights_sha256)
        || report.composite_model_parameter_sha256 != lower_hex_v1(composite_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bounded-logit successor report semantic mismatch",
        )
        .into());
    }
    Ok(config.logit_residual_clip as f32)
}

pub(crate) fn successor_parameter_layout_sha256_v1() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"mtg-kernel-structured-policy-successor-parameter-layout/v1");
    for (name, shape) in expected_history_parameters_v1() {
        hasher.update((name.len() as u64).to_be_bytes());
        hasher.update(name.as_bytes());
        hasher.update((shape.len() as u64).to_be_bytes());
        for dimension in shape {
            hasher.update((dimension as u64).to_be_bytes());
        }
    }
    hasher.update((HISTORY_PARAMETER_COUNT_V1 as u64).to_be_bytes());
    hasher.finalize().into()
}

fn composite_hash_v1(
    domain: &[u8],
    parent_model_parameter_sha256: [u8; 32],
    weights: &[u8],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(parent_model_parameter_sha256);
    hasher.update(weights);
    hasher.finalize().into()
}

pub(crate) fn load_native_structured_policy_successor_inference_v1(
    root: &Path,
) -> Result<NativeStructuredPolicySuccessorInferenceV1, Box<dyn Error>> {
    load_native_structured_policy_successor_inference_inner_v1(root, true)
}

/// Loads a staged package for an external Python/Rust transport comparison.
///
/// This validates the exact candidate, weights, and retained-parent bindings,
/// but does not treat the provisional report transport declaration as
/// qualified. It is deliberately not wired into shadow authority selection.
pub(crate) fn load_native_structured_policy_successor_transport_inference_v1(
    root: &Path,
) -> Result<NativeStructuredPolicySuccessorInferenceV1, Box<dyn Error>> {
    load_native_structured_policy_successor_inference_inner_v1(root, false)
}

fn load_native_structured_policy_successor_inference_inner_v1(
    root: &Path,
    require_qualified_report: bool,
) -> Result<NativeStructuredPolicySuccessorInferenceV1, Box<dyn Error>> {
    validate_inventory_v1(root)?;
    let candidate_bytes = fs::read(root.join(CANDIDATE_FILENAME_V1))?;
    let candidate: CandidateV1 = serde_json::from_value(strict_json_value_v1(&candidate_bytes)?)?;
    let version = match candidate.schema.as_str() {
        CANDIDATE_SCHEMA_V1 => SuccessorVersionV1::DistilledV1,
        CANDIDATE_SCHEMA_V2 => SuccessorVersionV1::TerminalRungV2,
        CANDIDATE_SCHEMA_V3 => SuccessorVersionV1::TerminalTrustProjectionV3,
        CANDIDATE_SCHEMA_V4 => SuccessorVersionV1::TerminalHeadOnlyV4,
        CANDIDATE_SCHEMA_V5 => SuccessorVersionV1::PolicySpaceResponseOracleV5,
        CANDIDATE_SCHEMA_V6 => SuccessorVersionV1::PolicyBlockResponseOracleV6,
        CANDIDATE_SCHEMA_V7 => SuccessorVersionV1::BoundedLogitResidualV7,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "successor candidate schema mismatch",
            )
            .into());
        }
    };
    let (expected_architecture, composite_domain, authority_kind) = match version {
        SuccessorVersionV1::DistilledV1 => {
            (ARCHITECTURE_V1, COMPOSITE_DOMAIN_V1, AUTHORITY_KIND_V1)
        }
        SuccessorVersionV1::TerminalRungV2 => {
            (ARCHITECTURE_V2, COMPOSITE_DOMAIN_V2, AUTHORITY_KIND_V2)
        }
        SuccessorVersionV1::TerminalTrustProjectionV3 => {
            (ARCHITECTURE_V3, COMPOSITE_DOMAIN_V3, AUTHORITY_KIND_V3)
        }
        SuccessorVersionV1::TerminalHeadOnlyV4 => {
            (ARCHITECTURE_V4, COMPOSITE_DOMAIN_V4, AUTHORITY_KIND_V4)
        }
        SuccessorVersionV1::PolicySpaceResponseOracleV5 => {
            (ARCHITECTURE_V5, COMPOSITE_DOMAIN_V5, AUTHORITY_KIND_V5)
        }
        SuccessorVersionV1::PolicyBlockResponseOracleV6 => {
            (ARCHITECTURE_V6, COMPOSITE_DOMAIN_V6, AUTHORITY_KIND_V6)
        }
        SuccessorVersionV1::BoundedLogitResidualV7 => {
            (ARCHITECTURE_V7, COMPOSITE_DOMAIN_V7, AUTHORITY_KIND_V7)
        }
    };
    let report_bytes = fs::read(root.join(REPORT_FILENAME_V1))?;
    let report_value = strict_json_value_v1(&report_bytes)?;
    let weights_bytes = fs::read(root.join(WEIGHTS_FILENAME_V1))?;
    let candidate_sha256 = raw_sha256_v1(&candidate_bytes);
    let report_sha256 = raw_sha256_v1(&report_bytes);
    let weights_sha256 = raw_sha256_v1(&weights_bytes);
    let parent_model_sha256 = parse_lower_hex32_v1(PARENT_MODEL_PARAMETER_SHA256_V1)?;
    let composite_sha256 = composite_hash_v1(composite_domain, parent_model_sha256, &weights_bytes);
    let parameter_copies = if version == SuccessorVersionV1::BoundedLogitResidualV7 {
        2
    } else {
        1
    };
    if candidate.publication_encoding != PUBLICATION_ENCODING_V1
        || candidate.parent.directory != PARENT_DIRECTORY_V1
        || candidate.parent.manifest_sha256 != PARENT_MANIFEST_SHA256_V1
        || candidate.parent.payload_sha256 != PARENT_PAYLOAD_SHA256_V1
        || candidate.parent.native_state_sha256 != PARENT_NATIVE_STATE_SHA256_V1
        || candidate.parent.model_parameter_sha256 != PARENT_MODEL_PARAMETER_SHA256_V1
        || candidate.parent.adam_step != PARENT_ADAM_STEP_V1
        || candidate.architecture.identity != expected_architecture
        || candidate.architecture.state_dim != NATIVE_FLAT_STATE_FEATURE_DIM_V2
        || candidate.architecture.object_dim != NATIVE_FLAT_OBJECT_FEATURE_DIM_V2
        || candidate.architecture.edge_dim != NATIVE_FLAT_EDGE_FEATURE_DIM_V2
        || candidate.architecture.action_dim != NATIVE_FLAT_ACTION_FEATURE_DIM_V2
        || candidate.architecture.ref_dim != NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2
        || candidate.architecture.hidden_dim != HIDDEN_DIM_V1
        || candidate.architecture.card_vocab != CARD_VOCAB_V1
        || candidate.architecture.card_embedding_dim != CARD_EMBEDDING_DIM_V1
        || candidate.architecture.group_vocab != HISTORY_GROUP_VOCAB_V1
        || candidate.architecture.group_embedding_dim != GROUP_EMBEDDING_DIM_V1
        || candidate.architecture.history_length != HISTORY_LENGTH_V1
        || candidate.architecture.history_feature_dim != HISTORY_FEATURE_DIM_V1
        || candidate.architecture.history_role_dim != HISTORY_ROLE_DIM_V1
        || candidate.architecture.value_model != VALUE_MODEL_V1
        || candidate.weights.filename != WEIGHTS_FILENAME_V1
        || candidate.weights.encoding != WEIGHTS_ENCODING_V1
        || candidate.weights.sha256 != lower_hex_v1(weights_sha256)
        || candidate.weights.byte_count != weights_bytes.len()
        || candidate.weights.parameter_count != HISTORY_PARAMETER_COUNT_V1 * parameter_copies
        || candidate.weights.byte_count
            != HISTORY_PARAMETER_COUNT_V1 * parameter_copies * size_of::<f32>()
        || candidate.report.filename != REPORT_FILENAME_V1
        || candidate.report.sha256 != lower_hex_v1(report_sha256)
        || candidate.composite_model_parameter_sha256 != lower_hex_v1(composite_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "successor candidate binding mismatch",
        )
        .into());
    }
    validate_parameter_bindings_v1(&candidate.weights.parameters, version)?;
    let report_logit_residual_clip = if version == SuccessorVersionV1::BoundedLogitResidualV7 {
        let parsed: ReportV7 = serde_json::from_value(report_value.clone())?;
        let clip = parsed.config.logit_residual_clip;
        if !clip.is_finite() || clip < 0.0 || clip > 0.40 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bounded-logit successor clip mismatch",
            )
            .into());
        }
        Some(clip as f32)
    } else {
        None
    };
    if require_qualified_report {
        match version {
            SuccessorVersionV1::DistilledV1 => {
                let report: ReportV1 = serde_json::from_value(report_value)?;
                validate_report_v1(report, weights_sha256, composite_sha256)?;
            }
            SuccessorVersionV1::TerminalRungV2 => {
                let report: ReportV2 = serde_json::from_value(report_value)?;
                validate_report_v2(report, weights_sha256, composite_sha256)?;
            }
            SuccessorVersionV1::TerminalTrustProjectionV3 => {
                let report: ReportV3 = serde_json::from_value(report_value)?;
                validate_report_v3(report, weights_sha256, composite_sha256)?;
            }
            SuccessorVersionV1::TerminalHeadOnlyV4 => {
                let report: ReportV4 = serde_json::from_value(report_value)?;
                validate_report_v4(report, weights_sha256, composite_sha256)?;
            }
            SuccessorVersionV1::PolicySpaceResponseOracleV5 => {
                let report: ReportV5 = serde_json::from_value(report_value)?;
                validate_report_v5(report, weights_sha256, composite_sha256)?;
            }
            SuccessorVersionV1::PolicyBlockResponseOracleV6 => {
                let report: ReportV6 = serde_json::from_value(report_value)?;
                validate_report_v6(report, weights_sha256, composite_sha256)?;
            }
            SuccessorVersionV1::BoundedLogitResidualV7 => {
                let report: ReportV7 = serde_json::from_value(report_value)?;
                validate_report_v7(report, weights_sha256, composite_sha256)?;
            }
        }
    }
    let (initializer_parameters, parameters) =
        if version == SuccessorVersionV1::BoundedLogitResidualV7 {
            let split = HISTORY_PARAMETER_COUNT_V1 * size_of::<f32>();
            (
                Some(decode_structured_residual_parameters_v1(
                    &weights_bytes[..split],
                )?),
                decode_structured_residual_parameters_v1(&weights_bytes[split..])?,
            )
        } else {
            (
                None,
                decode_structured_residual_parameters_v1(&weights_bytes)?,
            )
        };
    let parent_root = root.join(PARENT_DIRECTORY_V1);
    validate_parent_inventory_v1(&parent_root)?;
    let parent = load_xmage_cp7_outcome_inference_v1(&parent_root)?;
    if parent.manifest_sha256_v1() != parse_lower_hex32_v1(PARENT_MANIFEST_SHA256_V1)?
        || parent.payload_sha256_v1() != parse_lower_hex32_v1(PARENT_PAYLOAD_SHA256_V1)?
        || parent.native_state_sha256_v1() != parse_lower_hex32_v1(PARENT_NATIVE_STATE_SHA256_V1)?
        || parent.model_parameter_sha256_v1() != parent_model_sha256
        || parent.adam_step_v1() != PARENT_ADAM_STEP_V1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "loaded successor parent mismatch",
        )
        .into());
    }
    Ok(NativeStructuredPolicySuccessorInferenceV1 {
        parent,
        parameters,
        initializer_parameters,
        logit_residual_clip: report_logit_residual_clip,
        candidate_json_sha256: candidate_sha256,
        weights_sha256,
        report_sha256,
        composite_model_parameter_sha256: composite_sha256,
        authority_kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ParityFixtureV1 {
        schema: String,
        output_semantics: String,
        examples: Vec<ParityExampleV1>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ParityExampleV1 {
        acting_player: u8,
        candidate_seat: u8,
        history_length_bucket: usize,
        tensor: ParityTensorV1,
        history: Vec<ParityHistoryEntryV1>,
        expected_structured_logits: Vec<f32>,
        expected_value_residual_f32_bits: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ParityTensorV1 {
        state: Vec<f32>,
        object_features: Vec<Vec<f32>>,
        object_card_ids: Vec<i64>,
        object_groups: Vec<i64>,
        object_node_ids: Vec<i64>,
        edge_features: Vec<Vec<f32>>,
        edge_source_indices: Vec<i64>,
        edge_target_indices: Vec<i64>,
        action_features: Vec<Vec<f32>>,
        action_ref_features: Vec<Vec<f32>>,
        action_ref_card_ids: Vec<i64>,
        action_ref_action_indices: Vec<i64>,
        action_ref_node_indices: Vec<i64>,
    }

    impl ParityTensorV1 {
        fn into_native_v1(self) -> NativeFlatDecisionTensorV2 {
            NativeFlatDecisionTensorV2 {
                state: self.state,
                object_features: self.object_features.into_iter().flatten().collect(),
                object_card_ids: self.object_card_ids,
                object_groups: self.object_groups,
                object_node_ids: self.object_node_ids,
                edge_features: self.edge_features.into_iter().flatten().collect(),
                edge_source_indices: self.edge_source_indices,
                edge_target_indices: self.edge_target_indices,
                action_features: self.action_features.into_iter().flatten().collect(),
                action_ref_features: self.action_ref_features.into_iter().flatten().collect(),
                action_ref_card_ids: self.action_ref_card_ids,
                action_ref_action_indices: self.action_ref_action_indices,
                action_ref_node_indices: self.action_ref_node_indices,
            }
        }
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ParityHistoryEntryV1 {
        acting_player: u8,
        action_explicit_features: Vec<f32>,
        public_card_histogram: Vec<f32>,
    }

    #[test]
    fn successor_reuses_exact_complete_history_layout_v1() {
        let count = expected_history_parameters_v1()
            .into_iter()
            .map(|(_, shape)| shape.into_iter().product::<usize>())
            .sum::<usize>();
        assert_eq!(count, HISTORY_PARAMETER_COUNT_V1);
        assert_ne!(successor_parameter_layout_sha256_v1(), [0; 32]);
    }

    #[test]
    fn successor_composition_uses_absolute_logits_and_frozen_value_v1() {
        let output =
            compose_structured_policy_successor_output_v1(vec![1.25, -0.5], 0.375).unwrap();
        assert_eq!(output.logits_v1(), &[1.25, -0.5]);
        assert_eq!(output.value_v1().to_bits(), 0.375f32.to_bits());
        assert!(compose_structured_policy_successor_output_v1(vec![], 0.0).is_err());
        assert!(compose_structured_policy_successor_output_v1(vec![0.0], f32::NAN).is_err());
    }

    #[test]
    fn bounded_logit_residual_has_exact_zero_fallback_and_bound_v1() {
        let initial = vec![1.0, -0.5, 0.25];
        let trained = vec![-2.0, 3.0, 0.5];
        assert_eq!(
            bounded_structured_logit_residual_v1(&initial, &trained, 0.0).unwrap(),
            initial
        );
        let bounded = bounded_structured_logit_residual_v1(&initial, &trained, 0.06).unwrap();
        assert!(bounded
            .iter()
            .zip(&initial)
            .all(|(actual, before)| (actual - before).abs() <= 0.060001));
        assert!(bounded_structured_logit_residual_v1(&initial, &trained, -0.1).is_err());
    }

    #[test]
    fn successor_candidate_denies_unknown_fields_v1() {
        let value = br#"{"schema":"x","publication_encoding":"x","parent":{},"architecture":{},"weights":{},"report":{},"composite_model_parameter_sha256":"x","unexpected":true}
"#;
        let parsed = strict_json_value_v1(value).unwrap();
        assert!(serde_json::from_value::<CandidateV1>(parsed).is_err());
    }

    #[test]
    fn successor_report_denies_unknown_fields_v1() {
        let value = br#"{"schema":"x","source":{},"config":{},"policy_metrics":{},"transport":{},"weights_sha256":"x","composite_model_parameter_sha256":"x","unexpected":true}
"#;
        let parsed = strict_json_value_v1(value).unwrap();
        assert!(serde_json::from_value::<ReportV1>(parsed).is_err());
    }

    #[test]
    fn terminal_rung_report_denies_unknown_fields_v1() {
        let value = br#"{"schema":"x","initializer":{},"source":{},"config":{},"movement":{},"transport":{},"weights_sha256":"x","composite_model_parameter_sha256":"x","unexpected":true}
"#;
        let parsed = strict_json_value_v1(value).unwrap();
        assert!(serde_json::from_value::<ReportV2>(parsed).is_err());
    }

    #[test]
    fn terminal_trust_projection_report_denies_unknown_fields_v1() {
        let value = br#"{"schema":"x","initializer":{},"source":{},"config":{},"movement":{},"transport":{},"weights_sha256":"x","composite_model_parameter_sha256":"x","unexpected":true}
"#;
        let parsed = strict_json_value_v1(value).unwrap();
        assert!(serde_json::from_value::<ReportV3>(parsed).is_err());
    }

    #[test]
    fn terminal_head_only_report_denies_unknown_fields_v1() {
        let value = br#"{"schema":"x","initializer":{},"source":{},"config":{},"movement":{},"transport":{},"weights_sha256":"x","composite_model_parameter_sha256":"x","unexpected":true}
"#;
        let parsed = strict_json_value_v1(value).unwrap();
        assert!(serde_json::from_value::<ReportV4>(parsed).is_err());
    }

    #[test]
    fn policy_space_response_oracle_report_denies_unknown_fields_v1() {
        let value = br#"{"schema":"x","initializer":{},"source":{},"config":{},"adapter":{},"movement":null,"transport":{},"weights_sha256":"x","composite_model_parameter_sha256":"x","unexpected":true}
"#;
        let parsed = strict_json_value_v1(value).unwrap();
        assert!(serde_json::from_value::<ReportV5>(parsed).is_err());
    }

    #[test]
    fn bounded_logit_report_denies_unknown_fields_v1() {
        let value = br#"{"schema":"x","initializer":{},"source":{},"config":{},"movement":{},"transport":{},"weights_sha256":"x","composite_model_parameter_sha256":"x","unexpected":true}
"#;
        let parsed = strict_json_value_v1(value).unwrap();
        assert!(serde_json::from_value::<ReportV7>(parsed).is_err());
    }

    #[test]
    fn successor_composite_hash_is_domain_bound_v1() {
        let parent = [7u8; 32];
        assert_ne!(
            composite_hash_v1(COMPOSITE_DOMAIN_V1, parent, b"a"),
            composite_hash_v1(COMPOSITE_DOMAIN_V1, parent, b"b")
        );
        let mut raw = Sha256::new();
        raw.update(parent);
        raw.update(b"a");
        assert_ne!(
            composite_hash_v1(COMPOSITE_DOMAIN_V1, parent, b"a"),
            <[u8; 32]>::from(raw.finalize())
        );
        assert_ne!(
            composite_hash_v1(COMPOSITE_DOMAIN_V1, parent, b"a"),
            composite_hash_v1(COMPOSITE_DOMAIN_V2, parent, b"a")
        );
        assert_ne!(
            composite_hash_v1(COMPOSITE_DOMAIN_V2, parent, b"a"),
            composite_hash_v1(COMPOSITE_DOMAIN_V3, parent, b"a")
        );
        assert_ne!(
            composite_hash_v1(COMPOSITE_DOMAIN_V3, parent, b"a"),
            composite_hash_v1(COMPOSITE_DOMAIN_V4, parent, b"a")
        );
        assert_ne!(
            composite_hash_v1(COMPOSITE_DOMAIN_V4, parent, b"a"),
            composite_hash_v1(COMPOSITE_DOMAIN_V5, parent, b"a")
        );
        assert_ne!(
            composite_hash_v1(COMPOSITE_DOMAIN_V5, parent, b"a"),
            composite_hash_v1(COMPOSITE_DOMAIN_V6, parent, b"a")
        );
        assert_ne!(
            composite_hash_v1(COMPOSITE_DOMAIN_V6, parent, b"a"),
            composite_hash_v1(COMPOSITE_DOMAIN_V7, parent, b"a")
        );
    }

    #[test]
    #[ignore = "requires MTG_STRUCTURED_POLICY_SUCCESSOR_ROOT and MTG_STRUCTURED_POLICY_SUCCESSOR_PARITY_FIXTURE"]
    fn successor_external_parity_fixture_v1() {
        let root = std::env::var_os("MTG_STRUCTURED_POLICY_SUCCESSOR_ROOT")
            .expect("MTG_STRUCTURED_POLICY_SUCCESSOR_ROOT is set");
        let fixture_path = std::env::var_os("MTG_STRUCTURED_POLICY_SUCCESSOR_PARITY_FIXTURE")
            .expect("MTG_STRUCTURED_POLICY_SUCCESSOR_PARITY_FIXTURE is set");
        let inference =
            load_native_structured_policy_successor_transport_inference_v1(Path::new(&root))
                .expect("staged package must pass strict candidate, weights, and parent loading");
        let fixture: ParityFixtureV1 =
            serde_json::from_slice(&fs::read(fixture_path).unwrap()).unwrap();
        assert!(matches!(
            fixture.schema.as_str(),
            "mtg-kernel-structured-policy-successor-parity-fixture/v1"
                | "mtg-kernel-structured-policy-terminal-rung-parity-fixture/v1"
                | "mtg-kernel-structured-policy-terminal-trust-projection-parity-fixture/v1"
                | "mtg-kernel-structured-policy-terminal-head-only-parity-fixture/v1"
                | "mtg-kernel-structured-policy-space-response-oracle-parity-fixture/v1"
                | "mtg-kernel-structured-policy-block-response-oracle-parity-fixture/v1"
                | "mtg-kernel-structured-policy-bounded-logit-residual-parity-fixture/v1"
        ));
        assert!(matches!(
            fixture.output_semantics.as_str(),
            "absolute-structured-logits-and-exact-parent-value/v1"
                | "bounded-structured-logits-and-exact-parent-value/v1"
        ));
        assert_eq!(fixture.examples.len(), 10);
        let mut coverage = BTreeSet::new();
        let mut maximum_delta = 0.0f32;
        for example in fixture.examples {
            assert!(example.acting_player <= 1 && example.candidate_seat <= 1);
            let derived_bucket = match example.history.len() {
                0 => 0,
                1..=3 => 1,
                4..=7 => 4,
                8..=15 => 8,
                HISTORY_LENGTH_V1 => 16,
                _ => panic!("fixture history exceeds the fixed window"),
            };
            assert_eq!(derived_bucket, example.history_length_bucket);
            coverage.insert((example.acting_player, example.history_length_bucket));
            assert_eq!(example.expected_value_residual_f32_bits, "00000000");
            let history = example
                .history
                .into_iter()
                .map(|entry| {
                    NativeStructuredHistoryEntryV1::new_v1(
                        entry.acting_player,
                        entry.action_explicit_features.try_into().unwrap(),
                        entry.public_card_histogram.try_into().unwrap(),
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            let tensor = example.tensor.into_native_v1();
            let observed = structured_residual_v1(
                &inference.parameters,
                &tensor,
                Some((&history, example.acting_player)),
                HISTORY_GROUP_VOCAB_V1,
            )
            .unwrap();
            let observed_logits = match (
                &inference.initializer_parameters,
                inference.logit_residual_clip,
            ) {
                (Some(initializer_parameters), Some(clip)) => {
                    let initializer = structured_residual_v1(
                        initializer_parameters,
                        &tensor,
                        Some((&history, example.acting_player)),
                        HISTORY_GROUP_VOCAB_V1,
                    )
                    .unwrap();
                    bounded_structured_logit_residual_v1(
                        initializer.logits_v1(),
                        observed.logits_v1(),
                        clip,
                    )
                    .unwrap()
                }
                (None, None) => observed.logits_v1().to_vec(),
                _ => panic!("incomplete bounded-logit runtime"),
            };
            assert_eq!(
                observed_logits.len(),
                example.expected_structured_logits.len()
            );
            for (actual, expected) in observed_logits
                .iter()
                .zip(example.expected_structured_logits)
            {
                maximum_delta = maximum_delta.max((actual - expected).abs());
            }
            assert_eq!(observed.value_v1().unwrap().to_bits(), 0.0f32.to_bits());
        }
        assert_eq!(
            coverage,
            BTreeSet::from([
                (0, 0),
                (0, 1),
                (0, 4),
                (0, 8),
                (0, 16),
                (1, 0),
                (1, 1),
                (1, 4),
                (1, 8),
                (1, 16),
            ])
        );
        assert!(
            maximum_delta <= 3.0e-5,
            "maximum parity delta {maximum_delta}"
        );
        eprintln!("maximum_absolute_logit_error={maximum_delta:.9}");
    }
}
