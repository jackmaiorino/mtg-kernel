#[cfg(not(target_os = "windows"))]
compile_error!("mtgo_visible_competitive_classifier_v1 is Windows-only");

#[cfg(all(target_os = "windows", test))]
use mtgo_blackbox_v1::MtgoCompetitiveMatchRecordV1;
#[cfg(target_os = "windows")]
use mtgo_blackbox_v1::{
    validate_competitive_deck_manifest_v1, validate_observation_reconstruction_audit_v1,
    validate_observed_decision_v1, validate_visible_competitive_event_record_v1,
    validate_visible_competitive_lifecycle_snapshot_v1,
    validate_visible_competitive_sideboard_snapshot_v1, visible_frame_region_content_sha256_v1,
    MtgoCompetitiveDeckGateStateV1, MtgoCompetitiveDeckManifestV1, MtgoCompetitiveDeckPartitionV1,
    MtgoCompetitiveEntryTermsV1, MtgoCompetitiveEventCompletionV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveEventListingTargetV1, MtgoCompetitiveEventProgressV1,
    MtgoCompetitiveEventRecordVisibleFactKindV1, MtgoCompetitiveEventRecordVisibleFactV1,
    MtgoCompetitiveEventVisibleStatusV1, MtgoCompetitiveLifecyclePhaseV1, MtgoEvidenceSourceV1,
    MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1,
    MtgoObservationReconstructionAuditV1, MtgoObservedDecisionV1,
    MtgoPlayerVisibleConfirmedDuelDecisionV1, MtgoPlayerVisibleDuelDecisionInputV1,
    MtgoPlayerVisibleDuelGesturePlanV1, MtgoPlayerVisibleDuelGestureTargetRoleV1,
    MtgoPlayerVisibleGameplayPostconditionKindV1, MtgoRectPxV1, MtgoSizePxV1,
    MtgoVisibleActionControlSetV1, MtgoVisibleCompetitiveDeckGateV1,
    MtgoVisibleCompetitiveEventListingSelectionV1, MtgoVisibleCompetitiveEventRecordV1,
    MtgoVisibleCompetitiveLifecycleSnapshotV1, MtgoVisibleCompetitiveSideboardCardV1,
    MtgoVisibleCompetitiveSideboardSnapshotV1, MtgoVisibleCompetitiveSideboardZoneV1,
    MTGO_COMPETITIVE_DECK_GATE_SCHEMA_V1, MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
    MTGO_COMPETITIVE_EVENT_RECORD_SCHEMA_V1, MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
    MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
};
#[cfg(target_os = "windows")]
use mtgo_dxgi_capture_v1::{
    MtgoCompetitiveDeckChooserClassifierProcessResponseV1,
    MtgoCompetitiveDeckChooserClassifierRequestHeaderV1, MtgoCompetitiveDeckChooserStateV1,
    MtgoCompetitiveDeckGateClassifierProcessResponseV1,
    MtgoCompetitiveDeckGateClassifierRequestHeaderV1,
    MtgoCompetitiveEventListingClassifierProcessResponseV1,
    MtgoCompetitiveEventListingClassifierRequestHeaderV1,
    MtgoCompetitiveEventRecordClassifierProcessResponseV1,
    MtgoCompetitiveEventRecordClassifierRequestHeaderV1,
    MtgoCompetitiveNavigationClassifierProcessResponseV1,
    MtgoCompetitiveNavigationClassifierRequestHeaderV1,
    MtgoCompetitiveSideboardClassifierProcessResponseV1,
    MtgoCompetitiveSideboardClassifierRequestHeaderV1, MtgoDuelPerceptionProcessResponseV1,
    MtgoDuelPerceptionRequestHeaderV1, MtgoVisibleCompetitiveDeckChooserV1,
};
#[cfg(target_os = "windows")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use sha2::{Digest, Sha256};
#[cfg(target_os = "windows")]
use std::{
    collections::HashSet,
    fs::File,
    io::{self, Read, Write},
    path::Path,
    process::ExitCode,
};
#[cfg(target_os = "windows")]
use windows::{
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::DataWriter,
    Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
};

#[cfg(target_os = "windows")]
const EVENT_LISTING_MODE_ARGUMENT_V1: &str = "--mtgo-visible-competitive-event-listing-v1";
#[cfg(target_os = "windows")]
const DECK_GATE_MODE_ARGUMENT_V1: &str = "--mtgo-visible-competitive-deck-gate-v1";
#[cfg(target_os = "windows")]
const DECK_CHOOSER_MODE_ARGUMENT_V1: &str = "--mtgo-visible-competitive-deck-chooser-v1";
#[cfg(target_os = "windows")]
const NAVIGATION_MODE_ARGUMENT_V1: &str = "--mtgo-visible-competitive-navigation-v1";
#[cfg(target_os = "windows")]
const EVENT_RECORD_MODE_ARGUMENT_V1: &str = "--mtgo-visible-competitive-event-record-v1";
#[cfg(target_os = "windows")]
const SIDEBOARD_MODE_ARGUMENT_V1: &str = "--mtgo-visible-competitive-sideboard-v1";
#[cfg(target_os = "windows")]
const DUEL_PERCEPTION_MODE_ARGUMENT_V1: &str = "--mtgo-visible-duel-perception-v1";
#[cfg(target_os = "windows")]
const PLAYER_VISIBLE_DUEL_GESTURE_TARGET_MODE_ARGUMENT_V1: &str =
    "--mtgo-player-visible-duel-gesture-target-v1";
#[cfg(target_os = "windows")]
const PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_MODE_ARGUMENT_V1: &str =
    "--mtgo-player-visible-gameplay-postcondition-v1";
#[cfg(target_os = "windows")]
const EVENT_LISTING_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_EVENT_LISTING_V1\0";
#[cfg(target_os = "windows")]
const DECK_GATE_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_DECK_GATE_V1\0";
#[cfg(target_os = "windows")]
const DECK_CHOOSER_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_DECK_CHOOSER_V1\0";
#[cfg(target_os = "windows")]
const NAVIGATION_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_NAVIGATION_V1\0";
#[cfg(target_os = "windows")]
const EVENT_RECORD_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_EVENT_RECORD_V1\0";
#[cfg(target_os = "windows")]
const SIDEBOARD_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_COMPETITIVE_SIDEBOARD_V1\0";
#[cfg(target_os = "windows")]
const DUEL_PERCEPTION_PROTOCOL_MAGIC_V1: &[u8] = b"MTGO_VISIBLE_DUEL_PERCEPTION_V1\0";
#[cfg(target_os = "windows")]
const PLAYER_VISIBLE_DUEL_GESTURE_TARGET_PROTOCOL_MAGIC_V1: &[u8] =
    b"MTGO_PLAYER_VISIBLE_DUEL_GESTURE_TARGET_V1\0";
#[cfg(target_os = "windows")]
const PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_PROTOCOL_MAGIC_V1: &[u8] =
    b"MTGO_PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_V1\0";
#[cfg(target_os = "windows")]
const EVENT_LISTING_REQUEST_PROTOCOL_V1: &str = "mtgo_visible_competitive_event_listing_v1";
#[cfg(target_os = "windows")]
const DECK_GATE_REQUEST_PROTOCOL_V1: &str = "mtgo_visible_competitive_deck_gate_v1";
#[cfg(target_os = "windows")]
const DECK_CHOOSER_REQUEST_PROTOCOL_V1: &str = "mtgo_visible_competitive_deck_chooser_v1";
#[cfg(target_os = "windows")]
const DECK_GATE_REQUEST_SCOPE_V1: &str =
    "league_and_challenge_three_state_deck_gate_checked_untrusted_v1";
#[cfg(target_os = "windows")]
const DECK_CHOOSER_REQUEST_SCOPE_V1: &str =
    "league_and_challenge_exact_deck_chooser_checked_untrusted_v1";
#[cfg(target_os = "windows")]
const NAVIGATION_REQUEST_PROTOCOL_V1: &str = "mtgo_visible_competitive_navigation_v1";
#[cfg(target_os = "windows")]
const EVENT_RECORD_REQUEST_PROTOCOL_V1: &str = "mtgo_visible_competitive_event_record_v1";
#[cfg(target_os = "windows")]
const EVENT_RECORD_REQUEST_SCOPE_V1: &str =
    "league_and_challenge_eight_slice_event_record_checked_untrusted_v1";
#[cfg(target_os = "windows")]
const SIDEBOARD_REQUEST_PROTOCOL_V1: &str = "mtgo_visible_competitive_sideboard_v1";
#[cfg(target_os = "windows")]
const SIDEBOARD_REQUEST_SCOPE_V1: &str =
    "league_or_challenge_exact_deck_policy_between_game_sideboard_checked_untrusted_v1";
#[cfg(target_os = "windows")]
const DUEL_PERCEPTION_ASSET_SCOPE_V1: &str =
    "acting_player_duel_exact_visible_semantic_reference_checked_untrusted_v1";
#[cfg(target_os = "windows")]
const PLAYER_VISIBLE_DUEL_GESTURE_TARGET_ASSET_SCOPE_V1: &str =
    "player_visible_duel_exact_gesture_target_reference_checked_untrusted_v1";
#[cfg(target_os = "windows")]
const REQUEST_SCOPE_V1: &str =
    "league_and_challenge_selected_listing_exact_semantics_checked_untrusted_v1";
#[cfg(target_os = "windows")]
const ASSET_SCOPE_V1: &str = "league_and_challenge_selected_listing_windows_ocr_exact_control_v1";
#[cfg(target_os = "windows")]
const COMBINED_ASSET_SCOPE_V1: &str =
    "league_and_challenge_navigation_and_listing_exact_visible_regions_v1";
#[cfg(target_os = "windows")]
const PIXEL_FORMAT_V1: &str = "bgra8_unorm_top_down_tightly_packed_v1";
#[cfg(target_os = "windows")]
const REQUEST_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-listing-classifier-request-v1";
#[cfg(target_os = "windows")]
const DECK_GATE_REQUEST_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-gate-classifier-request-v1";
#[cfg(target_os = "windows")]
const DECK_CHOOSER_REQUEST_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-chooser-classifier-request-v1";
#[cfg(target_os = "windows")]
const NAVIGATION_REQUEST_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-navigation-classifier-request-v1";
#[cfg(target_os = "windows")]
const EVENT_RECORD_REQUEST_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-event-record-classifier-request-v1";
#[cfg(target_os = "windows")]
const SIDEBOARD_REQUEST_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-sideboard-classifier-request-v1";
#[cfg(target_os = "windows")]
const DUEL_PERCEPTION_REQUEST_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-duel-perception-request-v1";
#[cfg(target_os = "windows")]
const PLAYER_VISIBLE_DUEL_GESTURE_TARGET_REQUEST_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-duel-gesture-target-request-v1";
#[cfg(target_os = "windows")]
const PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_REQUEST_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-gameplay-postcondition-request-v1";
#[cfg(target_os = "windows")]
const DUEL_PERCEPTION_DYNAMIC_ID_DOMAIN_V1: &[u8] = b"mtgo-visible-duel-perception-dynamic-id-v1";
#[cfg(target_os = "windows")]
const NAVIGATION_SNAPSHOT_ID_DOMAIN_V1: &[u8] =
    b"mtgo-visible-competitive-navigation-snapshot-id-v1";
