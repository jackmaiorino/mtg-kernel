use mtgo_blackbox_v1::{
    MtgoPlayerVisibleConfirmedDuelDecisionV1, MtgoPlayerVisibleDuelGesturePlanV1,
    MtgoPlayerVisibleGameplayPostconditionKindV1, MtgoRectPxV1,
};
use serde::{Deserialize, Serialize};

/// Private visible-only request for declaring the complete set of regions
/// expected to change before one gesture primitive. Canonical BGRA8 pixels
/// follow this header on the child process stdin pipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MtgoPlayerVisibleGameplayPostconditionRequestHeaderV1 {
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
    pub(super) player_visible_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    pub(super) gesture_plan: MtgoPlayerVisibleDuelGesturePlanV1,
    pub(super) primitive_index: u16,
    pub(super) gesture_evaluation_commitment_sha256: String,
    pub(super) gesture_profile_admission_commitment_sha256: String,
    pub(super) runtime_identity_commitment_sha256: String,
    pub(super) runtime_binary_sha256: String,
    pub(super) assets_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MtgoPlayerVisibleGameplayPostconditionRegionCandidateV1 {
    pub(super) region_id: String,
    pub(super) kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
    pub(super) rect_client_px: MtgoRectPxV1,
    pub(super) content_sha256: String,
    pub(super) confidence_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MtgoPlayerVisibleGameplayPostconditionRegionSetV1 {
    pub(super) schema_version: u32,
    pub(super) frame_id: u64,
    pub(super) frame_sequence: u64,
    pub(super) primitive_index: u16,
    pub(super) candidate_set_complete: bool,
    pub(super) regions: Vec<MtgoPlayerVisibleGameplayPostconditionRegionCandidateV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MtgoPlayerVisibleGameplayPostconditionProcessResponseV1 {
    pub(super) schema_version: u32,
    pub(super) request_commitment_sha256: String,
    pub(super) region_set: MtgoPlayerVisibleGameplayPostconditionRegionSetV1,
}
