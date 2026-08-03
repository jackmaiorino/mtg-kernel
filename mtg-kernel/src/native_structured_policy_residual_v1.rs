//! Strict live inference for the fixed structured residual families.

use crate::flat_policy_v2::FlatScoringDecisionViewV2;
use crate::native_flat_tensorizer_v2::{
    NativeFlatDecisionTensorV2, NativeFlatTensorizerV2, NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2,
    NATIVE_FLAT_ACTION_FEATURE_DIM_V2, NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2,
    NATIVE_FLAT_EDGE_FEATURE_DIM_V2, NATIVE_FLAT_OBJECT_FEATURE_DIM_V2,
    NATIVE_FLAT_STATE_FEATURE_DIM_V2,
};
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1,
};
use crate::native_xmage_cp7_outcome_reinforce_v1::{
    load_xmage_cp7_outcome_inference_v1, NativeXmageCp7OutcomeInferenceV1,
};
use crate::rl::parse_strict_json_value;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;

const CANDIDATE_FILENAME_V1: &str = "structured_candidate.json";
const HISTORY_CANDIDATE_FILENAME_V1: &str = "structured_history_candidate.json";
const REPORT_FILENAME_V1: &str = "report.json";
const WEIGHTS_FILENAME_V1: &str = "weights.f32le";
const PARENT_DIRECTORY_V1: &str = "parent";
const PARENT_MANIFEST_FILENAME_V1: &str = "checkpoint.json";
const PARENT_STATE_FILENAME_V1: &str = "checkpoint.state.f32le";
const CANDIDATE_SCHEMA_V1: &str = "mtg-kernel-structured-policy-residual-candidate/v1";
const HISTORY_CANDIDATE_SCHEMA_V1: &str =
    "mtg-kernel-structured-history-policy-value-residual-candidate/v1";
const REPORT_SCHEMA_V1: &str = "mtg-kernel-structured-policy-residual-fit/v1";
const HISTORY_REPORT_SCHEMA_V1: &str = "mtg-kernel-structured-history-policy-value-residual-fit/v1";
const PUBLICATION_ENCODING_V1: &str = "json-pretty-sorted-utf8-trailing-lf/v1";
const WEIGHTS_ENCODING_V1: &str = "ordered-row-major-finite-f32-little-endian/v1";
const ARCHITECTURE_V1: &str = "stateless-structured-object-action-attention-policy-residual/v1";
const HISTORY_ARCHITECTURE_V1: &str =
    "complete-public-history-structured-object-action-attention-policy-value-residual/v1";
const VALUE_MODEL_V1: &str = "exact-parent-unchanged";
const REPORT_VALUE_MODEL_V1: &str = "exact-retained-parent-unchanged";
const HISTORY_VALUE_MODEL_V1: &str = "joint-terminal-residual/v1";
const COMPOSITE_DOMAIN_V1: &[u8] = b"mtg-kernel-structured-policy-residual-composite-model/v1";
const HISTORY_COMPOSITE_DOMAIN_V1: &[u8] =
    b"mtg-kernel-structured-history-policy-value-residual-composite-model/v1";
const PARENT_MANIFEST_SHA256_V1: &str =
    "706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb";
const PARENT_PAYLOAD_SHA256_V1: &str =
    "eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c";
const PARENT_NATIVE_STATE_SHA256_V1: &str =
    "2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8";
const PARENT_MODEL_PARAMETER_SHA256_V1: &str =
    "883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546";
const PARENT_ADAM_STEP_V1: u64 = 1;
const HIDDEN_DIM_V1: usize = 48;
pub(crate) const CARD_VOCAB_V1: usize = 136;
const CARD_EMBEDDING_DIM_V1: usize = 24;
const GROUP_VOCAB_V1: usize = 7;
const GROUP_EMBEDDING_DIM_V1: usize = 16;
const PARAMETER_COUNT_V1: usize = 63_521;
pub(crate) const HISTORY_LENGTH_V1: usize = 16;
const HISTORY_ROLE_DIM_V1: usize = 2;
const HISTORY_FEATURE_DIM_V1: usize =
    NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2 + HISTORY_ROLE_DIM_V1 + CARD_VOCAB_V1;
const HISTORY_PARAMETER_COUNT_V1: usize = 107_298;

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
    #[serde(default)]
    history_length: Option<usize>,
    #[serde(default)]
    history_feature_dim: Option<usize>,
    #[serde(default)]
    history_role_dim: Option<usize>,
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

struct TensorV1 {
    shape: Vec<usize>,
    values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeStructuredHistoryEntryV1 {
    acting_player: u8,
    action_explicit_features: [f32; NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2],
    public_card_histogram: [f32; CARD_VOCAB_V1],
}

impl NativeStructuredHistoryEntryV1 {
    pub(crate) fn new_v1(
        acting_player: u8,
        action_explicit_features: [f32; NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2],
        public_card_histogram: [f32; CARD_VOCAB_V1],
    ) -> Result<Self, ()> {
        if acting_player > 1
            || action_explicit_features
                .iter()
                .any(|value| !value.is_finite())
            || public_card_histogram.iter().any(|value| !value.is_finite())
        {
            return Err(());
        }
        Ok(Self {
            acting_player,
            action_explicit_features,
            public_card_histogram,
        })
    }
}

pub(crate) struct NativeStructuredPolicyResidualOutputV1 {
    logits: Vec<f32>,
    value: f32,
}

impl NativeStructuredPolicyResidualOutputV1 {
    pub(crate) fn logits_v1(&self) -> &[f32] {
        &self.logits
    }