#[cfg(target_os = "windows")]
const TARGET_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-competitive-event-listing-target-v1";
#[cfg(target_os = "windows")]
const MAX_HEADER_BYTES_V1: usize = 1024 * 1024;
#[cfg(target_os = "windows")]
const MAX_ASSETS_BYTES_V1: usize = 16 * 1024 * 1024;
#[cfg(target_os = "windows")]
const MAX_CANONICAL_BYTES_V1: u64 = 512 * 1024 * 1024;
#[cfg(target_os = "windows")]
const MAX_RUNTIME_ARTIFACT_BYTES_V1: u64 = 512 * 1024 * 1024;
#[cfg(target_os = "windows")]
const MAX_PROFILES_V1: usize = 256;
#[cfg(target_os = "windows")]
const MAX_CONTROL_REFERENCES_V1: usize = 64;
#[cfg(target_os = "windows")]
const MAX_SIDEBOARD_CARD_PROFILES_V1: usize = 512;
#[cfg(target_os = "windows")]
const MAX_DUEL_PERCEPTION_PROFILES_V1: usize = 256;
#[cfg(target_os = "windows")]
const MAX_PLAYER_VISIBLE_DUEL_GESTURE_TARGET_PROFILES_V1: usize = 1024;
#[cfg(target_os = "windows")]
const MAX_PLAYER_VISIBLE_DUEL_GESTURE_TARGETS_V1: usize = 8;
#[cfg(target_os = "windows")]
const MAX_PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_REGIONS_V1: usize = 16;
#[cfg(target_os = "windows")]
const MAX_EXPECTED_LABEL_BYTES_V1: usize = 256;

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveEventListingClassifierAssetsV1 {
    schema_version: u32,
    scope: String,
    canonical_pixel_format: String,
    profiles: Vec<MtgoCompetitiveEventListingOcrProfileV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    deck_gate_profiles: Vec<MtgoCompetitiveDeckGateProfileV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    deck_chooser_profiles: Vec<MtgoCompetitiveDeckChooserProfileV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    navigation_profiles: Vec<MtgoCompetitiveNavigationRegionProfileV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    event_record_profiles: Vec<MtgoCompetitiveEventRecordRegionProfileV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sideboard_profiles: Vec<MtgoCompetitiveSideboardRegionProfileV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveNavigationRegionProfileV1 {
    profile_id: String,
    event_kind: MtgoCompetitiveEventKindV1,
    phase: MtgoCompetitiveLifecyclePhaseV1,
    client_size_px: MtgoSizePxV1,
    event_identity_sha256: Option<String>,
    match_identity_sha256: Option<String>,
    game_number: Option<u8>,
    entry_terms: Option<MtgoCompetitiveEntryTermsV1>,
    facts: Vec<MtgoCompetitiveNavigationFactProfileV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveNavigationFactProfileV1 {
    kind: MtgoLifecycleVisibleFactKindV1,
    rect_client_px: MtgoRectPxV1,
    accepted_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveEventRecordRegionProfileV1 {
    profile_id: String,
    navigation_profile_id: String,
    event_kind: MtgoCompetitiveEventKindV1,
    lifecycle_phase: MtgoCompetitiveLifecyclePhaseV1,
    event_identity_sha256: String,
    status: MtgoCompetitiveEventVisibleStatusV1,
    progress: MtgoCompetitiveEventProgressV1,
    completion: Option<MtgoCompetitiveEventCompletionV1>,
    facts: Vec<MtgoCompetitiveEventRecordFactProfileV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveEventRecordFactProfileV1 {
    kind: MtgoCompetitiveEventRecordVisibleFactKindV1,
    rect_client_px: MtgoRectPxV1,
    accepted_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveSideboardRegionProfileV1 {
    profile_id: String,
    navigation_profile_id: String,
    deck_manifest: MtgoCompetitiveDeckManifestV1,
    mainboard_zone: MtgoCompetitiveSideboardZoneProfileV1,
    sideboard_zone: MtgoCompetitiveSideboardZoneProfileV1,
    cards: Vec<MtgoCompetitiveSideboardCardProfileV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveSideboardZoneProfileV1 {
    rect_client_px: MtgoRectPxV1,
    accepted_reference_sha256s: Vec<String>,
    empty_drop_rect_client_px: MtgoRectPxV1,
    accepted_empty_drop_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveSideboardCardProfileV1 {
    partition: MtgoCompetitiveDeckPartitionV1,
    card_name: String,
    count: u16,
    rect_client_px: MtgoRectPxV1,
    accepted_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoDuelPerceptionClassifierAssetsV1 {
    schema_version: u32,
    scope: String,
    canonical_pixel_format: String,
    profiles: Vec<MtgoDuelPerceptionReferenceProfileV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoDuelPerceptionReferenceProfileV1 {
    profile_id: String,
    perception_profile_commitment_sha256: String,
    perception_profile_admission_commitment_sha256: String,
    client_size_px: MtgoSizePxV1,
    decision_template: MtgoObservedDecisionV1,
    reconstruction_audit_template: MtgoObservationReconstructionAuditV1,
    visible_controls_template: MtgoVisibleActionControlSetV1,
    competitive_lifecycle_template: Option<MtgoVisibleCompetitiveLifecycleSnapshotV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleDuelGestureTargetClassifierAssetsV1 {
    schema_version: u32,
    scope: String,
    canonical_pixel_format: String,
    profiles: Vec<MtgoPlayerVisibleDuelGestureTargetReferenceProfileV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    gameplay_postcondition_profiles: Vec<MtgoPlayerVisibleGameplayPostconditionReferenceProfileV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleDuelGestureTargetReferenceProfileV1 {
    profile_id: String,
    gesture_evaluation_commitment_sha256: String,
    gesture_profile_admission_commitment_sha256: String,
    client_size_px: MtgoSizePxV1,
    decision_input: MtgoPlayerVisibleDuelDecisionInputV1,
    gesture_plan: MtgoPlayerVisibleDuelGesturePlanV1,
    primitive_index: u16,
    targets: Vec<MtgoPlayerVisibleDuelGestureTargetReferenceV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleDuelGestureTargetReferenceV1 {
    target_id: String,
    role: MtgoPlayerVisibleDuelGestureTargetRoleV1,
    rect_client_px: MtgoRectPxV1,
    accepted_reference_sha256s: Vec<String>,
    confidence_bps: u16,
    visibly_enabled: bool,
}

/// Private wire copy. The library deliberately does not export this transport
/// type, so the separately compiled classifier duplicates only the sanitized
/// player-visible schema.
#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleDuelGestureTargetRequestHeaderWireV1 {
    schema_version: u32,
    protocol: String,
    frame_id: u64,
    frame_sequence: u64,
    canonical_width: u32,
    canonical_height: u32,
    canonical_stride: u32,
    canonical_byte_length: usize,
    canonical_bgra8_sha256: String,
    source_capture_commitment_sha256: String,
    perception_result_commitment_sha256: String,
    decision_input: MtgoPlayerVisibleDuelDecisionInputV1,
    gesture_plan: MtgoPlayerVisibleDuelGesturePlanV1,
    primitive_index: u16,
    gesture_evaluation_commitment_sha256: String,
    gesture_profile_admission_commitment_sha256: String,
    runtime_identity_commitment_sha256: String,
    gesture_target_runtime_binary_sha256: String,
    gesture_target_assets_manifest_sha256: String,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleDuelGestureTargetCandidateWireV1 {
    target_id: String,
    role: MtgoPlayerVisibleDuelGestureTargetRoleV1,
    rect_client_px: MtgoRectPxV1,
    content_sha256: String,
    confidence_bps: u16,
    visibly_enabled: bool,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleDuelGestureTargetSetWireV1 {
    schema_version: u32,
    frame_id: u64,
    frame_sequence: u64,
    primitive_index: u16,
    candidate_set_complete: bool,
    targets: Vec<MtgoPlayerVisibleDuelGestureTargetCandidateWireV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleDuelGestureTargetProcessResponseWireV1 {
    schema_version: u32,
    request_commitment_sha256: String,
    target_set: MtgoPlayerVisibleDuelGestureTargetSetWireV1,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleGameplayPostconditionReferenceProfileV1 {
    profile_id: String,
    gesture_evaluation_commitment_sha256: String,
    gesture_profile_admission_commitment_sha256: String,
    client_size_px: MtgoSizePxV1,
    player_visible_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    gesture_plan: MtgoPlayerVisibleDuelGesturePlanV1,
    primitive_index: u16,
    regions: Vec<MtgoPlayerVisibleGameplayPostconditionReferenceRegionV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleGameplayPostconditionReferenceRegionV1 {
    region_id: String,
    kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
    rect_client_px: MtgoRectPxV1,
    accepted_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleGameplayPostconditionRequestHeaderWireV1 {
    schema_version: u32,
    protocol: String,
    frame_id: u64,
    frame_sequence: u64,
    canonical_width: u32,
    canonical_height: u32,
    canonical_stride: u32,
    canonical_byte_length: usize,
    canonical_bgra8_sha256: String,
    source_capture_commitment_sha256: String,
    perception_result_commitment_sha256: String,
    player_visible_decision: MtgoPlayerVisibleConfirmedDuelDecisionV1,
    gesture_plan: MtgoPlayerVisibleDuelGesturePlanV1,
    primitive_index: u16,
    gesture_evaluation_commitment_sha256: String,
    gesture_profile_admission_commitment_sha256: String,
    runtime_identity_commitment_sha256: String,
    runtime_binary_sha256: String,
    assets_manifest_sha256: String,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleGameplayPostconditionRegionCandidateWireV1 {
    region_id: String,
    kind: MtgoPlayerVisibleGameplayPostconditionKindV1,
    rect_client_px: MtgoRectPxV1,
    content_sha256: String,
    confidence_bps: u16,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleGameplayPostconditionRegionSetWireV1 {
    schema_version: u32,
    frame_id: u64,
    frame_sequence: u64,
    primitive_index: u16,
    candidate_set_complete: bool,
    regions: Vec<MtgoPlayerVisibleGameplayPostconditionRegionCandidateWireV1>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoPlayerVisibleGameplayPostconditionProcessResponseWireV1 {
    schema_version: u32,
    request_commitment_sha256: String,
    region_set: MtgoPlayerVisibleGameplayPostconditionRegionSetWireV1,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveEventListingOcrProfileV1 {
    profile_id: String,
    event_kind: MtgoCompetitiveEventKindV1,
    event_display_label_sha256: String,
    expected_visible_label: String,
    client_size_px: MtgoSizePxV1,
    label_search_rect_client_px: MtgoRectPxV1,
    open_entry_review_control_rect_client_px: MtgoRectPxV1,
    enabled_control_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveDeckGateProfileV1 {
    profile_id: String,
    event_kind: MtgoCompetitiveEventKindV1,
    event_display_label_sha256: String,
    expected_visible_event_label: String,
    deck_display_label_sha256: String,
    expected_visible_deck_label: String,
    state: MtgoCompetitiveDeckGateStateV1,
    client_size_px: MtgoSizePxV1,
    event_label_search_rect_client_px: MtgoRectPxV1,
    deck_status_search_rect_client_px: MtgoRectPxV1,
    select_deck_control_rect_client_px: MtgoRectPxV1,
    select_deck_control_reference_sha256s: Vec<String>,
    open_entry_review_status_rect_client_px: MtgoRectPxV1,
    open_entry_review_unavailable_reference_sha256s: Vec<String>,
    open_entry_review_enabled_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MtgoCompetitiveDeckChooserProfileV1 {
    profile_id: String,
    event_kind: MtgoCompetitiveEventKindV1,
    event_display_label_sha256: String,
    deck_display_label_sha256: String,
    expected_visible_deck_label: String,
    chooser_title_label_sha256: String,
    expected_visible_chooser_title: String,
    state: MtgoCompetitiveDeckChooserStateV1,
    client_size_px: MtgoSizePxV1,
    chooser_title_search_rect_client_px: MtgoRectPxV1,
    deck_label_search_rect_client_px: MtgoRectPxV1,
    deck_row_control_rect_client_px: MtgoRectPxV1,
    unselected_deck_row_reference_sha256s: Vec<String>,
    selected_deck_row_reference_sha256s: Vec<String>,
    selection_detail_rect_client_px: MtgoRectPxV1,
    unselected_selection_detail_reference_sha256s: Vec<String>,
    selected_selection_detail_reference_sha256s: Vec<String>,
    submit_control_rect_client_px: MtgoRectPxV1,
    disabled_submit_reference_sha256s: Vec<String>,
    enabled_submit_reference_sha256s: Vec<String>,
    confidence_bps: u16,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct OcrWordV1 {
    normalized: String,
    rect: MtgoRectPxV1,
}

#[cfg(target_os = "windows")]
struct ComGuardV1;

#[cfg(target_os = "windows")]
impl ComGuardV1 {
    fn initialize_v1() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|error| format!("initialize Windows Runtime apartment: {error}"))?;
        Ok(Self)
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComGuardV1 {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[cfg(target_os = "windows")]
fn main() -> ExitCode {
    match run_v1() {
        Ok(bytes) => match io::stdout().write_all(&bytes) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("MTGO_VISIBLE_COMPETITIVE_CLASSIFIER_REJECTED:write response: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("MTGO_VISIBLE_COMPETITIVE_CLASSIFIER_REJECTED:{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "windows")]
fn run_v1() -> Result<Vec<u8>, String> {
    let args = std::env::args().collect::<Vec<_>>();
    let current_exe = std::env::current_exe()
        .map_err(|error| format!("resolve classifier executable: {error}"))?;
    let actual_classifier_sha256 = hash_bounded_file_v1(
        &current_exe,
        MAX_RUNTIME_ARTIFACT_BYTES_V1,
        "classifier executable",
    )?;
    match args.get(1).map(String::as_str) {
        Some(EVENT_LISTING_MODE_ARGUMENT_V1) if args.len() == 2 => {
            run_event_listing_v1(&actual_classifier_sha256)
        }
        Some(DECK_GATE_MODE_ARGUMENT_V1) if args.len() == 2 => {
            run_deck_gate_v1(&actual_classifier_sha256)
        }
        Some(DECK_CHOOSER_MODE_ARGUMENT_V1) if args.len() == 2 => {
            run_deck_chooser_v1(&actual_classifier_sha256)
        }
        Some(NAVIGATION_MODE_ARGUMENT_V1) if args.len() == 2 => {
            run_navigation_v1(&actual_classifier_sha256)
        }
        Some(EVENT_RECORD_MODE_ARGUMENT_V1) if args.len() == 2 => {
            run_event_record_v1(&actual_classifier_sha256)
        }
        Some(SIDEBOARD_MODE_ARGUMENT_V1) if args.len() == 2 => {
            run_sideboard_v1(&actual_classifier_sha256)
        }
        Some(DUEL_PERCEPTION_MODE_ARGUMENT_V1)
            if args.len() == 6
                && args[2] == "--classifier-assets-manifest"
                && args[4] == "--card-database-profile" =>
        {
            run_duel_perception_v1(
                &actual_classifier_sha256,
                Path::new(&args[3]),
                Path::new(&args[5]),
            )
        }
        Some(DUEL_PERCEPTION_MODE_ARGUMENT_V1) => {
            Err("duel perception requires exact pinned artifact arguments".to_owned())
        }
        Some(PLAYER_VISIBLE_DUEL_GESTURE_TARGET_MODE_ARGUMENT_V1)
            if args.len() == 4 && args[2] == "--gesture-target-assets-manifest" =>
        {
            run_player_visible_duel_gesture_target_v1(
                &actual_classifier_sha256,
                Path::new(&args[3]),
            )
        }
        Some(PLAYER_VISIBLE_DUEL_GESTURE_TARGET_MODE_ARGUMENT_V1) => Err(
            "player-visible gesture target requires the exact pinned assets argument".to_owned(),
        ),
        Some(PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_MODE_ARGUMENT_V1)
            if args.len() == 4 && args[2] == "--gesture-target-assets-manifest" =>
        {
            run_player_visible_gameplay_postcondition_v1(
                &actual_classifier_sha256,
                Path::new(&args[3]),
            )
        }
        Some(PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_MODE_ARGUMENT_V1) => Err(
            "player-visible gameplay postcondition requires the exact pinned assets argument"
                .to_owned(),
        ),
        _ => Err("exactly one supported bounded classifier mode is required".to_owned()),
    }
}

#[cfg(target_os = "windows")]
fn run_event_listing_v1(actual_classifier_sha256: &str) -> Result<Vec<u8>, String> {
    let mut stdin = io::stdin().lock();
    let mut magic = vec![0_u8; EVENT_LISTING_PROTOCOL_MAGIC_V1.len()];
    stdin
        .read_exact(&mut magic)
        .map_err(|error| format!("read protocol magic: {error}"))?;
    if magic != EVENT_LISTING_PROTOCOL_MAGIC_V1 {
        return Err("selected-listing protocol magic differs".to_owned());
    }
    let header_length = read_u64_be_v1(&mut stdin, "header length")?;
    let assets_length = read_u64_be_v1(&mut stdin, "assets length")?;
    let header_length = bounded_usize_v1(header_length, MAX_HEADER_BYTES_V1, "header")?;
    let assets_length = bounded_usize_v1(assets_length, MAX_ASSETS_BYTES_V1, "assets")?;
    let header_json = read_exact_vec_v1(&mut stdin, header_length, "header")?;
    let assets_json = read_exact_vec_v1(&mut stdin, assets_length, "assets")?;
    let header = parse_canonical_json_v1::<MtgoCompetitiveEventListingClassifierRequestHeaderV1>(
        &header_json,
        "request header",
    )?;
    validate_header_identity_v1(&header, &assets_json, actual_classifier_sha256)?;
    let pixel_length = bounded_usize_v1(
        header.canonical_byte_length,
        usize::try_from(MAX_CANONICAL_BYTES_V1)
            .map_err(|_| "classifier byte bound does not fit this process".to_owned())?,
        "canonical pixels",
    )?;
    let canonical_bgra8 = read_exact_vec_v1(&mut stdin, pixel_length, "canonical pixels")?;
    let mut trailing = [0_u8; 1];
    if stdin
        .read(&mut trailing)
        .map_err(|error| format!("check request end: {error}"))?
        != 0
    {
        return Err("selected-listing request has trailing bytes".to_owned());
    }
    validate_pixels_v1(&header, &canonical_bgra8)?;
    let assets = parse_canonical_json_v1::<MtgoCompetitiveEventListingClassifierAssetsV1>(
        &assets_json,
        "classifier assets",
    )?;
    let profile = validate_assets_and_select_profile_v1(&assets, &header)?;
    let request_commitment_sha256 = commitment_v1(
        REQUEST_COMMITMENT_DOMAIN_V1,
        &[&header_json, &assets_json, &canonical_bgra8],
    );
    let words = recognize_words_v1(
        header.canonical_width,
        header.canonical_height,
        &canonical_bgra8,
    )?;
    let response = classify_from_words_v1(
        &header,
        profile,
        &canonical_bgra8,
        &words,
        request_commitment_sha256,
    )?;
    serde_json::to_vec(&response)
        .map_err(|error| format!("serialize selected-listing response: {error}"))
}

#[cfg(target_os = "windows")]
fn run_deck_gate_v1(actual_classifier_sha256: &str) -> Result<Vec<u8>, String> {
    let mut stdin = io::stdin().lock();
    let mut magic = vec![0_u8; DECK_GATE_PROTOCOL_MAGIC_V1.len()];
    stdin
        .read_exact(&mut magic)
        .map_err(|error| format!("read deck-gate protocol magic: {error}"))?;
    if magic != DECK_GATE_PROTOCOL_MAGIC_V1 {
        return Err("deck-gate protocol magic differs".to_owned());
    }
    let header_length = read_u64_be_v1(&mut stdin, "deck-gate header length")?;
    let assets_length = read_u64_be_v1(&mut stdin, "deck-gate assets length")?;
    let header_length = bounded_usize_v1(header_length, MAX_HEADER_BYTES_V1, "deck-gate header")?;
    let assets_length = bounded_usize_v1(assets_length, MAX_ASSETS_BYTES_V1, "deck-gate assets")?;
    let header_json = read_exact_vec_v1(&mut stdin, header_length, "deck-gate header")?;
    let assets_json = read_exact_vec_v1(&mut stdin, assets_length, "deck-gate assets")?;
    let header = parse_canonical_json_v1::<MtgoCompetitiveDeckGateClassifierRequestHeaderV1>(
        &header_json,
        "deck-gate request header",
    )?;
    validate_deck_gate_header_identity_v1(&header, &assets_json, actual_classifier_sha256)?;
    let pixel_length = bounded_usize_v1(
        header.canonical_byte_length,
        usize::try_from(MAX_CANONICAL_BYTES_V1)
            .map_err(|_| "classifier byte bound does not fit this process".to_owned())?,
        "deck-gate canonical pixels",
    )?;
    let canonical_bgra8 = read_exact_vec_v1(&mut stdin, pixel_length, "deck-gate pixels")?;
    let mut trailing = [0_u8; 1];
    if stdin
        .read(&mut trailing)
        .map_err(|error| format!("check deck-gate request end: {error}"))?
        != 0
    {
        return Err("deck-gate request has trailing bytes".to_owned());
    }
    validate_deck_gate_pixels_v1(&header, &canonical_bgra8)?;
    let assets = parse_canonical_json_v1::<MtgoCompetitiveEventListingClassifierAssetsV1>(
        &assets_json,
        "deck-gate classifier assets",
    )?;
    let profiles = validate_deck_gate_assets_v1(&assets, &header)?;
    let request_commitment_sha256 = commitment_v1(
        DECK_GATE_REQUEST_COMMITMENT_DOMAIN_V1,
        &[&header_json, &assets_json, &canonical_bgra8],
    );
    let words = recognize_words_v1(
        header.canonical_width,
        header.canonical_height,
        &canonical_bgra8,
    )?;
    let mut matches = profiles
        .into_iter()
        .filter_map(|profile| {
            classify_deck_gate_from_words_v1(
                &header,
                profile,
                &canonical_bgra8,
                &words,
                request_commitment_sha256.clone(),
            )
            .ok()
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "expected exactly one reviewed deck-gate state match, found {}",
            matches.len()
        ));
    }
    let response = matches.remove(0);
    serde_json::to_vec(&response).map_err(|error| format!("serialize deck-gate response: {error}"))
}

#[cfg(target_os = "windows")]
fn run_deck_chooser_v1(actual_classifier_sha256: &str) -> Result<Vec<u8>, String> {
    let mut stdin = io::stdin().lock();
    let mut magic = vec![0_u8; DECK_CHOOSER_PROTOCOL_MAGIC_V1.len()];
    stdin
        .read_exact(&mut magic)
        .map_err(|error| format!("read deck-chooser protocol magic: {error}"))?;
    if magic != DECK_CHOOSER_PROTOCOL_MAGIC_V1 {
        return Err("deck-chooser protocol magic differs".to_owned());
    }
    let header_length = read_u64_be_v1(&mut stdin, "deck-chooser header length")?;
    let assets_length = read_u64_be_v1(&mut stdin, "deck-chooser assets length")?;
    let header_length =
        bounded_usize_v1(header_length, MAX_HEADER_BYTES_V1, "deck-chooser header")?;
    let assets_length =
        bounded_usize_v1(assets_length, MAX_ASSETS_BYTES_V1, "deck-chooser assets")?;
    let header_json = read_exact_vec_v1(&mut stdin, header_length, "deck-chooser header")?;
    let assets_json = read_exact_vec_v1(&mut stdin, assets_length, "deck-chooser assets")?;
    let header = parse_canonical_json_v1::<MtgoCompetitiveDeckChooserClassifierRequestHeaderV1>(
        &header_json,
        "deck-chooser request header",
    )?;
    validate_deck_chooser_header_identity_v1(&header, &assets_json, actual_classifier_sha256)?;
    let pixel_length = bounded_usize_v1(
        header.canonical_byte_length,
        usize::try_from(MAX_CANONICAL_BYTES_V1)
            .map_err(|_| "classifier byte bound does not fit this process".to_owned())?,
        "deck-chooser canonical pixels",
    )?;
    let canonical_bgra8 = read_exact_vec_v1(&mut stdin, pixel_length, "deck-chooser pixels")?;
    let mut trailing = [0_u8; 1];
    if stdin
        .read(&mut trailing)
        .map_err(|error| format!("check deck-chooser request end: {error}"))?
        != 0
    {
        return Err("deck-chooser request has trailing bytes".to_owned());
    }
    validate_deck_chooser_pixels_v1(&header, &canonical_bgra8)?;
    let assets = parse_canonical_json_v1::<MtgoCompetitiveEventListingClassifierAssetsV1>(
        &assets_json,
        "deck-chooser classifier assets",
    )?;
    let profiles = validate_deck_chooser_assets_v1(&assets, &header)?;
    let request_commitment_sha256 = commitment_v1(
        DECK_CHOOSER_REQUEST_COMMITMENT_DOMAIN_V1,
        &[&header_json, &assets_json, &canonical_bgra8],
    );
    let words = recognize_words_v1(
        header.canonical_width,
        header.canonical_height,
        &canonical_bgra8,
    )?;
    let mut matches = profiles
        .into_iter()
        .filter_map(|profile| {
            classify_deck_chooser_from_words_v1(
                &header,
                profile,
                &canonical_bgra8,
                &words,
                request_commitment_sha256.clone(),
            )
            .ok()
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "expected exactly one reviewed deck-chooser state match, found {}",
            matches.len()
        ));
    }
    serde_json::to_vec(&matches.remove(0))
        .map_err(|error| format!("serialize deck-chooser response: {error}"))
}

#[cfg(target_os = "windows")]
fn run_navigation_v1(actual_classifier_sha256: &str) -> Result<Vec<u8>, String> {
    let mut stdin = io::stdin().lock();
    let mut magic = vec![0_u8; NAVIGATION_PROTOCOL_MAGIC_V1.len()];
    stdin
        .read_exact(&mut magic)
        .map_err(|error| format!("read navigation protocol magic: {error}"))?;
    if magic != NAVIGATION_PROTOCOL_MAGIC_V1 {
        return Err("navigation protocol magic differs".to_owned());
    }
    let header_length = read_u64_be_v1(&mut stdin, "navigation header length")?;
    let assets_length = read_u64_be_v1(&mut stdin, "navigation assets length")?;
    let header_length = bounded_usize_v1(header_length, MAX_HEADER_BYTES_V1, "navigation header")?;
    let assets_length = bounded_usize_v1(assets_length, MAX_ASSETS_BYTES_V1, "navigation assets")?;
    let header_json = read_exact_vec_v1(&mut stdin, header_length, "navigation header")?;
    let assets_json = read_exact_vec_v1(&mut stdin, assets_length, "navigation assets")?;
    let header = parse_canonical_json_v1::<MtgoCompetitiveNavigationClassifierRequestHeaderV1>(
        &header_json,
        "navigation request header",
    )?;
    validate_navigation_header_identity_v1(&header, &assets_json, actual_classifier_sha256)?;
    let pixel_length = bounded_usize_v1(
        header.canonical_byte_length,
        usize::try_from(MAX_CANONICAL_BYTES_V1)
            .map_err(|_| "classifier byte bound does not fit this process".to_owned())?,
        "navigation canonical pixels",
    )?;
    let canonical_bgra8 =
        read_exact_vec_v1(&mut stdin, pixel_length, "navigation canonical pixels")?;
    let mut trailing = [0_u8; 1];
    if stdin
        .read(&mut trailing)
        .map_err(|error| format!("check navigation request end: {error}"))?
        != 0
    {
        return Err("navigation request has trailing bytes".to_owned());
    }
    validate_navigation_pixels_v1(&header, &canonical_bgra8)?;
    let assets = parse_canonical_json_v1::<MtgoCompetitiveEventListingClassifierAssetsV1>(
        &assets_json,
        "combined classifier assets",
    )?;
    let profile =
        validate_navigation_assets_and_match_profile_v1(&assets, &header, &canonical_bgra8)?;
    let request_commitment_sha256 = commitment_v1(
        NAVIGATION_REQUEST_COMMITMENT_DOMAIN_V1,
        &[&header_json, &assets_json, &canonical_bgra8],
    );
    let response = classify_navigation_profile_v1(
        &header,
        profile,
        &canonical_bgra8,
        request_commitment_sha256,
    )?;
    serde_json::to_vec(&response).map_err(|error| format!("serialize navigation response: {error}"))
}

#[cfg(target_os = "windows")]
fn run_event_record_v1(actual_classifier_sha256: &str) -> Result<Vec<u8>, String> {
    let mut stdin = io::stdin().lock();
    let mut magic = vec![0_u8; EVENT_RECORD_PROTOCOL_MAGIC_V1.len()];
    stdin
        .read_exact(&mut magic)
        .map_err(|error| format!("read event-record protocol magic: {error}"))?;
    if magic != EVENT_RECORD_PROTOCOL_MAGIC_V1 {
        return Err("event-record protocol magic differs".to_owned());
    }
    let header_length = read_u64_be_v1(&mut stdin, "event-record header length")?;
    let assets_length = read_u64_be_v1(&mut stdin, "event-record assets length")?;
    let header_length =
        bounded_usize_v1(header_length, MAX_HEADER_BYTES_V1, "event-record header")?;
    let assets_length =
        bounded_usize_v1(assets_length, MAX_ASSETS_BYTES_V1, "event-record assets")?;
    let header_json = read_exact_vec_v1(&mut stdin, header_length, "event-record header")?;
    let assets_json = read_exact_vec_v1(&mut stdin, assets_length, "event-record assets")?;
    let header = parse_canonical_json_v1::<MtgoCompetitiveEventRecordClassifierRequestHeaderV1>(
        &header_json,
        "event-record request header",
    )?;
    validate_event_record_header_identity_v1(&header, &assets_json, actual_classifier_sha256)?;
    let pixel_length = bounded_usize_v1(
        header.canonical_byte_length,
        usize::try_from(MAX_CANONICAL_BYTES_V1)
            .map_err(|_| "classifier byte bound does not fit this process".to_owned())?,
        "event-record canonical pixels",
    )?;
    let canonical_bgra8 =
        read_exact_vec_v1(&mut stdin, pixel_length, "event-record canonical pixels")?;
    let mut trailing = [0_u8; 1];
    if stdin
        .read(&mut trailing)
        .map_err(|error| format!("check event-record request end: {error}"))?
        != 0
    {
        return Err("event-record request has trailing bytes".to_owned());
    }
    validate_event_record_pixels_v1(&header, &canonical_bgra8)?;
    let assets = parse_canonical_json_v1::<MtgoCompetitiveEventListingClassifierAssetsV1>(
        &assets_json,
        "combined classifier assets",
    )?;
    let navigation_profile = validate_navigation_assets_and_match_profile_for_frame_v1(
        &assets,
        header.canonical_width,
        header.canonical_height,
        &canonical_bgra8,
    )?;
    let event_record_profile = validate_event_record_assets_and_match_profile_v1(
        &assets,
        navigation_profile,
        &header.approved_account_alias_sha256,
        &canonical_bgra8,
    )?;
    let request_commitment_sha256 = commitment_be_v1(
        EVENT_RECORD_REQUEST_COMMITMENT_DOMAIN_V1,
        &[&header_json, &assets_json, &canonical_bgra8],
    );
    let response = classify_event_record_profile_v1(
        &header,
        navigation_profile,
        event_record_profile,
        &canonical_bgra8,
        request_commitment_sha256,
    )?;
    serde_json::to_vec(&response)
        .map_err(|error| format!("serialize event-record response: {error}"))
}

#[cfg(target_os = "windows")]
fn classify_event_record_profile_v1(
    header: &MtgoCompetitiveEventRecordClassifierRequestHeaderV1,
    navigation_profile: &MtgoCompetitiveNavigationRegionProfileV1,
    event_record_profile: &MtgoCompetitiveEventRecordRegionProfileV1,
    canonical_bgra8: &[u8],
    request_commitment_sha256: String,
) -> Result<MtgoCompetitiveEventRecordClassifierProcessResponseV1, String> {
    let lifecycle = build_navigation_lifecycle_from_frame_v1(
        header.frame_id,
        header.frame_sequence,
        header.canonical_width,
        header.canonical_height,
        &header.canonical_bgra8_sha256,
        navigation_profile,
        canonical_bgra8,
    )?;
    let checked_lifecycle =
        validate_visible_competitive_lifecycle_snapshot_v1(lifecycle.clone())
            .map_err(|error| format!("validate event-record source lifecycle: {error}"))?;
    let size = navigation_profile.client_size_px.clone();
    let facts = event_record_profile
        .facts
        .iter()
        .map(|fact| {
            let content_sha256 = visible_frame_region_content_sha256_v1(
                canonical_bgra8,
                &size,
                &fact.rect_client_px,
            )
            .map_err(|error| format!("hash event-record fact pixels: {error}"))?;
            if fact
                .accepted_reference_sha256s
                .binary_search(&content_sha256)
                .is_err()
            {
                return Err("event-record fact changed after profile selection".to_owned());
            }
            Ok(MtgoCompetitiveEventRecordVisibleFactV1 {
                kind: fact.kind,
                rect_client_px: fact.rect_client_px.clone(),
                content_sha256,
                confidence_bps: fact.confidence_bps,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let record = MtgoVisibleCompetitiveEventRecordV1 {
        schema_version: MTGO_COMPETITIVE_EVENT_RECORD_SCHEMA_V1,
        record_id: format!(
            "visible-region-event-record-v1-{}",
            &request_commitment_sha256[..24]
        ),
        source_lifecycle_snapshot_commitment_sha256: checked_lifecycle
            .snapshot_commitment_sha256()
            .to_owned(),
        approved_account_alias_sha256: header.approved_account_alias_sha256.clone(),
        event_kind: event_record_profile.event_kind,
        lifecycle_phase: event_record_profile.lifecycle_phase,
        frame_id: header.frame_id,
        frame_sequence: header.frame_sequence,
        frame_sha256: header.canonical_bgra8_sha256.clone(),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: header.canonical_width,
            height: header.canonical_height,
        },
        event_identity_sha256: event_record_profile.event_identity_sha256.clone(),
        status: event_record_profile.status,
        progress: event_record_profile.progress.clone(),
        completion: event_record_profile.completion,
        visible_record_complete: true,
        facts,
    };
    validate_visible_competitive_event_record_v1(
        &checked_lifecycle,
        &header.approved_account_alias_sha256,
        record.clone(),
    )
    .map_err(|error| format!("validate classified event record: {error}"))?;
    Ok(MtgoCompetitiveEventRecordClassifierProcessResponseV1 {
        schema_version: 1,
        request_commitment_sha256,
        lifecycle,
        record,
    })
}

#[cfg(target_os = "windows")]
fn run_sideboard_v1(actual_classifier_sha256: &str) -> Result<Vec<u8>, String> {
    let mut stdin = io::stdin().lock();
    let mut magic = vec![0_u8; SIDEBOARD_PROTOCOL_MAGIC_V1.len()];
    stdin
        .read_exact(&mut magic)
        .map_err(|error| format!("read sideboard protocol magic: {error}"))?;
    if magic != SIDEBOARD_PROTOCOL_MAGIC_V1 {
        return Err("sideboard protocol magic differs".to_owned());
    }
    let header_length = read_u64_be_v1(&mut stdin, "sideboard header length")?;
    let assets_length = read_u64_be_v1(&mut stdin, "sideboard assets length")?;
    let header_length = bounded_usize_v1(header_length, MAX_HEADER_BYTES_V1, "sideboard header")?;
    let assets_length = bounded_usize_v1(assets_length, MAX_ASSETS_BYTES_V1, "sideboard assets")?;
    let header_json = read_exact_vec_v1(&mut stdin, header_length, "sideboard header")?;
    let assets_json = read_exact_vec_v1(&mut stdin, assets_length, "sideboard assets")?;
    let header = parse_canonical_json_v1::<MtgoCompetitiveSideboardClassifierRequestHeaderV1>(
        &header_json,
        "sideboard request header",
    )?;
    validate_sideboard_header_identity_v1(&header, &assets_json, actual_classifier_sha256)?;
    let pixel_length = bounded_usize_v1(
        header.canonical_byte_length,
        usize::try_from(MAX_CANONICAL_BYTES_V1)
            .map_err(|_| "classifier byte bound does not fit this process".to_owned())?,
        "sideboard canonical pixels",
    )?;
    let canonical_bgra8 =
        read_exact_vec_v1(&mut stdin, pixel_length, "sideboard canonical pixels")?;
    let mut trailing = [0_u8; 1];
    if stdin
        .read(&mut trailing)
        .map_err(|error| format!("check sideboard request end: {error}"))?
        != 0
    {
        return Err("sideboard request has trailing bytes".to_owned());
    }
    validate_sideboard_pixels_v1(&header, &canonical_bgra8)?;
    let assets = parse_canonical_json_v1::<MtgoCompetitiveEventListingClassifierAssetsV1>(
        &assets_json,
        "combined classifier assets",
    )?;
    let navigation_profile = validate_navigation_assets_and_match_profile_for_frame_v1(
        &assets,
        header.canonical_width,
        header.canonical_height,
        &canonical_bgra8,
    )?;
    let sideboard_profile = validate_sideboard_assets_and_match_profile_v1(
        &assets,
        navigation_profile,
        &header,
        &canonical_bgra8,
    )?;
    let request_commitment_sha256 = commitment_be_v1(
        SIDEBOARD_REQUEST_COMMITMENT_DOMAIN_V1,
        &[
            &header_json,
            &assets_json,
            &canonical_bgra8,
            b"checked_untrusted_sideboard_request_no_live_classification_no_input",
        ],
    );
    let response = classify_sideboard_profile_v1(
        &header,
        navigation_profile,
        sideboard_profile,
        &canonical_bgra8,
        request_commitment_sha256,
    )?;
    serde_json::to_vec(&response).map_err(|error| format!("serialize sideboard response: {error}"))
}

#[cfg(target_os = "windows")]
fn run_duel_perception_v1(
    actual_classifier_sha256: &str,
    classifier_assets_manifest_path: &Path,
    card_database_profile_path: &Path,
) -> Result<Vec<u8>, String> {
    let mut stdin = io::stdin().lock();
    let mut magic = vec![0_u8; DUEL_PERCEPTION_PROTOCOL_MAGIC_V1.len()];
    stdin
        .read_exact(&mut magic)
        .map_err(|error| format!("read duel perception protocol magic: {error}"))?;
    if magic != DUEL_PERCEPTION_PROTOCOL_MAGIC_V1 {
        return Err("duel perception protocol magic differs".to_owned());
    }
    let header_length = read_u64_be_v1(&mut stdin, "duel perception header length")?;
    let header_length =
        bounded_usize_v1(header_length, MAX_HEADER_BYTES_V1, "duel perception header")?;
    let header_json = read_exact_vec_v1(&mut stdin, header_length, "duel perception header")?;
    let header = parse_canonical_json_v1::<MtgoDuelPerceptionRequestHeaderV1>(
        &header_json,
        "duel perception request header",
    )?;
    let assets_json = read_bounded_file_bytes_v1(
        classifier_assets_manifest_path,
        MAX_ASSETS_BYTES_V1 as u64,
        "duel perception classifier assets manifest",
    )?;
    let card_database_sha256 = hash_bounded_file_v1(
        card_database_profile_path,
        MAX_RUNTIME_ARTIFACT_BYTES_V1,
        "duel perception card database profile",
    )?;
    validate_duel_perception_header_identity_v1(
        &header,
        &assets_json,
        &card_database_sha256,
        actual_classifier_sha256,
    )?;
    let pixel_length = bounded_usize_v1(
        u64::try_from(header.canonical_byte_length)
            .map_err(|_| "duel perception byte length does not fit u64".to_owned())?,
        usize::try_from(MAX_CANONICAL_BYTES_V1)
            .map_err(|_| "classifier byte bound does not fit this process".to_owned())?,
        "duel perception canonical pixels",
    )?;
    let canonical_bgra8 =
        read_exact_vec_v1(&mut stdin, pixel_length, "duel perception canonical pixels")?;
    let mut trailing = [0_u8; 1];
    if stdin
        .read(&mut trailing)
        .map_err(|error| format!("check duel perception request end: {error}"))?
        != 0
    {
        return Err("duel perception request has trailing bytes".to_owned());
    }
    validate_duel_perception_pixels_v1(&header, &canonical_bgra8)?;
    let assets = parse_canonical_json_v1::<MtgoDuelPerceptionClassifierAssetsV1>(
        &assets_json,
        "duel perception classifier assets manifest",
    )?;
    let request_commitment_sha256 = commitment_be_v1(
        DUEL_PERCEPTION_REQUEST_COMMITMENT_DOMAIN_V1,
        &[&header_json, &canonical_bgra8],
    );
    let response = classify_duel_perception_profile_v1(
        &header,
        &assets,
        &canonical_bgra8,
        request_commitment_sha256,
    )?;
    serde_json::to_vec(&response)
        .map_err(|error| format!("serialize duel perception response: {error}"))
}

#[cfg(target_os = "windows")]
fn run_player_visible_duel_gesture_target_v1(
    actual_classifier_sha256: &str,
    gesture_target_assets_manifest_path: &Path,
) -> Result<Vec<u8>, String> {
    let mut stdin = io::stdin().lock();
    let mut magic = vec![0_u8; PLAYER_VISIBLE_DUEL_GESTURE_TARGET_PROTOCOL_MAGIC_V1.len()];
    stdin
        .read_exact(&mut magic)
        .map_err(|error| format!("read player-visible gesture-target protocol magic: {error}"))?;
    if magic != PLAYER_VISIBLE_DUEL_GESTURE_TARGET_PROTOCOL_MAGIC_V1 {
        return Err("player-visible gesture-target protocol magic differs".to_owned());
    }
    let header_length = read_u64_be_v1(&mut stdin, "player-visible gesture-target header length")?;
    let header_length = bounded_usize_v1(
        header_length,
        MAX_HEADER_BYTES_V1,
        "player-visible gesture-target header",
    )?;
    let header_json = read_exact_vec_v1(
        &mut stdin,
        header_length,
        "player-visible gesture-target header",
    )?;
    let header = parse_canonical_json_v1::<MtgoPlayerVisibleDuelGestureTargetRequestHeaderWireV1>(
        &header_json,
        "player-visible gesture-target request header",
    )?;
    let assets_json = read_bounded_file_bytes_v1(
        gesture_target_assets_manifest_path,
        MAX_ASSETS_BYTES_V1 as u64,
        "player-visible gesture-target assets manifest",
    )?;
    validate_player_visible_duel_gesture_target_header_identity_v1(
        &header,
        &assets_json,
        actual_classifier_sha256,
    )?;
    let pixel_length = bounded_usize_v1(
        u64::try_from(header.canonical_byte_length)
            .map_err(|_| "player-visible gesture-target byte length does not fit u64".to_owned())?,
        usize::try_from(MAX_CANONICAL_BYTES_V1)
            .map_err(|_| "classifier byte bound does not fit this process".to_owned())?,
        "player-visible gesture-target canonical pixels",
    )?;
    let canonical_bgra8 = read_exact_vec_v1(
        &mut stdin,
        pixel_length,
        "player-visible gesture-target canonical pixels",
    )?;
    let mut trailing = [0_u8; 1];
    if stdin
        .read(&mut trailing)
        .map_err(|error| format!("check player-visible gesture-target request end: {error}"))?
        != 0
    {
        return Err("player-visible gesture-target request has trailing bytes".to_owned());
    }
    validate_player_visible_duel_gesture_target_pixels_v1(&header, &canonical_bgra8)?;
    let assets = parse_canonical_json_v1::<MtgoPlayerVisibleDuelGestureTargetClassifierAssetsV1>(
        &assets_json,
        "player-visible gesture-target assets manifest",
    )?;
    let profile = validate_player_visible_duel_gesture_target_assets_and_match_profile_v1(
        &assets,
        &header,
        &canonical_bgra8,
    )?;
    let request_commitment_sha256 = commitment_v1(
        PLAYER_VISIBLE_DUEL_GESTURE_TARGET_REQUEST_COMMITMENT_DOMAIN_V1,
        &[&header_json, &canonical_bgra8],
    );
    let response = build_player_visible_duel_gesture_target_response_v1(
        &header,
        profile,
        &canonical_bgra8,
        request_commitment_sha256,
    )?;
    serde_json::to_vec(&response)
        .map_err(|error| format!("serialize player-visible gesture-target response: {error}"))
}

#[cfg(target_os = "windows")]
fn run_player_visible_gameplay_postcondition_v1(
    actual_classifier_sha256: &str,
    assets_manifest_path: &Path,
) -> Result<Vec<u8>, String> {
    let mut stdin = io::stdin().lock();
    let mut magic = vec![0_u8; PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_PROTOCOL_MAGIC_V1.len()];
    stdin
        .read_exact(&mut magic)
        .map_err(|error| format!("read player-visible gameplay postcondition magic: {error}"))?;
    if magic != PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_PROTOCOL_MAGIC_V1 {
        return Err("player-visible gameplay postcondition protocol magic differs".to_owned());
    }
    let header_length = read_u64_be_v1(
        &mut stdin,
        "player-visible gameplay postcondition header length",
    )?;
    let header_length = bounded_usize_v1(
        header_length,
        MAX_HEADER_BYTES_V1,
        "player-visible gameplay postcondition header",
    )?;
    let header_json = read_exact_vec_v1(
        &mut stdin,
        header_length,
        "player-visible gameplay postcondition header",
    )?;
    let header =
        parse_canonical_json_v1::<MtgoPlayerVisibleGameplayPostconditionRequestHeaderWireV1>(
            &header_json,
            "player-visible gameplay postcondition request header",
        )?;
    let assets_json = read_bounded_file_bytes_v1(
        assets_manifest_path,
        MAX_ASSETS_BYTES_V1 as u64,
        "player-visible gameplay postcondition assets manifest",
    )?;
    validate_player_visible_gameplay_postcondition_header_identity_v1(
        &header,
        &assets_json,
        actual_classifier_sha256,
    )?;
    let pixel_length = bounded_usize_v1(
        u64::try_from(header.canonical_byte_length).map_err(|_| {
            "player-visible gameplay postcondition byte length does not fit u64".to_owned()
        })?,
        usize::try_from(MAX_CANONICAL_BYTES_V1)
            .map_err(|_| "classifier byte bound does not fit this process".to_owned())?,
        "player-visible gameplay postcondition canonical pixels",
    )?;
    let canonical_bgra8 = read_exact_vec_v1(
        &mut stdin,
        pixel_length,
        "player-visible gameplay postcondition canonical pixels",
    )?;
    let mut trailing = [0_u8; 1];
    if stdin.read(&mut trailing).map_err(|error| {
        format!("check player-visible gameplay postcondition request end: {error}")
    })? != 0
    {
        return Err("player-visible gameplay postcondition request has trailing bytes".to_owned());
    }
    validate_player_visible_gameplay_postcondition_pixels_v1(&header, &canonical_bgra8)?;
    let assets = parse_canonical_json_v1::<MtgoPlayerVisibleDuelGestureTargetClassifierAssetsV1>(
        &assets_json,
        "player-visible gameplay postcondition assets manifest",
    )?;
    let profile = validate_player_visible_gameplay_postcondition_assets_and_match_profile_v1(
        &assets,
        &header,
        &canonical_bgra8,
    )?;
    let request_commitment_sha256 = commitment_v1(
        PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_REQUEST_COMMITMENT_DOMAIN_V1,
        &[&header_json, &canonical_bgra8],
    );
    let response = build_player_visible_gameplay_postcondition_response_v1(
        &header,
        profile,
        &canonical_bgra8,
        request_commitment_sha256,
    )?;
    serde_json::to_vec(&response).map_err(|error| {
        format!("serialize player-visible gameplay postcondition response: {error}")
    })
}

#[cfg(target_os = "windows")]
fn validate_player_visible_gameplay_postcondition_header_identity_v1(
    header: &MtgoPlayerVisibleGameplayPostconditionRequestHeaderWireV1,
    assets_json: &[u8],
    actual_classifier_sha256: &str,
) -> Result<(), String> {
    if header.schema_version != 1
        || header.protocol != "mtgo_player_visible_gameplay_postcondition_v1"
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
        || header.primitive_index > 31
    {
        return Err("player-visible gameplay postcondition request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("player-visible gameplay postcondition stride overflow")?;
    let expected_length = usize::try_from(header.canonical_width)
        .ok()
        .and_then(|width| {
            usize::try_from(header.canonical_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("player-visible gameplay postcondition pixel length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || expected_length == 0
        || u64::try_from(expected_length)
            .ok()
            .is_none_or(|length| length > MAX_CANONICAL_BYTES_V1)
        || header.assets_manifest_sha256 != sha256_hex_v1(assets_json)
        || header.runtime_binary_sha256 != actual_classifier_sha256
    {
        return Err(
            "player-visible gameplay postcondition runtime, geometry, or artifacts differ"
                .to_owned(),
        );
    }
    for digest in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.perception_result_commitment_sha256.as_str(),
        header.gesture_evaluation_commitment_sha256.as_str(),
        header.gesture_profile_admission_commitment_sha256.as_str(),
        header.runtime_identity_commitment_sha256.as_str(),
        header.runtime_binary_sha256.as_str(),
        header.assets_manifest_sha256.as_str(),
    ] {
        validate_sha256_v1(
            digest,
            "player-visible gameplay postcondition request commitment",
        )?;
    }
    let gesture =
        mtgo_blackbox_v1::validate_player_visible_duel_gesture_plan_v1(header.gesture_plan.clone())
            .map_err(|error| {
                format!("validate player-visible gameplay postcondition plan: {error}")
            })?;
    if usize::from(header.primitive_index) >= gesture.primitives_v1().len()
        || gesture.selected_action_v1() != &header.player_visible_decision.selected_action
    {
        return Err("player-visible gameplay postcondition action or primitive differs".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_player_visible_gameplay_postcondition_pixels_v1(
    header: &MtgoPlayerVisibleGameplayPostconditionRequestHeaderWireV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if canonical_bgra8.len() != header.canonical_byte_length
        || sha256_hex_v1(canonical_bgra8) != header.canonical_bgra8_sha256
    {
        return Err(
            "player-visible gameplay postcondition pixels differ from the request header"
                .to_owned(),
        );
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_player_visible_duel_gesture_target_header_identity_v1(
    header: &MtgoPlayerVisibleDuelGestureTargetRequestHeaderWireV1,
    assets_json: &[u8],
    actual_classifier_sha256: &str,
) -> Result<(), String> {
    if header.schema_version != 1
        || header.protocol != "mtgo_player_visible_duel_gesture_target_v1"
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
        || header.primitive_index > 31
    {
        return Err("player-visible gesture-target request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("player-visible gesture-target stride overflow")?;
    let expected_length = usize::try_from(header.canonical_width)
        .ok()
        .and_then(|width| {
            usize::try_from(header.canonical_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("player-visible gesture-target pixel length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || expected_length == 0
        || u64::try_from(expected_length)
            .ok()
            .is_none_or(|length| length > MAX_CANONICAL_BYTES_V1)
        || header.gesture_target_assets_manifest_sha256 != sha256_hex_v1(assets_json)
        || header.gesture_target_runtime_binary_sha256 != actual_classifier_sha256
    {
        return Err(
            "player-visible gesture-target runtime, geometry, or artifacts differ".to_owned(),
        );
    }
    for digest in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.perception_result_commitment_sha256.as_str(),
        header.gesture_evaluation_commitment_sha256.as_str(),
        header.gesture_profile_admission_commitment_sha256.as_str(),
        header.runtime_identity_commitment_sha256.as_str(),
        header.gesture_target_runtime_binary_sha256.as_str(),
        header.gesture_target_assets_manifest_sha256.as_str(),
    ] {
        validate_sha256_v1(digest, "player-visible gesture-target request commitment")?;
    }
    let gesture =
        mtgo_blackbox_v1::validate_player_visible_duel_gesture_plan_v1(header.gesture_plan.clone())
            .map_err(|error| format!("validate player-visible gesture-target plan: {error}"))?;
    if usize::from(header.primitive_index) >= gesture.primitives_v1().len()
        || !header
            .decision_input
            .ordered_legal_actions
            .contains(gesture.selected_action_v1())
    {
        return Err("player-visible gesture-target action or primitive is absent".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_player_visible_duel_gesture_target_pixels_v1(
    header: &MtgoPlayerVisibleDuelGestureTargetRequestHeaderWireV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if canonical_bgra8.len() != header.canonical_byte_length
        || sha256_hex_v1(canonical_bgra8) != header.canonical_bgra8_sha256
    {
        return Err(
            "player-visible gesture-target pixels differ from the request header".to_owned(),
        );
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_player_visible_duel_gesture_target_assets_and_match_profile_v1<'a>(
    assets: &'a MtgoPlayerVisibleDuelGestureTargetClassifierAssetsV1,
    header: &MtgoPlayerVisibleDuelGestureTargetRequestHeaderWireV1,
    canonical_bgra8: &[u8],
) -> Result<&'a MtgoPlayerVisibleDuelGestureTargetReferenceProfileV1, String> {
    if assets.schema_version != 1
        || assets.scope != PLAYER_VISIBLE_DUEL_GESTURE_TARGET_ASSET_SCOPE_V1
        || assets.canonical_pixel_format != PIXEL_FORMAT_V1
        || assets.profiles.is_empty()
        || assets.profiles.len() > MAX_PLAYER_VISIBLE_DUEL_GESTURE_TARGET_PROFILES_V1
    {
        return Err(
            "player-visible gesture-target assets are outside the bounded schema".to_owned(),
        );
    }
    let bounds = MtgoRectPxV1 {
        x: 0,
        y: 0,
        width: header.canonical_width,
        height: header.canonical_height,
    };
    let mut profile_ids = HashSet::new();
    let mut matches = Vec::new();
    for profile in &assets.profiles {
        validate_identifier_v1(
            &profile.profile_id,
            "player-visible gesture-target profile id",
        )?;
        if !profile_ids.insert(profile.profile_id.as_str()) {
            return Err("player-visible gesture-target profile ids must be unique".to_owned());
        }
        validate_sha256_v1(
            &profile.gesture_evaluation_commitment_sha256,
            "player-visible gesture evaluation commitment",
        )?;
        validate_sha256_v1(
            &profile.gesture_profile_admission_commitment_sha256,
            "player-visible gesture profile admission commitment",
        )?;
        let checked = mtgo_blackbox_v1::validate_player_visible_duel_gesture_plan_v1(
            profile.gesture_plan.clone(),
        )
        .map_err(|error| format!("validate player-visible gesture-target profile plan: {error}"))?;
        let primitive = checked
            .primitives_v1()
            .get(usize::from(profile.primitive_index))
            .ok_or("player-visible gesture-target profile primitive is absent")?;
        if !profile
            .decision_input
            .ordered_legal_actions
            .contains(checked.selected_action_v1())
        {
            return Err("player-visible gesture-target profile action is absent".to_owned());
        }
        let required_roles =
            mtgo_blackbox_v1::required_player_visible_duel_gesture_target_roles_v1(primitive);
        if profile.client_size_px.width == 0
            || profile.client_size_px.height == 0
            || profile.client_size_px.width > 16_384
            || profile.client_size_px.height > 16_384
            || profile.targets.len() != required_roles.len()
            || profile.targets.len() > MAX_PLAYER_VISIBLE_DUEL_GESTURE_TARGETS_V1
        {
            return Err(
                "player-visible gesture-target profile geometry or target set is invalid"
                    .to_owned(),
            );
        }
        let profile_bounds = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: profile.client_size_px.width,
            height: profile.client_size_px.height,
        };
        let mut target_ids = HashSet::new();
        let mut target_rects = HashSet::new();
        let mut target_roles = HashSet::new();
        let mut pixels_match = true;
        for target in &profile.targets {
            validate_identifier_v1(&target.target_id, "player-visible gesture target id")?;
            if !target_ids.insert(target.target_id.as_str())
                || !target_rects.insert((
                    target.rect_client_px.x,
                    target.rect_client_px.y,
                    target.rect_client_px.width,
                    target.rect_client_px.height,
                ))
                || !target_roles.insert(target.role.clone())
                || !rect_inside_v1(&target.rect_client_px, &profile_bounds)
                || !target.visibly_enabled
                || !(mtgo_blackbox_v1::MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1..=10_000)
                    .contains(&target.confidence_bps)
                || target.accepted_reference_sha256s.is_empty()
                || target.accepted_reference_sha256s.len() > 32
            {
                return Err(
                    "player-visible gesture-target reference is invalid or duplicated".to_owned(),
                );
            }
            let mut reference_hashes = HashSet::new();
            for accepted in &target.accepted_reference_sha256s {
                validate_sha256_v1(accepted, "player-visible gesture-target reference hash")?;
                if !reference_hashes.insert(accepted.as_str()) {
                    return Err(
                        "player-visible gesture-target reference hashes must be unique".to_owned(),
                    );
                }
            }
            if profile.client_size_px.width == header.canonical_width
                && profile.client_size_px.height == header.canonical_height
            {
                let actual = visible_frame_region_content_sha256_v1(
                    canonical_bgra8,
                    &profile.client_size_px,
                    &target.rect_client_px,
                )
                .map_err(|error| format!("hash player-visible gesture target: {error}"))?;
                pixels_match &= target.accepted_reference_sha256s.contains(&actual);
            }
        }
        if target_roles.len() != required_roles.len()
            || !required_roles
                .iter()
                .all(|role| target_roles.contains(role))
            || (profile.targets.len() == 2
                && rects_overlap_v1(
                    &profile.targets[0].rect_client_px,
                    &profile.targets[1].rect_client_px,
                ))
        {
            return Err("player-visible gesture-target roles or rectangles are invalid".to_owned());
        }
        if profile.client_size_px.width == bounds.width
            && profile.client_size_px.height == bounds.height
            && profile.gesture_evaluation_commitment_sha256
                == header.gesture_evaluation_commitment_sha256
            && profile.gesture_profile_admission_commitment_sha256
                == header.gesture_profile_admission_commitment_sha256
            && profile.decision_input == header.decision_input
            && profile.gesture_plan == header.gesture_plan
            && profile.primitive_index == header.primitive_index
            && pixels_match
        {
            matches.push(profile);
        }
    }
    if matches.len() != 1 {
        return Err(
            "expected exactly one reviewed player-visible gesture-target profile match".to_owned(),
        );
    }
    Ok(matches[0])
}

#[cfg(target_os = "windows")]
fn build_player_visible_duel_gesture_target_response_v1(
    header: &MtgoPlayerVisibleDuelGestureTargetRequestHeaderWireV1,
    profile: &MtgoPlayerVisibleDuelGestureTargetReferenceProfileV1,
    canonical_bgra8: &[u8],
    request_commitment_sha256: String,
) -> Result<MtgoPlayerVisibleDuelGestureTargetProcessResponseWireV1, String> {
    let targets = profile
        .targets
        .iter()
        .map(|target| {
            let content_sha256 = visible_frame_region_content_sha256_v1(
                canonical_bgra8,
                &profile.client_size_px,
                &target.rect_client_px,
            )
            .map_err(|error| format!("hash selected player-visible gesture target: {error}"))?;
            Ok(MtgoPlayerVisibleDuelGestureTargetCandidateWireV1 {
                target_id: target.target_id.clone(),
                role: target.role.clone(),
                rect_client_px: target.rect_client_px.clone(),
                content_sha256,
                confidence_bps: target.confidence_bps,
                visibly_enabled: target.visibly_enabled,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(MtgoPlayerVisibleDuelGestureTargetProcessResponseWireV1 {
        schema_version: 1,
        request_commitment_sha256,
        target_set: MtgoPlayerVisibleDuelGestureTargetSetWireV1 {
            schema_version: 1,
            frame_id: header.frame_id,
            frame_sequence: header.frame_sequence,
            primitive_index: header.primitive_index,
            candidate_set_complete: true,
            targets,
        },
    })
}

#[cfg(target_os = "windows")]
fn validate_player_visible_gameplay_postcondition_assets_and_match_profile_v1<'a>(
    assets: &'a MtgoPlayerVisibleDuelGestureTargetClassifierAssetsV1,
    header: &MtgoPlayerVisibleGameplayPostconditionRequestHeaderWireV1,
    canonical_bgra8: &[u8],
) -> Result<&'a MtgoPlayerVisibleGameplayPostconditionReferenceProfileV1, String> {
    if assets.schema_version != 1
        || assets.scope != PLAYER_VISIBLE_DUEL_GESTURE_TARGET_ASSET_SCOPE_V1
        || assets.canonical_pixel_format != PIXEL_FORMAT_V1
        || assets.gameplay_postcondition_profiles.is_empty()
        || assets.gameplay_postcondition_profiles.len()
            > MAX_PLAYER_VISIBLE_DUEL_GESTURE_TARGET_PROFILES_V1
    {
        return Err(
            "player-visible gameplay postcondition assets are outside the bounded schema"
                .to_owned(),
        );
    }
    let mut profile_ids = HashSet::new();
    let mut matches = Vec::new();
    for profile in &assets.gameplay_postcondition_profiles {
        let profile_bounds = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: profile.client_size_px.width,
            height: profile.client_size_px.height,
        };
        validate_identifier_v1(
            &profile.profile_id,
            "player-visible gameplay postcondition profile id",
        )?;
        if !profile_ids.insert(profile.profile_id.as_str()) {
            return Err(
                "player-visible gameplay postcondition profile ids must be unique".to_owned(),
            );
        }
        validate_sha256_v1(
            &profile.gesture_evaluation_commitment_sha256,
            "player-visible gameplay postcondition gesture evaluation",
        )?;
        validate_sha256_v1(
            &profile.gesture_profile_admission_commitment_sha256,
            "player-visible gameplay postcondition gesture admission",
        )?;
        let checked = mtgo_blackbox_v1::validate_player_visible_duel_gesture_plan_v1(
            profile.gesture_plan.clone(),
        )
        .map_err(|error| {
            format!("validate player-visible gameplay postcondition profile plan: {error}")
        })?;
        if checked
            .primitives_v1()
            .get(usize::from(profile.primitive_index))
            .is_none()
            || checked.selected_action_v1() != &profile.player_visible_decision.selected_action
            || profile.client_size_px.width == 0
            || profile.client_size_px.height == 0
            || profile.regions.is_empty()
            || profile.regions.len() > MAX_PLAYER_VISIBLE_GAMEPLAY_POSTCONDITION_REGIONS_V1
        {
            return Err(
                "player-visible gameplay postcondition profile is structurally invalid".to_owned(),
            );
        }
        let mut region_ids = HashSet::new();
        let mut kinds = HashSet::new();
        let mut rects = Vec::new();
        let mut pixels_match = true;
        for region in &profile.regions {
            validate_identifier_v1(
                &region.region_id,
                "player-visible gameplay postcondition region id",
            )?;
            let rect_key = (
                region.rect_client_px.x,
                region.rect_client_px.y,
                region.rect_client_px.width,
                region.rect_client_px.height,
            );
            if !region_ids.insert(region.region_id.as_str())
                || !kinds.insert(region.kind)
                || !rect_inside_v1(&region.rect_client_px, &profile_bounds)
                || rects
                    .iter()
                    .any(|existing| rect_keys_intersect_v1(*existing, rect_key))
                || !(mtgo_blackbox_v1::MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1..=10_000)
                    .contains(&region.confidence_bps)
                || region.accepted_reference_sha256s.is_empty()
                || region.accepted_reference_sha256s.len() > 32
            {
                return Err(
                    "player-visible gameplay postcondition region is invalid or duplicated"
                        .to_owned(),
                );
            }
            rects.push(rect_key);
            let mut hashes = HashSet::new();
            for accepted in &region.accepted_reference_sha256s {
                validate_sha256_v1(
                    accepted,
                    "player-visible gameplay postcondition reference hash",
                )?;
                if !hashes.insert(accepted.as_str()) {
                    return Err(
                        "player-visible gameplay postcondition reference hashes must be unique"
                            .to_owned(),
                    );
                }
            }
            if profile.client_size_px.width == header.canonical_width
                && profile.client_size_px.height == header.canonical_height
            {
                let actual = visible_frame_region_content_sha256_v1(
                    canonical_bgra8,
                    &profile.client_size_px,
                    &region.rect_client_px,
                )
                .map_err(|error| {
                    format!("hash player-visible gameplay postcondition region: {error}")
                })?;
                pixels_match &= region.accepted_reference_sha256s.contains(&actual);
            }
        }
        let shape_record = mtgo_blackbox_v1::MtgoPlayerVisibleGameplayBeforeInputRecordV1 {
            schema_version: mtgo_blackbox_v1::MTGO_PLAYER_VISIBLE_GAMEPLAY_BEFORE_INPUT_SCHEMA_V1,
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_identity_sha256: "1".repeat(64),
            match_identity_sha256: "2".repeat(64),
            game_number: 1,
            deployment_commitment_sha256: "3".repeat(64),
            decision_commitment_sha256: "4".repeat(64),
            selection_commitment_sha256: "5".repeat(64),
            source_frame_id: 1,
            source_frame_sequence: 1,
            source_frame_sha256: "6".repeat(64),
            client_size_px: profile.client_size_px.clone(),
            player_visible_decision: profile.player_visible_decision.clone(),
            gesture_plan: profile.gesture_plan.clone(),
            primitive_index: profile.primitive_index,
            primitive_is_final: usize::from(profile.primitive_index) + 1
                == checked.primitives_v1().len(),
            region_set_complete: true,
            regions: profile
                .regions
                .iter()
                .map(
                    |region| mtgo_blackbox_v1::MtgoPlayerVisibleGameplayBeforeRegionV1 {
                        kind: region.kind,
                        rect_client_px: region.rect_client_px.clone(),
                        before_bgra8_sha256: region.accepted_reference_sha256s[0].clone(),
                    },
                )
                .collect(),
            expected_game_log_baseline_commitment_sha256: None,
        };
        mtgo_blackbox_v1::check_untrusted_player_visible_gameplay_before_input_v1(shape_record)
            .map_err(|error| {
                format!("validate player-visible gameplay postcondition profile shape: {error}")
            })?;
        if profile.client_size_px.width == header.canonical_width
            && profile.client_size_px.height == header.canonical_height
            && profile.gesture_evaluation_commitment_sha256
                == header.gesture_evaluation_commitment_sha256
            && profile.gesture_profile_admission_commitment_sha256
                == header.gesture_profile_admission_commitment_sha256
            && profile.player_visible_decision == header.player_visible_decision
            && profile.gesture_plan == header.gesture_plan
            && profile.primitive_index == header.primitive_index
            && pixels_match
        {
            matches.push(profile);
        }
    }
    if matches.len() != 1 {
        return Err(
            "expected exactly one reviewed player-visible gameplay postcondition profile match"
                .to_owned(),
        );
    }
    Ok(matches[0])
}

#[cfg(target_os = "windows")]
fn build_player_visible_gameplay_postcondition_response_v1(
    header: &MtgoPlayerVisibleGameplayPostconditionRequestHeaderWireV1,
    profile: &MtgoPlayerVisibleGameplayPostconditionReferenceProfileV1,
    canonical_bgra8: &[u8],
    request_commitment_sha256: String,
) -> Result<MtgoPlayerVisibleGameplayPostconditionProcessResponseWireV1, String> {
    let regions = profile
        .regions
        .iter()
        .map(|region| {
            let content_sha256 = visible_frame_region_content_sha256_v1(
                canonical_bgra8,
                &profile.client_size_px,
                &region.rect_client_px,
            )
            .map_err(|error| {
                format!("hash selected player-visible gameplay postcondition region: {error}")
            })?;
            Ok(
                MtgoPlayerVisibleGameplayPostconditionRegionCandidateWireV1 {
                    region_id: region.region_id.clone(),
                    kind: region.kind,
                    rect_client_px: region.rect_client_px.clone(),
                    content_sha256,
                    confidence_bps: region.confidence_bps,
                },
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(
        MtgoPlayerVisibleGameplayPostconditionProcessResponseWireV1 {
            schema_version: 1,
            request_commitment_sha256,
            region_set: MtgoPlayerVisibleGameplayPostconditionRegionSetWireV1 {
                schema_version: 1,
                frame_id: header.frame_id,
                frame_sequence: header.frame_sequence,
                primitive_index: header.primitive_index,
                candidate_set_complete: true,
                regions,
            },
        },
    )
}

#[cfg(target_os = "windows")]
fn rect_keys_intersect_v1(left: (u32, u32, u32, u32), right: (u32, u32, u32, u32)) -> bool {
    let left_right = u64::from(left.0) + u64::from(left.2);
    let left_bottom = u64::from(left.1) + u64::from(left.3);
    let right_right = u64::from(right.0) + u64::from(right.2);
    let right_bottom = u64::from(right.1) + u64::from(right.3);
    u64::from(left.0) < right_right
        && u64::from(right.0) < left_right
        && u64::from(left.1) < right_bottom
        && u64::from(right.1) < left_bottom
}

#[cfg(target_os = "windows")]
fn classify_duel_perception_profile_v1(
    header: &MtgoDuelPerceptionRequestHeaderV1,
    assets: &MtgoDuelPerceptionClassifierAssetsV1,
    canonical_bgra8: &[u8],
    request_commitment_sha256: String,
) -> Result<MtgoDuelPerceptionProcessResponseV1, String> {
    validate_duel_perception_assets_v1(assets)?;
    let mut matches = Vec::new();
    for profile in &assets.profiles {
        if profile.perception_profile_commitment_sha256
            != header.perception_profile_commitment_sha256
            || profile.perception_profile_admission_commitment_sha256
                != header.perception_profile_admission_commitment_sha256
            || profile.client_size_px.width != header.canonical_width
            || profile.client_size_px.height != header.canonical_height
        {
            continue;
        }
        if duel_perception_profile_pixels_match_v1(profile, canonical_bgra8)? {
            matches.push(profile);
        }
    }
    if matches.len() != 1 {
        return Err("expected exactly one reviewed duel perception profile match".to_owned());
    }
    build_duel_perception_response_v1(
        header,
        matches[0],
        canonical_bgra8,
        request_commitment_sha256,
    )
}

#[cfg(target_os = "windows")]
fn build_duel_perception_response_v1(
    header: &MtgoDuelPerceptionRequestHeaderV1,
    profile: &MtgoDuelPerceptionReferenceProfileV1,
    canonical_bgra8: &[u8],
    request_commitment_sha256: String,
) -> Result<MtgoDuelPerceptionProcessResponseV1, String> {
    let dynamic_id = commitment_be_v1(
        DUEL_PERCEPTION_DYNAMIC_ID_DOMAIN_V1,
        &[
            request_commitment_sha256.as_bytes(),
            profile.profile_id.as_bytes(),
        ],
    );
    let mut decision = profile.decision_template.clone();
    if decision.frames.len() != 1 {
        return Err("duel perception decision template must contain exactly one frame".to_owned());
    }
    decision.decision_id = format!("visible-duel-decision-v1-{}", &dynamic_id[..24]);
    decision.frame_id = header.frame_id;
    decision.frames[0].frame_id = header.frame_id;
    decision.frames[0].sequence = header.frame_sequence;
    decision.frames[0].sha256 = header.canonical_bgra8_sha256.clone();
    decision.frames[0].client_bounds = MtgoRectPxV1 {
        x: 0,
        y: 0,
        width: header.canonical_width,
        height: header.canonical_height,
    };
    for (index, evidence) in decision.evidence.iter_mut().enumerate() {
        evidence.sequence = header
            .frame_sequence
            .checked_add(
                u64::try_from(index)
                    .map_err(|_| "duel perception evidence index overflow".to_owned())?,
            )
            .ok_or("duel perception evidence sequence overflow")?;
        match &mut evidence.source {
            MtgoEvidenceSourceV1::FrameRegion { frame_id, .. } => {
                *frame_id = header.frame_id;
            }
            MtgoEvidenceSourceV1::VisibleGameLogText { .. }
            | MtgoEvidenceSourceV1::DerivedPublicFact { .. } => {}
            MtgoEvidenceSourceV1::VisibleAccessibilityText { .. }
            | MtgoEvidenceSourceV1::ManualVisibleAnnotation { .. } => {
                return Err(
                    "duel perception templates cannot use accessibility or manual evidence"
                        .to_owned(),
                );
            }
        }
    }
    validate_duel_decision_region_pixels_v1(&decision, canonical_bgra8, &profile.client_size_px)?;
    let validated = validate_observed_decision_v1(decision.clone())
        .map_err(|error| format!("validate classified duel decision: {error}"))?;

    let mut reconstruction_audit = profile.reconstruction_audit_template.clone();
    reconstruction_audit.audit_id = format!("visible-duel-audit-v1-{}", &dynamic_id[..24]);
    reconstruction_audit.frame.sequence = header.frame_sequence;
    reconstruction_audit.frame.manifest_sha256 = header.source_manifest_sha256.clone();
    reconstruction_audit.frame.frame_sha256 = header.canonical_bgra8_sha256.clone();
    reconstruction_audit.frame.client_size_px = profile.client_size_px.clone();
    validate_reconstruction_audit_region_pixels_v1(
        &reconstruction_audit,
        canonical_bgra8,
        &profile.client_size_px,
    )?;
    validate_observation_reconstruction_audit_v1(reconstruction_audit.clone())
        .map_err(|error| format!("validate classified duel reconstruction audit: {error}"))?;

    let mut visible_controls = profile.visible_controls_template.clone();
    visible_controls.decision_commitment_sha256 = validated.decision_commitment_sha256().to_owned();
    visible_controls.frame_id = header.frame_id;
    visible_controls.frame_sequence = header.frame_sequence;
    validate_duel_visible_controls_v1(&decision, &visible_controls)?;

    let competitive_lifecycle = profile
        .competitive_lifecycle_template
        .clone()
        .map(|mut lifecycle| {
            lifecycle.snapshot_id = format!("visible-duel-lifecycle-v1-{}", &dynamic_id[..24]);
            lifecycle.frame_id = header.frame_id;
            lifecycle.frame_sequence = header.frame_sequence;
            lifecycle.frame_sha256 = header.canonical_bgra8_sha256.clone();
            lifecycle.client_bounds = MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: header.canonical_width,
                height: header.canonical_height,
            };
            validate_duel_lifecycle_region_pixels_v1(
                &lifecycle,
                canonical_bgra8,
                &profile.client_size_px,
            )?;
            validate_visible_competitive_lifecycle_snapshot_v1(lifecycle.clone())
                .map_err(|error| format!("validate classified duel lifecycle: {error}"))?;
            Ok::<_, String>(lifecycle)
        })
        .transpose()?;

    Ok(MtgoDuelPerceptionProcessResponseV1 {
        schema_version: 1,
        request_commitment_sha256,
        reconstruction_audit,
        decision,
        visible_controls,
        competitive_lifecycle,
    })
}

#[cfg(target_os = "windows")]
fn validate_duel_perception_assets_v1(
    assets: &MtgoDuelPerceptionClassifierAssetsV1,
) -> Result<(), String> {
    if assets.schema_version != 1
        || assets.scope != DUEL_PERCEPTION_ASSET_SCOPE_V1
        || assets.canonical_pixel_format != PIXEL_FORMAT_V1
        || assets.profiles.is_empty()
        || assets.profiles.len() > MAX_DUEL_PERCEPTION_PROFILES_V1
    {
        return Err("duel perception classifier assets are outside the bounded schema".to_owned());
    }
    let mut profile_ids = HashSet::new();
    for profile in &assets.profiles {
        validate_identifier_v1(&profile.profile_id, "duel perception profile id")?;
        validate_sha256_v1(
            &profile.perception_profile_commitment_sha256,
            "duel perception profile commitment",
        )?;
        validate_sha256_v1(
            &profile.perception_profile_admission_commitment_sha256,
            "duel perception profile admission commitment",
        )?;
        if !profile_ids.insert(profile.profile_id.as_str()) {
            return Err("duel perception profiles must be uniquely identified".to_owned());
        }
        if profile.client_size_px.width == 0
            || profile.client_size_px.height == 0
            || profile.client_size_px.width > 16_384
            || profile.client_size_px.height > 16_384
            || profile.decision_template.frames.len() != 1
            || profile.decision_template.evidence.is_empty()
        {
            return Err("duel perception profile geometry or template is invalid".to_owned());
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn duel_perception_profile_pixels_match_v1(
    profile: &MtgoDuelPerceptionReferenceProfileV1,
    canonical_bgra8: &[u8],
) -> Result<bool, String> {
    let mut matches = true;
    for evidence in &profile.decision_template.evidence {
        match &evidence.source {
            MtgoEvidenceSourceV1::FrameRegion {
                rect,
                content_sha256,
                ..
            } => {
                validate_sha256_v1(content_sha256, "duel decision region reference")?;
                let actual = visible_frame_region_content_sha256_v1(
                    canonical_bgra8,
                    &profile.client_size_px,
                    rect,
                )
                .map_err(|error| format!("hash duel decision reference region: {error}"))?;
                matches &= actual == *content_sha256;
            }
            MtgoEvidenceSourceV1::VisibleGameLogText { .. }
            | MtgoEvidenceSourceV1::DerivedPublicFact { .. } => {}
            MtgoEvidenceSourceV1::VisibleAccessibilityText { .. }
            | MtgoEvidenceSourceV1::ManualVisibleAnnotation { .. } => {
                return Err(
                    "duel perception templates cannot use accessibility or manual evidence"
                        .to_owned(),
                );
            }
        }
    }
    for group in &profile.reconstruction_audit_template.groups {
        for region in &group.visible_regions {
            validate_sha256_v1(&region.bgra8_sha256, "duel audit region reference")?;
            let actual = visible_frame_region_content_sha256_v1(
                canonical_bgra8,
                &profile.client_size_px,
                &region.rect_client_px,
            )
            .map_err(|error| format!("hash duel audit reference region: {error}"))?;
            matches &= actual == region.bgra8_sha256;
        }
    }
    if let Some(lifecycle) = &profile.competitive_lifecycle_template {
        for fact in &lifecycle.facts {
            validate_sha256_v1(&fact.content_sha256, "duel lifecycle region reference")?;
            let actual = visible_frame_region_content_sha256_v1(
                canonical_bgra8,
                &profile.client_size_px,
                &fact.rect_client_px,
            )
            .map_err(|error| format!("hash duel lifecycle reference region: {error}"))?;
            matches &= actual == fact.content_sha256;
        }
    }
    Ok(matches)
}

#[cfg(target_os = "windows")]
fn validate_duel_decision_region_pixels_v1(
    decision: &MtgoObservedDecisionV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), String> {
    for evidence in &decision.evidence {
        if let MtgoEvidenceSourceV1::FrameRegion {
            rect,
            content_sha256,
            ..
        } = &evidence.source
        {
            let actual = visible_frame_region_content_sha256_v1(canonical_bgra8, size, rect)
                .map_err(|error| format!("hash classified duel evidence region: {error}"))?;
            if actual != *content_sha256 {
                return Err("duel decision region changed after profile selection".to_owned());
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_reconstruction_audit_region_pixels_v1(
    audit: &MtgoObservationReconstructionAuditV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), String> {
    for group in &audit.groups {
        for region in &group.visible_regions {
            let actual = visible_frame_region_content_sha256_v1(
                canonical_bgra8,
                size,
                &region.rect_client_px,
            )
            .map_err(|error| format!("hash classified duel audit region: {error}"))?;
            if actual != region.bgra8_sha256 {
                return Err("duel reconstruction region changed after profile selection".to_owned());
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_duel_lifecycle_region_pixels_v1(
    lifecycle: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), String> {
    for fact in &lifecycle.facts {
        let actual =
            visible_frame_region_content_sha256_v1(canonical_bgra8, size, &fact.rect_client_px)
                .map_err(|error| format!("hash classified duel lifecycle region: {error}"))?;
        if actual != fact.content_sha256 {
            return Err("duel lifecycle region changed after profile selection".to_owned());
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_duel_visible_controls_v1(
    decision: &MtgoObservedDecisionV1,
    controls: &MtgoVisibleActionControlSetV1,
) -> Result<(), String> {
    if controls.schema_version != 1
        || controls.frame_id != decision.frame_id
        || controls.frame_sequence != decision.frames[0].sequence
        || !controls.prompt_reconciled
        || !controls.candidate_set_complete
        || controls.controls.is_empty()
        || controls.controls.len() > 128
        || controls.controls.len() != decision.payload.legal_actions.len()
    {
        return Err("duel visible controls do not bind the complete decision".to_owned());
    }
    let evidence_rect = |evidence_id: u64| -> Result<&MtgoRectPxV1, String> {
        let evidence = decision
            .evidence
            .iter()
            .find(|candidate| candidate.evidence_id == evidence_id)
            .ok_or("duel visible control cites unknown evidence")?;
        let MtgoEvidenceSourceV1::FrameRegion { rect, .. } = &evidence.source else {
            return Err("duel visible control evidence is not a frame region".to_owned());
        };
        Ok(rect)
    };
    let prompt_rect = evidence_rect(controls.prompt_frame_region_evidence_id)?;
    let mut control_ids = HashSet::new();
    let mut evidence_ids = HashSet::new();
    for control in &controls.controls {
        validate_identifier_v1(&control.control_id, "duel visible control id")?;
        if !control_ids.insert(control.control_id.as_str())
            || !evidence_ids.insert(control.frame_region_evidence_id)
            || !control.visibly_enabled
            || !(9_500..=10_000).contains(&control.confidence_bps)
        {
            return Err("duel visible control is duplicated, hidden, or uncertain".to_owned());
        }
        if decision
            .payload
            .legal_actions
            .iter()
            .filter(|semantic| *semantic == &control.semantic)
            .count()
            != 1
            || controls
                .controls
                .iter()
                .filter(|candidate| candidate.semantic == control.semantic)
                .count()
                != 1
        {
            return Err("duel visible controls must map one-to-one to legal actions".to_owned());
        }
        let control_rect = evidence_rect(control.frame_region_evidence_id)?;
        if rects_intersect_local_v1(prompt_rect, control_rect) {
            return Err("duel visible control overlaps the prompt region".to_owned());
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn rects_intersect_local_v1(left: &MtgoRectPxV1, right: &MtgoRectPxV1) -> bool {
    let Some(left_right) = left.x.checked_add(left.width) else {
        return true;
    };
    let Some(left_bottom) = left.y.checked_add(left.height) else {
        return true;
    };
    let Some(right_right) = right.x.checked_add(right.width) else {
        return true;
    };
    let Some(right_bottom) = right.y.checked_add(right.height) else {
        return true;
    };
    left.x < right_right && left_right > right.x && left.y < right_bottom && left_bottom > right.y
}

#[cfg(target_os = "windows")]
fn classify_sideboard_profile_v1(
    header: &MtgoCompetitiveSideboardClassifierRequestHeaderV1,
    navigation_profile: &MtgoCompetitiveNavigationRegionProfileV1,
    sideboard_profile: &MtgoCompetitiveSideboardRegionProfileV1,
    canonical_bgra8: &[u8],
    request_commitment_sha256: String,
) -> Result<MtgoCompetitiveSideboardClassifierProcessResponseV1, String> {
    let lifecycle = build_navigation_lifecycle_from_frame_v1(
        header.frame_id,
        header.frame_sequence,
        header.canonical_width,
        header.canonical_height,
        &header.canonical_bgra8_sha256,
        navigation_profile,
        canonical_bgra8,
    )?;
    let checked_lifecycle =
        validate_visible_competitive_lifecycle_snapshot_v1(lifecycle.clone())
            .map_err(|error| format!("validate sideboard source lifecycle: {error}"))?;
    if checked_lifecycle.snapshot_commitment_sha256()
        != header.source_lifecycle_snapshot_commitment_sha256
    {
        return Err("sideboard lifecycle differs from the prior navigation response".to_owned());
    }
    let manifest = validate_competitive_deck_manifest_v1(sideboard_profile.deck_manifest.clone())
        .map_err(|error| format!("validate sideboard profile deck manifest: {error}"))?;
    validate_sideboard_manifest_identity_v1(&manifest, header)?;
    let size = navigation_profile.client_size_px.clone();
    let mainboard_zone = build_sideboard_zone_from_frame_v1(
        "mainboard",
        &sideboard_profile.mainboard_zone,
        canonical_bgra8,
        &size,
    )?;
    let sideboard_zone = build_sideboard_zone_from_frame_v1(
        "sideboard",
        &sideboard_profile.sideboard_zone,
        canonical_bgra8,
        &size,
    )?;
    let cards = sideboard_profile
        .cards
        .iter()
        .map(|card| build_sideboard_card_from_frame_v1(card, canonical_bgra8, &size))
        .collect::<Result<Vec<_>, String>>()?;
    let sideboard = MtgoVisibleCompetitiveSideboardSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
        snapshot_id: format!(
            "visible-region-sideboard-v1-{}",
            &request_commitment_sha256[..24]
        ),
        event_kind: navigation_profile.event_kind,
        event_identity_sha256: navigation_profile
            .event_identity_sha256
            .clone()
            .ok_or("sideboard lifecycle has no event identity")?,
        match_identity_sha256: navigation_profile
            .match_identity_sha256
            .clone()
            .ok_or("sideboard lifecycle has no match identity")?,
        game_number: navigation_profile
            .game_number
            .ok_or("sideboard lifecycle has no game number")?,
        frame_id: header.frame_id,
        frame_sequence: header.frame_sequence,
        frame_sha256: header.canonical_bgra8_sha256.clone(),
        lifecycle_snapshot_commitment_sha256: checked_lifecycle
            .snapshot_commitment_sha256()
            .to_owned(),
        deck_manifest_commitment_sha256: header.deck_manifest_commitment_sha256.clone(),
        policy_deployment_commitment_sha256: header.policy_deployment_commitment_sha256.clone(),
        visible_configuration_complete: true,
        mainboard_zone,
        sideboard_zone,
        cards,
    };
    validate_visible_competitive_sideboard_snapshot_v1(
        checked_lifecycle,
        &manifest,
        sideboard.clone(),
    )
    .map_err(|error| format!("validate classified sideboard snapshot: {error}"))?;
    Ok(MtgoCompetitiveSideboardClassifierProcessResponseV1 {
        schema_version: 1,
        request_commitment_sha256,
        lifecycle,
        sideboard,
    })
}

#[cfg(target_os = "windows")]
fn build_sideboard_zone_from_frame_v1(
    label: &str,
    profile: &MtgoCompetitiveSideboardZoneProfileV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<MtgoVisibleCompetitiveSideboardZoneV1, String> {
    let content_sha256 =
        visible_frame_region_content_sha256_v1(canonical_bgra8, size, &profile.rect_client_px)
            .map_err(|error| format!("hash {label} sideboard zone: {error}"))?;
    let empty_drop_content_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        size,
        &profile.empty_drop_rect_client_px,
    )
    .map_err(|error| format!("hash {label} sideboard empty drop target: {error}"))?;
    if profile
        .accepted_reference_sha256s
        .binary_search(&content_sha256)
        .is_err()
        || profile
            .accepted_empty_drop_reference_sha256s
            .binary_search(&empty_drop_content_sha256)
            .is_err()
    {
        return Err(format!(
            "{label} sideboard zone changed after profile selection"
        ));
    }
    Ok(MtgoVisibleCompetitiveSideboardZoneV1 {
        rect_client_px: profile.rect_client_px.clone(),
        content_sha256,
        empty_drop_rect_client_px: profile.empty_drop_rect_client_px.clone(),
        empty_drop_content_sha256,
        confidence_bps: profile.confidence_bps,
    })
}

#[cfg(target_os = "windows")]
fn build_sideboard_card_from_frame_v1(
    profile: &MtgoCompetitiveSideboardCardProfileV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<MtgoVisibleCompetitiveSideboardCardV1, String> {
    let content_sha256 =
        visible_frame_region_content_sha256_v1(canonical_bgra8, size, &profile.rect_client_px)
            .map_err(|error| format!("hash visible sideboard card: {error}"))?;
    if profile
        .accepted_reference_sha256s
        .binary_search(&content_sha256)
        .is_err()
    {
        return Err("visible sideboard card changed after profile selection".to_owned());
    }
    Ok(MtgoVisibleCompetitiveSideboardCardV1 {
        partition: profile.partition,
        card_name: profile.card_name.clone(),
        count: profile.count,
        rect_client_px: profile.rect_client_px.clone(),
        content_sha256,
        confidence_bps: profile.confidence_bps,
    })
}

#[cfg(target_os = "windows")]
fn classify_navigation_profile_v1(
    header: &MtgoCompetitiveNavigationClassifierRequestHeaderV1,
    profile: &MtgoCompetitiveNavigationRegionProfileV1,
    canonical_bgra8: &[u8],
    request_commitment_sha256: String,
) -> Result<MtgoCompetitiveNavigationClassifierProcessResponseV1, String> {
    let lifecycle = build_navigation_lifecycle_from_frame_v1(
        header.frame_id,
        header.frame_sequence,
        header.canonical_width,
        header.canonical_height,
        &header.canonical_bgra8_sha256,
        profile,
        canonical_bgra8,
    )?;
    Ok(MtgoCompetitiveNavigationClassifierProcessResponseV1 {
        schema_version: 1,
        request_commitment_sha256,
        lifecycle,
    })
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
fn build_navigation_lifecycle_from_frame_v1(
    frame_id: u64,
    frame_sequence: u64,
    width: u32,
    height: u32,
    frame_sha256: &str,
    profile: &MtgoCompetitiveNavigationRegionProfileV1,
    canonical_bgra8: &[u8],
) -> Result<MtgoVisibleCompetitiveLifecycleSnapshotV1, String> {
    let size = profile.client_size_px.clone();
    let facts = profile
        .facts
        .iter()
        .map(|fact| {
            let content_sha256 = visible_frame_region_content_sha256_v1(
                canonical_bgra8,
                &size,
                &fact.rect_client_px,
            )
            .map_err(|error| format!("hash navigation fact pixels: {error}"))?;
            if fact
                .accepted_reference_sha256s
                .binary_search(&content_sha256)
                .is_err()
            {
                return Err("navigation fact changed after profile selection".to_owned());
            }
            Ok(MtgoLifecycleVisibleFactV1 {
                kind: fact.kind,
                rect_client_px: fact.rect_client_px.clone(),
                content_sha256,
                confidence_bps: fact.confidence_bps,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let frame_id_bytes = frame_id.to_be_bytes();
    let frame_sequence_bytes = frame_sequence.to_be_bytes();
    let width_bytes = width.to_be_bytes();
    let height_bytes = height.to_be_bytes();
    let snapshot_id_binding = commitment_v1(
        NAVIGATION_SNAPSHOT_ID_DOMAIN_V1,
        &[
            &frame_id_bytes,
            &frame_sequence_bytes,
            &width_bytes,
            &height_bytes,
            frame_sha256.as_bytes(),
            profile.profile_id.as_bytes(),
        ],
    );
    let lifecycle = MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        snapshot_id: format!(
            "visible-region-navigation-v1-{}",
            &snapshot_id_binding[..24]
        ),
        event_kind: profile.event_kind,
        phase: profile.phase,
        frame_id,
        frame_sequence,
        frame_sha256: frame_sha256.to_owned(),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width,
            height,
        },
        event_identity_sha256: profile.event_identity_sha256.clone(),
        match_identity_sha256: profile.match_identity_sha256.clone(),
        game_number: profile.game_number,
        entry_terms: profile.entry_terms.clone(),
        visible_state_complete: true,
        facts,
    };
    validate_visible_competitive_lifecycle_snapshot_v1(lifecycle.clone())
        .map_err(|error| format!("validate classified navigation lifecycle: {error}"))?;
    Ok(lifecycle)
}

#[cfg(target_os = "windows")]
fn classify_from_words_v1(
    header: &MtgoCompetitiveEventListingClassifierRequestHeaderV1,
    profile: &MtgoCompetitiveEventListingOcrProfileV1,
    canonical_bgra8: &[u8],
    words: &[OcrWordV1],
    request_commitment_sha256: String,
) -> Result<MtgoCompetitiveEventListingClassifierProcessResponseV1, String> {
    let expected_tokens = normalized_tokens_v1(&profile.expected_visible_label);
    let matches = find_exact_token_sequence_v1(words, &expected_tokens)
        .into_iter()
        .filter(|rect| rect_inside_v1(rect, &profile.label_search_rect_client_px))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "expected exactly one target label inside its reviewed card region, found {}",
            matches.len()
        ));
    }
    let event_label_rect_client_px = matches[0].clone();
    let size = profile.client_size_px.clone();
    let event_label_region_sha256 =
        visible_frame_region_content_sha256_v1(canonical_bgra8, &size, &event_label_rect_client_px)
            .map_err(|error| format!("hash selected event label pixels: {error}"))?;
    let control_region_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &profile.open_entry_review_control_rect_client_px,
    )
    .map_err(|error| format!("hash Open Entry Review control pixels: {error}"))?;
    if profile
        .enabled_control_reference_sha256s
        .binary_search(&control_region_sha256)
        .is_err()
    {
        return Err(
            "Open Entry Review control does not match any reviewed enabled reference".to_owned(),
        );
    }
    if rects_overlap_v1(
        &event_label_rect_client_px,
        &profile.open_entry_review_control_rect_client_px,
    ) {
        return Err("selected label overlaps the reviewed Open Entry Review control".to_owned());
    }
    let selection = MtgoVisibleCompetitiveEventListingSelectionV1 {
        schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
        selection_id: format!(
            "windows-ocr-listing-v1:{}",
            &request_commitment_sha256[..24]
        ),
        target_commitment_sha256: header.target_commitment_sha256.clone(),
        event_kind: header.target.event_kind,
        event_identity_sha256: header.target.event_identity_sha256.clone(),
        event_display_label_sha256: header.target.event_display_label_sha256.clone(),
        source_lifecycle_snapshot_commitment_sha256: header
            .source_lifecycle_snapshot_commitment_sha256
            .clone(),
        frame_id: header.frame_id,
        frame_sequence: header.frame_sequence,
        frame_sha256: header.canonical_bgra8_sha256.clone(),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: header.canonical_width,
            height: header.canonical_height,
        },
        event_label_rect_client_px,
        event_label_region_sha256,
        open_entry_review_control_rect_client_px: profile
            .open_entry_review_control_rect_client_px
            .clone(),
        open_entry_review_control_region_sha256: control_region_sha256,
        open_entry_review_control_enabled: true,
        confidence_bps: profile.confidence_bps,
    };
    Ok(MtgoCompetitiveEventListingClassifierProcessResponseV1 {
        schema_version: 1,
        request_commitment_sha256,
        selection,
    })
}

#[cfg(target_os = "windows")]
fn classify_deck_gate_from_words_v1(
    header: &MtgoCompetitiveDeckGateClassifierRequestHeaderV1,
    profile: &MtgoCompetitiveDeckGateProfileV1,
    canonical_bgra8: &[u8],
    words: &[OcrWordV1],
    request_commitment_sha256: String,
) -> Result<MtgoCompetitiveDeckGateClassifierProcessResponseV1, String> {
    let event_matches = find_exact_token_sequence_v1(
        words,
        &normalized_tokens_v1(&profile.expected_visible_event_label),
    )
    .into_iter()
    .filter(|rect| rect_inside_v1(rect, &profile.event_label_search_rect_client_px))
    .collect::<Vec<_>>();
    if event_matches.len() != 1 {
        return Err("deck-gate profile did not find one exact event label".to_owned());
    }
    let selected_deck_matches = find_exact_token_sequence_v1(
        words,
        &normalized_tokens_v1(&profile.expected_visible_deck_label),
    )
    .into_iter()
    .filter(|rect| rect_inside_v1(rect, &profile.deck_status_search_rect_client_px))
    .collect::<Vec<_>>();
    let missing_deck_matches =
        find_exact_token_sequence_v1(words, &normalized_tokens_v1("Please Select a Deck"))
            .into_iter()
            .filter(|rect| rect_inside_v1(rect, &profile.deck_status_search_rect_client_px))
            .collect::<Vec<_>>();
    match profile.state {
        MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection => {
            if missing_deck_matches.len() != 1 || !selected_deck_matches.is_empty() {
                return Err("awaiting-deck OCR state differs".to_owned());
            }
        }
        MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected
        | MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable => {
            if !missing_deck_matches.is_empty() || selected_deck_matches.len() != 1 {
                return Err("selected-deck OCR state differs".to_owned());
            }
        }
    }
    let size = profile.client_size_px.clone();
    let event_label_rect_client_px = event_matches[0].clone();
    let event_label_region_sha256 =
        visible_frame_region_content_sha256_v1(canonical_bgra8, &size, &event_label_rect_client_px)
            .map_err(|error| format!("hash deck-gate event label pixels: {error}"))?;
    let select_deck_control_region_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &profile.select_deck_control_rect_client_px,
    )
    .map_err(|error| format!("hash deck-gate deck control pixels: {error}"))?;
    if profile
        .select_deck_control_reference_sha256s
        .binary_search(&select_deck_control_region_sha256)
        .is_err()
    {
        return Err("deck-gate deck control is not one reviewed enabled reference".to_owned());
    }
    let open_entry_review_status_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &profile.open_entry_review_status_rect_client_px,
    )
    .map_err(|error| format!("hash deck-gate Open Entry Review status pixels: {error}"))?;
    let open_review_available =
        profile.state == MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable;
    let expected_open_review_references = if open_review_available {
        &profile.open_entry_review_enabled_reference_sha256s
    } else {
        &profile.open_entry_review_unavailable_reference_sha256s
    };
    if expected_open_review_references
        .binary_search(&open_entry_review_status_sha256)
        .is_err()
    {
        return Err("deck-gate Open Entry Review status differs from reviewed state".to_owned());
    }

    let missing_deck_prompt_rect_client_px = missing_deck_matches.first().cloned();
    let missing_deck_prompt_region_sha256 = missing_deck_prompt_rect_client_px
        .as_ref()
        .map(|rect| {
            visible_frame_region_content_sha256_v1(canonical_bgra8, &size, rect)
                .map_err(|error| format!("hash missing-deck prompt pixels: {error}"))
        })
        .transpose()?;
    let selected_deck_rect_client_px = selected_deck_matches.first().cloned();
    let selected_deck_region_sha256 = selected_deck_rect_client_px
        .as_ref()
        .map(|rect| {
            visible_frame_region_content_sha256_v1(canonical_bgra8, &size, rect)
                .map_err(|error| format!("hash selected-deck label pixels: {error}"))
        })
        .transpose()?;
    let selected_deck_label_sha256 = selected_deck_rect_client_px
        .as_ref()
        .map(|_| header.target.deck_display_label_sha256.clone());
    let open_entry_review_control_rect_client_px =
        open_review_available.then(|| profile.open_entry_review_status_rect_client_px.clone());
    let open_entry_review_control_region_sha256 =
        open_review_available.then_some(open_entry_review_status_sha256);
    let gate = MtgoVisibleCompetitiveDeckGateV1 {
        schema_version: MTGO_COMPETITIVE_DECK_GATE_SCHEMA_V1,
        observation_id: format!(
            "windows-ocr-deck-gate-v1:{}",
            &request_commitment_sha256[..24]
        ),
        target_commitment_sha256: header.target_commitment_sha256.clone(),
        event_kind: header.target.event_kind,
        event_identity_sha256: header.target.event_identity_sha256.clone(),
        event_display_label_sha256: header.target.event_display_label_sha256.clone(),
        source_lifecycle_snapshot_commitment_sha256: header
            .source_lifecycle_snapshot_commitment_sha256
            .clone(),
        frame_id: header.frame_id,
        frame_sequence: header.frame_sequence,
        frame_sha256: header.canonical_bgra8_sha256.clone(),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: header.canonical_width,
            height: header.canonical_height,
        },
        state: profile.state,
        event_label_rect_client_px,
        event_label_region_sha256,
        select_deck_control_rect_client_px: profile.select_deck_control_rect_client_px.clone(),
        select_deck_control_region_sha256,
        select_deck_control_enabled: true,
        missing_deck_prompt_rect_client_px,
        missing_deck_prompt_region_sha256,
        selected_deck_label_sha256,
        selected_deck_rect_client_px,
        selected_deck_region_sha256,
        open_entry_review_control_rect_client_px,
        open_entry_review_control_region_sha256,
        open_entry_review_control_enabled: open_review_available,
        confidence_bps: profile.confidence_bps,
    };
    Ok(MtgoCompetitiveDeckGateClassifierProcessResponseV1 {
        schema_version: 1,
        request_commitment_sha256,
        gate,
    })
}

#[cfg(target_os = "windows")]
fn classify_deck_chooser_from_words_v1(
    header: &MtgoCompetitiveDeckChooserClassifierRequestHeaderV1,
    profile: &MtgoCompetitiveDeckChooserProfileV1,
    canonical_bgra8: &[u8],
    words: &[OcrWordV1],
    request_commitment_sha256: String,
) -> Result<MtgoCompetitiveDeckChooserClassifierProcessResponseV1, String> {
    let title_matches = find_exact_token_sequence_v1(
        words,
        &normalized_tokens_v1(&profile.expected_visible_chooser_title),
    )
    .into_iter()
    .filter(|rect| rect_inside_v1(rect, &profile.chooser_title_search_rect_client_px))
    .collect::<Vec<_>>();
    let deck_matches = find_exact_token_sequence_v1(
        words,
        &normalized_tokens_v1(&profile.expected_visible_deck_label),
    )
    .into_iter()
    .filter(|rect| {
        rect_inside_v1(rect, &profile.deck_label_search_rect_client_px)
            && rect_inside_v1(rect, &profile.deck_row_control_rect_client_px)
    })
    .collect::<Vec<_>>();
    if title_matches.len() != 1 || deck_matches.len() != 1 {
        return Err("deck-chooser profile did not find one exact title and deck label".to_owned());
    }
    let size = profile.client_size_px.clone();
    let chooser_title_rect_client_px = title_matches[0].clone();
    let selected_deck_label_rect_client_px = deck_matches[0].clone();
    let chooser_title_region_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &chooser_title_rect_client_px,
    )
    .map_err(|error| format!("hash deck-chooser title pixels: {error}"))?;
    let selected_deck_label_region_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &selected_deck_label_rect_client_px,
    )
    .map_err(|error| format!("hash deck-chooser exact deck label pixels: {error}"))?;
    let deck_row_control_region_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &profile.deck_row_control_rect_client_px,
    )
    .map_err(|error| format!("hash deck-chooser row pixels: {error}"))?;
    let selection_detail_region_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &profile.selection_detail_rect_client_px,
    )
    .map_err(|error| format!("hash deck-chooser selection-detail pixels: {error}"))?;
    let submit_control_region_sha256 = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        &size,
        &profile.submit_control_rect_client_px,
    )
    .map_err(|error| format!("hash deck-chooser Submit pixels: {error}"))?;
    let selected = profile.state == MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected;
    let row_references = if selected {
        &profile.selected_deck_row_reference_sha256s
    } else {
        &profile.unselected_deck_row_reference_sha256s
    };
    let detail_references = if selected {
        &profile.selected_selection_detail_reference_sha256s
    } else {
        &profile.unselected_selection_detail_reference_sha256s
    };
    let submit_references = if selected {
        &profile.enabled_submit_reference_sha256s
    } else {
        &profile.disabled_submit_reference_sha256s
    };
    if row_references
        .binary_search(&deck_row_control_region_sha256)
        .is_err()
        || detail_references
            .binary_search(&selection_detail_region_sha256)
            .is_err()
        || submit_references
            .binary_search(&submit_control_region_sha256)
            .is_err()
    {
        return Err("deck-chooser row, detail, or Submit differs from reviewed pixels".to_owned());
    }
    let chooser = MtgoVisibleCompetitiveDeckChooserV1 {
        schema_version: 1,
        observation_id: format!(
            "windows-ocr-deck-chooser-v1:{}",
            &request_commitment_sha256[..24]
        ),
        target_commitment_sha256: header.target_commitment_sha256.clone(),
        source_deck_gate_observation_commitment_sha256: header
            .source_deck_gate_observation_commitment_sha256
            .clone(),
        event_kind: header.target.event_kind,
        event_identity_sha256: header.target.event_identity_sha256.clone(),
        frame_id: header.frame_id,
        frame_sequence: header.frame_sequence,
        frame_sha256: header.canonical_bgra8_sha256.clone(),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: header.canonical_width,
            height: header.canonical_height,
        },
        state: profile.state,
        chooser_title_label_sha256: profile.chooser_title_label_sha256.clone(),
        chooser_title_rect_client_px,
        chooser_title_region_sha256,
        selected_deck_label_sha256: header.target.deck_display_label_sha256.clone(),
        selected_deck_label_rect_client_px,
        selected_deck_label_region_sha256,
        deck_row_control_rect_client_px: profile.deck_row_control_rect_client_px.clone(),
        deck_row_control_region_sha256,
        deck_row_selected: selected,
        selection_detail_rect_client_px: profile.selection_detail_rect_client_px.clone(),
        selection_detail_region_sha256,
        submit_control_rect_client_px: profile.submit_control_rect_client_px.clone(),
        submit_control_region_sha256,
        submit_control_enabled: selected,
        confidence_bps: profile.confidence_bps,
    };
    Ok(MtgoCompetitiveDeckChooserClassifierProcessResponseV1 {
        schema_version: 1,
        request_commitment_sha256,
        chooser,
    })
}

#[cfg(target_os = "windows")]
fn validate_duel_perception_header_identity_v1(
    header: &MtgoDuelPerceptionRequestHeaderV1,
    assets_json: &[u8],
    actual_card_database_sha256: &str,
    actual_classifier_sha256: &str,
) -> Result<(), String> {
    if header.schema_version != 1
        || header.protocol != "mtgo_visible_duel_perception_v1"
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("duel perception request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("duel perception stride overflow")?;
    let expected_length = usize::try_from(header.canonical_width)
        .ok()
        .and_then(|width| {
            usize::try_from(header.canonical_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("duel perception pixel length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || expected_length == 0
        || u64::try_from(expected_length)
            .ok()
            .is_none_or(|length| length > MAX_CANONICAL_BYTES_V1)
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(assets_json)
        || header.card_database_profile_sha256 != actual_card_database_sha256
        || header.perception_pipeline_binary_sha256 != actual_classifier_sha256
    {
        return Err("duel perception runtime, geometry, or artifacts differ".to_owned());
    }
    for digest in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_manifest_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.source_frame_profile_binding_sha256.as_str(),
        header.perception_profile_commitment_sha256.as_str(),
        header
            .perception_profile_admission_commitment_sha256
            .as_str(),
        header.runtime_identity_commitment_sha256.as_str(),
        header.perception_pipeline_binary_sha256.as_str(),
        header.classifier_assets_manifest_sha256.as_str(),
        header.card_database_profile_sha256.as_str(),
    ] {
        validate_sha256_v1(digest, "duel perception request commitment")?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_duel_perception_pixels_v1(
    header: &MtgoDuelPerceptionRequestHeaderV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if canonical_bgra8.len() != header.canonical_byte_length
        || sha256_hex_v1(canonical_bgra8) != header.canonical_bgra8_sha256
    {
        return Err("duel perception pixels differ from the request header".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_header_identity_v1(
    header: &MtgoCompetitiveEventListingClassifierRequestHeaderV1,
    assets_json: &[u8],
    actual_classifier_sha256: &str,
) -> Result<(), String> {
    if header.schema_version != 1
        || header.protocol != EVENT_LISTING_REQUEST_PROTOCOL_V1
        || header.parser_scope != REQUEST_SCOPE_V1
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("selected-listing request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("selected-listing stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("selected-listing pixel length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || expected_length == 0
        || expected_length > MAX_CANONICAL_BYTES_V1
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(assets_json)
        || header.classifier_binary_sha256 != actual_classifier_sha256
    {
        return Err("selected-listing runtime, geometry, or assets differ".to_owned());
    }
    for digest in header_digests_v1(header) {
        validate_sha256_v1(digest, "request commitment")?;
    }
    validate_target_v1(&header.target)?;
    if header.target.approved_account_alias_sha256 != header.approved_account_alias_sha256
        || header.target_commitment_sha256 != target_commitment_v1(&header.target)?
    {
        return Err("selected-listing request changed its exact account or target".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_pixels_v1(
    header: &MtgoCompetitiveEventListingClassifierRequestHeaderV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if u64::try_from(canonical_bgra8.len()).ok() != Some(header.canonical_byte_length)
        || sha256_hex_v1(canonical_bgra8) != header.canonical_bgra8_sha256
    {
        return Err("selected-listing pixels differ from the request header".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_deck_gate_header_identity_v1(
    header: &MtgoCompetitiveDeckGateClassifierRequestHeaderV1,
    assets_json: &[u8],
    actual_classifier_sha256: &str,
) -> Result<(), String> {
    if header.schema_version != 1
        || header.protocol != DECK_GATE_REQUEST_PROTOCOL_V1
        || header.parser_scope != DECK_GATE_REQUEST_SCOPE_V1
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("deck-gate request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("deck-gate stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("deck-gate pixel length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || expected_length == 0
        || expected_length > MAX_CANONICAL_BYTES_V1
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(assets_json)
        || header.classifier_binary_sha256 != actual_classifier_sha256
    {
        return Err("deck-gate runtime, geometry, or assets differ".to_owned());
    }
    for digest in deck_gate_header_digests_v1(header) {
        validate_sha256_v1(digest, "deck-gate request commitment")?;
    }
    validate_target_v1(&header.target)?;
    if header.target.approved_account_alias_sha256 != header.approved_account_alias_sha256
        || header.target_commitment_sha256 != target_commitment_v1(&header.target)?
    {
        return Err("deck-gate request changed its exact account or target".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_deck_gate_pixels_v1(
    header: &MtgoCompetitiveDeckGateClassifierRequestHeaderV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if u64::try_from(canonical_bgra8.len()).ok() != Some(header.canonical_byte_length)
        || sha256_hex_v1(canonical_bgra8) != header.canonical_bgra8_sha256
    {
        return Err("deck-gate pixels differ from the request header".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_deck_chooser_header_identity_v1(
    header: &MtgoCompetitiveDeckChooserClassifierRequestHeaderV1,
    assets_json: &[u8],
    actual_classifier_sha256: &str,
) -> Result<(), String> {
    if header.schema_version != 1
        || header.protocol != DECK_CHOOSER_REQUEST_PROTOCOL_V1
        || header.parser_scope != DECK_CHOOSER_REQUEST_SCOPE_V1
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("deck-chooser request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("deck-chooser stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("deck-chooser pixel length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || expected_length == 0
        || expected_length > MAX_CANONICAL_BYTES_V1
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(assets_json)
        || header.classifier_binary_sha256 != actual_classifier_sha256
    {
        return Err("deck-chooser runtime, geometry, or assets differ".to_owned());
    }
    for digest in deck_chooser_header_digests_v1(header) {
        validate_sha256_v1(digest, "deck-chooser request commitment")?;
    }
    validate_target_v1(&header.target)?;
    if header.target.approved_account_alias_sha256 != header.approved_account_alias_sha256
        || header.target_commitment_sha256 != target_commitment_v1(&header.target)?
    {
        return Err("deck-chooser request changed its exact account or target".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_deck_chooser_pixels_v1(
    header: &MtgoCompetitiveDeckChooserClassifierRequestHeaderV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if u64::try_from(canonical_bgra8.len()).ok() != Some(header.canonical_byte_length)
        || sha256_hex_v1(canonical_bgra8) != header.canonical_bgra8_sha256
    {
        return Err("deck-chooser pixels differ from the request header".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_navigation_header_identity_v1(
    header: &MtgoCompetitiveNavigationClassifierRequestHeaderV1,
    assets_json: &[u8],
    actual_classifier_sha256: &str,
) -> Result<(), String> {
    if header.schema_version != 1
        || header.protocol != NAVIGATION_REQUEST_PROTOCOL_V1
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("navigation request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("navigation stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("navigation pixel length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || expected_length == 0
        || expected_length > MAX_CANONICAL_BYTES_V1
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(assets_json)
        || header.classifier_binary_sha256 != actual_classifier_sha256
    {
        return Err("navigation runtime, geometry, or assets differ".to_owned());
    }
    for digest in navigation_header_digests_v1(header) {
        validate_sha256_v1(digest, "navigation request commitment")?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_navigation_pixels_v1(
    header: &MtgoCompetitiveNavigationClassifierRequestHeaderV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if u64::try_from(canonical_bgra8.len()).ok() != Some(header.canonical_byte_length)
        || sha256_hex_v1(canonical_bgra8) != header.canonical_bgra8_sha256
    {
        return Err("navigation pixels differ from the request header".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_event_record_header_identity_v1(
    header: &MtgoCompetitiveEventRecordClassifierRequestHeaderV1,
    assets_json: &[u8],
    actual_classifier_sha256: &str,
) -> Result<(), String> {
    if header.schema_version != 1
        || header.protocol != EVENT_RECORD_REQUEST_PROTOCOL_V1
        || header.parser_scope != EVENT_RECORD_REQUEST_SCOPE_V1
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("event-record request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("event-record stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("event-record pixel length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || expected_length == 0
        || expected_length > MAX_CANONICAL_BYTES_V1
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(assets_json)
        || header.classifier_binary_sha256 != actual_classifier_sha256
    {
        return Err("event-record runtime, geometry, or assets differ".to_owned());
    }
    for digest in event_record_header_digests_v1(header) {
        validate_sha256_v1(digest, "event-record request commitment")?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_event_record_pixels_v1(
    header: &MtgoCompetitiveEventRecordClassifierRequestHeaderV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if u64::try_from(canonical_bgra8.len()).ok() != Some(header.canonical_byte_length)
        || sha256_hex_v1(canonical_bgra8) != header.canonical_bgra8_sha256
    {
        return Err("event-record pixels differ from the request header".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_sideboard_header_identity_v1(
    header: &MtgoCompetitiveSideboardClassifierRequestHeaderV1,
    assets_json: &[u8],
    actual_classifier_sha256: &str,
) -> Result<(), String> {
    if header.schema_version != 1
        || header.protocol != SIDEBOARD_REQUEST_PROTOCOL_V1
        || header.parser_scope != SIDEBOARD_REQUEST_SCOPE_V1
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("sideboard request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("sideboard stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("sideboard pixel length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || expected_length == 0
        || expected_length > MAX_CANONICAL_BYTES_V1
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(assets_json)
        || header.classifier_binary_sha256 != actual_classifier_sha256
    {
        return Err("sideboard runtime, geometry, or assets differ".to_owned());
    }
    for digest in sideboard_header_digests_v1(header) {
        validate_sha256_v1(digest, "sideboard request commitment")?;
    }
    if header.deck_list_sha256 == header.deck_format_sha256
        || header.deck_list_sha256 == header.policy_deployment_commitment_sha256
        || header.deck_manifest_commitment_sha256 == header.deck_format_sha256
        || header.deck_manifest_commitment_sha256 == header.policy_deployment_commitment_sha256
        || header.deck_format_sha256 == header.policy_deployment_commitment_sha256
    {
        return Err("sideboard deck, format, and policy identities are crossed".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_sideboard_pixels_v1(
    header: &MtgoCompetitiveSideboardClassifierRequestHeaderV1,
    canonical_bgra8: &[u8],
) -> Result<(), String> {
    if u64::try_from(canonical_bgra8.len()).ok() != Some(header.canonical_byte_length)
        || sha256_hex_v1(canonical_bgra8) != header.canonical_bgra8_sha256
    {
        return Err("sideboard pixels differ from the request header".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_deck_gate_assets_v1<'a>(
    assets: &'a MtgoCompetitiveEventListingClassifierAssetsV1,
    header: &MtgoCompetitiveDeckGateClassifierRequestHeaderV1,
) -> Result<Vec<&'a MtgoCompetitiveDeckGateProfileV1>, String> {
    if assets.schema_version != 1
        || assets.scope != COMBINED_ASSET_SCOPE_V1
        || assets.canonical_pixel_format != PIXEL_FORMAT_V1
        || assets.deck_gate_profiles.is_empty()
        || assets.deck_gate_profiles.len() > MAX_PROFILES_V1
    {
        return Err("deck-gate assets identity or profile count is invalid".to_owned());
    }
    let client_bounds = MtgoRectPxV1 {
        x: 0,
        y: 0,
        width: header.canonical_width,
        height: header.canonical_height,
    };
    let mut previous_profile_id: Option<&str> = None;
    let mut profile_ids = HashSet::new();
    for profile in &assets.deck_gate_profiles {
        validate_identifier_v1(&profile.profile_id, "deck-gate profile id")?;
        if !profile_ids.insert(profile.profile_id.as_str())
            || previous_profile_id.is_some_and(|previous| previous >= profile.profile_id.as_str())
            || profile.expected_visible_event_label.is_empty()
            || profile.expected_visible_event_label.len() > MAX_EXPECTED_LABEL_BYTES_V1
            || profile.expected_visible_event_label.trim() != profile.expected_visible_event_label
            || profile
                .expected_visible_event_label
                .chars()
                .any(char::is_control)
            || sha256_hex_v1(profile.expected_visible_event_label.as_bytes())
                != profile.event_display_label_sha256
            || profile.expected_visible_deck_label.is_empty()
            || profile.expected_visible_deck_label.len() > MAX_EXPECTED_LABEL_BYTES_V1
            || profile.expected_visible_deck_label.trim() != profile.expected_visible_deck_label
            || profile
                .expected_visible_deck_label
                .chars()
                .any(char::is_control)
            || sha256_hex_v1(profile.expected_visible_deck_label.as_bytes())
                != profile.deck_display_label_sha256
            || profile.client_size_px.width != header.canonical_width
            || profile.client_size_px.height != header.canonical_height
            || !(9_500..=10_000).contains(&profile.confidence_bps)
        {
            return Err("deck-gate profile identity, label, order, or geometry differs".to_owned());
        }
        previous_profile_id = Some(profile.profile_id.as_str());
        let rects = [
            &profile.event_label_search_rect_client_px,
            &profile.deck_status_search_rect_client_px,
            &profile.select_deck_control_rect_client_px,
            &profile.open_entry_review_status_rect_client_px,
        ];
        if rects
            .iter()
            .any(|rect| !rect_inside_v1(rect, &client_bounds))
        {
            return Err("deck-gate profile rectangle is outside the client".to_owned());
        }
        for first in 0..rects.len() {
            for second in (first + 1)..rects.len() {
                if rects_overlap_v1(rects[first], rects[second]) {
                    return Err("deck-gate profile rectangles overlap".to_owned());
                }
            }
        }
        validate_sorted_references_v1(
            &profile.select_deck_control_reference_sha256s,
            "deck-gate deck control references",
            true,
        )?;
        let review_available =
            profile.state == MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable;
        validate_sorted_references_v1(
            &profile.open_entry_review_unavailable_reference_sha256s,
            "deck-gate unavailable review references",
            !review_available,
        )?;
        validate_sorted_references_v1(
            &profile.open_entry_review_enabled_reference_sha256s,
            "deck-gate enabled review references",
            review_available,
        )?;
        if (review_available
            && !profile
                .open_entry_review_unavailable_reference_sha256s
                .is_empty())
            || (!review_available
                && !profile
                    .open_entry_review_enabled_reference_sha256s
                    .is_empty())
        {
            return Err(
                "deck-gate profile mixes available and unavailable review states".to_owned(),
            );
        }
    }
    let matches = assets
        .deck_gate_profiles
        .iter()
        .filter(|profile| {
            profile.event_kind == header.target.event_kind
                && profile.event_display_label_sha256 == header.target.event_display_label_sha256
                && profile.deck_display_label_sha256 == header.target.deck_display_label_sha256
        })
        .collect::<Vec<_>>();
    if matches.len() != 3 {
        return Err("deck-gate target requires exactly three reviewed state profiles".to_owned());
    }
    let mut state_seen = [false; 3];
    for profile in &matches {
        let rank = deck_gate_state_rank_v1(profile.state);
        if state_seen[rank] {
            return Err("deck-gate target contains a duplicate state profile".to_owned());
        }
        state_seen[rank] = true;
    }
    if state_seen != [true, true, true] {
        return Err("deck-gate target is missing a required state profile".to_owned());
    }
    Ok(matches)
}

#[cfg(target_os = "windows")]
fn validate_deck_chooser_assets_v1<'a>(
    assets: &'a MtgoCompetitiveEventListingClassifierAssetsV1,
    header: &MtgoCompetitiveDeckChooserClassifierRequestHeaderV1,
) -> Result<Vec<&'a MtgoCompetitiveDeckChooserProfileV1>, String> {
    if assets.schema_version != 1
        || assets.scope != COMBINED_ASSET_SCOPE_V1
        || assets.canonical_pixel_format != PIXEL_FORMAT_V1
        || assets.deck_chooser_profiles.is_empty()
        || assets.deck_chooser_profiles.len() > MAX_PROFILES_V1
    {
        return Err("deck-chooser assets identity or profile count is invalid".to_owned());
    }
    let client_bounds = MtgoRectPxV1 {
        x: 0,
        y: 0,
        width: header.canonical_width,
        height: header.canonical_height,
    };
    let mut previous_profile_id: Option<&str> = None;
    let mut profile_ids = HashSet::new();
    for profile in &assets.deck_chooser_profiles {
        validate_identifier_v1(&profile.profile_id, "deck-chooser profile id")?;
        validate_sha256_v1(
            &profile.event_display_label_sha256,
            "deck-chooser event display label",
        )?;
        for (label, expected_hash, label_name) in [
            (
                profile.expected_visible_chooser_title.as_str(),
                profile.chooser_title_label_sha256.as_str(),
                "deck-chooser title",
            ),
            (
                profile.expected_visible_deck_label.as_str(),
                profile.deck_display_label_sha256.as_str(),
                "deck-chooser deck label",
            ),
        ] {
            if label.is_empty()
                || label.len() > MAX_EXPECTED_LABEL_BYTES_V1
                || label.trim() != label
                || label.chars().any(char::is_control)
                || sha256_hex_v1(label.as_bytes()) != expected_hash
            {
                return Err(format!("{label_name} is invalid"));
            }
        }
        if !profile_ids.insert(profile.profile_id.as_str())
            || previous_profile_id.is_some_and(|previous| previous >= profile.profile_id.as_str())
            || profile.client_size_px.width != header.canonical_width
            || profile.client_size_px.height != header.canonical_height
            || !(9_500..=10_000).contains(&profile.confidence_bps)
        {
            return Err("deck-chooser profile identity, order, or geometry differs".to_owned());
        }
        previous_profile_id = Some(profile.profile_id.as_str());
        for rect in [
            &profile.chooser_title_search_rect_client_px,
            &profile.deck_label_search_rect_client_px,
            &profile.deck_row_control_rect_client_px,
            &profile.selection_detail_rect_client_px,
            &profile.submit_control_rect_client_px,
        ] {
            if !rect_inside_v1(rect, &client_bounds) {
                return Err("deck-chooser profile rectangle is outside the client".to_owned());
            }
        }
        if !rect_inside_v1(
            &profile.deck_label_search_rect_client_px,
            &profile.deck_row_control_rect_client_px,
        ) || rects_overlap_v1(
            &profile.chooser_title_search_rect_client_px,
            &profile.deck_row_control_rect_client_px,
        ) || rects_overlap_v1(
            &profile.chooser_title_search_rect_client_px,
            &profile.selection_detail_rect_client_px,
        ) || rects_overlap_v1(
            &profile.chooser_title_search_rect_client_px,
            &profile.submit_control_rect_client_px,
        ) || rects_overlap_v1(
            &profile.deck_row_control_rect_client_px,
            &profile.selection_detail_rect_client_px,
        ) || rects_overlap_v1(
            &profile.deck_row_control_rect_client_px,
            &profile.submit_control_rect_client_px,
        ) || rects_overlap_v1(
            &profile.selection_detail_rect_client_px,
            &profile.submit_control_rect_client_px,
        ) {
            return Err("deck-chooser profile evidence and controls overlap".to_owned());
        }
        let selected = profile.state == MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected;
        validate_sorted_references_v1(
            &profile.unselected_deck_row_reference_sha256s,
            "deck-chooser unselected row references",
            !selected,
        )?;
        validate_sorted_references_v1(
            &profile.selected_deck_row_reference_sha256s,
            "deck-chooser selected row references",
            selected,
        )?;
        validate_sorted_references_v1(
            &profile.unselected_selection_detail_reference_sha256s,
            "deck-chooser unselected detail references",
            !selected,
        )?;
        validate_sorted_references_v1(
            &profile.selected_selection_detail_reference_sha256s,
            "deck-chooser selected detail references",
            selected,
        )?;
        validate_sorted_references_v1(
            &profile.disabled_submit_reference_sha256s,
            "deck-chooser disabled Submit references",
            !selected,
        )?;
        validate_sorted_references_v1(
            &profile.enabled_submit_reference_sha256s,
            "deck-chooser enabled Submit references",
            selected,
        )?;
        if (selected
            && (!profile.unselected_deck_row_reference_sha256s.is_empty()
                || !profile
                    .unselected_selection_detail_reference_sha256s
                    .is_empty()
                || !profile.disabled_submit_reference_sha256s.is_empty()))
            || (!selected
                && (!profile.selected_deck_row_reference_sha256s.is_empty()
                    || !profile
                        .selected_selection_detail_reference_sha256s
                        .is_empty()
                    || !profile.enabled_submit_reference_sha256s.is_empty()))
        {
            return Err("deck-chooser profile mixes selected and unselected states".to_owned());
        }
    }
    let matches = assets
        .deck_chooser_profiles
        .iter()
        .filter(|profile| {
            profile.event_kind == header.target.event_kind
                && profile.event_display_label_sha256 == header.target.event_display_label_sha256
                && profile.deck_display_label_sha256 == header.target.deck_display_label_sha256
        })
        .collect::<Vec<_>>();
    if matches.len() != 2 {
        return Err("deck-chooser target requires exactly two reviewed state profiles".to_owned());
    }
    let base = matches[0];
    if matches.iter().skip(1).any(|profile| {
        profile.expected_visible_chooser_title != base.expected_visible_chooser_title
            || profile.chooser_title_label_sha256 != base.chooser_title_label_sha256
            || profile.expected_visible_deck_label != base.expected_visible_deck_label
            || profile.client_size_px != base.client_size_px
            || profile.chooser_title_search_rect_client_px
                != base.chooser_title_search_rect_client_px
            || profile.deck_label_search_rect_client_px != base.deck_label_search_rect_client_px
            || profile.deck_row_control_rect_client_px != base.deck_row_control_rect_client_px
            || profile.selection_detail_rect_client_px != base.selection_detail_rect_client_px
            || profile.submit_control_rect_client_px != base.submit_control_rect_client_px
            || profile.confidence_bps != base.confidence_bps
    }) {
        return Err("deck-chooser state profiles changed the exact visible layout".to_owned());
    }
    let mut state_seen = [false; 2];
    for profile in &matches {
        let rank = deck_chooser_state_rank_v1(profile.state);
        if state_seen[rank] {
            return Err("deck-chooser target contains a duplicate state profile".to_owned());
        }
        state_seen[rank] = true;
    }
    if state_seen != [true, true] {
        return Err("deck-chooser target is missing a required state profile".to_owned());
    }
    Ok(matches)
}

#[cfg(target_os = "windows")]
fn validate_sorted_references_v1(
    references: &[String],
    label: &str,
    required: bool,
) -> Result<(), String> {
    if references.len() > MAX_CONTROL_REFERENCES_V1 || (required && references.is_empty()) {
        return Err(format!("{label} count is invalid"));
    }
    let mut previous: Option<&str> = None;
    for reference in references {
        validate_sha256_v1(reference, label)?;
        if previous.is_some_and(|value| value >= reference.as_str()) {
            return Err(format!("{label} must be unique and sorted"));
        }
        previous = Some(reference.as_str());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn deck_gate_state_rank_v1(state: MtgoCompetitiveDeckGateStateV1) -> usize {
    match state {
        MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection => 0,
        MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected => 1,
        MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable => 2,
    }
}

#[cfg(target_os = "windows")]
fn deck_chooser_state_rank_v1(state: MtgoCompetitiveDeckChooserStateV1) -> usize {
    match state {
        MtgoCompetitiveDeckChooserStateV1::AwaitingExactDeckSelection => 0,
        MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected => 1,
    }
}

#[cfg(target_os = "windows")]
fn validate_assets_and_select_profile_v1<'a>(
    assets: &'a MtgoCompetitiveEventListingClassifierAssetsV1,
    header: &MtgoCompetitiveEventListingClassifierRequestHeaderV1,
) -> Result<&'a MtgoCompetitiveEventListingOcrProfileV1, String> {
    if assets.schema_version != 1
        || !matches!(
            assets.scope.as_str(),
            ASSET_SCOPE_V1 | COMBINED_ASSET_SCOPE_V1
        )
        || assets.canonical_pixel_format != PIXEL_FORMAT_V1
        || assets.profiles.is_empty()
        || assets.profiles.len() > MAX_PROFILES_V1
    {
        return Err("selected-listing assets identity or profile count is invalid".to_owned());
    }
    let client_bounds = MtgoRectPxV1 {
        x: 0,
        y: 0,
        width: header.canonical_width,
        height: header.canonical_height,
    };
    let mut profile_ids = HashSet::new();
    let mut label_hashes = HashSet::new();
    let mut previous_label_hash: Option<&str> = None;
    for profile in &assets.profiles {
        validate_identifier_v1(&profile.profile_id, "profile id")?;
        validate_sha256_v1(&profile.event_display_label_sha256, "profile label")?;
        if profile.expected_visible_label.is_empty()
            || profile.expected_visible_label.len() > MAX_EXPECTED_LABEL_BYTES_V1
            || profile.expected_visible_label.trim() != profile.expected_visible_label
            || profile.expected_visible_label.chars().any(char::is_control)
            || sha256_hex_v1(profile.expected_visible_label.as_bytes())
                != profile.event_display_label_sha256
            || profile.client_size_px.width != header.canonical_width
            || profile.client_size_px.height != header.canonical_height
            || !(9_500..=10_000).contains(&profile.confidence_bps)
            || !profile_ids.insert(profile.profile_id.as_str())
            || !label_hashes.insert(profile.event_display_label_sha256.as_str())
            || previous_label_hash
                .is_some_and(|previous| previous >= profile.event_display_label_sha256.as_str())
        {
            return Err("selected-listing OCR profile identity is invalid or unordered".to_owned());
        }
        previous_label_hash = Some(profile.event_display_label_sha256.as_str());
        if !rect_inside_v1(&profile.label_search_rect_client_px, &client_bounds)
            || !rect_inside_v1(
                &profile.open_entry_review_control_rect_client_px,
                &client_bounds,
            )
            || rects_overlap_v1(
                &profile.label_search_rect_client_px,
                &profile.open_entry_review_control_rect_client_px,
            )
            || profile.enabled_control_reference_sha256s.is_empty()
            || profile.enabled_control_reference_sha256s.len() > MAX_CONTROL_REFERENCES_V1
        {
            return Err("selected-listing OCR profile geometry is invalid".to_owned());
        }
        let mut previous_reference: Option<&str> = None;
        for reference in &profile.enabled_control_reference_sha256s {
            validate_sha256_v1(reference, "enabled control reference")?;
            if previous_reference.is_some_and(|previous| previous >= reference.as_str()) {
                return Err("enabled control references must be unique and sorted".to_owned());
            }
            previous_reference = Some(reference.as_str());
        }
    }
    let matches = assets
        .profiles
        .iter()
        .filter(|profile| {
            profile.event_kind == header.target.event_kind
                && profile.event_display_label_sha256 == header.target.event_display_label_sha256
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err("no unique OCR profile matches the exact target label and mode".to_owned());
    }
    Ok(matches[0])
}

#[cfg(target_os = "windows")]
fn validate_navigation_assets_and_match_profile_v1<'a>(
    assets: &'a MtgoCompetitiveEventListingClassifierAssetsV1,
    header: &MtgoCompetitiveNavigationClassifierRequestHeaderV1,
    canonical_bgra8: &[u8],
) -> Result<&'a MtgoCompetitiveNavigationRegionProfileV1, String> {
    validate_navigation_assets_and_match_profile_for_frame_v1(
        assets,
        header.canonical_width,
        header.canonical_height,
        canonical_bgra8,
    )
}

#[cfg(target_os = "windows")]
fn validate_navigation_assets_and_match_profile_for_frame_v1<'a>(
    assets: &'a MtgoCompetitiveEventListingClassifierAssetsV1,
    canonical_width: u32,
    canonical_height: u32,
    canonical_bgra8: &[u8],
) -> Result<&'a MtgoCompetitiveNavigationRegionProfileV1, String> {
    if assets.schema_version != 1
        || assets.scope != COMBINED_ASSET_SCOPE_V1
        || assets.canonical_pixel_format != PIXEL_FORMAT_V1
        || assets.navigation_profiles.is_empty()
        || assets.navigation_profiles.len() > MAX_PROFILES_V1
    {
        return Err("navigation assets identity or profile count is invalid".to_owned());
    }
    let client_bounds = MtgoRectPxV1 {
        x: 0,
        y: 0,
        width: canonical_width,
        height: canonical_height,
    };
    let size = MtgoSizePxV1 {
        width: canonical_width,
        height: canonical_height,
    };
    let mut profile_ids = HashSet::new();
    let mut previous_profile_id: Option<&str> = None;
    let mut matching_profiles = Vec::new();
    for profile in &assets.navigation_profiles {
        validate_identifier_v1(&profile.profile_id, "navigation profile id")?;
        if !profile_ids.insert(profile.profile_id.as_str())
            || previous_profile_id.is_some_and(|previous| previous >= profile.profile_id.as_str())
            || profile.client_size_px != size
            || profile.facts.is_empty()
            || profile.facts.len() > 32
        {
            return Err("navigation profile identity, geometry, or order is invalid".to_owned());
        }
        previous_profile_id = Some(profile.profile_id.as_str());
        let mut fact_kinds = HashSet::new();
        let mut previous_fact_rank = None;
        let mut profile_matches = true;
        for fact in &profile.facts {
            let fact_rank = lifecycle_fact_rank_v1(fact.kind);
            if !fact_kinds.insert(fact.kind)
                || previous_fact_rank.is_some_and(|previous| previous >= fact_rank)
                || !rect_inside_v1(&fact.rect_client_px, &client_bounds)
                || fact.accepted_reference_sha256s.is_empty()
                || fact.accepted_reference_sha256s.len() > MAX_CONTROL_REFERENCES_V1
                || !(9_500..=10_000).contains(&fact.confidence_bps)
            {
                return Err("navigation fact profile is invalid or unordered".to_owned());
            }
            previous_fact_rank = Some(fact_rank);
            let mut previous_reference: Option<&str> = None;
            for reference in &fact.accepted_reference_sha256s {
                validate_sha256_v1(reference, "navigation fact reference")?;
                if previous_reference.is_some_and(|previous| previous >= reference.as_str()) {
                    return Err("navigation fact references must be unique and sorted".to_owned());
                }
                previous_reference = Some(reference.as_str());
            }
            let observed = visible_frame_region_content_sha256_v1(
                canonical_bgra8,
                &size,
                &fact.rect_client_px,
            )
            .map_err(|error| format!("hash navigation profile region: {error}"))?;
            if fact
                .accepted_reference_sha256s
                .binary_search(&observed)
                .is_err()
            {
                profile_matches = false;
            }
        }
        validate_navigation_profile_contract_v1(profile)?;
        if profile_matches {
            matching_profiles.push(profile);
        }
    }
    if matching_profiles.len() != 1 {
        return Err(format!(
            "expected exactly one reviewed navigation profile match, found {}",
            matching_profiles.len()
        ));
    }
    Ok(matching_profiles[0])
}

#[cfg(target_os = "windows")]
fn validate_navigation_profile_contract_v1(
    profile: &MtgoCompetitiveNavigationRegionProfileV1,
) -> Result<(), String> {
    let facts = profile
        .facts
        .iter()
        .map(|fact| MtgoLifecycleVisibleFactV1 {
            kind: fact.kind,
            rect_client_px: fact.rect_client_px.clone(),
            content_sha256: fact.accepted_reference_sha256s[0].clone(),
            confidence_bps: fact.confidence_bps,
        })
        .collect();
    validate_visible_competitive_lifecycle_snapshot_v1(MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        snapshot_id: "navigation-profile-contract-v1".to_owned(),
        event_kind: profile.event_kind,
        phase: profile.phase,
        frame_id: 1,
        frame_sequence: 1,
        frame_sha256: "0".repeat(64),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: profile.client_size_px.width,
            height: profile.client_size_px.height,
        },
        event_identity_sha256: profile.event_identity_sha256.clone(),
        match_identity_sha256: profile.match_identity_sha256.clone(),
        game_number: profile.game_number,
        entry_terms: profile.entry_terms.clone(),
        visible_state_complete: true,
        facts,
    })
    .map(|_| ())
    .map_err(|error| format!("navigation profile lifecycle contract is invalid: {error}"))
}

#[cfg(target_os = "windows")]
fn lifecycle_fact_rank_v1(kind: MtgoLifecycleVisibleFactKindV1) -> u8 {
    use MtgoLifecycleVisibleFactKindV1::*;
    match kind {
        EventBrowserVisible => 0,
        EntryReviewVisible => 1,
        EntryTermsVisible => 2,
        EnteredEventVisible => 3,
        PairingVisible => 4,
        PairingAcceptControlEnabled => 5,
        MatchSurfaceVisible => 6,
        LocalClockVisible => 7,
        OpponentClockVisible => 8,
        SideboardSurfaceVisible => 9,
        SideboardTimerVisible => 10,
        SideboardConfigurationVisible => 11,
        SideboardNoChangesConfirmed => 12,
        SideboardSubmitControlEnabled => 13,
        MatchResultVisible => 14,
        MatchContinueControlEnabled => 15,
        EventResultVisible => 16,
        EventCloseControlEnabled => 17,
        ReconnectVisible => 18,
        ReconnectResumeControlEnabled => 19,
    }
}

#[cfg(target_os = "windows")]
fn validate_event_record_assets_and_match_profile_v1<'a>(
    assets: &'a MtgoCompetitiveEventListingClassifierAssetsV1,
    selected_navigation_profile: &MtgoCompetitiveNavigationRegionProfileV1,
    approved_account_alias_sha256: &str,
    canonical_bgra8: &[u8],
) -> Result<&'a MtgoCompetitiveEventRecordRegionProfileV1, String> {
    if assets.schema_version != 1
        || assets.scope != COMBINED_ASSET_SCOPE_V1
        || assets.canonical_pixel_format != PIXEL_FORMAT_V1
        || assets.event_record_profiles.is_empty()
        || assets.event_record_profiles.len() > MAX_PROFILES_V1
    {
        return Err("event-record assets identity or profile count is invalid".to_owned());
    }
    validate_sha256_v1(approved_account_alias_sha256, "approved account alias")?;
    let mut profile_ids = HashSet::new();
    let mut previous_profile_id: Option<&str> = None;
    let mut matching_profiles = Vec::new();
    for profile in &assets.event_record_profiles {
        validate_identifier_v1(&profile.profile_id, "event-record profile id")?;
        validate_identifier_v1(
            &profile.navigation_profile_id,
            "event-record navigation profile id",
        )?;
        validate_sha256_v1(
            &profile.event_identity_sha256,
            "event-record event identity",
        )?;
        if !profile_ids.insert(profile.profile_id.as_str())
            || previous_profile_id.is_some_and(|previous| previous >= profile.profile_id.as_str())
            || profile.facts.len() < 2
            || profile.facts.len() > 4
        {
            return Err(
                "event-record profile identity, fact count, or order is invalid".to_owned(),
            );
        }
        previous_profile_id = Some(profile.profile_id.as_str());
        let navigation_matches = assets
            .navigation_profiles
            .iter()
            .filter(|candidate| candidate.profile_id == profile.navigation_profile_id)
            .collect::<Vec<_>>();
        if navigation_matches.len() != 1 {
            return Err(
                "event-record profile does not reference one navigation profile".to_owned(),
            );
        }
        let navigation_profile = navigation_matches[0];
        let client_bounds = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: navigation_profile.client_size_px.width,
            height: navigation_profile.client_size_px.height,
        };
        let mut fact_kinds = HashSet::new();
        let mut previous_fact_rank = None;
        let mut profile_matches =
            profile.navigation_profile_id == selected_navigation_profile.profile_id;
        for fact in &profile.facts {
            let fact_rank = event_record_fact_rank_v1(fact.kind);
            if !fact_kinds.insert(fact.kind)
                || previous_fact_rank.is_some_and(|previous| previous >= fact_rank)
                || !rect_inside_v1(&fact.rect_client_px, &client_bounds)
                || fact.accepted_reference_sha256s.is_empty()
                || fact.accepted_reference_sha256s.len() > MAX_CONTROL_REFERENCES_V1
                || !(9_500..=10_000).contains(&fact.confidence_bps)
            {
                return Err("event-record fact profile is invalid or unordered".to_owned());
            }
            previous_fact_rank = Some(fact_rank);
            let mut previous_reference: Option<&str> = None;
            for reference in &fact.accepted_reference_sha256s {
                validate_sha256_v1(reference, "event-record fact reference")?;
                if previous_reference.is_some_and(|previous| previous >= reference.as_str()) {
                    return Err("event-record fact references must be unique and sorted".to_owned());
                }
                previous_reference = Some(reference.as_str());
            }
            let observed = visible_frame_region_content_sha256_v1(
                canonical_bgra8,
                &navigation_profile.client_size_px,
                &fact.rect_client_px,
            )
            .map_err(|error| format!("hash event-record profile region: {error}"))?;
            if fact
                .accepted_reference_sha256s
                .binary_search(&observed)
                .is_err()
            {
                profile_matches = false;
            }
        }
        validate_event_record_profile_contract_v1(
            profile,
            navigation_profile,
            approved_account_alias_sha256,
        )?;
        if profile_matches {
            matching_profiles.push(profile);
        }
    }
    if matching_profiles.len() != 1 {
        return Err(format!(
            "expected exactly one reviewed event-record profile match, found {}",
            matching_profiles.len()
        ));
    }
    Ok(matching_profiles[0])
}

#[cfg(target_os = "windows")]
fn validate_event_record_profile_contract_v1(
    profile: &MtgoCompetitiveEventRecordRegionProfileV1,
    navigation_profile: &MtgoCompetitiveNavigationRegionProfileV1,
    approved_account_alias_sha256: &str,
) -> Result<(), String> {
    if profile.event_kind != navigation_profile.event_kind
        || profile.lifecycle_phase != navigation_profile.phase
        || navigation_profile.event_identity_sha256.as_deref()
            != Some(profile.event_identity_sha256.as_str())
    {
        return Err("event-record profile and navigation lifecycle differ".to_owned());
    }
    let lifecycle = MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        snapshot_id: "event-record-profile-source-v1".to_owned(),
        event_kind: navigation_profile.event_kind,
        phase: navigation_profile.phase,
        frame_id: 1,
        frame_sequence: 1,
        frame_sha256: "0".repeat(64),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: navigation_profile.client_size_px.width,
            height: navigation_profile.client_size_px.height,
        },
        event_identity_sha256: navigation_profile.event_identity_sha256.clone(),
        match_identity_sha256: navigation_profile.match_identity_sha256.clone(),
        game_number: navigation_profile.game_number,
        entry_terms: navigation_profile.entry_terms.clone(),
        visible_state_complete: true,
        facts: navigation_profile
            .facts
            .iter()
            .map(|fact| MtgoLifecycleVisibleFactV1 {
                kind: fact.kind,
                rect_client_px: fact.rect_client_px.clone(),
                content_sha256: fact.accepted_reference_sha256s[0].clone(),
                confidence_bps: fact.confidence_bps,
            })
            .collect(),
    };
    let checked_lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(lifecycle.clone())
        .map_err(|error| {
        format!("event-record profile source lifecycle is invalid: {error}")
    })?;
    let record = MtgoVisibleCompetitiveEventRecordV1 {
        schema_version: MTGO_COMPETITIVE_EVENT_RECORD_SCHEMA_V1,
        record_id: "event-record-profile-contract-v1".to_owned(),
        source_lifecycle_snapshot_commitment_sha256: checked_lifecycle
            .snapshot_commitment_sha256()
            .to_owned(),
        approved_account_alias_sha256: approved_account_alias_sha256.to_owned(),
        event_kind: profile.event_kind,
        lifecycle_phase: profile.lifecycle_phase,
        frame_id: lifecycle.frame_id,
        frame_sequence: lifecycle.frame_sequence,
        frame_sha256: lifecycle.frame_sha256,
        client_bounds: lifecycle.client_bounds,
        event_identity_sha256: profile.event_identity_sha256.clone(),
        status: profile.status,
        progress: profile.progress.clone(),
        completion: profile.completion,
        visible_record_complete: true,
        facts: profile
            .facts
            .iter()
            .map(|fact| MtgoCompetitiveEventRecordVisibleFactV1 {
                kind: fact.kind,
                rect_client_px: fact.rect_client_px.clone(),
                content_sha256: fact.accepted_reference_sha256s[0].clone(),
                confidence_bps: fact.confidence_bps,
            })
            .collect(),
    };
    validate_visible_competitive_event_record_v1(
        &checked_lifecycle,
        approved_account_alias_sha256,
        record,
    )
    .map(|_| ())
    .map_err(|error| format!("event-record profile contract is invalid: {error}"))
}

#[cfg(target_os = "windows")]
fn event_record_fact_rank_v1(kind: MtgoCompetitiveEventRecordVisibleFactKindV1) -> u8 {
    use MtgoCompetitiveEventRecordVisibleFactKindV1::*;
    match kind {
        EventStatusVisible => 0,
        EventProgressVisible => 1,
        EventStandingVisible => 2,
        EventResultVisible => 3,
    }
}

#[cfg(target_os = "windows")]
fn validate_sideboard_assets_and_match_profile_v1<'a>(
    assets: &'a MtgoCompetitiveEventListingClassifierAssetsV1,
    selected_navigation_profile: &MtgoCompetitiveNavigationRegionProfileV1,
    header: &MtgoCompetitiveSideboardClassifierRequestHeaderV1,
    canonical_bgra8: &[u8],
) -> Result<&'a MtgoCompetitiveSideboardRegionProfileV1, String> {
    if assets.schema_version != 1
        || assets.scope != COMBINED_ASSET_SCOPE_V1
        || assets.canonical_pixel_format != PIXEL_FORMAT_V1
        || assets.sideboard_profiles.is_empty()
        || assets.sideboard_profiles.len() > MAX_PROFILES_V1
    {
        return Err("sideboard assets identity or profile count is invalid".to_owned());
    }
    let mut profile_ids = HashSet::new();
    let mut previous_profile_id: Option<&str> = None;
    let mut matching_profiles = Vec::new();
    for profile in &assets.sideboard_profiles {
        validate_identifier_v1(&profile.profile_id, "sideboard profile id")?;
        validate_identifier_v1(
            &profile.navigation_profile_id,
            "sideboard navigation profile id",
        )?;
        if !profile_ids.insert(profile.profile_id.as_str())
            || previous_profile_id.is_some_and(|previous| previous >= profile.profile_id.as_str())
            || profile.cards.is_empty()
            || profile.cards.len() > MAX_SIDEBOARD_CARD_PROFILES_V1
        {
            return Err("sideboard profile identity, card count, or order is invalid".to_owned());
        }
        previous_profile_id = Some(profile.profile_id.as_str());
        let navigation_matches = assets
            .navigation_profiles
            .iter()
            .filter(|candidate| candidate.profile_id == profile.navigation_profile_id)
            .collect::<Vec<_>>();
        if navigation_matches.len() != 1 {
            return Err("sideboard profile does not reference one navigation profile".to_owned());
        }
        let navigation_profile = navigation_matches[0];
        let manifest = validate_competitive_deck_manifest_v1(profile.deck_manifest.clone())
            .map_err(|error| format!("sideboard profile deck manifest is invalid: {error}"))?;
        validate_sideboard_zone_profile_v1("mainboard", &profile.mainboard_zone)?;
        validate_sideboard_zone_profile_v1("sideboard", &profile.sideboard_zone)?;
        let mut prior_card_key = None;
        for card in &profile.cards {
            let key = (card.partition, card.card_name.as_str());
            if prior_card_key.is_some_and(|prior| prior >= key)
                || card.card_name.is_empty()
                || card.card_name.len() > MAX_EXPECTED_LABEL_BYTES_V1
                || card.card_name.trim() != card.card_name
                || card.count == 0
                || !(9_500..=10_000).contains(&card.confidence_bps)
            {
                return Err("sideboard card profile is invalid or unordered".to_owned());
            }
            prior_card_key = Some(key);
            validate_reference_set_v1(
                &card.accepted_reference_sha256s,
                "sideboard card reference",
            )?;
        }
        validate_sideboard_profile_contract_v1(profile, navigation_profile)?;
        let mut profile_matches = profile.navigation_profile_id
            == selected_navigation_profile.profile_id
            && manifest.deck_list_sha256() == header.deck_list_sha256
            && manifest.manifest_commitment_sha256() == header.deck_manifest_commitment_sha256
            && manifest.format_sha256() == header.deck_format_sha256;
        let size = &navigation_profile.client_size_px;
        profile_matches &=
            sideboard_zone_profile_matches_v1(&profile.mainboard_zone, canonical_bgra8, size)?;
        profile_matches &=
            sideboard_zone_profile_matches_v1(&profile.sideboard_zone, canonical_bgra8, size)?;
        for card in &profile.cards {
            let observed =
                visible_frame_region_content_sha256_v1(canonical_bgra8, size, &card.rect_client_px)
                    .map_err(|error| format!("hash sideboard card profile region: {error}"))?;
            if card
                .accepted_reference_sha256s
                .binary_search(&observed)
                .is_err()
            {
                profile_matches = false;
            }
        }
        if profile_matches {
            matching_profiles.push(profile);
        }
    }
    if matching_profiles.len() != 1 {
        return Err(format!(
            "expected exactly one reviewed sideboard profile match, found {}",
            matching_profiles.len()
        ));
    }
    Ok(matching_profiles[0])
}

#[cfg(target_os = "windows")]
fn validate_sideboard_manifest_identity_v1(
    manifest: &mtgo_blackbox_v1::ValidatedMtgoCompetitiveDeckManifestV1,
    header: &MtgoCompetitiveSideboardClassifierRequestHeaderV1,
) -> Result<(), String> {
    if manifest.deck_list_sha256() != header.deck_list_sha256
        || manifest.manifest_commitment_sha256() != header.deck_manifest_commitment_sha256
        || manifest.format_sha256() != header.deck_format_sha256
    {
        return Err("sideboard profile and request deck identities differ".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_sideboard_zone_profile_v1(
    label: &str,
    profile: &MtgoCompetitiveSideboardZoneProfileV1,
) -> Result<(), String> {
    if !(9_500..=10_000).contains(&profile.confidence_bps) {
        return Err(format!("{label} sideboard zone confidence is invalid"));
    }
    validate_reference_set_v1(
        &profile.accepted_reference_sha256s,
        &format!("{label} sideboard zone reference"),
    )?;
    validate_reference_set_v1(
        &profile.accepted_empty_drop_reference_sha256s,
        &format!("{label} sideboard empty-drop reference"),
    )
}

#[cfg(target_os = "windows")]
fn sideboard_zone_profile_matches_v1(
    profile: &MtgoCompetitiveSideboardZoneProfileV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<bool, String> {
    let zone =
        visible_frame_region_content_sha256_v1(canonical_bgra8, size, &profile.rect_client_px)
            .map_err(|error| format!("hash sideboard zone profile region: {error}"))?;
    let empty_drop = visible_frame_region_content_sha256_v1(
        canonical_bgra8,
        size,
        &profile.empty_drop_rect_client_px,
    )
    .map_err(|error| format!("hash sideboard empty-drop profile region: {error}"))?;
    Ok(profile
        .accepted_reference_sha256s
        .binary_search(&zone)
        .is_ok()
        && profile
            .accepted_empty_drop_reference_sha256s
            .binary_search(&empty_drop)
            .is_ok())
}

#[cfg(target_os = "windows")]
fn validate_reference_set_v1(references: &[String], label: &str) -> Result<(), String> {
    if references.is_empty() || references.len() > MAX_CONTROL_REFERENCES_V1 {
        return Err(format!("{label} count is invalid"));
    }
    let mut previous: Option<&str> = None;
    for reference in references {
        validate_sha256_v1(reference, label)?;
        if previous.is_some_and(|value| value >= reference.as_str()) {
            return Err(format!("{label}s must be unique and sorted"));
        }
        previous = Some(reference.as_str());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_sideboard_profile_contract_v1(
    profile: &MtgoCompetitiveSideboardRegionProfileV1,
    navigation_profile: &MtgoCompetitiveNavigationRegionProfileV1,
) -> Result<(), String> {
    if navigation_profile.phase != MtgoCompetitiveLifecyclePhaseV1::Sideboarding
        || navigation_profile.event_identity_sha256.is_none()
        || navigation_profile.match_identity_sha256.is_none()
        || !matches!(navigation_profile.game_number, Some(1..=2))
        || navigation_profile.entry_terms.is_some()
    {
        return Err("sideboard profile source lifecycle is not between games".to_owned());
    }
    let lifecycle = MtgoVisibleCompetitiveLifecycleSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        snapshot_id: "sideboard-profile-source-v1".to_owned(),
        event_kind: navigation_profile.event_kind,
        phase: navigation_profile.phase,
        frame_id: 1,
        frame_sequence: 1,
        frame_sha256: "0".repeat(64),
        client_bounds: MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: navigation_profile.client_size_px.width,
            height: navigation_profile.client_size_px.height,
        },
        event_identity_sha256: navigation_profile.event_identity_sha256.clone(),
        match_identity_sha256: navigation_profile.match_identity_sha256.clone(),
        game_number: navigation_profile.game_number,
        entry_terms: None,
        visible_state_complete: true,
        facts: navigation_profile
            .facts
            .iter()
            .map(|fact| MtgoLifecycleVisibleFactV1 {
                kind: fact.kind,
                rect_client_px: fact.rect_client_px.clone(),
                content_sha256: fact.accepted_reference_sha256s[0].clone(),
                confidence_bps: fact.confidence_bps,
            })
            .collect(),
    };
    let checked_lifecycle =
        validate_visible_competitive_lifecycle_snapshot_v1(lifecycle.clone())
            .map_err(|error| format!("sideboard profile lifecycle is invalid: {error}"))?;
    let manifest = validate_competitive_deck_manifest_v1(profile.deck_manifest.clone())
        .map_err(|error| format!("sideboard profile deck manifest is invalid: {error}"))?;
    let policy_deployment_commitment_sha256 = distinct_sha256_v1(&[
        manifest.deck_list_sha256(),
        manifest.manifest_commitment_sha256(),
        manifest.format_sha256(),
    ])?;
    let sideboard = MtgoVisibleCompetitiveSideboardSnapshotV1 {
        schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
        snapshot_id: "sideboard-profile-contract-v1".to_owned(),
        event_kind: navigation_profile.event_kind,
        event_identity_sha256: navigation_profile
            .event_identity_sha256
            .clone()
            .ok_or("sideboard profile event identity is absent")?,
        match_identity_sha256: navigation_profile
            .match_identity_sha256
            .clone()
            .ok_or("sideboard profile match identity is absent")?,
        game_number: navigation_profile
            .game_number
            .ok_or("sideboard profile game number is absent")?,
        frame_id: lifecycle.frame_id,
        frame_sequence: lifecycle.frame_sequence,
        frame_sha256: lifecycle.frame_sha256,
        lifecycle_snapshot_commitment_sha256: checked_lifecycle
            .snapshot_commitment_sha256()
            .to_owned(),
        deck_manifest_commitment_sha256: manifest.manifest_commitment_sha256().to_owned(),
        policy_deployment_commitment_sha256,
        visible_configuration_complete: true,
        mainboard_zone: sideboard_zone_from_first_reference_v1(&profile.mainboard_zone),
        sideboard_zone: sideboard_zone_from_first_reference_v1(&profile.sideboard_zone),
        cards: profile
            .cards
            .iter()
            .map(sideboard_card_from_first_reference_v1)
            .collect(),
    };
    validate_visible_competitive_sideboard_snapshot_v1(checked_lifecycle, &manifest, sideboard)
        .map(|_| ())
        .map_err(|error| format!("sideboard profile contract is invalid: {error}"))
}

#[cfg(target_os = "windows")]
fn sideboard_zone_from_first_reference_v1(
    profile: &MtgoCompetitiveSideboardZoneProfileV1,
) -> MtgoVisibleCompetitiveSideboardZoneV1 {
    MtgoVisibleCompetitiveSideboardZoneV1 {
        rect_client_px: profile.rect_client_px.clone(),
        content_sha256: profile.accepted_reference_sha256s[0].clone(),
        empty_drop_rect_client_px: profile.empty_drop_rect_client_px.clone(),
        empty_drop_content_sha256: profile.accepted_empty_drop_reference_sha256s[0].clone(),
        confidence_bps: profile.confidence_bps,
    }
}

#[cfg(target_os = "windows")]
fn sideboard_card_from_first_reference_v1(
    profile: &MtgoCompetitiveSideboardCardProfileV1,
) -> MtgoVisibleCompetitiveSideboardCardV1 {
    MtgoVisibleCompetitiveSideboardCardV1 {
        partition: profile.partition,
        card_name: profile.card_name.clone(),
        count: profile.count,
        rect_client_px: profile.rect_client_px.clone(),
        content_sha256: profile.accepted_reference_sha256s[0].clone(),
        confidence_bps: profile.confidence_bps,
    }
}

#[cfg(target_os = "windows")]
fn distinct_sha256_v1(excluded: &[&str]) -> Result<String, String> {
    for byte in b'0'..=b'9' {
        let candidate = char::from(byte).to_string().repeat(64);
        if !excluded.contains(&candidate.as_str()) {
            return Ok(candidate);
        }
    }
    Err("could not construct a distinct structural sideboard identity".to_owned())
}

#[cfg(target_os = "windows")]
fn recognize_words_v1(
    width: u32,
    height: u32,
    canonical_bgra8: &[u8],
) -> Result<Vec<OcrWordV1>, String> {
    let _com = ComGuardV1::initialize_v1()?;
    let writer = DataWriter::new().map_err(|error| format!("create OCR buffer: {error}"))?;
    writer
        .WriteBytes(canonical_bgra8)
        .map_err(|error| format!("write OCR buffer: {error}"))?;
    let buffer = writer
        .DetachBuffer()
        .map_err(|error| format!("detach OCR buffer: {error}"))?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        i32::try_from(width).map_err(|_| "OCR width exceeds i32".to_owned())?,
        i32::try_from(height).map_err(|_| "OCR height exceeds i32".to_owned())?,
    )
    .map_err(|error| format!("create OCR software bitmap: {error}"))?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|error| format!("create Windows OCR engine: {error}"))?;
    let maximum =
        OcrEngine::MaxImageDimension().map_err(|error| format!("read OCR image limit: {error}"))?;
    if width.max(height) > maximum {
        return Err("image exceeds the Windows OCR dimension limit".to_owned());
    }
    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(|error| format!("start Windows OCR: {error}"))?
        .join()
        .map_err(|error| format!("complete Windows OCR: {error}"))?;
    let lines = result
        .Lines()
        .map_err(|error| format!("read OCR lines: {error}"))?;
    let mut words = Vec::new();
    for line_index in 0..lines
        .Size()
        .map_err(|error| format!("read OCR line count: {error}"))?
    {
        let line = lines
            .GetAt(line_index)
            .map_err(|error| format!("read OCR line {line_index}: {error}"))?;
        let line_words = line
            .Words()
            .map_err(|error| format!("read OCR words for line {line_index}: {error}"))?;
        for word_index in 0..line_words
            .Size()
            .map_err(|error| format!("read OCR word count for line {line_index}: {error}"))?
        {
            let word = line_words
                .GetAt(word_index)
                .map_err(|error| format!("read OCR word {line_index}:{word_index}: {error}"))?;
            let normalized = normalize_visible_text_v1(
                &word
                    .Text()
                    .map_err(|error| format!("read OCR word text: {error}"))?
                    .to_string(),
            );
            if normalized.is_empty() {
                continue;
            }
            let raw_rect = word
                .BoundingRect()
                .map_err(|error| format!("read OCR word rectangle: {error}"))?;
            words.push(OcrWordV1 {
                normalized,
                rect: checked_ocr_rect_v1(
                    raw_rect.X,
                    raw_rect.Y,
                    raw_rect.Width,
                    raw_rect.Height,
                    width,
                    height,
                )?,
            });
        }
    }
    Ok(words)
}

#[cfg(target_os = "windows")]
fn find_exact_token_sequence_v1(words: &[OcrWordV1], expected: &[String]) -> Vec<MtgoRectPxV1> {
    if expected.is_empty() || expected.len() > words.len() {
        return Vec::new();
    }
    words
        .windows(expected.len())
        .filter(|window| {
            window
                .iter()
                .zip(expected)
                .all(|(word, expected)| word.normalized == *expected)
        })
        .filter_map(union_word_rects_v1)
        .collect()
}

#[cfg(target_os = "windows")]
fn union_word_rects_v1(words: &[OcrWordV1]) -> Option<MtgoRectPxV1> {
    let first = &words.first()?.rect;
    let mut left = first.x;
    let mut top = first.y;
    let mut right = first.x.checked_add(first.width)?;
    let mut bottom = first.y.checked_add(first.height)?;
    for word in &words[1..] {
        left = left.min(word.rect.x);
        top = top.min(word.rect.y);
        right = right.max(word.rect.x.checked_add(word.rect.width)?);
        bottom = bottom.max(word.rect.y.checked_add(word.rect.height)?);
    }
    Some(MtgoRectPxV1 {
        x: left,
        y: top,
        width: right.checked_sub(left)?,
        height: bottom.checked_sub(top)?,
    })
}

#[cfg(target_os = "windows")]
fn checked_ocr_rect_v1(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    image_width: u32,
    image_height: u32,
) -> Result<MtgoRectPxV1, String> {
    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || x < 0.0
        || y < 0.0
        || width <= 0.0
        || height <= 0.0
    {
        return Err("Windows OCR returned an invalid word rectangle".to_owned());
    }
    let left = x.floor() as u32;
    let top = y.floor() as u32;
    let right = (x + width).ceil() as u32;
    let bottom = (y + height).ceil() as u32;
    if right > image_width || bottom > image_height || right <= left || bottom <= top {
        return Err("Windows OCR returned an out-of-bounds word rectangle".to_owned());
    }
    Ok(MtgoRectPxV1 {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

#[cfg(target_os = "windows")]
fn normalized_tokens_v1(value: &str) -> Vec<String> {
    normalize_visible_text_v1(value)
        .split(' ')
        .map(str::to_owned)
        .collect()
}

#[cfg(target_os = "windows")]
fn normalize_visible_text_v1(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(target_os = "windows")]
fn rect_inside_v1(rect: &MtgoRectPxV1, bounds: &MtgoRectPxV1) -> bool {
    let Some(right) = rect.x.checked_add(rect.width) else {
        return false;
    };
    let Some(bottom) = rect.y.checked_add(rect.height) else {
        return false;
    };
    let Some(bounds_right) = bounds.x.checked_add(bounds.width) else {
        return false;
    };
    let Some(bounds_bottom) = bounds.y.checked_add(bounds.height) else {
        return false;
    };
    rect.width > 0
        && rect.height > 0
        && rect.x >= bounds.x
        && rect.y >= bounds.y
        && right <= bounds_right
        && bottom <= bounds_bottom
}

#[cfg(target_os = "windows")]
fn rects_overlap_v1(first: &MtgoRectPxV1, second: &MtgoRectPxV1) -> bool {
    let Some(first_right) = first.x.checked_add(first.width) else {
        return true;
    };
    let Some(first_bottom) = first.y.checked_add(first.height) else {
        return true;
    };
    let Some(second_right) = second.x.checked_add(second.width) else {
        return true;
    };
    let Some(second_bottom) = second.y.checked_add(second.height) else {
        return true;
    };
    first.x < second_right
        && second.x < first_right
        && first.y < second_bottom
        && second.y < first_bottom
}

#[cfg(target_os = "windows")]
fn target_commitment_v1(target: &MtgoCompetitiveEventListingTargetV1) -> Result<String, String> {
    let target_json = serde_json::to_vec(target)
        .map_err(|error| format!("serialize selected-listing target: {error}"))?;
    Ok(commitment_v1(
        TARGET_COMMITMENT_DOMAIN_V1,
        &[target_json.as_slice()],
    ))
}

#[cfg(target_os = "windows")]
fn validate_target_v1(target: &MtgoCompetitiveEventListingTargetV1) -> Result<(), String> {
    if target.schema_version != MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1 {
        return Err("selected-listing target schema is invalid".to_owned());
    }
    validate_identifier_v1(&target.target_id, "target id")?;
    for digest in [
        target.approved_account_alias_sha256.as_str(),
        target.event_identity_sha256.as_str(),
        target.event_display_label_sha256.as_str(),
        target.deck_display_label_sha256.as_str(),
        target.deck_list_sha256.as_str(),
        target.deck_manifest_commitment_sha256.as_str(),
        target.deck_format_sha256.as_str(),
        target.policy_deployment_commitment_sha256.as_str(),
    ] {
        validate_sha256_v1(digest, "target commitment")?;
    }
    if target.policy_deployment_commitment_sha256 == target.deck_list_sha256
        || target.policy_deployment_commitment_sha256 == target.deck_manifest_commitment_sha256
        || target.policy_deployment_commitment_sha256 == target.deck_format_sha256
    {
        return Err("selected-listing target deployment identities are not distinct".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn header_digests_v1(header: &MtgoCompetitiveEventListingClassifierRequestHeaderV1) -> [&str; 12] {
    [
        &header.canonical_bgra8_sha256,
        &header.source_capture_commitment_sha256,
        &header.source_frame_profile_binding_sha256,
        &header.source_navigation_classification_result_commitment_sha256,
        &header.source_lifecycle_snapshot_commitment_sha256,
        &header.navigation_profile_commitment_sha256,
        &header.navigation_profile_admission_commitment_sha256,
        &header.approved_account_alias_sha256,
        &header.runtime_identity_commitment_sha256,
        &header.classifier_binary_sha256,
        &header.classifier_assets_manifest_sha256,
        &header.target_commitment_sha256,
    ]
}

#[cfg(target_os = "windows")]
fn deck_gate_header_digests_v1(
    header: &MtgoCompetitiveDeckGateClassifierRequestHeaderV1,
) -> [&str; 12] {
    [
        &header.canonical_bgra8_sha256,
        &header.source_capture_commitment_sha256,
        &header.source_frame_profile_binding_sha256,
        &header.source_navigation_classification_result_commitment_sha256,
        &header.source_lifecycle_snapshot_commitment_sha256,
        &header.navigation_profile_commitment_sha256,
        &header.navigation_profile_admission_commitment_sha256,
        &header.approved_account_alias_sha256,
        &header.runtime_identity_commitment_sha256,
        &header.classifier_binary_sha256,
        &header.classifier_assets_manifest_sha256,
        &header.target_commitment_sha256,
    ]
}

#[cfg(target_os = "windows")]
fn deck_chooser_header_digests_v1(
    header: &MtgoCompetitiveDeckChooserClassifierRequestHeaderV1,
) -> [&str; 13] {
    [
        &header.canonical_bgra8_sha256,
        &header.source_capture_commitment_sha256,
        &header.source_frame_profile_binding_sha256,
        &header.source_deck_gate_observation_commitment_sha256,
        &header.source_deck_gate_result_commitment_sha256,
        &header.source_window_continuity_commitment_sha256,
        &header.navigation_profile_commitment_sha256,
        &header.navigation_profile_admission_commitment_sha256,
        &header.approved_account_alias_sha256,
        &header.runtime_identity_commitment_sha256,
        &header.classifier_binary_sha256,
        &header.classifier_assets_manifest_sha256,
        &header.target_commitment_sha256,
    ]
}

#[cfg(target_os = "windows")]
fn navigation_header_digests_v1(
    header: &MtgoCompetitiveNavigationClassifierRequestHeaderV1,
) -> [&str; 11] {
    [
        &header.canonical_bgra8_sha256,
        &header.source_manifest_sha256,
        &header.source_capture_commitment_sha256,
        &header.source_profile_binding_sha256,
        &header.source_frame_profile_binding_sha256,
        &header.navigation_profile_commitment_sha256,
        &header.navigation_profile_admission_commitment_sha256,
        &header.approved_account_alias_sha256,
        &header.runtime_identity_commitment_sha256,
        &header.classifier_binary_sha256,
        &header.classifier_assets_manifest_sha256,
    ]
}

#[cfg(target_os = "windows")]
fn event_record_header_digests_v1(
    header: &MtgoCompetitiveEventRecordClassifierRequestHeaderV1,
) -> [&str; 9] {
    [
        &header.canonical_bgra8_sha256,
        &header.source_capture_commitment_sha256,
        &header.source_frame_profile_binding_sha256,
        &header.navigation_profile_commitment_sha256,
        &header.navigation_profile_admission_commitment_sha256,
        &header.approved_account_alias_sha256,
        &header.runtime_identity_commitment_sha256,
        &header.classifier_binary_sha256,
        &header.classifier_assets_manifest_sha256,
    ]
}

#[cfg(target_os = "windows")]
fn sideboard_header_digests_v1(
    header: &MtgoCompetitiveSideboardClassifierRequestHeaderV1,
) -> [&str; 15] {
    [
        &header.canonical_bgra8_sha256,
        &header.source_capture_commitment_sha256,
        &header.source_frame_profile_binding_sha256,
        &header.source_navigation_classification_result_commitment_sha256,
        &header.source_lifecycle_snapshot_commitment_sha256,
        &header.navigation_profile_commitment_sha256,
        &header.navigation_profile_admission_commitment_sha256,
        &header.approved_account_alias_sha256,
        &header.runtime_identity_commitment_sha256,
        &header.classifier_binary_sha256,
        &header.classifier_assets_manifest_sha256,
        &header.deck_list_sha256,
        &header.deck_manifest_commitment_sha256,
        &header.deck_format_sha256,
        &header.policy_deployment_commitment_sha256,
    ]
}

#[cfg(target_os = "windows")]
fn parse_canonical_json_v1<T>(bytes: &[u8], label: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    let value = serde_json::from_slice(bytes).map_err(|error| format!("parse {label}: {error}"))?;
    let canonical = serde_json::to_vec(&value)
        .map_err(|error| format!("serialize canonical {label}: {error}"))?;
    if canonical != bytes {
        return Err(format!("{label} is not canonical JSON"));
    }
    Ok(value)
}

#[cfg(target_os = "windows")]
fn read_u64_be_v1(reader: &mut impl Read, label: &str) -> Result<u64, String> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("read {label}: {error}"))?;
    Ok(u64::from_be_bytes(bytes))
}

#[cfg(target_os = "windows")]
fn bounded_usize_v1(value: u64, maximum: usize, label: &str) -> Result<usize, String> {
    let value = usize::try_from(value).map_err(|_| format!("{label} length does not fit"))?;
    if value == 0 || value > maximum {
        return Err(format!("{label} length is outside bounds"));
    }
    Ok(value)
}

#[cfg(target_os = "windows")]
fn read_exact_vec_v1(
    reader: &mut impl Read,
    length: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0_u8; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("read {label}: {error}"))?;
    Ok(bytes)
}

#[cfg(target_os = "windows")]
fn hash_bounded_file_v1(path: &Path, maximum: u64, label: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("open {label}: {error}"))?;
    let length = file
        .metadata()
        .map_err(|error| format!("inspect {label}: {error}"))?
        .len();
    if length == 0 || length > maximum {
        return Err(format!("{label} length is outside bounds"));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("read {label}: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(target_os = "windows")]
fn read_bounded_file_bytes_v1(path: &Path, maximum: u64, label: &str) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|error| format!("open {label}: {error}"))?;
    let length = file
        .metadata()
        .map_err(|error| format!("inspect {label}: {error}"))?
        .len();
    if length == 0 || length > maximum {
        return Err(format!("{label} length is outside bounds"));
    }
    let length = usize::try_from(length).map_err(|_| format!("{label} length does not fit"))?;
    let mut bytes = vec![0_u8; length];
    file.read_exact(&mut bytes)
        .map_err(|error| format!("read {label}: {error}"))?;
    Ok(bytes)
}

#[cfg(target_os = "windows")]
fn validate_identifier_v1(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn validate_sha256_v1(value: &str, label: &str) -> Result<(), String> {
    if !looks_like_sha256_v1(value) {
        return Err(format!("{label} must be lowercase SHA-256"));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn looks_like_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(target_os = "windows")]
fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(target_os = "windows")]
fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(target_os = "windows")]
fn commitment_be_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    fn digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn target_v1(
        label: &str,
        kind: MtgoCompetitiveEventKindV1,
    ) -> MtgoCompetitiveEventListingTargetV1 {
        MtgoCompetitiveEventListingTargetV1 {
            schema_version: 1,
            target_id: "test-target-v1".to_owned(),
            event_kind: kind,
            approved_account_alias_sha256: digest('a'),
            event_identity_sha256: digest('b'),
            event_display_label_sha256: sha256_hex_v1(label.as_bytes()),
            deck_display_label_sha256: digest('9'),
            deck_list_sha256: digest('c'),
            deck_manifest_commitment_sha256: digest('d'),
            deck_format_sha256: digest('e'),
            policy_deployment_commitment_sha256: digest('f'),
        }
    }

    fn header_v1(
        pixels: &[u8],
        assets_sha256: String,
        label: &str,
    ) -> MtgoCompetitiveEventListingClassifierRequestHeaderV1 {
        let target = target_v1(label, MtgoCompetitiveEventKindV1::League);
        MtgoCompetitiveEventListingClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: EVENT_LISTING_REQUEST_PROTOCOL_V1.to_owned(),
            parser_scope: REQUEST_SCOPE_V1.to_owned(),
            frame_id: 7,
            frame_sequence: 9,
            captured_at_unix_millis: 11,
            canonical_width: 32,
            canonical_height: 16,
            canonical_stride: 128,
            canonical_byte_length: pixels.len() as u64,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: digest('1'),
            source_frame_profile_binding_sha256: digest('2'),
            source_navigation_classification_result_commitment_sha256: digest('3'),
            source_lifecycle_snapshot_commitment_sha256: digest('4'),
            navigation_profile_commitment_sha256: digest('5'),
            navigation_profile_admission_commitment_sha256: digest('6'),
            approved_account_alias_sha256: target.approved_account_alias_sha256.clone(),
            runtime_identity_commitment_sha256: digest('7'),
            classifier_binary_sha256: digest('8'),
            classifier_assets_manifest_sha256: assets_sha256,
            target_commitment_sha256: target_commitment_v1(&target).unwrap(),
            target,
        }
    }

    fn profile_v1(pixels: &[u8], label: &str) -> MtgoCompetitiveEventListingOcrProfileV1 {
        let control = MtgoRectPxV1 {
            x: 20,
            y: 4,
            width: 8,
            height: 4,
        };
        MtgoCompetitiveEventListingOcrProfileV1 {
            profile_id: "modern-league-32x16-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_display_label_sha256: sha256_hex_v1(label.as_bytes()),
            expected_visible_label: label.to_owned(),
            client_size_px: MtgoSizePxV1 {
                width: 32,
                height: 16,
            },
            label_search_rect_client_px: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            open_entry_review_control_rect_client_px: control.clone(),
            enabled_control_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                pixels,
                &MtgoSizePxV1 {
                    width: 32,
                    height: 16,
                },
                &control,
            )
            .unwrap()],
            confidence_bps: 9_500,
        }
    }

    fn words_v1() -> Vec<OcrWordV1> {
        vec![
            OcrWordV1 {
                normalized: "Modern".to_owned(),
                rect: MtgoRectPxV1 {
                    x: 2,
                    y: 4,
                    width: 5,
                    height: 3,
                },
            },
            OcrWordV1 {
                normalized: "League".to_owned(),
                rect: MtgoRectPxV1 {
                    x: 8,
                    y: 4,
                    width: 6,
                    height: 3,
                },
            },
        ]
    }

    fn navigation_header_v1(
        pixels: &[u8],
        assets_sha256: String,
    ) -> MtgoCompetitiveNavigationClassifierRequestHeaderV1 {
        MtgoCompetitiveNavigationClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: NAVIGATION_REQUEST_PROTOCOL_V1.to_owned(),
            frame_id: 17,
            frame_sequence: 19,
            captured_at_unix_millis: 23,
            canonical_width: 32,
            canonical_height: 16,
            canonical_stride: 128,
            canonical_byte_length: pixels.len() as u64,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_manifest_sha256: digest('1'),
            source_capture_commitment_sha256: digest('2'),
            source_profile_binding_sha256: digest('3'),
            source_frame_profile_binding_sha256: digest('4'),
            navigation_profile_commitment_sha256: digest('5'),
            navigation_profile_admission_commitment_sha256: digest('6'),
            approved_account_alias_sha256: digest('7'),
            runtime_identity_commitment_sha256: digest('8'),
            classifier_binary_sha256: digest('9'),
            classifier_assets_manifest_sha256: assets_sha256,
        }
    }

    fn event_record_header_v1(
        pixels: &[u8],
        assets_sha256: String,
    ) -> MtgoCompetitiveEventRecordClassifierRequestHeaderV1 {
        MtgoCompetitiveEventRecordClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: EVENT_RECORD_REQUEST_PROTOCOL_V1.to_owned(),
            frame_id: 29,
            frame_sequence: 31,
            captured_at_unix_millis: 37,
            canonical_width: 32,
            canonical_height: 16,
            canonical_stride: 128,
            canonical_byte_length: pixels.len() as u64,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: digest('1'),
            source_frame_profile_binding_sha256: digest('2'),
            parser_scope: EVENT_RECORD_REQUEST_SCOPE_V1.to_owned(),
            navigation_profile_commitment_sha256: digest('3'),
            navigation_profile_admission_commitment_sha256: digest('4'),
            approved_account_alias_sha256: digest('5'),
            runtime_identity_commitment_sha256: digest('6'),
            classifier_binary_sha256: digest('7'),
            classifier_assets_manifest_sha256: assets_sha256,
        }
    }

    fn navigation_profile_v1(pixels: &[u8]) -> MtgoCompetitiveNavigationRegionProfileV1 {
        let size = MtgoSizePxV1 {
            width: 32,
            height: 16,
        };
        let pairing = MtgoRectPxV1 {
            x: 1,
            y: 1,
            width: 12,
            height: 5,
        };
        let accept = MtgoRectPxV1 {
            x: 20,
            y: 4,
            width: 8,
            height: 4,
        };
        MtgoCompetitiveNavigationRegionProfileV1 {
            profile_id: "challenge-pairing-ready-32x16-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            phase: MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            client_size_px: size.clone(),
            event_identity_sha256: Some(digest('a')),
            match_identity_sha256: Some(digest('b')),
            game_number: None,
            entry_terms: None,
            facts: vec![
                MtgoCompetitiveNavigationFactProfileV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::PairingVisible,
                    rect_client_px: pairing.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        pixels, &size, &pairing,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                },
                MtgoCompetitiveNavigationFactProfileV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::PairingAcceptControlEnabled,
                    rect_client_px: accept.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        pixels, &size, &accept,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                },
            ],
        }
    }

    fn event_record_profile_v1(pixels: &[u8]) -> MtgoCompetitiveEventRecordRegionProfileV1 {
        let size = MtgoSizePxV1 {
            width: 32,
            height: 16,
        };
        let status = MtgoRectPxV1 {
            x: 1,
            y: 10,
            width: 12,
            height: 3,
        };
        let progress = MtgoRectPxV1 {
            x: 16,
            y: 10,
            width: 12,
            height: 3,
        };
        MtgoCompetitiveEventRecordRegionProfileV1 {
            profile_id: "challenge-pairing-record-32x16-v1".to_owned(),
            navigation_profile_id: "challenge-pairing-ready-32x16-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            lifecycle_phase: MtgoCompetitiveLifecyclePhaseV1::PairingReady,
            event_identity_sha256: digest('a'),
            status: MtgoCompetitiveEventVisibleStatusV1::PairingReady,
            progress: MtgoCompetitiveEventProgressV1::Challenge {
                match_record: MtgoCompetitiveMatchRecordV1 {
                    wins: 0,
                    losses: 0,
                    draws: 0,
                    matches_completed: 0,
                },
                rounds_completed: 0,
                rounds_total: 8,
                match_points: 0,
                standing_rank: None,
                field_size: None,
            },
            completion: None,
            facts: vec![
                MtgoCompetitiveEventRecordFactProfileV1 {
                    kind: MtgoCompetitiveEventRecordVisibleFactKindV1::EventStatusVisible,
                    rect_client_px: status.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        pixels, &size, &status,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                },
                MtgoCompetitiveEventRecordFactProfileV1 {
                    kind: MtgoCompetitiveEventRecordVisibleFactKindV1::EventProgressVisible,
                    rect_client_px: progress.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        pixels, &size, &progress,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                },
            ],
        }
    }

    fn sideboard_fixture_assets_v1(pixels: &[u8]) -> MtgoCompetitiveEventListingClassifierAssetsV1 {
        let size = MtgoSizePxV1 {
            width: 64,
            height: 48,
        };
        let lifecycle_facts = [
            (
                MtgoLifecycleVisibleFactKindV1::SideboardSurfaceVisible,
                MtgoRectPxV1 {
                    x: 1,
                    y: 1,
                    width: 10,
                    height: 6,
                },
            ),
            (
                MtgoLifecycleVisibleFactKindV1::SideboardTimerVisible,
                MtgoRectPxV1 {
                    x: 13,
                    y: 1,
                    width: 10,
                    height: 6,
                },
            ),
            (
                MtgoLifecycleVisibleFactKindV1::SideboardConfigurationVisible,
                MtgoRectPxV1 {
                    x: 25,
                    y: 1,
                    width: 10,
                    height: 6,
                },
            ),
            (
                MtgoLifecycleVisibleFactKindV1::SideboardSubmitControlEnabled,
                MtgoRectPxV1 {
                    x: 37,
                    y: 1,
                    width: 10,
                    height: 6,
                },
            ),
        ]
        .into_iter()
        .map(|(kind, rect)| MtgoCompetitiveNavigationFactProfileV1 {
            kind,
            accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                pixels, &size, &rect,
            )
            .unwrap()],
            rect_client_px: rect,
            confidence_bps: 9_500,
        })
        .collect();
        let mainboard_zone_rect = MtgoRectPxV1 {
            x: 0,
            y: 12,
            width: 64,
            height: 15,
        };
        let mainboard_empty_rect = MtgoRectPxV1 {
            x: 50,
            y: 14,
            width: 8,
            height: 8,
        };
        let sideboard_zone_rect = MtgoRectPxV1 {
            x: 0,
            y: 30,
            width: 64,
            height: 15,
        };
        let sideboard_empty_rect = MtgoRectPxV1 {
            x: 50,
            y: 32,
            width: 8,
            height: 8,
        };
        let mainboard_card_rect = MtgoRectPxV1 {
            x: 2,
            y: 14,
            width: 8,
            height: 8,
        };
        let sideboard_card_rect = MtgoRectPxV1 {
            x: 2,
            y: 32,
            width: 8,
            height: 8,
        };
        let region_hash = |rect: &MtgoRectPxV1| {
            visible_frame_region_content_sha256_v1(pixels, &size, rect).unwrap()
        };
        MtgoCompetitiveEventListingClassifierAssetsV1 {
            schema_version: 1,
            scope: COMBINED_ASSET_SCOPE_V1.to_owned(),
            canonical_pixel_format: PIXEL_FORMAT_V1.to_owned(),
            profiles: Vec::new(),
            deck_gate_profiles: Vec::new(),
            deck_chooser_profiles: Vec::new(),
            navigation_profiles: vec![MtgoCompetitiveNavigationRegionProfileV1 {
                profile_id: "league-sideboard-64x48-v1".to_owned(),
                event_kind: MtgoCompetitiveEventKindV1::League,
                phase: MtgoCompetitiveLifecyclePhaseV1::Sideboarding,
                client_size_px: size.clone(),
                event_identity_sha256: Some(digest('a')),
                match_identity_sha256: Some(digest('b')),
                game_number: Some(1),
                entry_terms: None,
                facts: lifecycle_facts,
            }],
            event_record_profiles: Vec::new(),
            sideboard_profiles: vec![MtgoCompetitiveSideboardRegionProfileV1 {
                profile_id: "league-sideboard-deck-64x48-v1".to_owned(),
                navigation_profile_id: "league-sideboard-64x48-v1".to_owned(),
                deck_manifest: MtgoCompetitiveDeckManifestV1 {
                    schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
                    deck_list_sha256: digest('1'),
                    format_sha256: digest('2'),
                    starting_mainboard_count: 1,
                    starting_sideboard_count: 1,
                    configuration: mtgo_blackbox_v1::MtgoCompetitiveDeckConfigurationV1 {
                        mainboard: vec![mtgo_blackbox_v1::MtgoCompetitiveDeckCardCountV1 {
                            card_db_id: 66,
                            card_name: "Lightning Bolt".to_owned(),
                            count: 1,
                        }],
                        sideboard: vec![mtgo_blackbox_v1::MtgoCompetitiveDeckCardCountV1 {
                            card_db_id: 101,
                            card_name: "Searing Blaze".to_owned(),
                            count: 1,
                        }],
                    },
                },
                mainboard_zone: MtgoCompetitiveSideboardZoneProfileV1 {
                    rect_client_px: mainboard_zone_rect.clone(),
                    accepted_reference_sha256s: vec![region_hash(&mainboard_zone_rect)],
                    empty_drop_rect_client_px: mainboard_empty_rect.clone(),
                    accepted_empty_drop_reference_sha256s: vec![region_hash(&mainboard_empty_rect)],
                    confidence_bps: 9_500,
                },
                sideboard_zone: MtgoCompetitiveSideboardZoneProfileV1 {
                    rect_client_px: sideboard_zone_rect.clone(),
                    accepted_reference_sha256s: vec![region_hash(&sideboard_zone_rect)],
                    empty_drop_rect_client_px: sideboard_empty_rect.clone(),
                    accepted_empty_drop_reference_sha256s: vec![region_hash(&sideboard_empty_rect)],
                    confidence_bps: 9_500,
                },
                cards: vec![
                    MtgoCompetitiveSideboardCardProfileV1 {
                        partition: MtgoCompetitiveDeckPartitionV1::Mainboard,
                        card_name: "Lightning Bolt".to_owned(),
                        count: 1,
                        rect_client_px: mainboard_card_rect.clone(),
                        accepted_reference_sha256s: vec![region_hash(&mainboard_card_rect)],
                        confidence_bps: 9_500,
                    },
                    MtgoCompetitiveSideboardCardProfileV1 {
                        partition: MtgoCompetitiveDeckPartitionV1::Sideboard,
                        card_name: "Searing Blaze".to_owned(),
                        count: 1,
                        rect_client_px: sideboard_card_rect.clone(),
                        accepted_reference_sha256s: vec![region_hash(&sideboard_card_rect)],
                        confidence_bps: 9_500,
                    },
                ],
            }],
        }
    }

    fn sideboard_header_v1(
        pixels: &[u8],
        assets: &MtgoCompetitiveEventListingClassifierAssetsV1,
    ) -> MtgoCompetitiveSideboardClassifierRequestHeaderV1 {
        let navigation = &assets.navigation_profiles[0];
        let lifecycle = build_navigation_lifecycle_from_frame_v1(
            41,
            43,
            64,
            48,
            &sha256_hex_v1(pixels),
            navigation,
            pixels,
        )
        .unwrap();
        let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(lifecycle).unwrap();
        let manifest = validate_competitive_deck_manifest_v1(
            assets.sideboard_profiles[0].deck_manifest.clone(),
        )
        .unwrap();
        let assets_bytes = serde_json::to_vec(assets).unwrap();
        MtgoCompetitiveSideboardClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: SIDEBOARD_REQUEST_PROTOCOL_V1.to_owned(),
            parser_scope: SIDEBOARD_REQUEST_SCOPE_V1.to_owned(),
            frame_id: 41,
            frame_sequence: 43,
            captured_at_unix_millis: 47,
            canonical_width: 64,
            canonical_height: 48,
            canonical_stride: 256,
            canonical_byte_length: pixels.len() as u64,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: digest('3'),
            source_frame_profile_binding_sha256: digest('4'),
            source_navigation_classification_result_commitment_sha256: digest('5'),
            source_lifecycle_snapshot_commitment_sha256: lifecycle
                .snapshot_commitment_sha256()
                .to_owned(),
            navigation_profile_commitment_sha256: digest('6'),
            navigation_profile_admission_commitment_sha256: digest('7'),
            approved_account_alias_sha256: digest('8'),
            runtime_identity_commitment_sha256: digest('9'),
            classifier_binary_sha256: digest('c'),
            classifier_assets_manifest_sha256: sha256_hex_v1(&assets_bytes),
            deck_list_sha256: manifest.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: manifest.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: manifest.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('f'),
        }
    }

    fn combined_assets_v1(pixels: &[u8]) -> MtgoCompetitiveEventListingClassifierAssetsV1 {
        MtgoCompetitiveEventListingClassifierAssetsV1 {
            schema_version: 1,
            scope: COMBINED_ASSET_SCOPE_V1.to_owned(),
            canonical_pixel_format: PIXEL_FORMAT_V1.to_owned(),
            profiles: Vec::new(),
            deck_gate_profiles: Vec::new(),
            deck_chooser_profiles: Vec::new(),
            navigation_profiles: vec![navigation_profile_v1(pixels)],
            event_record_profiles: vec![event_record_profile_v1(pixels)],
            sideboard_profiles: Vec::new(),
        }
    }

    #[test]
    fn exact_label_and_reviewed_enabled_control_produce_one_bound_selection() {
        let pixels = (0..32 * 16 * 4)
            .map(|index| ((index * 13 + 7) % 251) as u8)
            .collect::<Vec<_>>();
        let label = "Modern League";
        let profile = profile_v1(&pixels, label);
        let header = header_v1(&pixels, digest('9'), label);
        let response =
            classify_from_words_v1(&header, &profile, &pixels, &words_v1(), digest('0')).unwrap();
        assert_eq!(
            response.selection.event_kind,
            MtgoCompetitiveEventKindV1::League
        );
        assert_eq!(response.selection.confidence_bps, 9_500);
        assert!(response.selection.open_entry_review_control_enabled);
        assert_eq!(response.selection.event_label_rect_client_px.x, 2);
    }

    #[test]
    fn duplicate_label_or_unreviewed_control_fails_closed() {
        let pixels = vec![17_u8; 32 * 16 * 4];
        let label = "Modern League";
        let mut profile = profile_v1(&pixels, label);
        let header = header_v1(&pixels, digest('9'), label);
        let mut duplicate = words_v1();
        duplicate.extend(words_v1());
        assert!(
            classify_from_words_v1(&header, &profile, &pixels, &duplicate, digest('0')).is_err()
        );

        profile.enabled_control_reference_sha256s = vec![digest('1')];
        assert!(
            classify_from_words_v1(&header, &profile, &pixels, &words_v1(), digest('0')).is_err()
        );
    }

    #[test]
    fn assets_require_unique_sorted_exact_target_profiles() {
        let pixels = vec![23_u8; 32 * 16 * 4];
        let label = "Modern League";
        let mut assets = MtgoCompetitiveEventListingClassifierAssetsV1 {
            schema_version: 1,
            scope: ASSET_SCOPE_V1.to_owned(),
            canonical_pixel_format: PIXEL_FORMAT_V1.to_owned(),
            profiles: vec![profile_v1(&pixels, label)],
            deck_gate_profiles: Vec::new(),
            deck_chooser_profiles: Vec::new(),
            navigation_profiles: Vec::new(),
            event_record_profiles: Vec::new(),
            sideboard_profiles: Vec::new(),
        };
        let assets_bytes = serde_json::to_vec(&assets).unwrap();
        let header = header_v1(&pixels, sha256_hex_v1(&assets_bytes), label);
        assert!(validate_assets_and_select_profile_v1(&assets, &header).is_ok());

        assets.profiles.push(assets.profiles[0].clone());
        assert!(validate_assets_and_select_profile_v1(&assets, &header).is_err());
        assets.profiles.pop();
        assets.profiles[0].label_search_rect_client_px = assets.profiles[0]
            .open_entry_review_control_rect_client_px
            .clone();
        assert!(validate_assets_and_select_profile_v1(&assets, &header).is_err());
    }

    #[test]
    fn header_binds_exact_binary_assets_target_and_pixels() {
        let pixels = vec![31_u8; 32 * 16 * 4];
        let label = "Modern League";
        let assets = MtgoCompetitiveEventListingClassifierAssetsV1 {
            schema_version: 1,
            scope: ASSET_SCOPE_V1.to_owned(),
            canonical_pixel_format: PIXEL_FORMAT_V1.to_owned(),
            profiles: vec![profile_v1(&pixels, label)],
            deck_gate_profiles: Vec::new(),
            deck_chooser_profiles: Vec::new(),
            navigation_profiles: Vec::new(),
            event_record_profiles: Vec::new(),
            sideboard_profiles: Vec::new(),
        };
        let assets_bytes = serde_json::to_vec(&assets).unwrap();
        let header = header_v1(&pixels, sha256_hex_v1(&assets_bytes), label);
        assert!(validate_header_identity_v1(
            &header,
            &assets_bytes,
            &header.classifier_binary_sha256
        )
        .is_ok());
        assert!(validate_pixels_v1(&header, &pixels).is_ok());

        let mut changed_pixels = pixels;
        changed_pixels[0] ^= 1;
        assert!(validate_pixels_v1(&header, &changed_pixels).is_err());
        assert!(
            validate_header_identity_v1(&header, b"{}", &header.classifier_binary_sha256).is_err()
        );
        assert!(validate_header_identity_v1(&header, &assets_bytes, &digest('9')).is_err());
    }

    #[test]
    fn exact_navigation_regions_produce_one_valid_lifecycle_snapshot() {
        let pixels = (0..32 * 16 * 4)
            .map(|index| ((index * 17 + 11) % 251) as u8)
            .collect::<Vec<_>>();
        let assets = combined_assets_v1(&pixels);
        let assets_bytes = serde_json::to_vec(&assets).unwrap();
        let header = navigation_header_v1(&pixels, sha256_hex_v1(&assets_bytes));
        assert!(validate_navigation_header_identity_v1(
            &header,
            &assets_bytes,
            &header.classifier_binary_sha256
        )
        .is_ok());
        assert!(validate_navigation_pixels_v1(&header, &pixels).is_ok());
        let profile =
            validate_navigation_assets_and_match_profile_v1(&assets, &header, &pixels).unwrap();
        let response =
            classify_navigation_profile_v1(&header, profile, &pixels, digest('c')).unwrap();
        assert_eq!(
            response.lifecycle.phase,
            MtgoCompetitiveLifecyclePhaseV1::PairingReady
        );
        assert_eq!(
            response.lifecycle.event_kind,
            MtgoCompetitiveEventKindV1::Challenge
        );
        assert_eq!(response.lifecycle.facts.len(), 2);
        assert_eq!(response.request_commitment_sha256, digest('c'));
    }

    #[test]
    fn navigation_region_drift_and_ambiguous_profiles_fail_closed() {
        let pixels = vec![41_u8; 32 * 16 * 4];
        let mut assets = combined_assets_v1(&pixels);
        let assets_bytes = serde_json::to_vec(&assets).unwrap();
        let header = navigation_header_v1(&pixels, sha256_hex_v1(&assets_bytes));

        let mut changed_pixels = pixels.clone();
        changed_pixels[(4 * 32 + 20) * 4] ^= 1;
        assert!(
            validate_navigation_assets_and_match_profile_v1(&assets, &header, &changed_pixels)
                .is_err()
        );

        let mut duplicate = assets.navigation_profiles[0].clone();
        duplicate.profile_id = "challenge-pairing-ready-32x16-v2".to_owned();
        assets.navigation_profiles.push(duplicate);
        assert!(
            validate_navigation_assets_and_match_profile_v1(&assets, &header, &pixels).is_err()
        );
    }

    #[test]
    fn navigation_profiles_must_satisfy_the_phase_contract() {
        let pixels = vec![53_u8; 32 * 16 * 4];
        let mut profile = navigation_profile_v1(&pixels);
        profile.event_identity_sha256 = None;
        assert!(validate_navigation_profile_contract_v1(&profile).is_err());

        profile = navigation_profile_v1(&pixels);
        profile.facts.pop();
        assert!(validate_navigation_profile_contract_v1(&profile).is_err());
    }

    #[test]
    fn exact_event_record_regions_produce_one_source_bound_record() {
        let pixels = (0..32 * 16 * 4)
            .map(|index| ((index * 23 + 17) % 251) as u8)
            .collect::<Vec<_>>();
        let assets = combined_assets_v1(&pixels);
        let assets_bytes = serde_json::to_vec(&assets).unwrap();
        let header = event_record_header_v1(&pixels, sha256_hex_v1(&assets_bytes));
        assert!(validate_event_record_header_identity_v1(
            &header,
            &assets_bytes,
            &header.classifier_binary_sha256
        )
        .is_ok());
        assert!(validate_event_record_pixels_v1(&header, &pixels).is_ok());
        let navigation = validate_navigation_assets_and_match_profile_for_frame_v1(
            &assets,
            header.canonical_width,
            header.canonical_height,
            &pixels,
        )
        .unwrap();
        let event_record = validate_event_record_assets_and_match_profile_v1(
            &assets,
            navigation,
            &header.approved_account_alias_sha256,
            &pixels,
        )
        .unwrap();
        let mut navigation_header = navigation_header_v1(&pixels, sha256_hex_v1(&assets_bytes));
        navigation_header.frame_id = header.frame_id;
        navigation_header.frame_sequence = header.frame_sequence;
        let navigation_response =
            classify_navigation_profile_v1(&navigation_header, navigation, &pixels, digest('c'))
                .unwrap();
        let response = classify_event_record_profile_v1(
            &header,
            navigation,
            event_record,
            &pixels,
            digest('d'),
        )
        .unwrap();
        assert_eq!(
            response.record.status,
            MtgoCompetitiveEventVisibleStatusV1::PairingReady
        );
        assert_eq!(response.record.event_identity_sha256, digest('a'));
        assert_eq!(response.record.frame_id, header.frame_id);
        assert_eq!(response.record.facts.len(), 2);
        assert_eq!(
            validate_visible_competitive_lifecycle_snapshot_v1(
                navigation_response.lifecycle.clone()
            )
            .unwrap()
            .snapshot_commitment_sha256(),
            validate_visible_competitive_lifecycle_snapshot_v1(response.lifecycle.clone())
                .unwrap()
                .snapshot_commitment_sha256()
        );
        assert_eq!(
            response.record.source_lifecycle_snapshot_commitment_sha256,
            validate_visible_competitive_lifecycle_snapshot_v1(response.lifecycle)
                .unwrap()
                .snapshot_commitment_sha256()
        );
    }

    #[test]
    fn event_record_profile_drift_ambiguity_and_semantic_mismatch_fail_closed() {
        let pixels = vec![59_u8; 32 * 16 * 4];
        let mut assets = combined_assets_v1(&pixels);
        let navigation = &assets.navigation_profiles[0];
        let account = digest('5');

        let mut changed_pixels = pixels.clone();
        changed_pixels[(10 * 32 + 1) * 4] ^= 1;
        assert!(validate_event_record_assets_and_match_profile_v1(
            &assets,
            navigation,
            &account,
            &changed_pixels
        )
        .is_err());

        let mut duplicate = assets.event_record_profiles[0].clone();
        duplicate.profile_id = "challenge-pairing-record-32x16-v2".to_owned();
        assets.event_record_profiles.push(duplicate);
        assert!(validate_event_record_assets_and_match_profile_v1(
            &assets, navigation, &account, &pixels
        )
        .is_err());

        let mut invalid = event_record_profile_v1(&pixels);
        invalid.status = MtgoCompetitiveEventVisibleStatusV1::WaitingForPairing;
        assert!(validate_event_record_profile_contract_v1(&invalid, navigation, &account).is_err());
    }

    #[test]
    fn exact_sideboard_regions_produce_one_source_bound_snapshot() {
        let pixels = (0..64 * 48 * 4)
            .map(|index| ((index * 29 + 31) % 251) as u8)
            .collect::<Vec<_>>();
        let assets = sideboard_fixture_assets_v1(&pixels);
        let assets_bytes = serde_json::to_vec(&assets).unwrap();
        let header = sideboard_header_v1(&pixels, &assets);
        assert!(validate_sideboard_header_identity_v1(
            &header,
            &assets_bytes,
            &header.classifier_binary_sha256,
        )
        .is_ok());
        assert!(validate_sideboard_pixels_v1(&header, &pixels).is_ok());
        let navigation = validate_navigation_assets_and_match_profile_for_frame_v1(
            &assets,
            header.canonical_width,
            header.canonical_height,
            &pixels,
        )
        .unwrap();
        let sideboard =
            validate_sideboard_assets_and_match_profile_v1(&assets, navigation, &header, &pixels)
                .unwrap();
        let response =
            classify_sideboard_profile_v1(&header, navigation, sideboard, &pixels, digest('d'))
                .unwrap();
        assert_eq!(
            response.sideboard.event_kind,
            MtgoCompetitiveEventKindV1::League
        );
        assert_eq!(response.sideboard.game_number, 1);
        assert_eq!(response.sideboard.cards.len(), 2);
        assert_eq!(
            validate_visible_competitive_lifecycle_snapshot_v1(response.lifecycle)
                .unwrap()
                .snapshot_commitment_sha256(),
            header.source_lifecycle_snapshot_commitment_sha256
        );
    }

    #[test]
    fn sideboard_drift_ambiguity_deck_and_lifecycle_substitution_fail_closed() {
        let pixels = vec![73_u8; 64 * 48 * 4];
        let mut assets = sideboard_fixture_assets_v1(&pixels);
        let header = sideboard_header_v1(&pixels, &assets);
        let navigation = assets.navigation_profiles[0].clone();

        let mut changed_pixels = pixels.clone();
        changed_pixels[(14 * 64 + 2) * 4] ^= 1;
        assert!(validate_sideboard_assets_and_match_profile_v1(
            &assets,
            &navigation,
            &header,
            &changed_pixels,
        )
        .is_err());

        let mut wrong_deck = header.clone();
        wrong_deck.deck_list_sha256 = digest('e');
        assert!(validate_sideboard_assets_and_match_profile_v1(
            &assets,
            &navigation,
            &wrong_deck,
            &pixels,
        )
        .is_err());

        let mut duplicate = assets.sideboard_profiles[0].clone();
        duplicate.profile_id = "league-sideboard-deck-64x48-v2".to_owned();
        assets.sideboard_profiles.push(duplicate);
        assert!(validate_sideboard_assets_and_match_profile_v1(
            &assets,
            &navigation,
            &header,
            &pixels,
        )
        .is_err());

        let mut wrong_lifecycle = header.clone();
        wrong_lifecycle.source_lifecycle_snapshot_commitment_sha256 = digest('e');
        assert!(classify_sideboard_profile_v1(
            &wrong_lifecycle,
            &navigation,
            &assets.sideboard_profiles[0],
            &pixels,
            digest('d'),
        )
        .is_err());

        let mut invalid = assets.sideboard_profiles[0].clone();
        invalid.cards[0].count = 2;
        assert!(validate_sideboard_profile_contract_v1(&invalid, &navigation).is_err());
    }

    fn player_visible_target_fixture_v1(
        pixels: &[u8],
    ) -> (
        MtgoPlayerVisibleDuelGestureTargetClassifierAssetsV1,
        MtgoPlayerVisibleDuelGestureTargetRequestHeaderWireV1,
    ) {
        let size = MtgoSizePxV1 {
            width: 32,
            height: 16,
        };
        let rect = MtgoRectPxV1 {
            x: 2,
            y: 2,
            width: 4,
            height: 4,
        };
        let selected_action = mtgo_blackbox_v1::MtgoPlayerVisibleDuelActionV1::Pass {
            actor: mtgo_blackbox_v1::MtgoPlayerRelativeRoleV1::SeatedPlayer,
        };
        let decision_input = MtgoPlayerVisibleDuelDecisionInputV1 {
            current_state: mtgo_blackbox_v1::MtgoPlayerVisibleDuelStateV1 {
                acting_player: mtgo_blackbox_v1::MtgoPlayerRelativeRoleV1::SeatedPlayer,
                turn: 1,
                phase: mtg_kernel::rl::ZoneIndependentStepV1::Main1,
                active_player: mtgo_blackbox_v1::MtgoPlayerRelativeRoleV1::SeatedPlayer,
                priority_player: mtgo_blackbox_v1::MtgoPlayerRelativeRoleV1::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6]; 2],
                hand_counts: [7, 7],
                library_counts: [53, 53],
                battlefield: [Vec::new(), Vec::new()],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: mtgo_blackbox_v1::MtgoPlayerVisibleCombatStateV1 {
                    attackers_declared: false,
                    blockers_declared: false,
                    ordered_attackers: Vec::new(),
                    blocker_assignments: Vec::new(),
                },
                visible_object_relations: Vec::new(),
                own_hand: Vec::new(),
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            ordered_legal_actions: vec![selected_action.clone()],
        };
        let gesture_plan = MtgoPlayerVisibleDuelGesturePlanV1 {
            schema_version: mtgo_blackbox_v1::MTGO_PLAYER_VISIBLE_DUEL_GESTURE_PLAN_SCHEMA_V1,
            selected_action,
            primitive_set_complete: true,
            primitives: vec![
                mtgo_blackbox_v1::MtgoPlayerVisibleDuelGesturePrimitiveV1::ActivatePrimary {
                    activation: serde_json::from_str("\"single_left_click\"").unwrap(),
                },
            ],
        };
        let profile = MtgoPlayerVisibleDuelGestureTargetReferenceProfileV1 {
            profile_id: "pass-primary-32x16-v1".to_owned(),
            gesture_evaluation_commitment_sha256: digest('5'),
            gesture_profile_admission_commitment_sha256: digest('6'),
            client_size_px: size.clone(),
            decision_input: decision_input.clone(),
            gesture_plan: gesture_plan.clone(),
            primitive_index: 0,
            targets: vec![MtgoPlayerVisibleDuelGestureTargetReferenceV1 {
                target_id: "pass-control-v1".to_owned(),
                role: MtgoPlayerVisibleDuelGestureTargetRoleV1::PrimarySemanticControl,
                rect_client_px: rect.clone(),
                accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                    pixels, &size, &rect,
                )
                .unwrap()],
                confidence_bps: 9_500,
                visibly_enabled: true,
            }],
        };
        let assets = MtgoPlayerVisibleDuelGestureTargetClassifierAssetsV1 {
            schema_version: 1,
            scope: PLAYER_VISIBLE_DUEL_GESTURE_TARGET_ASSET_SCOPE_V1.to_owned(),
            canonical_pixel_format: PIXEL_FORMAT_V1.to_owned(),
            profiles: vec![profile],
            gameplay_postcondition_profiles: Vec::new(),
        };
        let assets_json = serde_json::to_vec(&assets).unwrap();
        let header = MtgoPlayerVisibleDuelGestureTargetRequestHeaderWireV1 {
            schema_version: 1,
            protocol: "mtgo_player_visible_duel_gesture_target_v1".to_owned(),
            frame_id: 41,
            frame_sequence: 43,
            canonical_width: size.width,
            canonical_height: size.height,
            canonical_stride: size.width * 4,
            canonical_byte_length: pixels.len(),
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: digest('1'),
            perception_result_commitment_sha256: digest('2'),
            decision_input,
            gesture_plan,
            primitive_index: 0,
            gesture_evaluation_commitment_sha256: digest('5'),
            gesture_profile_admission_commitment_sha256: digest('6'),
            runtime_identity_commitment_sha256: digest('7'),
            gesture_target_runtime_binary_sha256: digest('8'),
            gesture_target_assets_manifest_sha256: sha256_hex_v1(&assets_json),
        };
        (assets, header)
    }

    fn player_visible_gameplay_postcondition_fixture_v1(
        pixels: &[u8],
    ) -> (
        MtgoPlayerVisibleDuelGestureTargetClassifierAssetsV1,
        MtgoPlayerVisibleGameplayPostconditionRequestHeaderWireV1,
    ) {
        let (mut assets, target_header) = player_visible_target_fixture_v1(pixels);
        let rect = MtgoRectPxV1 {
            x: 10,
            y: 2,
            width: 6,
            height: 3,
        };
        let player_visible_decision = MtgoPlayerVisibleConfirmedDuelDecisionV1 {
            current_state: target_header.decision_input.current_state.clone(),
            selected_action: target_header.gesture_plan.selected_action.clone(),
        };
        assets.gameplay_postcondition_profiles.push(
            MtgoPlayerVisibleGameplayPostconditionReferenceProfileV1 {
                profile_id: "pass-prompt-postcondition-32x16-v1".to_owned(),
                gesture_evaluation_commitment_sha256: target_header
                    .gesture_evaluation_commitment_sha256
                    .clone(),
                gesture_profile_admission_commitment_sha256: target_header
                    .gesture_profile_admission_commitment_sha256
                    .clone(),
                client_size_px: MtgoSizePxV1 {
                    width: target_header.canonical_width,
                    height: target_header.canonical_height,
                },
                player_visible_decision: player_visible_decision.clone(),
                gesture_plan: target_header.gesture_plan.clone(),
                primitive_index: target_header.primitive_index,
                regions: vec![MtgoPlayerVisibleGameplayPostconditionReferenceRegionV1 {
                    region_id: "priority-prompt-v1".to_owned(),
                    kind: MtgoPlayerVisibleGameplayPostconditionKindV1::PromptChanged,
                    rect_client_px: rect.clone(),
                    accepted_reference_sha256s: vec![visible_frame_region_content_sha256_v1(
                        pixels,
                        &MtgoSizePxV1 {
                            width: target_header.canonical_width,
                            height: target_header.canonical_height,
                        },
                        &rect,
                    )
                    .unwrap()],
                    confidence_bps: 9_500,
                }],
            },
        );
        let assets_json = serde_json::to_vec(&assets).unwrap();
        let header = MtgoPlayerVisibleGameplayPostconditionRequestHeaderWireV1 {
            schema_version: 1,
            protocol: "mtgo_player_visible_gameplay_postcondition_v1".to_owned(),
            frame_id: target_header.frame_id,
            frame_sequence: target_header.frame_sequence,
            canonical_width: target_header.canonical_width,
            canonical_height: target_header.canonical_height,
            canonical_stride: target_header.canonical_stride,
            canonical_byte_length: target_header.canonical_byte_length,
            canonical_bgra8_sha256: target_header.canonical_bgra8_sha256,
            source_capture_commitment_sha256: target_header.source_capture_commitment_sha256,
            perception_result_commitment_sha256: target_header.perception_result_commitment_sha256,
            player_visible_decision,
            gesture_plan: target_header.gesture_plan,
            primitive_index: target_header.primitive_index,
            gesture_evaluation_commitment_sha256: target_header
                .gesture_evaluation_commitment_sha256,
            gesture_profile_admission_commitment_sha256: target_header
                .gesture_profile_admission_commitment_sha256,
            runtime_identity_commitment_sha256: target_header.runtime_identity_commitment_sha256,
            runtime_binary_sha256: target_header.gesture_target_runtime_binary_sha256,
            assets_manifest_sha256: sha256_hex_v1(&assets_json),
        };
        (assets, header)
    }

    #[test]
    fn exact_player_visible_gesture_target_profile_returns_rehashed_target() {
        let pixels = (0..32 * 16 * 4)
            .map(|index| ((index * 31 + 19) % 251) as u8)
            .collect::<Vec<_>>();
        let (assets, header) = player_visible_target_fixture_v1(&pixels);
        let assets_json = serde_json::to_vec(&assets).unwrap();
        assert!(
            validate_player_visible_duel_gesture_target_header_identity_v1(
                &header,
                &assets_json,
                &header.gesture_target_runtime_binary_sha256,
            )
            .is_ok()
        );
        let profile = validate_player_visible_duel_gesture_target_assets_and_match_profile_v1(
            &assets, &header, &pixels,
        )
        .unwrap();
        let response = build_player_visible_duel_gesture_target_response_v1(
            &header,
            profile,
            &pixels,
            digest('9'),
        )
        .unwrap();
        assert!(response.target_set.candidate_set_complete);
        assert_eq!(response.target_set.targets.len(), 1);
        assert_eq!(
            response.target_set.targets[0].content_sha256,
            assets.profiles[0].targets[0].accepted_reference_sha256s[0]
        );
    }

    #[test]
    fn player_visible_gesture_target_pixel_drift_and_ambiguity_fail_closed() {
        let pixels = vec![37_u8; 32 * 16 * 4];
        let (mut assets, header) = player_visible_target_fixture_v1(&pixels);
        let mut changed_pixels = pixels.clone();
        changed_pixels[(2 * 32 + 2) * 4] ^= 1;
        assert!(
            validate_player_visible_duel_gesture_target_assets_and_match_profile_v1(
                &assets,
                &header,
                &changed_pixels,
            )
            .is_err()
        );

        let mut duplicate = assets.profiles[0].clone();
        duplicate.profile_id = "pass-primary-32x16-v2".to_owned();
        assets.profiles.push(duplicate);
        assert!(
            validate_player_visible_duel_gesture_target_assets_and_match_profile_v1(
                &assets, &header, &pixels,
            )
            .is_err()
        );
    }

    #[test]
    fn player_visible_gesture_target_artifact_or_role_drift_fails_closed() {
        let pixels = vec![43_u8; 32 * 16 * 4];
        let (mut assets, header) = player_visible_target_fixture_v1(&pixels);
        let assets_json = serde_json::to_vec(&assets).unwrap();
        assert!(
            validate_player_visible_duel_gesture_target_header_identity_v1(
                &header,
                b"{}",
                &header.gesture_target_runtime_binary_sha256,
            )
            .is_err()
        );
        assert!(
            validate_player_visible_duel_gesture_target_header_identity_v1(
                &header,
                &assets_json,
                &digest('f'),
            )
            .is_err()
        );

        assets.profiles[0].targets[0].role =
            MtgoPlayerVisibleDuelGestureTargetRoleV1::SubmitControl;
        assert!(
            validate_player_visible_duel_gesture_target_assets_and_match_profile_v1(
                &assets, &header, &pixels,
            )
            .is_err()
        );
    }

    #[test]
    fn exact_player_visible_gameplay_postcondition_profile_rehashes_complete_regions() {
        let pixels = (0..32 * 16 * 4)
            .map(|index| ((index * 17 + 29) % 251) as u8)
            .collect::<Vec<_>>();
        let (assets, header) = player_visible_gameplay_postcondition_fixture_v1(&pixels);
        let assets_json = serde_json::to_vec(&assets).unwrap();
        validate_player_visible_gameplay_postcondition_header_identity_v1(
            &header,
            &assets_json,
            &header.runtime_binary_sha256,
        )
        .unwrap();
        validate_player_visible_gameplay_postcondition_pixels_v1(&header, &pixels).unwrap();
        let profile = validate_player_visible_gameplay_postcondition_assets_and_match_profile_v1(
            &assets, &header, &pixels,
        )
        .unwrap();
        let response = build_player_visible_gameplay_postcondition_response_v1(
            &header,
            profile,
            &pixels,
            digest('9'),
        )
        .unwrap();
        assert!(response.region_set.candidate_set_complete);
        assert_eq!(response.region_set.regions.len(), 1);
        assert_eq!(
            response.region_set.regions[0].content_sha256,
            assets.gameplay_postcondition_profiles[0].regions[0].accepted_reference_sha256s[0]
        );
    }

    #[test]
    fn player_visible_gameplay_postcondition_drift_ambiguity_and_overlap_fail_closed() {
        let pixels = (0..32 * 16 * 4)
            .map(|index| ((index * 13 + 7) % 251) as u8)
            .collect::<Vec<_>>();
        let (mut assets, header) = player_visible_gameplay_postcondition_fixture_v1(&pixels);
        let rect = assets.gameplay_postcondition_profiles[0].regions[0]
            .rect_client_px
            .clone();
        let changed_index =
            (usize::try_from(rect.y).unwrap() * 32 + usize::try_from(rect.x).unwrap()) * 4;
        let mut changed_pixels = pixels.clone();
        changed_pixels[changed_index] ^= 1;
        assert!(
            validate_player_visible_gameplay_postcondition_assets_and_match_profile_v1(
                &assets,
                &header,
                &changed_pixels,
            )
            .is_err()
        );

        let mut duplicate = assets.gameplay_postcondition_profiles[0].clone();
        duplicate.profile_id = "pass-prompt-postcondition-32x16-v2".to_owned();
        assets.gameplay_postcondition_profiles.push(duplicate);
        assert!(
            validate_player_visible_gameplay_postcondition_assets_and_match_profile_v1(
                &assets, &header, &pixels,
            )
            .is_err()
        );

        assets.gameplay_postcondition_profiles.pop();
        let profile = &mut assets.gameplay_postcondition_profiles[0];
        profile
            .regions
            .push(MtgoPlayerVisibleGameplayPostconditionReferenceRegionV1 {
                region_id: "phase-bar-overlap-v1".to_owned(),
                kind: MtgoPlayerVisibleGameplayPostconditionKindV1::PhaseBarChanged,
                rect_client_px: rect,
                accepted_reference_sha256s: vec![
                    profile.regions[0].accepted_reference_sha256s[0].clone()
                ],
                confidence_bps: 9_500,
            });
        assert!(
            validate_player_visible_gameplay_postcondition_assets_and_match_profile_v1(
                &assets, &header, &pixels,
            )
            .is_err()
        );
    }

    fn deck_gate_words_v1(state: MtgoCompetitiveDeckGateStateV1) -> Vec<OcrWordV1> {
        let mut words = vec![
            OcrWordV1 {
                normalized: "Modern".to_owned(),
                rect: MtgoRectPxV1 {
                    x: 2,
                    y: 2,
                    width: 5,
                    height: 4,
                },
            },
            OcrWordV1 {
                normalized: "Challenge".to_owned(),
                rect: MtgoRectPxV1 {
                    x: 8,
                    y: 2,
                    width: 8,
                    height: 4,
                },
            },
            OcrWordV1 {
                normalized: "64".to_owned(),
                rect: MtgoRectPxV1 {
                    x: 17,
                    y: 2,
                    width: 3,
                    height: 4,
                },
            },
        ];
        if state == MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection {
            for (index, value) in ["Please", "Select", "a", "Deck"].into_iter().enumerate() {
                words.push(OcrWordV1 {
                    normalized: value.to_owned(),
                    rect: MtgoRectPxV1 {
                        x: 2 + u32::try_from(index).unwrap() * 7,
                        y: 14,
                        width: 5,
                        height: 4,
                    },
                });
            }
        } else {
            words.push(OcrWordV1 {
                normalized: "mtgo-kernel-modern-basics-v1".to_owned(),
                rect: MtgoRectPxV1 {
                    x: 2,
                    y: 14,
                    width: 30,
                    height: 4,
                },
            });
        }
        words
    }

    fn deck_gate_assets_v1(
        pixels: &[u8],
        matching_state: MtgoCompetitiveDeckGateStateV1,
    ) -> MtgoCompetitiveEventListingClassifierAssetsV1 {
        let size = MtgoSizePxV1 {
            width: 64,
            height: 48,
        };
        let event_label_search_rect_client_px = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 32,
            height: 10,
        };
        let deck_status_search_rect_client_px = MtgoRectPxV1 {
            x: 0,
            y: 12,
            width: 36,
            height: 10,
        };
        let select_deck_control_rect_client_px = MtgoRectPxV1 {
            x: 42,
            y: 0,
            width: 8,
            height: 8,
        };
        let open_entry_review_status_rect_client_px = MtgoRectPxV1 {
            x: 42,
            y: 12,
            width: 8,
            height: 8,
        };
        let select_hash = visible_frame_region_content_sha256_v1(
            pixels,
            &size,
            &select_deck_control_rect_client_px,
        )
        .unwrap();
        let open_hash = visible_frame_region_content_sha256_v1(
            pixels,
            &size,
            &open_entry_review_status_rect_client_px,
        )
        .unwrap();
        let event_label = "Modern Challenge 64";
        let deck_label = "mtgo-kernel-modern-basics-v1";
        let states = [
            (
                "a-awaiting-deck-v1",
                MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection,
            ),
            (
                "b-compatible-deck-selected-v1",
                MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected,
            ),
            (
                "c-open-entry-review-v1",
                MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable,
            ),
        ];
        let deck_gate_profiles = states
            .into_iter()
            .map(|(profile_id, state)| {
                let matching = state == matching_state;
                let review_available =
                    state == MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable;
                MtgoCompetitiveDeckGateProfileV1 {
                    profile_id: profile_id.to_owned(),
                    event_kind: MtgoCompetitiveEventKindV1::Challenge,
                    event_display_label_sha256: sha256_hex_v1(event_label.as_bytes()),
                    expected_visible_event_label: event_label.to_owned(),
                    deck_display_label_sha256: sha256_hex_v1(deck_label.as_bytes()),
                    expected_visible_deck_label: deck_label.to_owned(),
                    state,
                    client_size_px: size.clone(),
                    event_label_search_rect_client_px: event_label_search_rect_client_px.clone(),
                    deck_status_search_rect_client_px: deck_status_search_rect_client_px.clone(),
                    select_deck_control_rect_client_px: select_deck_control_rect_client_px.clone(),
                    select_deck_control_reference_sha256s: vec![select_hash.clone()],
                    open_entry_review_status_rect_client_px:
                        open_entry_review_status_rect_client_px.clone(),
                    open_entry_review_unavailable_reference_sha256s: if review_available {
                        Vec::new()
                    } else if matching {
                        vec![open_hash.clone()]
                    } else {
                        vec![digest('d')]
                    },
                    open_entry_review_enabled_reference_sha256s: if review_available {
                        if matching {
                            vec![open_hash.clone()]
                        } else {
                            vec![digest('e')]
                        }
                    } else {
                        Vec::new()
                    },
                    confidence_bps: 9_500,
                }
            })
            .collect();
        MtgoCompetitiveEventListingClassifierAssetsV1 {
            schema_version: 1,
            scope: COMBINED_ASSET_SCOPE_V1.to_owned(),
            canonical_pixel_format: PIXEL_FORMAT_V1.to_owned(),
            profiles: Vec::new(),
            deck_gate_profiles,
            deck_chooser_profiles: Vec::new(),
            navigation_profiles: Vec::new(),
            event_record_profiles: Vec::new(),
            sideboard_profiles: Vec::new(),
        }
    }

    fn deck_gate_header_v1(
        pixels: &[u8],
        assets: &MtgoCompetitiveEventListingClassifierAssetsV1,
    ) -> MtgoCompetitiveDeckGateClassifierRequestHeaderV1 {
        let target = MtgoCompetitiveEventListingTargetV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
            target_id: "modern-challenge-64-deck-gate-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            approved_account_alias_sha256: digest('a'),
            event_identity_sha256: digest('b'),
            event_display_label_sha256: sha256_hex_v1(b"Modern Challenge 64"),
            deck_display_label_sha256: sha256_hex_v1(b"mtgo-kernel-modern-basics-v1"),
            deck_list_sha256: digest('c'),
            deck_manifest_commitment_sha256: digest('1'),
            deck_format_sha256: digest('2'),
            policy_deployment_commitment_sha256: digest('3'),
        };
        let assets_bytes = serde_json::to_vec(assets).unwrap();
        MtgoCompetitiveDeckGateClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: DECK_GATE_REQUEST_PROTOCOL_V1.to_owned(),
            parser_scope: DECK_GATE_REQUEST_SCOPE_V1.to_owned(),
            frame_id: 11,
            frame_sequence: 13,
            captured_at_unix_millis: 17,
            canonical_width: 64,
            canonical_height: 48,
            canonical_stride: 256,
            canonical_byte_length: pixels.len() as u64,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: digest('4'),
            source_frame_profile_binding_sha256: digest('5'),
            source_navigation_classification_result_commitment_sha256: digest('6'),
            source_lifecycle_snapshot_commitment_sha256: digest('7'),
            navigation_profile_commitment_sha256: digest('8'),
            navigation_profile_admission_commitment_sha256: digest('9'),
            approved_account_alias_sha256: digest('a'),
            runtime_identity_commitment_sha256: digest('f'),
            classifier_binary_sha256: digest('0'),
            classifier_assets_manifest_sha256: sha256_hex_v1(&assets_bytes),
            target_commitment_sha256: target_commitment_v1(&target).unwrap(),
            target,
        }
    }

    #[test]
    fn exact_three_state_deck_gate_profiles_classify_without_authority() {
        let pixels = (0..64 * 48 * 4)
            .map(|index| ((index * 17 + 11) % 251) as u8)
            .collect::<Vec<_>>();
        for state in [
            MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection,
            MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected,
            MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable,
        ] {
            let assets = deck_gate_assets_v1(&pixels, state);
            let header = deck_gate_header_v1(&pixels, &assets);
            let profiles = validate_deck_gate_assets_v1(&assets, &header).unwrap();
            let mut matches = profiles
                .into_iter()
                .filter_map(|profile| {
                    classify_deck_gate_from_words_v1(
                        &header,
                        profile,
                        &pixels,
                        &deck_gate_words_v1(state),
                        digest('1'),
                    )
                    .ok()
                })
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1);
            let response = matches.remove(0);
            assert_eq!(response.gate.state, state);
            assert_eq!(
                response.gate.open_entry_review_control_enabled,
                state == MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable
            );
            assert!(!response.gate.select_deck_control_region_sha256.is_empty());
        }
    }

    #[test]
    fn deck_gate_profile_drift_incomplete_corpus_and_ambiguity_fail_closed() {
        let pixels = vec![37_u8; 64 * 48 * 4];
        let state = MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected;
        let mut assets = deck_gate_assets_v1(&pixels, state);
        let header = deck_gate_header_v1(&pixels, &assets);
        assets.deck_gate_profiles.pop();
        assert!(validate_deck_gate_assets_v1(&assets, &header).is_err());

        let mut assets = deck_gate_assets_v1(&pixels, state);
        assets.deck_gate_profiles[1].deck_status_search_rect_client_px = assets.deck_gate_profiles
            [1]
        .event_label_search_rect_client_px
        .clone();
        assert!(validate_deck_gate_assets_v1(&assets, &header).is_err());

        let assets = deck_gate_assets_v1(&pixels, state);
        let profiles = validate_deck_gate_assets_v1(&assets, &header).unwrap();
        let mut duplicated_words = deck_gate_words_v1(state);
        duplicated_words.push(OcrWordV1 {
            normalized: "mtgo-kernel-modern-basics-v1".to_owned(),
            rect: MtgoRectPxV1 {
                x: 2,
                y: 18,
                width: 30,
                height: 3,
            },
        });
        assert!(profiles.into_iter().all(|profile| {
            classify_deck_gate_from_words_v1(
                &header,
                profile,
                &pixels,
                &duplicated_words,
                digest('1'),
            )
            .is_err()
        }));
    }

    #[test]
    fn deck_gate_header_binds_binary_assets_target_and_pixels() {
        let pixels = vec![41_u8; 64 * 48 * 4];
        let assets = deck_gate_assets_v1(
            &pixels,
            MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected,
        );
        let assets_bytes = serde_json::to_vec(&assets).unwrap();
        let header = deck_gate_header_v1(&pixels, &assets);
        validate_deck_gate_header_identity_v1(
            &header,
            &assets_bytes,
            &header.classifier_binary_sha256,
        )
        .unwrap();
        validate_deck_gate_pixels_v1(&header, &pixels).unwrap();

        let mut changed_pixels = pixels.clone();
        changed_pixels[0] ^= 1;
        assert!(validate_deck_gate_pixels_v1(&header, &changed_pixels).is_err());
        assert!(validate_deck_gate_header_identity_v1(
            &header,
            b"{}",
            &header.classifier_binary_sha256,
        )
        .is_err());
        let mut changed_target = header.clone();
        changed_target.target.deck_display_label_sha256 = digest('d');
        assert!(validate_deck_gate_header_identity_v1(
            &changed_target,
            &assets_bytes,
            &changed_target.classifier_binary_sha256,
        )
        .is_err());
    }

    fn deck_chooser_words_v1() -> Vec<OcrWordV1> {
        let mut words = vec![
            OcrWordV1 {
                normalized: "Modern".to_owned(),
                rect: MtgoRectPxV1 {
                    x: 2,
                    y: 3,
                    width: 10,
                    height: 3,
                },
            },
            OcrWordV1 {
                normalized: "Decks".to_owned(),
                rect: MtgoRectPxV1 {
                    x: 13,
                    y: 3,
                    width: 8,
                    height: 3,
                },
            },
        ];
        words.push(OcrWordV1 {
            normalized: "mtgo-kernel-modern-basics-v1".to_owned(),
            rect: MtgoRectPxV1 {
                x: 3,
                y: 18,
                width: 26,
                height: 4,
            },
        });
        words
    }

    fn deck_chooser_assets_v1(
        pixels: &[u8],
        matching_state: MtgoCompetitiveDeckChooserStateV1,
    ) -> MtgoCompetitiveEventListingClassifierAssetsV1 {
        let size = MtgoSizePxV1 {
            width: 64,
            height: 48,
        };
        let chooser_title_search_rect_client_px = MtgoRectPxV1 {
            x: 1,
            y: 1,
            width: 31,
            height: 8,
        };
        let deck_row_control_rect_client_px = MtgoRectPxV1 {
            x: 1,
            y: 15,
            width: 31,
            height: 11,
        };
        let deck_label_search_rect_client_px = deck_row_control_rect_client_px.clone();
        let selection_detail_rect_client_px = MtgoRectPxV1 {
            x: 34,
            y: 15,
            width: 28,
            height: 11,
        };
        let submit_control_rect_client_px = MtgoRectPxV1 {
            x: 42,
            y: 30,
            width: 16,
            height: 8,
        };
        let row_hash =
            visible_frame_region_content_sha256_v1(pixels, &size, &deck_row_control_rect_client_px)
                .unwrap();
        let detail_hash =
            visible_frame_region_content_sha256_v1(pixels, &size, &selection_detail_rect_client_px)
                .unwrap();
        let submit_hash =
            visible_frame_region_content_sha256_v1(pixels, &size, &submit_control_rect_client_px)
                .unwrap();
        let profiles = [
            (
                "a-awaiting-exact-deck-v1",
                MtgoCompetitiveDeckChooserStateV1::AwaitingExactDeckSelection,
            ),
            (
                "b-exact-deck-selected-v1",
                MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected,
            ),
        ]
        .into_iter()
        .map(|(profile_id, state)| {
            let matching = state == matching_state;
            let selected = state == MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected;
            let state_row_hash = if matching {
                row_hash.clone()
            } else {
                digest(if selected { 'd' } else { 'e' })
            };
            let state_submit_hash = if matching {
                submit_hash.clone()
            } else {
                digest(if selected { '6' } else { '7' })
            };
            let state_detail_hash = if matching {
                detail_hash.clone()
            } else {
                digest(if selected { '1' } else { '2' })
            };
            MtgoCompetitiveDeckChooserProfileV1 {
                profile_id: profile_id.to_owned(),
                event_kind: MtgoCompetitiveEventKindV1::Challenge,
                event_display_label_sha256: sha256_hex_v1(b"Modern Challenge 64"),
                deck_display_label_sha256: sha256_hex_v1(b"mtgo-kernel-modern-basics-v1"),
                expected_visible_deck_label: "mtgo-kernel-modern-basics-v1".to_owned(),
                chooser_title_label_sha256: sha256_hex_v1(b"Modern Decks"),
                expected_visible_chooser_title: "Modern Decks".to_owned(),
                state,
                client_size_px: size.clone(),
                chooser_title_search_rect_client_px: chooser_title_search_rect_client_px.clone(),
                deck_label_search_rect_client_px: deck_label_search_rect_client_px.clone(),
                deck_row_control_rect_client_px: deck_row_control_rect_client_px.clone(),
                unselected_deck_row_reference_sha256s: if selected {
                    Vec::new()
                } else {
                    vec![state_row_hash.clone()]
                },
                selected_deck_row_reference_sha256s: if selected {
                    vec![state_row_hash]
                } else {
                    Vec::new()
                },
                selection_detail_rect_client_px: selection_detail_rect_client_px.clone(),
                unselected_selection_detail_reference_sha256s: if selected {
                    Vec::new()
                } else {
                    vec![state_detail_hash.clone()]
                },
                selected_selection_detail_reference_sha256s: if selected {
                    vec![state_detail_hash]
                } else {
                    Vec::new()
                },
                submit_control_rect_client_px: submit_control_rect_client_px.clone(),
                disabled_submit_reference_sha256s: if selected {
                    Vec::new()
                } else {
                    vec![state_submit_hash.clone()]
                },
                enabled_submit_reference_sha256s: if selected {
                    vec![state_submit_hash]
                } else {
                    Vec::new()
                },
                confidence_bps: 9_500,
            }
        })
        .collect();
        MtgoCompetitiveEventListingClassifierAssetsV1 {
            schema_version: 1,
            scope: COMBINED_ASSET_SCOPE_V1.to_owned(),
            canonical_pixel_format: PIXEL_FORMAT_V1.to_owned(),
            profiles: Vec::new(),
            deck_gate_profiles: Vec::new(),
            deck_chooser_profiles: profiles,
            navigation_profiles: Vec::new(),
            event_record_profiles: Vec::new(),
            sideboard_profiles: Vec::new(),
        }
    }

    fn deck_chooser_header_v1(
        pixels: &[u8],
        assets: &MtgoCompetitiveEventListingClassifierAssetsV1,
    ) -> MtgoCompetitiveDeckChooserClassifierRequestHeaderV1 {
        let target = MtgoCompetitiveEventListingTargetV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
            target_id: "modern-challenge-64-deck-chooser-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            approved_account_alias_sha256: digest('a'),
            event_identity_sha256: digest('b'),
            event_display_label_sha256: sha256_hex_v1(b"Modern Challenge 64"),
            deck_display_label_sha256: sha256_hex_v1(b"mtgo-kernel-modern-basics-v1"),
            deck_list_sha256: digest('c'),
            deck_manifest_commitment_sha256: digest('1'),
            deck_format_sha256: digest('2'),
            policy_deployment_commitment_sha256: digest('3'),
        };
        let assets_bytes = serde_json::to_vec(assets).unwrap();
        MtgoCompetitiveDeckChooserClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: DECK_CHOOSER_REQUEST_PROTOCOL_V1.to_owned(),
            parser_scope: DECK_CHOOSER_REQUEST_SCOPE_V1.to_owned(),
            frame_id: 19,
            frame_sequence: 23,
            captured_at_unix_millis: 29,
            canonical_width: 64,
            canonical_height: 48,
            canonical_stride: 256,
            canonical_byte_length: pixels.len() as u64,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: digest('4'),
            source_frame_profile_binding_sha256: digest('5'),
            source_deck_gate_observation_commitment_sha256: digest('6'),
            source_deck_gate_result_commitment_sha256: digest('7'),
            source_window_continuity_commitment_sha256: digest('8'),
            navigation_profile_commitment_sha256: digest('9'),
            navigation_profile_admission_commitment_sha256: digest('d'),
            approved_account_alias_sha256: digest('a'),
            runtime_identity_commitment_sha256: digest('e'),
            classifier_binary_sha256: digest('f'),
            classifier_assets_manifest_sha256: sha256_hex_v1(&assets_bytes),
            target_commitment_sha256: target_commitment_v1(&target).unwrap(),
            target,
        }
    }

    #[test]
    fn exact_two_state_deck_chooser_profiles_classify_without_authority() {
        let pixels = (0..64 * 48 * 4)
            .map(|index| ((index * 19 + 13) % 251) as u8)
            .collect::<Vec<_>>();
        for state in [
            MtgoCompetitiveDeckChooserStateV1::AwaitingExactDeckSelection,
            MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected,
        ] {
            let assets = deck_chooser_assets_v1(&pixels, state);
            let header = deck_chooser_header_v1(&pixels, &assets);
            let profiles = validate_deck_chooser_assets_v1(&assets, &header).unwrap();
            let mut matches = profiles
                .into_iter()
                .filter_map(|profile| {
                    classify_deck_chooser_from_words_v1(
                        &header,
                        profile,
                        &pixels,
                        &deck_chooser_words_v1(),
                        digest('1'),
                    )
                    .ok()
                })
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1);
            let response = matches.remove(0);
            assert_eq!(response.chooser.state, state);
            assert_eq!(
                response.chooser.submit_control_enabled,
                state == MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected
            );
            assert_eq!(
                response.chooser.deck_row_selected,
                state == MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected
            );
        }
    }

    #[test]
    fn deck_chooser_incomplete_drifted_and_ambiguous_profiles_fail_closed() {
        let pixels = vec![43_u8; 64 * 48 * 4];
        let state = MtgoCompetitiveDeckChooserStateV1::AwaitingExactDeckSelection;
        let mut assets = deck_chooser_assets_v1(&pixels, state);
        let header = deck_chooser_header_v1(&pixels, &assets);
        assets.deck_chooser_profiles.pop();
        assert!(validate_deck_chooser_assets_v1(&assets, &header).is_err());

        let mut assets = deck_chooser_assets_v1(&pixels, state);
        assets.deck_chooser_profiles[0].submit_control_rect_client_px = assets
            .deck_chooser_profiles[0]
            .deck_row_control_rect_client_px
            .clone();
        assert!(validate_deck_chooser_assets_v1(&assets, &header).is_err());

        let mut assets = deck_chooser_assets_v1(&pixels, state);
        assets.deck_chooser_profiles[0].selection_detail_rect_client_px = assets
            .deck_chooser_profiles[0]
            .deck_row_control_rect_client_px
            .clone();
        assert!(validate_deck_chooser_assets_v1(&assets, &header).is_err());

        let mut assets = deck_chooser_assets_v1(&pixels, state);
        assets.deck_chooser_profiles[1].expected_visible_chooser_title =
            "Select Your Deck".to_owned();
        assets.deck_chooser_profiles[1].chooser_title_label_sha256 =
            sha256_hex_v1(b"Select Your Deck");
        assert!(validate_deck_chooser_assets_v1(&assets, &header).is_err());

        let assets = deck_chooser_assets_v1(&pixels, state);
        let profiles = validate_deck_chooser_assets_v1(&assets, &header).unwrap();
        let mut words = deck_chooser_words_v1();
        words.push(OcrWordV1 {
            normalized: "mtgo-kernel-modern-basics-v1".to_owned(),
            rect: MtgoRectPxV1 {
                x: 3,
                y: 22,
                width: 26,
                height: 3,
            },
        });
        assert!(profiles.into_iter().all(|profile| {
            classify_deck_chooser_from_words_v1(&header, profile, &pixels, &words, digest('1'))
                .is_err()
        }));
    }

    #[test]
    fn deck_chooser_header_binds_binary_assets_target_and_pixels() {
        let pixels = vec![47_u8; 64 * 48 * 4];
        let assets = deck_chooser_assets_v1(
            &pixels,
            MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected,
        );
        let assets_bytes = serde_json::to_vec(&assets).unwrap();
        let header = deck_chooser_header_v1(&pixels, &assets);
        validate_deck_chooser_header_identity_v1(
            &header,
            &assets_bytes,
            &header.classifier_binary_sha256,
        )
        .unwrap();
        validate_deck_chooser_pixels_v1(&header, &pixels).unwrap();

        let mut changed_pixels = pixels.clone();
        changed_pixels[0] ^= 1;
        assert!(validate_deck_chooser_pixels_v1(&header, &changed_pixels).is_err());
        assert!(validate_deck_chooser_header_identity_v1(
            &header,
            b"{}",
            &header.classifier_binary_sha256,
        )
        .is_err());
        let mut changed_target = header.clone();
        changed_target.target.deck_display_label_sha256 = digest('0');
        assert!(validate_deck_chooser_header_identity_v1(
            &changed_target,
            &assets_bytes,
            &changed_target.classifier_binary_sha256,
        )
        .is_err());
    }
}
