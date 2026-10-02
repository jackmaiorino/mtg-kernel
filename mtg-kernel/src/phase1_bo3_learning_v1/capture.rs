use super::require;
use crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2;
use crate::native_flat_tensorizer_v3::NativeFlatDecisionTensorV3;
use crate::phase1_agent_v1::{AgentSideboardPolicyV1, CompleteAgentPackageV1};
use crate::phase1_bo3_collection_v1::{Bo3CollectionConfigV1, Bo3CollectionResultV1};
use crate::sideboard_play_policy_v1::FrozenPlayDecisionScoresV1;
use serde::{Deserialize, Serialize};

pub const TRAINABLE_BO3_REQUEST_SCHEMA_V1: &str = "mtg-kernel-trainable-bo3-request/v1";
pub const TRAINABLE_BO3_RESULT_SCHEMA_V1: &str = "mtg-kernel-trainable-bo3-result/v1";
pub const BO3_NATIVE_CAPTURE_SCHEMA_V1: &str = "mtg-kernel-bo3-native-capture/v1";
const MAX_CAPTURE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_RECORD_PAYLOAD_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3NativeCaptureLimitsV1 {
    pub max_payload_bytes: u64,
    pub max_json_bytes: u64,
}
impl Bo3NativeCaptureLimitsV1 {
    pub(crate) fn validate(&self) -> Result<(), String> {
        require(
            (1..=MAX_CAPTURE_BYTES).contains(&self.max_payload_bytes)
                && (1..=MAX_CAPTURE_BYTES).contains(&self.max_json_bytes),
            "native capture limit outside 1..=256 MiB",
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainableBo3RequestV1 {
    pub schema: String,
    pub config: Bo3CollectionConfigV1,
    pub packages: [CompleteAgentPackageV1; 2],
    pub capture_limits: Bo3NativeCaptureLimitsV1,
}
impl TrainableBo3RequestV1 {
    pub fn from_json_v1(text: &str) -> Result<Self, String> {
        require(
            text.len() <= 4 * 1024 * 1024,
            "trainable request exceeds 4 MiB",
        )?;
        crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
        let request: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        request.validate()?;
        Ok(request)
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        require(
            self.schema == TRAINABLE_BO3_REQUEST_SCHEMA_V1,
            "trainable request schema differs",
        )?;
        self.capture_limits.validate()?;
        for package in &self.packages {
            require(
                matches!(package.sideboard, AgentSideboardPolicyV1::Keep),
                "initial trainable capture requires Keep sideboarding at both seats",
            )?;
        }
        crate::phase1_bo3_collection_v1::validate_configuration(
            &self.config,
            self.packages.each_ref(),
        )
    }
}

/// These are original scorer inputs, not a new observation contract. Float
/// tensors are stored as exact binary32 bits; integer index arrays stay i64.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3CapturedTensorBitsV1 {
    pub state: Vec<u32>,
    pub object_features: Vec<u32>,
    pub object_card_ids: Vec<i64>,
    pub object_groups: Vec<i64>,
    pub object_node_ids: Vec<i64>,
    pub edge_features: Vec<u32>,
    pub edge_source_indices: Vec<i64>,
    pub edge_target_indices: Vec<i64>,
    pub action_features: Vec<u32>,
    pub action_ref_features: Vec<u32>,
    pub action_ref_card_ids: Vec<i64>,
    pub action_ref_action_indices: Vec<i64>,
    pub action_ref_node_indices: Vec<i64>,
}
fn array_bytes(floats: &[usize], integers: &[usize]) -> Result<u64, String> {
    floats
        .iter()
        .map(|n| (*n, 4_u64))
        .chain(integers.iter().map(|n| (*n, 8)))
        .try_fold(0_u64, |sum, (n, width)| {
            (n as u64)
                .checked_mul(width)
                .and_then(|n| sum.checked_add(n))
                .ok_or_else(|| "native payload size overflow".into())
        })
}
impl Bo3CapturedTensorBitsV1 {
    pub(crate) fn from_tensor(t: &NativeFlatDecisionTensorV3) -> Self {
        let t = &t.common;
        let bits = |values: &[f32]| values.iter().map(|v| v.to_bits()).collect();
        Self {
            state: bits(&t.state),
            object_features: bits(&t.object_features),
            object_card_ids: t.object_card_ids.clone(),
            object_groups: t.object_groups.clone(),
            object_node_ids: t.object_node_ids.clone(),
            edge_features: bits(&t.edge_features),
            edge_source_indices: t.edge_source_indices.clone(),
            edge_target_indices: t.edge_target_indices.clone(),
            action_features: bits(&t.action_features),
            action_ref_features: bits(&t.action_ref_features),
            action_ref_card_ids: t.action_ref_card_ids.clone(),
            action_ref_action_indices: t.action_ref_action_indices.clone(),
            action_ref_node_indices: t.action_ref_node_indices.clone(),
        }
    }
    pub(crate) fn payload_bytes(&self) -> Result<u64, String> {
        array_bytes(
            &[
                self.state.len(),
                self.object_features.len(),
                self.edge_features.len(),
                self.action_features.len(),
                self.action_ref_features.len(),
            ],
            &[
                self.object_card_ids.len(),
                self.object_groups.len(),
                self.object_node_ids.len(),
                self.edge_source_indices.len(),
                self.edge_target_indices.len(),
                self.action_ref_card_ids.len(),
                self.action_ref_action_indices.len(),
                self.action_ref_node_indices.len(),
            ],
        )
    }
    pub(crate) fn into_tensor(self) -> NativeFlatDecisionTensorV3 {
        let floats = |bits: Vec<u32>| bits.into_iter().map(f32::from_bits).collect();
        NativeFlatDecisionTensorV3 {
            common: NativeFlatDecisionTensorV2 {
                state: floats(self.state),
                object_features: floats(self.object_features),
                object_card_ids: self.object_card_ids,
                object_groups: self.object_groups,
                object_node_ids: self.object_node_ids,
                edge_features: floats(self.edge_features),
                edge_source_indices: self.edge_source_indices,
                edge_target_indices: self.edge_target_indices,
                action_features: floats(self.action_features),
                action_ref_features: floats(self.action_ref_features),
                action_ref_card_ids: self.action_ref_card_ids,
                action_ref_action_indices: self.action_ref_action_indices,
                action_ref_node_indices: self.action_ref_node_indices,
            },
        }
    }
}
fn borrowed_payload_bytes(t: &NativeFlatDecisionTensorV3, logits: usize) -> Result<u64, String> {
    let t = &t.common;
    array_bytes(
        &[
            t.state.len(),
            t.object_features.len(),
            t.edge_features.len(),
            t.action_features.len(),
            t.action_ref_features.len(),
            logits,
            1,
        ],
        &[
            t.object_card_ids.len(),
            t.object_groups.len(),
            t.object_node_ids.len(),
            t.edge_source_indices.len(),
            t.edge_target_indices.len(),
            t.action_ref_card_ids.len(),
            t.action_ref_action_indices.len(),
            t.action_ref_node_indices.len(),
        ],
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3NativeDecisionCaptureV1 {
    pub decision_index: u64,
    pub tensor_bits: Bo3CapturedTensorBitsV1,
    pub raw_logit_bits: Vec<u32>,
    pub raw_value_bits: u32,
}
impl Bo3NativeDecisionCaptureV1 {
    pub(crate) fn payload_bytes(&self) -> Result<u64, String> {
        self.tensor_bits
            .payload_bytes()?
            .checked_add(array_bytes(&[self.raw_logit_bits.len(), 1], &[])?)
            .ok_or_else(|| "native record payload overflow".into())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3NativeCaptureV1 {
    pub schema: String,
    pub records: Vec<Bo3NativeDecisionCaptureV1>,
    pub committed_payload_bytes: u64,
    /// Sum of canonical record JSON sizes, excluding container punctuation.
    pub committed_json_bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct TrainableBo3ResultV1 {
    pub schema: String,
    pub request: TrainableBo3RequestV1,
    pub result: Bo3CollectionResultV1,
    pub native_capture: Bo3NativeCaptureV1,
}

pub(crate) struct PendingNativeCapture {
    record: Bo3NativeDecisionCaptureV1,
    payload: u64,
    json: u64,
}
pub(crate) struct CaptureBuffer {
    limits: Bo3NativeCaptureLimitsV1,
    result: Bo3NativeCaptureV1,
}
impl CaptureBuffer {
    pub(crate) fn new(limits: Bo3NativeCaptureLimitsV1) -> Result<Self, String> {
        limits.validate()?;
        Ok(Self {
            limits,
            result: Bo3NativeCaptureV1 {
                schema: BO3_NATIVE_CAPTURE_SCHEMA_V1.into(),
                records: Vec::new(),
                committed_payload_bytes: 0,
                committed_json_bytes: 0,
            },
        })
    }
    pub(crate) fn prepare(
        &self,
        decision_index: u64,
        tensor: &NativeFlatDecisionTensorV3,
        scores: &FrozenPlayDecisionScoresV1,
        action_count: usize,
    ) -> Result<PendingNativeCapture, String> {
        require(
            scores.value.is_finite() && scores.logits.iter().all(|value| value.is_finite()),
            "native capture requires finite original scorer outputs",
        )?;
        require(
            scores.logits.len() == action_count,
            "captured scorer/action width differs",
        )?;
        let payload = borrowed_payload_bytes(tensor, scores.logits.len())?;
        require(
            payload <= MAX_RECORD_PAYLOAD_BYTES
                && self.result.committed_payload_bytes.saturating_add(payload)
                    <= self.limits.max_payload_bytes,
            "native capture payload limit reached",
        )?;
        let record = Bo3NativeDecisionCaptureV1 {
            decision_index,
            tensor_bits: Bo3CapturedTensorBitsV1::from_tensor(tensor),
            raw_logit_bits: scores.logits.iter().map(|v| v.to_bits()).collect(),
            raw_value_bits: scores.value.to_bits(),
        };
        let json = super::canonical_size(
            &record,
            self.limits
                .max_json_bytes
                .saturating_sub(self.result.committed_json_bytes),
        )?;
        require(
            self.result.committed_json_bytes.saturating_add(json) <= self.limits.max_json_bytes,
            "native capture JSON limit reached",
        )?;
        Ok(PendingNativeCapture {
            record,
            payload,
            json,
        })
    }
    pub(crate) fn commit(&mut self, pending: PendingNativeCapture) {
        self.result.committed_payload_bytes += pending.payload;
        self.result.committed_json_bytes += pending.json;
        self.result.records.push(pending.record);
    }
    pub(crate) fn finish(self) -> Bo3NativeCaptureV1 {
        self.result
    }
}

/// Collect once through the existing selection/commit path. This is a new
/// result schema; no missing tensor/output fields are inferred for V1 files.
pub fn collect_trainable_bo3_v1(
    request: TrainableBo3RequestV1,
) -> Result<TrainableBo3ResultV1, String> {
    request.validate()?;
    for package in &request.packages {
        super::preparation::admitted_source(&package.gameplay)?;
    }
    let mut capture = CaptureBuffer::new(request.capture_limits.clone())?;
    let result = crate::phase1_bo3_collection_v1::collect_bo3_with_native_capture_v1(
        request.config.clone(),
        request.packages.clone(),
        &mut capture,
    )?;
    Ok(TrainableBo3ResultV1 {
        schema: TRAINABLE_BO3_RESULT_SCHEMA_V1.into(),
        request,
        result,
        native_capture: capture.finish(),
    })
}