    pub(crate) const fn value_v1(&self) -> f32 {
        self.value
    }
}

pub(crate) struct NativeStructuredPolicyResidualInferenceV1 {
    parent: NativeXmageCp7OutcomeInferenceV1,
    parameters: BTreeMap<String, TensorV1>,
    candidate_json_sha256: [u8; 32],
    weights_sha256: [u8; 32],
    report_sha256: [u8; 32],
    composite_model_parameter_sha256: [u8; 32],
    history_aware: bool,
}

impl NativeStructuredPolicyResidualInferenceV1 {
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

    pub(crate) const fn is_history_aware_v1(&self) -> bool {
        self.history_aware
    }

    pub(crate) fn score_decision_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
    ) -> Result<NativeStructuredPolicyResidualOutputV1, ()> {
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
        self.score_tensor_v1(parent, tensor, action_count, &[], 0)
    }

    pub(crate) fn score_decision_with_history_v1(
        &self,
        decision: FlatScoringDecisionViewV2<'_>,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
    ) -> Result<NativeStructuredPolicyResidualOutputV1, ()> {
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
        self.score_tensor_v1(parent, tensor, action_count, history, acting_player)
    }

    fn score_tensor_v1(
        &self,
        parent: crate::native_xmage_cp7_outcome_reinforce_v1::NativeXmageCp7OutcomeInferenceOutputV1,
        tensor: NativeFlatDecisionTensorV2,
        action_count: usize,
        history: &[NativeStructuredHistoryEntryV1],
        acting_player: u8,
    ) -> Result<NativeStructuredPolicyResidualOutputV1, ()> {
        if acting_player > 1 {
            return Err(());
        }
        let residual = structured_residual_v1(
            &self.parameters,
            &tensor,
            self.history_aware.then_some((history, acting_player)),
        )?;
        if residual.logits.len() != action_count {
            return Err(());
        }
        let logits = parent
            .logits_v1()
            .iter()
            .zip(residual.logits)
            .map(|(parent, residual)| parent + residual)
            .collect::<Vec<_>>();
        let value = parent.value_v1() + residual.value.unwrap_or(0.0);
        if logits.iter().any(|value| !value.is_finite()) || !value.is_finite() {
            return Err(());
        }
        Ok(NativeStructuredPolicyResidualOutputV1 { logits, value })
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

fn expected_parameters_v1() -> Vec<(&'static str, Vec<usize>)> {
    vec![
        ("state.0.weight", vec![48, 219]),
        ("state.0.bias", vec![48]),
        ("state.2.weight", vec![48, 48]),
        ("state.2.bias", vec![48]),
        ("object.0.weight", vec![48, 138]),
        ("object.0.bias", vec![48]),
        ("card.weight", vec![136, 24]),
        ("group.weight", vec![7, 16]),
        ("edge.0.weight", vec![48, 89]),
        ("edge.0.bias", vec![48]),
        ("edge.2.weight", vec![48, 48]),
        ("edge.2.bias", vec![48]),
        ("group_mix.weight", vec![48, 48]),
        ("action.0.weight", vec![48, 195]),
        ("action.0.bias", vec![48]),
        ("ref.0.weight", vec![48, 73]),
        ("ref.0.bias", vec![48]),
        ("query.weight", vec![48, 96]),
        ("query.bias", vec![48]),
        ("combine.0.weight", vec![48, 240]),
        ("combine.0.bias", vec![48]),
        ("combine.2.weight", vec![48, 48]),
        ("combine.2.bias", vec![48]),
        ("policy_head.weight", vec![1, 48]),
        ("policy_head.bias", vec![1]),
    ]
}

fn expected_history_parameters_v1() -> Vec<(&'static str, Vec<usize>)> {
    vec![
        ("state.0.weight", vec![48, 219]),
        ("state.0.bias", vec![48]),
        ("state.2.weight", vec![48, 48]),
        ("state.2.bias", vec![48]),
        ("history.weight_ih_l0", vec![144, 237]),
        ("history.weight_hh_l0", vec![144, 48]),
        ("history.bias_ih_l0", vec![144]),
        ("history.bias_hh_l0", vec![144]),
        ("history_mix.weight", vec![48, 48]),
        ("object.0.weight", vec![48, 138]),
        ("object.0.bias", vec![48]),
        ("card.weight", vec![136, 24]),
        ("group.weight", vec![7, 16]),
        ("edge.0.weight", vec![48, 89]),
        ("edge.0.bias", vec![48]),
        ("edge.2.weight", vec![48, 48]),
        ("edge.2.bias", vec![48]),
        ("group_mix.weight", vec![48, 48]),
        ("action.0.weight", vec![48, 195]),
        ("action.0.bias", vec![48]),
        ("ref.0.weight", vec![48, 73]),
        ("ref.0.bias", vec![48]),
        ("query.weight", vec![48, 96]),
        ("query.bias", vec![48]),
        ("combine.0.weight", vec![48, 240]),
        ("combine.0.bias", vec![48]),
        ("combine.2.weight", vec![48, 48]),
        ("combine.2.bias", vec![48]),
        ("policy_head.weight", vec![1, 48]),
        ("policy_head.bias", vec![1]),
        ("value_head.weight", vec![1, 144]),
        ("value_head.bias", vec![1]),
    ]
}

fn parameter_v1<'a>(
    parameters: &'a BTreeMap<String, TensorV1>,
    name: &str,
) -> Result<&'a TensorV1, ()> {
    parameters.get(name).ok_or(())
}

