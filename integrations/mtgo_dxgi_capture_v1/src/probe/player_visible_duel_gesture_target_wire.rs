use mtgo_blackbox_v1::{
    MtgoPlayerVisibleDuelDecisionInputV1, MtgoPlayerVisibleDuelGesturePlanV1,
    MtgoPlayerVisibleDuelGestureTargetRoleV1, MtgoRectPxV1,
};
use serde::{Deserialize, Serialize};

/// Visible-only source-frame request for one gesture primitive. Exact pixels
/// follow this header on a private pipe. Frame and integrity fields are local
/// transport bindings and none of these wire types is exported from the crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MtgoPlayerVisibleDuelGestureTargetRequestHeaderV1 {
    pub(super) schema_version: u32,
    pub(super) protocol: String,
    pub(super) frame_id: u64,
    pub(super) frame_sequence: u64,
    pub(super) canonical_width: u32,
    pub(super) canonical_height: u32,
    pub(super) canonical_stride: u32,
    pub(super) canonical_byte_length: usize,
    pub(super) canonical_bgra8_sha256: String,
    pub(super) source_capture_commitment_sha256: String,
    pub(super) perception_result_commitment_sha256: String,
    pub(super) decision_input: MtgoPlayerVisibleDuelDecisionInputV1,
    pub(super) gesture_plan: MtgoPlayerVisibleDuelGesturePlanV1,
    pub(super) primitive_index: u16,
    pub(super) gesture_evaluation_commitment_sha256: String,
    pub(super) gesture_profile_admission_commitment_sha256: String,
    pub(super) runtime_identity_commitment_sha256: String,
    pub(super) gesture_target_runtime_binary_sha256: String,
    pub(super) gesture_target_assets_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MtgoPlayerVisibleDuelGestureTargetCandidateV1 {
    pub(super) target_id: String,
    pub(super) role: MtgoPlayerVisibleDuelGestureTargetRoleV1,
    pub(super) rect_client_px: MtgoRectPxV1,
    pub(super) content_sha256: String,
    pub(super) confidence_bps: u16,
    pub(super) visibly_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MtgoPlayerVisibleDuelGestureTargetSetV1 {
    pub(super) schema_version: u32,
    pub(super) frame_id: u64,
    pub(super) frame_sequence: u64,
    pub(super) primitive_index: u16,
    pub(super) candidate_set_complete: bool,
    pub(super) targets: Vec<MtgoPlayerVisibleDuelGestureTargetCandidateV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MtgoPlayerVisibleDuelGestureTargetProcessResponseV1 {
    pub(super) schema_version: u32,
    pub(super) request_commitment_sha256: String,
    pub(super) target_set: MtgoPlayerVisibleDuelGestureTargetSetV1,
}