fn linear_v1(weight: &TensorV1, bias: Option<&TensorV1>, input: &[f32]) -> Result<Vec<f32>, ()> {
    if weight.shape.len() != 2 || weight.shape[1] != input.len() {
        return Err(());
    }
    let output_dim = weight.shape[0];
    if weight.values.len() != output_dim.checked_mul(input.len()).ok_or(())?
        || bias.is_some_and(|bias| bias.shape != [output_dim] || bias.values.len() != output_dim)
    {
        return Err(());
    }
    let mut output = Vec::with_capacity(output_dim);
    for row in weight.values.chunks_exact(input.len()) {
        let mut sum = 0.0f32;
        for (value, input) in row.iter().zip(input) {
            sum += value * input;
        }
        output.push(sum);
    }
    if let Some(bias) = bias {
        for (value, bias) in output.iter_mut().zip(&bias.values) {
            *value += bias;
        }
    }
    if output.iter().any(|value| !value.is_finite()) {
        return Err(());
    }
    Ok(output)
}

fn named_linear_v1(
    parameters: &BTreeMap<String, TensorV1>,
    weight: &str,
    bias: Option<&str>,
    input: &[f32],
) -> Result<Vec<f32>, ()> {
    linear_v1(
        parameter_v1(parameters, weight)?,
        bias.map(|name| parameter_v1(parameters, name))
            .transpose()?,
        input,
    )
}

fn tanh_v1(values: &mut [f32]) {
    for value in values {
        *value = value.tanh();
    }
}

fn embedding_row_v1<'a>(tensor: &'a TensorV1, row: usize) -> Result<&'a [f32], ()> {
    if tensor.shape.len() != 2 || row >= tensor.shape[0] {
        return Err(());
    }
    let width = tensor.shape[1];
    tensor
        .values
        .get(row.checked_mul(width).ok_or(())?..(row + 1).checked_mul(width).ok_or(())?)
        .ok_or(())
}

fn nonnegative_modulo_v1(value: i64, modulus: usize) -> Result<usize, ()> {
    usize::try_from(value)
        .map(|value| value % modulus)
        .map_err(|_| ())
}

fn checked_index_v1(value: i64, count: usize) -> Result<usize, ()> {
    let value = usize::try_from(value).map_err(|_| ())?;
    (value < count).then_some(value).ok_or(())
}

struct StructuredResidualV1 {
    logits: Vec<f32>,
    value: Option<f32>,
}

fn sigmoid_v1(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}

fn history_hidden_v1(
    parameters: &BTreeMap<String, TensorV1>,
    history: &[NativeStructuredHistoryEntryV1],
    acting_player: u8,
) -> Result<Vec<f32>, ()> {
    if acting_player > 1 || history.len() > HISTORY_LENGTH_V1 {
        return Err(());
    }
    let mut hidden = vec![0.0f32; HIDDEN_DIM_V1];
    for entry in history {
        let mut input = Vec::with_capacity(HISTORY_FEATURE_DIM_V1);
        input.extend_from_slice(&entry.action_explicit_features);
        input.push(f32::from(entry.acting_player == acting_player));
        input.push(f32::from(entry.acting_player != acting_player));
        input.extend_from_slice(&entry.public_card_histogram);
        let input_gates = named_linear_v1(
            parameters,
            "history.weight_ih_l0",
            Some("history.bias_ih_l0"),
            &input,
        )?;
        let hidden_gates = named_linear_v1(
            parameters,
            "history.weight_hh_l0",
            Some("history.bias_hh_l0"),
            &hidden,
        )?;
        if input_gates.len() != HIDDEN_DIM_V1 * 3 || hidden_gates.len() != HIDDEN_DIM_V1 * 3 {
            return Err(());
        }
        let mut next = vec![0.0f32; HIDDEN_DIM_V1];
        for index in 0..HIDDEN_DIM_V1 {
            let reset = sigmoid_v1(input_gates[index] + hidden_gates[index]);
            let update = sigmoid_v1(
                input_gates[HIDDEN_DIM_V1 + index] + hidden_gates[HIDDEN_DIM_V1 + index],
            );
            let new = (input_gates[HIDDEN_DIM_V1 * 2 + index]
                + reset * hidden_gates[HIDDEN_DIM_V1 * 2 + index])
                .tanh();
            next[index] = (1.0 - update) * new + update * hidden[index];
        }
        if next.iter().any(|value| !value.is_finite()) {
            return Err(());
        }
        hidden = next;
    }
    Ok(hidden)
}

fn structured_residual_v1(
    parameters: &BTreeMap<String, TensorV1>,
    tensor: &NativeFlatDecisionTensorV2,
    history: Option<(&[NativeStructuredHistoryEntryV1], u8)>,
) -> Result<StructuredResidualV1, ()> {
    if tensor.state.len() != NATIVE_FLAT_STATE_FEATURE_DIM_V2
        || tensor.object_card_ids.is_empty()
        || tensor.object_groups.len() != tensor.object_card_ids.len()
        || tensor.object_features.len()
            != tensor
                .object_card_ids
                .len()
                .checked_mul(NATIVE_FLAT_OBJECT_FEATURE_DIM_V2)
                .ok_or(())?
        || tensor.edge_source_indices.len() != tensor.edge_target_indices.len()
        || tensor.edge_features.len()
            != tensor
                .edge_source_indices
                .len()
                .checked_mul(NATIVE_FLAT_EDGE_FEATURE_DIM_V2)
                .ok_or(())?
        || tensor.action_features.is_empty()
        || !tensor
            .action_features
            .len()
            .is_multiple_of(NATIVE_FLAT_ACTION_FEATURE_DIM_V2)
        || tensor.action_ref_action_indices.len() != tensor.action_ref_node_indices.len()
        || tensor.action_ref_features.len()
            != tensor
                .action_ref_action_indices
                .len()
                .checked_mul(NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2)
                .ok_or(())?
    {
        return Err(());
    }
    let object_count = tensor.object_card_ids.len();
    let action_count = tensor.action_features.len() / NATIVE_FLAT_ACTION_FEATURE_DIM_V2;

    let mut state_h = named_linear_v1(
        parameters,
        "state.0.weight",
        Some("state.0.bias"),
        &tensor.state,
    )?;
    tanh_v1(&mut state_h);
    state_h = named_linear_v1(parameters, "state.2.weight", Some("state.2.bias"), &state_h)?;
    tanh_v1(&mut state_h);
    if let Some((history, acting_player)) = history {
        let history_h = history_hidden_v1(parameters, history, acting_player)?;
        let mixed = named_linear_v1(parameters, "history_mix.weight", None, &history_h)?;
        for (state, value) in state_h.iter_mut().zip(mixed) {
            *state += value;
        }
    }

    let card_embedding = parameter_v1(parameters, "card.weight")?;
    let group_embedding = parameter_v1(parameters, "group.weight")?;
    let mut object_h = Vec::with_capacity(object_count);
    let mut groups = Vec::with_capacity(object_count);
    for index in 0..object_count {
        let group = nonnegative_modulo_v1(tensor.object_groups[index], GROUP_VOCAB_V1)?;
        let card = nonnegative_modulo_v1(tensor.object_card_ids[index], CARD_VOCAB_V1)?;
        groups.push(group);
        let start = index * NATIVE_FLAT_OBJECT_FEATURE_DIM_V2;
        let mut input = Vec::with_capacity(
            NATIVE_FLAT_OBJECT_FEATURE_DIM_V2 + CARD_EMBEDDING_DIM_V1 + GROUP_EMBEDDING_DIM_V1,
        );
        input.extend_from_slice(
            &tensor.object_features[start..start + NATIVE_FLAT_OBJECT_FEATURE_DIM_V2],
        );
        input.extend_from_slice(embedding_row_v1(card_embedding, card)?);
        input.extend_from_slice(embedding_row_v1(group_embedding, group)?);
        let mut hidden =
            named_linear_v1(parameters, "object.0.weight", Some("object.0.bias"), &input)?;
        tanh_v1(&mut hidden);
        object_h.push(hidden);
    }

    if !tensor.edge_source_indices.is_empty() {
        let mut aggregate = vec![vec![0.0f32; HIDDEN_DIM_V1]; object_count];
        let mut degree = vec![0usize; object_count];
        for edge in 0..tensor.edge_source_indices.len() {
            let source = checked_index_v1(tensor.edge_source_indices[edge], object_count)?;
            let target = checked_index_v1(tensor.edge_target_indices[edge], object_count)?;
            let start = edge * NATIVE_FLAT_EDGE_FEATURE_DIM_V2;
            let mut input = Vec::with_capacity(HIDDEN_DIM_V1 + NATIVE_FLAT_EDGE_FEATURE_DIM_V2);
            input.extend_from_slice(&object_h[source]);
            input.extend_from_slice(
                &tensor.edge_features[start..start + NATIVE_FLAT_EDGE_FEATURE_DIM_V2],
            );
            let mut message =
                named_linear_v1(parameters, "edge.0.weight", Some("edge.0.bias"), &input)?;
            tanh_v1(&mut message);
            message = named_linear_v1(parameters, "edge.2.weight", Some("edge.2.bias"), &message)?;
            for (sum, value) in aggregate[target].iter_mut().zip(message) {
                *sum += value;
            }
            degree[target] += 1;
        }
        for index in 0..object_count {
            let denominator = 1.0 + degree[index] as f32;
            for hidden in 0..HIDDEN_DIM_V1 {
                object_h[index][hidden] += aggregate[index][hidden] / denominator;
            }
        }
    }

    let mut pooled = vec![vec![0.0f32; HIDDEN_DIM_V1]; GROUP_VOCAB_V1];
    let mut group_counts = [0usize; GROUP_VOCAB_V1];
    for (hidden, group) in object_h.iter().zip(&groups) {
        for (sum, value) in pooled[*group].iter_mut().zip(hidden) {
            *sum += value;
        }
        group_counts[*group] += 1;
    }
    for group in 0..GROUP_VOCAB_V1 {
        let denominator = group_counts[group].max(1) as f32;
        for value in &mut pooled[group] {
            *value /= denominator;
        }
    }
    for index in 0..object_count {
        let mixed = named_linear_v1(parameters, "group_mix.weight", None, &pooled[groups[index]])?;
        for (hidden, value) in object_h[index].iter_mut().zip(mixed) {
            *hidden += value;
        }
    }

    let mut action_h = Vec::with_capacity(action_count);
    for action in tensor
        .action_features
        .chunks_exact(NATIVE_FLAT_ACTION_FEATURE_DIM_V2)
    {
        let mut hidden =
            named_linear_v1(parameters, "action.0.weight", Some("action.0.bias"), action)?;
        tanh_v1(&mut hidden);
        action_h.push(hidden);
    }

    let mut ref_aggregate = vec![vec![0.0f32; HIDDEN_DIM_V1]; action_count];
    let mut ref_counts = vec![0usize; action_count];
    for reference in 0..tensor.action_ref_action_indices.len() {
        let action = checked_index_v1(tensor.action_ref_action_indices[reference], action_count)?;
        let node = checked_index_v1(tensor.action_ref_node_indices[reference], object_count)?;
        let start = reference * NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2;
        let mut input = Vec::with_capacity(NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2 + HIDDEN_DIM_V1);
        input.extend_from_slice(
            &tensor.action_ref_features[start..start + NATIVE_FLAT_ACTION_REF_FEATURE_DIM_V2],
        );
        input.extend_from_slice(&object_h[node]);
        let mut hidden = named_linear_v1(parameters, "ref.0.weight", Some("ref.0.bias"), &input)?;
        tanh_v1(&mut hidden);
        for (sum, value) in ref_aggregate[action].iter_mut().zip(hidden) {
            *sum += value;
        }
        ref_counts[action] += 1;
    }
    for action in 0..action_count {
        let denominator = ref_counts[action].max(1) as f32;
        for value in &mut ref_aggregate[action] {
            *value /= denominator;
        }
    }

    let mut residual = Vec::with_capacity(action_count);
    let mut joints = Vec::with_capacity(action_count);
    let inverse_root_width = 1.0 / (HIDDEN_DIM_V1 as f32).sqrt();
    for action in 0..action_count {
        let mut query_input = Vec::with_capacity(HIDDEN_DIM_V1 * 2);
        query_input.extend_from_slice(&action_h[action]);
        query_input.extend_from_slice(&ref_aggregate[action]);
        let query = named_linear_v1(parameters, "query.weight", Some("query.bias"), &query_input)?;
        let mut attention_logits = Vec::with_capacity(object_count);
        for object in &object_h {
            let mut dot = 0.0f32;
            for (query, key) in query.iter().zip(object) {
                dot += query * key;
            }
            attention_logits.push(dot * inverse_root_width);
        }
        let maximum = attention_logits
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        let mut attention_sum = 0.0f32;
        for value in &mut attention_logits {
            *value = (*value - maximum).exp();
            attention_sum += *value;
        }
        if !attention_sum.is_finite() || attention_sum <= 0.0 {
            return Err(());
        }
        let mut context = vec![0.0f32; HIDDEN_DIM_V1];
        for (weight, object) in attention_logits.iter().zip(&object_h) {
            let weight = *weight / attention_sum;
            for (sum, value) in context.iter_mut().zip(object) {
                *sum += weight * value;
            }
        }
        let mut joint_input = Vec::with_capacity(HIDDEN_DIM_V1 * 5);
        joint_input.extend_from_slice(&action_h[action]);
        joint_input.extend_from_slice(&ref_aggregate[action]);
        joint_input.extend_from_slice(&context);
        joint_input.extend_from_slice(&state_h);
        joint_input.extend(
            action_h[action]
                .iter()
                .zip(&context)
                .map(|(action, context)| action * context),
        );
        let mut joint = named_linear_v1(
            parameters,
            "combine.0.weight",
            Some("combine.0.bias"),
            &joint_input,
        )?;
        tanh_v1(&mut joint);
        joint = named_linear_v1(
            parameters,
            "combine.2.weight",
            Some("combine.2.bias"),
            &joint,
        )?;
        tanh_v1(&mut joint);
        let output = named_linear_v1(
            parameters,
            "policy_head.weight",
            Some("policy_head.bias"),
            &joint,
        )?;
        residual.push(output[0]);
        joints.push(joint);
    }
    if residual.iter().any(|value| !value.is_finite()) {
        return Err(());
    }
    let value = if history.is_some() {
        let mut object_mean = vec![0.0f32; HIDDEN_DIM_V1];
        for object in &object_h {
            for (sum, value) in object_mean.iter_mut().zip(object) {
                *sum += value;
            }
        }
        for value in &mut object_mean {
            *value /= object_count as f32;
        }
        let mut group_mean = vec![0.0f32; HIDDEN_DIM_V1];
        for group in &pooled {
            for (sum, value) in group_mean.iter_mut().zip(group) {
                *sum += value;
            }
        }
        for value in &mut group_mean {
            *value /= GROUP_VOCAB_V1 as f32;
        }
        let mut action_mean = vec![0.0f32; HIDDEN_DIM_V1];
        for joint in &joints {
            for (sum, value) in action_mean.iter_mut().zip(joint) {
                *sum += value;
            }
        }
        for value in &mut action_mean {
            *value /= action_count as f32;
        }
        let mut input = Vec::with_capacity(HIDDEN_DIM_V1 * 3);
        input.extend_from_slice(&state_h);
        input.extend(
            object_mean
                .iter()
                .zip(group_mean)
                .map(|(object, group)| object + group),
        );
        input.extend_from_slice(&action_mean);
        Some(
            named_linear_v1(
                parameters,
                "value_head.weight",
                Some("value_head.bias"),
                &input,
            )?[0],
        )
    } else {
        None
    };
    Ok(StructuredResidualV1 {
        logits: residual,
        value,
    })
}

fn raw_sha256_v1(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn lower_hex_v1(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_lower_hex32_v1(value: &str) -> Result<[u8; 32], Box<dyn Error>> {
    if value.len() != 64
        || value
            .as_bytes()
            .iter()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid lowercase SHA-256").into());
    }
    let mut output = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = u8::from_str_radix(std::str::from_utf8(pair)?, 16)?;
    }
    Ok(output)
}

fn strict_json_value_v1(bytes: &[u8]) -> Result<Value, Box<dyn Error>> {
    if !bytes.ends_with(b"\n") || bytes.contains(&b'\r') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid JSON framing").into());
    }
    Ok(parse_strict_json_value(std::str::from_utf8(bytes)?)?)
}

fn validate_report_v1(
    report: &Value,
    weights_sha256: [u8; 32],
    composite_sha256: [u8; 32],
    history_aware: bool,
) -> Result<(), Box<dyn Error>> {
    let config = report
        .get("config")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "missing structured report config",
            )
        })?;
    let movement = report
        .get("calibrated_movement")
        .and_then(Value::as_object)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing structured movement"))?;
    let seat_metrics = report
        .pointer("/policy_metrics/by_acting_seat")
        .and_then(Value::as_object)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing seat metrics"))?;
    let exact_number =
        |name: &str, expected: u64| config.get(name).and_then(Value::as_u64) == Some(expected);
    let exact_float = |name: &str, expected: f64| {
        config.get(name).and_then(Value::as_f64).map(f64::to_bits) == Some(expected.to_bits())
    };
    let movement_finite = [
        "mean_total_variation",
        "p90_total_variation",
        "mean_parent_to_candidate_kl",
    ]
    .into_iter()
    .all(|name| {
        movement
            .get(name)
            .and_then(Value::as_f64)
            .is_some_and(f64::is_finite)
    });
    let seat_positive = ["0", "1"].into_iter().all(|seat| {
        seat_metrics
            .get(seat)
            .and_then(|value| value.get("relative_nll_improvement"))
            .and_then(Value::as_f64)
            .is_some_and(|value| value.is_finite() && value > 0.0)
    });
    let mean_tv = movement
        .get("mean_total_variation")
        .and_then(Value::as_f64)
        .unwrap_or(f64::NAN);
    let common_invalid = !exact_number("dim", HIDDEN_DIM_V1 as u64)
        || !exact_number("card_vocab", CARD_VOCAB_V1 as u64)
        || !exact_number("group_vocab", GROUP_VOCAB_V1 as u64)
        || !exact_number("seed", 20_260_802)
        || !exact_float("learning_rate", 3.0e-4)
        || !exact_float("weight_decay", 1.0e-4)
        || !movement_finite
        || !seat_positive
        || report.get("weights_sha256").and_then(Value::as_str)
            != Some(lower_hex_v1(weights_sha256).as_str())
        || report
            .get("composite_model_parameter_sha256")
            .and_then(Value::as_str)
            != Some(lower_hex_v1(composite_sha256).as_str());
    let family_invalid = if history_aware {
        let value_seats = report
            .pointer("/value_metrics/by_candidate_seat")
            .and_then(Value::as_object);
        report.get("schema").and_then(Value::as_str) != Some(HISTORY_REPORT_SCHEMA_V1)
            || config.get("architecture").and_then(Value::as_str)
                != Some(HISTORY_ARCHITECTURE_V1)
            || config.get("value_model").and_then(Value::as_str)
                != Some(HISTORY_VALUE_MODEL_V1)
            || !exact_number("epochs", 5)
            || !exact_number("batch_size", 64)
            || !exact_number("history_length", HISTORY_LENGTH_V1 as u64)
            || !exact_number("history_feature_dim", HISTORY_FEATURE_DIM_V1 as u64)
            || !exact_float("residual_scale", 1.0)
            || value_seats.is_none_or(|seats| {
                ["0", "1"].into_iter().any(|seat| {
                    let Some(metrics) = seats.get(seat) else {
                        return true;
                    };
                    let parent = metrics.get("parent_mse").and_then(Value::as_f64);
                    let candidate = metrics.get("candidate_mse").and_then(Value::as_f64);
                    !matches!((parent, candidate), (Some(parent), Some(candidate)) if parent.is_finite() && candidate.is_finite() && candidate < parent)
                })
            })
    } else {
        report.get("schema").and_then(Value::as_str) != Some(REPORT_SCHEMA_V1)
            || config.get("architecture").and_then(Value::as_str) != Some(ARCHITECTURE_V1)
            || config.get("value_model").and_then(Value::as_str) != Some(REPORT_VALUE_MODEL_V1)
            || !exact_number("epochs", 20)
            || !exact_number("batch_size", 32)
            || !exact_float("target_mean_total_variation", 0.02)
            || mean_tv > 0.020_000_000_001
    };
    if common_invalid || family_invalid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured report semantic mismatch",
        )
        .into());
    }
    Ok(())
}

fn validate_inventory_v1(root: &Path, candidate_filename: &str) -> Result<(), Box<dyn Error>> {
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 structured inventory")
        })?;
        let file_type = entry.file_type()?;
        if file_type.is_file() {
            files.insert(name);
        } else if file_type.is_dir() {
            directories.insert(name);
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid structured inventory type",
            )
            .into());
        }
    }
    if files
        != BTreeSet::from([
            candidate_filename.to_owned(),
            REPORT_FILENAME_V1.to_owned(),
            WEIGHTS_FILENAME_V1.to_owned(),
        ])
        || directories != BTreeSet::from([PARENT_DIRECTORY_V1.to_owned()])
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured inventory is not exact",
        )
        .into());
    }
    Ok(())
}

pub(crate) fn load_native_structured_policy_residual_inference_v1(
    root: &Path,
) -> Result<NativeStructuredPolicyResidualInferenceV1, Box<dyn Error>> {
    let stateless_exists = root.join(CANDIDATE_FILENAME_V1).try_exists()?;
    let history_aware = root.join(HISTORY_CANDIDATE_FILENAME_V1).try_exists()?;
    if stateless_exists == history_aware {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured root must contain exactly one candidate family",
        )
        .into());
    }
    let candidate_filename = if history_aware {
        HISTORY_CANDIDATE_FILENAME_V1
    } else {
        CANDIDATE_FILENAME_V1
    };
    validate_inventory_v1(root, candidate_filename)?;
    let candidate_bytes = fs::read(root.join(candidate_filename))?;
    let candidate: CandidateV1 = serde_json::from_value(strict_json_value_v1(&candidate_bytes)?)?;
    let report_bytes = fs::read(root.join(REPORT_FILENAME_V1))?;
    let report = strict_json_value_v1(&report_bytes)?;
    let weights_bytes = fs::read(root.join(WEIGHTS_FILENAME_V1))?;
    let candidate_sha256 = raw_sha256_v1(&candidate_bytes);
    let report_sha256 = raw_sha256_v1(&report_bytes);
    let weights_sha256 = raw_sha256_v1(&weights_bytes);
    let parent_model_sha256 = parse_lower_hex32_v1(PARENT_MODEL_PARAMETER_SHA256_V1)?;
    let mut composite_hasher = Sha256::new();
    composite_hasher.update(if history_aware {
        HISTORY_COMPOSITE_DOMAIN_V1
    } else {
        COMPOSITE_DOMAIN_V1
    });
    composite_hasher.update(parent_model_sha256);
    composite_hasher.update(&weights_bytes);
    let composite_sha256: [u8; 32] = composite_hasher.finalize().into();
    let expected_schema = if history_aware {
        HISTORY_CANDIDATE_SCHEMA_V1
    } else {
        CANDIDATE_SCHEMA_V1
    };
    let expected_architecture = if history_aware {
        HISTORY_ARCHITECTURE_V1
    } else {
        ARCHITECTURE_V1
    };
    let expected_value_model = if history_aware {
        HISTORY_VALUE_MODEL_V1
    } else {
        VALUE_MODEL_V1
    };
    let expected_parameter_count = if history_aware {
        HISTORY_PARAMETER_COUNT_V1
    } else {
        PARAMETER_COUNT_V1
    };
    let history_binding_invalid = if history_aware {
        candidate.architecture.history_length != Some(HISTORY_LENGTH_V1)
            || candidate.architecture.history_feature_dim != Some(HISTORY_FEATURE_DIM_V1)
            || candidate.architecture.history_role_dim != Some(HISTORY_ROLE_DIM_V1)
    } else {
        candidate.architecture.history_length.is_some()
            || candidate.architecture.history_feature_dim.is_some()
            || candidate.architecture.history_role_dim.is_some()
    };
    if candidate.schema != expected_schema
        || candidate.publication_encoding != PUBLICATION_ENCODING_V1
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
        || candidate.architecture.group_vocab != GROUP_VOCAB_V1
        || candidate.architecture.group_embedding_dim != GROUP_EMBEDDING_DIM_V1
        || candidate.architecture.value_model != expected_value_model
        || history_binding_invalid
        || candidate.weights.filename != WEIGHTS_FILENAME_V1
        || candidate.weights.encoding != WEIGHTS_ENCODING_V1
        || candidate.weights.sha256 != lower_hex_v1(weights_sha256)
        || candidate.weights.byte_count != weights_bytes.len()
        || candidate.weights.parameter_count != expected_parameter_count
        || candidate.weights.byte_count != expected_parameter_count * size_of::<f32>()
        || candidate.report.filename != REPORT_FILENAME_V1
        || candidate.report.sha256 != lower_hex_v1(report_sha256)
        || candidate.composite_model_parameter_sha256 != lower_hex_v1(composite_sha256)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "structured candidate binding mismatch",
        )
        .into());
    }
    validate_report_v1(&report, weights_sha256, composite_sha256, history_aware)?;

    let expected = if history_aware {
        expected_history_parameters_v1()
    } else {
        expected_parameters_v1()
    };
    if candidate.weights.parameters.len() != expected.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "parameter list mismatch").into());
    }
    let values = weights_bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("four-byte chunk")))
        .collect::<Vec<_>>();
    if values.len() != expected_parameter_count || values.iter().any(|value| !value.is_finite()) {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "invalid structured weights").into(),
        );
    }
    let mut offset = 0usize;
    let mut parameters = BTreeMap::new();
    for (binding, (expected_name, expected_shape)) in
        candidate.weights.parameters.iter().zip(expected)
    {
        let count = expected_shape
            .iter()
            .try_fold(1usize, |product, value| {
                product.checked_mul(*value).ok_or(())
            })
            .map_err(|()| io::Error::new(io::ErrorKind::InvalidData, "parameter shape overflow"))?;
        if binding.name != expected_name
            || binding.shape != expected_shape
            || binding.offset_f32 != offset
            || binding.count_f32 != count
        {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "parameter binding mismatch").into(),
            );
        }
        parameters.insert(
            binding.name.clone(),
            TensorV1 {
                shape: binding.shape.clone(),
                values: values[offset..offset + count].to_vec(),
            },
        );
        offset += count;
    }
    if offset != expected_parameter_count {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "parameter count mismatch").into());
    }

    let parent_directory = root.join(PARENT_DIRECTORY_V1);
    let parent_inventory = fs::read_dir(&parent_directory)?
        .map(|entry| {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid parent inventory",
                ));
            }
            entry.file_name().into_string().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "non-UTF8 parent inventory")
            })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if parent_inventory
        != BTreeSet::from([
            PARENT_MANIFEST_FILENAME_V1.to_owned(),
            PARENT_STATE_FILENAME_V1.to_owned(),
        ])
    {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "parent inventory mismatch").into());
    }
    let parent = load_xmage_cp7_outcome_inference_v1(&parent_directory)?;
    if parent.manifest_sha256_v1() != parse_lower_hex32_v1(PARENT_MANIFEST_SHA256_V1)?
        || parent.payload_sha256_v1() != parse_lower_hex32_v1(PARENT_PAYLOAD_SHA256_V1)?
        || parent.native_state_sha256_v1() != parse_lower_hex32_v1(PARENT_NATIVE_STATE_SHA256_V1)?
        || parent.model_parameter_sha256_v1() != parent_model_sha256
        || parent.adam_step_v1() != PARENT_ADAM_STEP_V1
    {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "loaded parent mismatch").into());
    }
    Ok(NativeStructuredPolicyResidualInferenceV1 {
        parent,
        parameters,
        candidate_json_sha256: candidate_sha256,
        weights_sha256,
        report_sha256,
        composite_model_parameter_sha256: composite_sha256,
        history_aware,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_layout_is_exact_v1() {
        let mut offset = 0usize;
        for (_, shape) in expected_parameters_v1() {
            offset += shape.into_iter().product::<usize>();
        }
        assert_eq!(offset, PARAMETER_COUNT_V1);
    }

    #[test]
    fn history_parameter_layout_is_exact_v1() {
        let mut offset = 0usize;
        for (_, shape) in expected_history_parameters_v1() {
            offset += shape.into_iter().product::<usize>();
        }
        assert_eq!(offset, HISTORY_PARAMETER_COUNT_V1);
    }

    #[test]
    fn linear_uses_row_major_weight_and_bias_v1() {
        let weight = TensorV1 {
            shape: vec![2, 3],
            values: vec![1.0, 2.0, 3.0, -1.0, 0.5, 2.0],
        };
        let bias = TensorV1 {
            shape: vec![2],
            values: vec![0.25, -0.5],
        };
        assert_eq!(
            linear_v1(&weight, Some(&bias), &[2.0, 3.0, 4.0]).unwrap(),
            vec![20.25, 7.0]
        );
    }

    #[test]
    fn history_gru_uses_pytorch_gate_order_v1() {
        let mut parameters = BTreeMap::new();
        parameters.insert(
            "history.weight_ih_l0".to_owned(),
            TensorV1 {
                shape: vec![HIDDEN_DIM_V1 * 3, HISTORY_FEATURE_DIM_V1],
                values: vec![0.0; HIDDEN_DIM_V1 * 3 * HISTORY_FEATURE_DIM_V1],
            },
        );
        parameters.insert(
            "history.weight_hh_l0".to_owned(),
            TensorV1 {
                shape: vec![HIDDEN_DIM_V1 * 3, HIDDEN_DIM_V1],
                values: vec![0.0; HIDDEN_DIM_V1 * 3 * HIDDEN_DIM_V1],
            },
        );
        let mut input_bias = vec![0.0; HIDDEN_DIM_V1 * 3];
        input_bias[..HIDDEN_DIM_V1].fill(0.2);
        input_bias[HIDDEN_DIM_V1..HIDDEN_DIM_V1 * 2].fill(-0.4);
        input_bias[HIDDEN_DIM_V1 * 2..].fill(0.3);
        parameters.insert(
            "history.bias_ih_l0".to_owned(),
            TensorV1 {
                shape: vec![HIDDEN_DIM_V1 * 3],
                values: input_bias,
            },
        );
        let mut hidden_bias = vec![0.0; HIDDEN_DIM_V1 * 3];
        hidden_bias[..HIDDEN_DIM_V1].fill(0.1);
        hidden_bias[HIDDEN_DIM_V1..HIDDEN_DIM_V1 * 2].fill(0.2);
        hidden_bias[HIDDEN_DIM_V1 * 2..].fill(-0.2);
        parameters.insert(
            "history.bias_hh_l0".to_owned(),
            TensorV1 {
                shape: vec![HIDDEN_DIM_V1 * 3],
                values: hidden_bias,
            },
        );
        let entry = NativeStructuredHistoryEntryV1::new_v1(
            0,
            [0.0; NATIVE_FLAT_ACTION_EXPLICIT_FEATURE_DIM_V2],
            [0.0; CARD_VOCAB_V1],
        )
        .unwrap();
        let observed = history_hidden_v1(&parameters, &[entry.clone(), entry], 1).unwrap();
        let reset = sigmoid_v1(0.3);
        let update = sigmoid_v1(-0.2);
        let new = (0.3 + reset * -0.2).tanh();
        let first = (1.0 - update) * new;
        let expected = (1.0 - update) * new + update * first;
        assert!(observed
            .iter()
            .all(|value| (*value - expected).abs() <= 1.0e-7));
    }

    #[test]
    #[ignore = "requires MTG_KERNEL_STRUCTURED_POLICY_RESIDUAL_ROOT"]
    fn external_fixed_candidate_loads_strictly_v1() {
        let root = std::env::var_os("MTG_KERNEL_STRUCTURED_POLICY_RESIDUAL_ROOT")
            .expect("MTG_KERNEL_STRUCTURED_POLICY_RESIDUAL_ROOT is set");
        let inference =
            load_native_structured_policy_residual_inference_v1(Path::new(&root)).unwrap();
        assert_eq!(
            lower_hex_v1(inference.candidate_json_sha256_v1()),
            "3918ebc432aa65216898707ef1cc63d49f4251a0968ab0200ecceb222fb93aee"
        );
        assert_eq!(
            lower_hex_v1(inference.weights_sha256_v1()),
            "fc159303af67f888e92e50d85b43899cac7bf373e0aed77a7ddd86a5ede0c406"
        );
        assert_eq!(
            lower_hex_v1(inference.report_sha256_v1()),
            "164853713285ffdaac6aa1ceb393bd6fd20386b1f081cdd55eac30a7454820a6"
        );
        assert_eq!(
            lower_hex_v1(inference.composite_model_parameter_sha256_v1()),
            "3ec3b507ec6475f0208b195a00d68ff075f7c914c43df6310b56ba75e82a4445"
        );
    }
}
