use super::{
    competitive_navigation_classifier_assets_manifest_bytes_v1,
    invoke_verified_competitive_sideboard_classifier_process_v1, sha256_hex_v1,
    verify_runtime_identity_now_v1, OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
};
use mtgo_blackbox_v1::{
    validate_competitive_sideboard_selection_v1,
    validate_visible_competitive_lifecycle_snapshot_v1,
    validate_visible_competitive_sideboard_snapshot_v1, visible_frame_region_content_sha256_v1,
    CheckedUntrustedMtgoCompetitiveSideboardPlanV1,
    CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1, MtgoCompetitiveDeckConfigurationV1,
    MtgoCompetitiveEventKindV1, MtgoCompetitiveLifecyclePhaseV1,
    MtgoCompetitiveSideboardSelectionV1, MtgoCompetitiveSideboardTransferV1, MtgoSizePxV1,
    MtgoVisibleCompetitiveLifecycleSnapshotV1, MtgoVisibleCompetitiveSideboardSnapshotV1,
    ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

const COMPETITIVE_SIDEBOARD_CLASSIFIER_REQUEST_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-sideboard-classifier-request-v1";
const COMPETITIVE_SIDEBOARD_CLASSIFIER_RESULT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-sideboard-classifier-result-v1";
const COMPETITIVE_SIDEBOARD_MODEL_PLAN_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-sideboard-model-plan-v1";
const MAX_SIDEBOARD_REQUEST_HEADER_BYTES_V1: usize = 1024 * 1024;
const MAX_SIDEBOARD_ASSETS_MANIFEST_BYTES_V1: usize = 16 * 1024 * 1024;

/// Canonical JSON header followed by the exact classifier assets and the
/// tightly packed BGRA8 frame in the private sideboard parser protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveSideboardClassifierRequestHeaderV1 {
    pub schema_version: u32,
    pub protocol: String,
    pub parser_scope: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
    pub canonical_width: u32,
    pub canonical_height: u32,
    pub canonical_stride: u32,
    pub canonical_byte_length: u64,
    pub canonical_bgra8_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub source_navigation_classification_result_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub classifier_binary_sha256: String,
    pub classifier_assets_manifest_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveSideboardClassifierProcessResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub lifecycle: MtgoVisibleCompetitiveLifecycleSnapshotV1,
    pub sideboard: MtgoVisibleCompetitiveSideboardSnapshotV1,
}

/// Structurally checked request bytes. This value retains no pixels and does
/// not prove that a request came from an opaque capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUntrustedMtgoCompetitiveSideboardClassifierRequestV1 {
    header: MtgoCompetitiveSideboardClassifierRequestHeaderV1,
    request_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveSideboardClassifierRequestV1 {
    pub fn header_v1(&self) -> &MtgoCompetitiveSideboardClassifierRequestHeaderV1 {
        &self.header
    }

    pub fn request_commitment_sha256_v1(&self) -> &str {
        &self.request_commitment_sha256
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoClassifiedCompetitiveSideboardCommitmentsV1 {
    pub navigation_profile_commitment_sha256: String,
    pub navigation_profile_admission_commitment_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub approved_account_alias_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub source_navigation_classification_result_commitment_sha256: String,
    pub source_lifecycle_snapshot_commitment_sha256: String,
    pub request_commitment_sha256: String,
    pub classifier_response_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub sideboard_snapshot_commitment_sha256: String,
    pub classification_result_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
}

/// One exact classified navigation frame passed through the bounded sideboard
/// parser and same-frame visible-region rehashing. The parser is not ratified
/// by the current build, so this remains checked-untrusted and non-actionable.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitiveSideboardV1;
/// fn cannot_act(value: &OpaqueMtgoClassifiedCompetitiveSideboardV1) {
///     let _ = value.canonical_bgra8();
///     let _ = value.card_rectangles();
///     let _ = value.input_command();
///     let _ = value.submit_sideboard();
/// }
/// ```
pub struct OpaqueMtgoClassifiedCompetitiveSideboardV1 {
    pub(crate) source_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    pub(crate) sideboard: CheckedUntrustedMtgoCompetitiveSideboardSnapshotV1,
    pub(crate) visible_cards: Vec<mtgo_blackbox_v1::MtgoVisibleCompetitiveSideboardCardV1>,
    pub(crate) mainboard_zone: mtgo_blackbox_v1::MtgoVisibleCompetitiveSideboardZoneV1,
    pub(crate) sideboard_zone: mtgo_blackbox_v1::MtgoVisibleCompetitiveSideboardZoneV1,
    pub(crate) commitments: MtgoClassifiedCompetitiveSideboardCommitmentsV1,
}

impl OpaqueMtgoClassifiedCompetitiveSideboardV1 {
    pub fn commitments_v1(&self) -> MtgoClassifiedCompetitiveSideboardCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn configuration_v1(&self) -> &MtgoCompetitiveDeckConfigurationV1 {
        self.sideboard.configuration_v1()
    }

    pub fn source_snapshot_commitment_sha256_v1(&self) -> &str {
        self.sideboard.snapshot_commitment_sha256()
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPlannedCompetitiveSideboardCommitmentsV1 {
    pub classification_result_commitment_sha256: String,
    pub source_snapshot_commitment_sha256: String,
    pub deck_list_sha256: String,
    pub deck_manifest_commitment_sha256: String,
    pub deck_format_sha256: String,
    pub policy_deployment_commitment_sha256: String,
    pub plan_commitment_sha256: String,
    pub model_plan_binding_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub match_identity_sha256: String,
    pub game_number: u8,
    pub source_frame_sequence: u64,
    pub transfer_count: u16,
    pub no_changes_selected: bool,
}

/// Coordinate-free model-selected plan retaining the exact opaque source
/// frame. It exposes transfer semantics only, never their visible rectangles.
pub struct OpaqueMtgoPlannedCompetitiveSideboardV1 {
    pub(crate) source_frame: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    pub(crate) plan: CheckedUntrustedMtgoCompetitiveSideboardPlanV1,
    pub(crate) visible_cards: Vec<mtgo_blackbox_v1::MtgoVisibleCompetitiveSideboardCardV1>,
    pub(crate) mainboard_zone: mtgo_blackbox_v1::MtgoVisibleCompetitiveSideboardZoneV1,
    pub(crate) sideboard_zone: mtgo_blackbox_v1::MtgoVisibleCompetitiveSideboardZoneV1,
    pub(crate) commitments: MtgoPlannedCompetitiveSideboardCommitmentsV1,
}

impl OpaqueMtgoPlannedCompetitiveSideboardV1 {
    pub fn commitments_v1(&self) -> MtgoPlannedCompetitiveSideboardCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn target_configuration_v1(&self) -> &MtgoCompetitiveDeckConfigurationV1 {
        self.plan.target_configuration_v1()
    }

    pub fn transfers_v1(&self) -> &[MtgoCompetitiveSideboardTransferV1] {
        self.plan.transfers_v1()
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

pub fn check_untrusted_competitive_sideboard_classifier_request_v1(
    canonical_header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitiveSideboardClassifierRequestV1, String> {
    if canonical_header_json.is_empty()
        || canonical_header_json.len() > MAX_SIDEBOARD_REQUEST_HEADER_BYTES_V1
        || classifier_assets_manifest.is_empty()
        || classifier_assets_manifest.len() > MAX_SIDEBOARD_ASSETS_MANIFEST_BYTES_V1
    {
        return Err("sideboard classifier request header or assets are outside bounds".to_owned());
    }
    let header: MtgoCompetitiveSideboardClassifierRequestHeaderV1 =
        serde_json::from_slice(canonical_header_json)
            .map_err(|error| format!("parse sideboard classifier request: {error}"))?;
    let canonical = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize sideboard classifier request: {error}"))?;
    if canonical != canonical_header_json {
        return Err("sideboard classifier request is not canonical JSON".to_owned());
    }
    if header.schema_version != 1
        || header.protocol != "mtgo_visible_competitive_sideboard_v1"
        || header.parser_scope
            != "league_or_challenge_exact_deck_policy_between_game_sideboard_checked_untrusted_v1"
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
    {
        return Err("sideboard classifier request identity is invalid".to_owned());
    }
    let stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("sideboard classifier stride overflow")?;
    let byte_length = u64::from(stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("sideboard classifier byte length overflow")?;
    if header.canonical_stride != stride
        || header.canonical_byte_length != byte_length
        || usize::try_from(byte_length).ok() != Some(canonical_bgra8.len())
        || header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(classifier_assets_manifest)
    {
        return Err("sideboard classifier pixels or assets differ from the header".to_owned());
    }
    for digest in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.source_frame_profile_binding_sha256.as_str(),
        header
            .source_navigation_classification_result_commitment_sha256
            .as_str(),
        header.source_lifecycle_snapshot_commitment_sha256.as_str(),
        header.navigation_profile_commitment_sha256.as_str(),
        header
            .navigation_profile_admission_commitment_sha256
            .as_str(),
        header.approved_account_alias_sha256.as_str(),
        header.runtime_identity_commitment_sha256.as_str(),
        header.classifier_binary_sha256.as_str(),
        header.classifier_assets_manifest_sha256.as_str(),
        header.deck_list_sha256.as_str(),
        header.deck_manifest_commitment_sha256.as_str(),
        header.deck_format_sha256.as_str(),
        header.policy_deployment_commitment_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256(digest) {
            return Err("sideboard classifier request contains an invalid commitment".to_owned());
        }
    }
    if header.deck_list_sha256 == header.deck_format_sha256
        || header.deck_list_sha256 == header.policy_deployment_commitment_sha256
        || header.deck_manifest_commitment_sha256 == header.deck_format_sha256
        || header.deck_manifest_commitment_sha256 == header.policy_deployment_commitment_sha256
        || header.deck_format_sha256 == header.policy_deployment_commitment_sha256
    {
        return Err(
            "sideboard deck, format, and policy identities must remain distinct".to_owned(),
        );
    }
    let request_commitment_sha256 = commitment_parts_v1(
        COMPETITIVE_SIDEBOARD_CLASSIFIER_REQUEST_DOMAIN_V1,
        &[
            canonical_header_json,
            classifier_assets_manifest,
            canonical_bgra8,
            b"checked_untrusted_sideboard_request_no_live_classification_no_input",
        ],
    );
    Ok(
        CheckedUntrustedMtgoCompetitiveSideboardClassifierRequestV1 {
            header,
            request_commitment_sha256,
        },
    )
}

/// Runs the exact navigation-profile-pinned binary in sideboard mode over the
/// retained source frame. The result remains checked-untrusted until a reviewed
/// sideboard corpus and parser identity are separately ratified.
pub fn classify_checked_untrusted_competitive_sideboard_v1(
    source: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    manifest: &ValidatedMtgoCompetitiveDeckManifestV1,
    policy_deployment_commitment_sha256: String,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitiveSideboardV1, String> {
    if !(100..=60_000).contains(&timeout_ms) {
        return Err("sideboard classifier timeout must be between 100 and 60000 ms".to_owned());
    }
    let source_commitments = source.commitments_v1();
    let runtime_commitments = runtime.commitments_v1();
    if source.phase_v1() != MtgoCompetitiveLifecyclePhaseV1::Sideboarding
        || source.lifecycle_snapshot_v1().phase() != MtgoCompetitiveLifecyclePhaseV1::Sideboarding
        || source_commitments.runtime_identity_commitment_sha256
            != runtime_commitments.runtime_identity_commitment_sha256
        || source_commitments.source_frame.profile_commitment_sha256
            != runtime_commitments.navigation_profile_commitment_sha256
        || source_commitments
            .source_frame
            .profile_admission_commitment_sha256
            != runtime_commitments.navigation_profile_admission_commitment_sha256
        || source_commitments
            .source_frame
            .approved_account_alias_sha256
            != runtime_commitments.approved_account_alias_sha256
    {
        return Err("sideboard source, runtime, profile, account, or phase differs".to_owned());
    }
    if !looks_like_lower_sha256(&policy_deployment_commitment_sha256)
        || policy_deployment_commitment_sha256 == manifest.deck_list_sha256()
        || policy_deployment_commitment_sha256 == manifest.manifest_commitment_sha256()
        || policy_deployment_commitment_sha256 == manifest.format_sha256()
    {
        return Err("sideboard policy deployment identity is invalid or crossed".to_owned());
    }
    verify_runtime_identity_now_v1(runtime)?;

    let raw_source = &source._source_frame.source_frame;
    let width = raw_source.manifest.frame.canonical_width;
    let height = raw_source.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("sideboard classifier canonical stride overflow")?;
    let byte_length = u64::try_from(raw_source.canonical_bgra8.len())
        .map_err(|_| "sideboard classifier canonical byte length overflow")?;
    if raw_source.manifest.frame.canonical_stride != stride
        || raw_source.manifest.frame.canonical_byte_length != raw_source.canonical_bgra8.len()
        || raw_source.manifest.frame.canonical_bgra8_sha256
            != sha256_hex_v1(&raw_source.canonical_bgra8)
        || raw_source.capture_commitment_sha256
            != source_commitments
                .source_frame
                .source_capture
                .capture_commitment_sha256
    {
        return Err("opaque sideboard source pixels no longer match capture metadata".to_owned());
    }
    let assets = competitive_navigation_classifier_assets_manifest_bytes_v1(runtime);
    let header = MtgoCompetitiveSideboardClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_sideboard_v1".to_owned(),
        parser_scope:
            "league_or_challenge_exact_deck_policy_between_game_sideboard_checked_untrusted_v1"
                .to_owned(),
        frame_id: source_commitments.frame_id,
        frame_sequence: source_commitments.frame_sequence,
        captured_at_unix_millis: raw_source.manifest.captured_at_unix_millis,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: stride,
        canonical_byte_length: byte_length,
        canonical_bgra8_sha256: raw_source.manifest.frame.canonical_bgra8_sha256.clone(),
        source_capture_commitment_sha256: raw_source.capture_commitment_sha256.clone(),
        source_frame_profile_binding_sha256: source_commitments
            .source_frame
            .frame_profile_binding_sha256
            .clone(),
        source_navigation_classification_result_commitment_sha256: source_commitments
            .classification_result_commitment_sha256
            .clone(),
        source_lifecycle_snapshot_commitment_sha256: source_commitments
            .lifecycle_snapshot_commitment_sha256
            .clone(),
        navigation_profile_commitment_sha256: source_commitments
            .source_frame
            .profile_commitment_sha256
            .clone(),
        navigation_profile_admission_commitment_sha256: source_commitments
            .source_frame
            .profile_admission_commitment_sha256
            .clone(),
        approved_account_alias_sha256: source_commitments
            .source_frame
            .approved_account_alias_sha256
            .clone(),
        runtime_identity_commitment_sha256: runtime_commitments
            .runtime_identity_commitment_sha256
            .clone(),
        classifier_binary_sha256: runtime_commitments.classifier_binary_sha256.clone(),
        classifier_assets_manifest_sha256: runtime_commitments
            .classifier_assets_manifest_sha256
            .clone(),
        deck_list_sha256: manifest.deck_list_sha256().to_owned(),
        deck_manifest_commitment_sha256: manifest.manifest_commitment_sha256().to_owned(),
        deck_format_sha256: manifest.format_sha256().to_owned(),
        policy_deployment_commitment_sha256: policy_deployment_commitment_sha256.clone(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize sideboard classifier request: {error}"))?;
    let checked_request = check_untrusted_competitive_sideboard_classifier_request_v1(
        &header_json,
        assets,
        &raw_source.canonical_bgra8,
    )?;
    let request_commitment_sha256 = checked_request.request_commitment_sha256_v1().to_owned();
    let response_bytes = invoke_verified_competitive_sideboard_classifier_process_v1(
        runtime,
        &header_json,
        assets,
        &raw_source.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_runtime_identity_now_v1(runtime)?;
    let response =
        parse_sideboard_classifier_response_v1(&response_bytes, &request_commitment_sha256)?;
    let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(response.lifecycle.clone())
        .map_err(|error| format!("validate sideboard lifecycle response: {error}"))?;
    if lifecycle.snapshot_commitment_sha256()
        != source_commitments.lifecycle_snapshot_commitment_sha256
        || lifecycle.frame_id_v1() != source_commitments.frame_id
        || lifecycle.frame_sequence() != source_commitments.frame_sequence
        || lifecycle.frame_sha256_v1() != raw_source.manifest.frame.canonical_bgra8_sha256
        || lifecycle.client_bounds_v1().x != 0
        || lifecycle.client_bounds_v1().y != 0
        || lifecycle.client_bounds_v1().width != width
        || lifecycle.client_bounds_v1().height != height
    {
        return Err("sideboard lifecycle response differs from the exact source frame".to_owned());
    }
    let size = MtgoSizePxV1 { width, height };
    rehash_lifecycle_visible_facts_v1(&response.lifecycle, &raw_source.canonical_bgra8, &size)?;
    rehash_sideboard_visible_cards_v1(&response.sideboard, &raw_source.canonical_bgra8, &size)?;
    let visible_cards = response.sideboard.cards.clone();
    let mainboard_zone = response.sideboard.mainboard_zone.clone();
    let sideboard_zone = response.sideboard.sideboard_zone.clone();
    let sideboard =
        validate_visible_competitive_sideboard_snapshot_v1(lifecycle, manifest, response.sideboard)
            .map_err(|error| format!("validate visible sideboard response: {error}"))?;
    let classifier_response_sha256 = sha256_hex_v1(&response_bytes);
    let sideboard_snapshot_commitment_sha256 = sideboard.snapshot_commitment_sha256().to_owned();
    let classification_result_commitment_sha256 = commitment_parts_v1(
        COMPETITIVE_SIDEBOARD_CLASSIFIER_RESULT_DOMAIN_V1,
        &[
            source_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            source_commitments
                .lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            request_commitment_sha256.as_bytes(),
            classifier_response_sha256.as_bytes(),
            manifest.deck_list_sha256().as_bytes(),
            manifest.manifest_commitment_sha256().as_bytes(),
            manifest.format_sha256().as_bytes(),
            policy_deployment_commitment_sha256.as_bytes(),
            sideboard_snapshot_commitment_sha256.as_bytes(),
            b"same_frame_rehashed_sideboard_checked_untrusted_no_input_no_submit",
        ],
    );
    let commitments = MtgoClassifiedCompetitiveSideboardCommitmentsV1 {
        navigation_profile_commitment_sha256: source_commitments
            .source_frame
            .profile_commitment_sha256,
        navigation_profile_admission_commitment_sha256: source_commitments
            .source_frame
            .profile_admission_commitment_sha256,
        runtime_identity_commitment_sha256: runtime_commitments.runtime_identity_commitment_sha256,
        approved_account_alias_sha256: source_commitments
            .source_frame
            .approved_account_alias_sha256,
        source_capture_commitment_sha256: source_commitments
            .source_frame
            .source_capture
            .capture_commitment_sha256,
        source_frame_profile_binding_sha256: source_commitments
            .source_frame
            .frame_profile_binding_sha256,
        source_navigation_classification_result_commitment_sha256: source_commitments
            .classification_result_commitment_sha256,
        source_lifecycle_snapshot_commitment_sha256: source_commitments
            .lifecycle_snapshot_commitment_sha256,
        request_commitment_sha256,
        classifier_response_sha256,
        deck_list_sha256: manifest.deck_list_sha256().to_owned(),
        deck_manifest_commitment_sha256: manifest.manifest_commitment_sha256().to_owned(),
        deck_format_sha256: manifest.format_sha256().to_owned(),
        policy_deployment_commitment_sha256,
        sideboard_snapshot_commitment_sha256,
        classification_result_commitment_sha256,
        event_kind: sideboard.event_kind(),
        event_identity_sha256: sideboard.event_identity_sha256().to_owned(),
        match_identity_sha256: sideboard.match_identity_sha256().to_owned(),
        game_number: sideboard.game_number(),
        frame_id: sideboard.frame_id(),
        frame_sequence: sideboard.frame_sequence(),
        captured_at_unix_millis: raw_source.manifest.captured_at_unix_millis,
    };
    Ok(OpaqueMtgoClassifiedCompetitiveSideboardV1 {
        source_frame: source,
        sideboard,
        visible_cards,
        mainboard_zone,
        sideboard_zone,
        commitments,
    })
}

pub fn plan_classified_competitive_sideboard_v1(
    classified: OpaqueMtgoClassifiedCompetitiveSideboardV1,
    selection: MtgoCompetitiveSideboardSelectionV1,
) -> Result<OpaqueMtgoPlannedCompetitiveSideboardV1, String> {
    let OpaqueMtgoClassifiedCompetitiveSideboardV1 {
        source_frame,
        sideboard,
        visible_cards,
        mainboard_zone,
        sideboard_zone,
        commitments: classified_commitments,
    } = classified;
    let plan = validate_competitive_sideboard_selection_v1(sideboard, selection)
        .map_err(|error| format!("validate model-selected sideboard: {error}"))?;
    let transfer_count = u16::try_from(plan.transfers_v1().len())
        .map_err(|_| "sideboard transfer count overflow")?;
    let no_changes_selected = plan.no_changes_selected_v1();
    let plan_commitment_sha256 = plan.plan_commitment_sha256().to_owned();
    let model_plan_binding_commitment_sha256 = commitment_parts_v1(
        COMPETITIVE_SIDEBOARD_MODEL_PLAN_DOMAIN_V1,
        &[
            classified_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            classified_commitments
                .sideboard_snapshot_commitment_sha256
                .as_bytes(),
            classified_commitments.deck_list_sha256.as_bytes(),
            classified_commitments
                .deck_manifest_commitment_sha256
                .as_bytes(),
            classified_commitments.deck_format_sha256.as_bytes(),
            classified_commitments
                .policy_deployment_commitment_sha256
                .as_bytes(),
            plan_commitment_sha256.as_bytes(),
            &transfer_count.to_be_bytes(),
            &[u8::from(no_changes_selected)],
            b"coordinate_free_model_sideboard_plan_no_input_no_submit",
        ],
    );
    let commitments = MtgoPlannedCompetitiveSideboardCommitmentsV1 {
        classification_result_commitment_sha256: classified_commitments
            .classification_result_commitment_sha256,
        source_snapshot_commitment_sha256: classified_commitments
            .sideboard_snapshot_commitment_sha256,
        deck_list_sha256: classified_commitments.deck_list_sha256,
        deck_manifest_commitment_sha256: classified_commitments.deck_manifest_commitment_sha256,
        deck_format_sha256: classified_commitments.deck_format_sha256,
        policy_deployment_commitment_sha256: classified_commitments
            .policy_deployment_commitment_sha256,
        plan_commitment_sha256,
        model_plan_binding_commitment_sha256,
        event_kind: classified_commitments.event_kind,
        event_identity_sha256: classified_commitments.event_identity_sha256,
        match_identity_sha256: classified_commitments.match_identity_sha256,
        game_number: classified_commitments.game_number,
        source_frame_sequence: classified_commitments.frame_sequence,
        transfer_count,
        no_changes_selected,
    };
    Ok(OpaqueMtgoPlannedCompetitiveSideboardV1 {
        source_frame,
        plan,
        visible_cards,
        mainboard_zone,
        sideboard_zone,
        commitments,
    })
}

fn parse_sideboard_classifier_response_v1(
    response_bytes: &[u8],
    request_commitment_sha256: &str,
) -> Result<MtgoCompetitiveSideboardClassifierProcessResponseV1, String> {
    let response: MtgoCompetitiveSideboardClassifierProcessResponseV1 =
        serde_json::from_slice(response_bytes).map_err(|error| {
            format!("sideboard classifier response is not one strict JSON value: {error}")
        })?;
    let canonical = serde_json::to_vec(&response)
        .map_err(|error| format!("serialize sideboard classifier response: {error}"))?;
    if canonical != response_bytes {
        return Err("sideboard classifier response is not canonical JSON".to_owned());
    }
    if response.schema_version != 1
        || response.request_commitment_sha256 != request_commitment_sha256
    {
        return Err("sideboard classifier response does not bind the exact request".to_owned());
    }
    Ok(response)
}

fn rehash_lifecycle_visible_facts_v1(
    lifecycle: &MtgoVisibleCompetitiveLifecycleSnapshotV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), String> {
    for fact in &lifecycle.facts {
        let actual =
            visible_frame_region_content_sha256_v1(canonical_bgra8, size, &fact.rect_client_px)
                .map_err(|error| format!("rehash sideboard lifecycle fact: {error}"))?;
        if actual != fact.content_sha256 {
            return Err("sideboard lifecycle fact differs from retained pixels".to_owned());
        }
    }
    Ok(())
}

fn rehash_sideboard_visible_cards_v1(
    sideboard: &MtgoVisibleCompetitiveSideboardSnapshotV1,
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), String> {
    for zone in [&sideboard.mainboard_zone, &sideboard.sideboard_zone] {
        for (label, rect, expected) in [
            ("zone", &zone.rect_client_px, zone.content_sha256.as_str()),
            (
                "empty drop target",
                &zone.empty_drop_rect_client_px,
                zone.empty_drop_content_sha256.as_str(),
            ),
        ] {
            let actual = visible_frame_region_content_sha256_v1(canonical_bgra8, size, rect)
                .map_err(|error| format!("rehash visible sideboard {label}: {error}"))?;
            if actual != expected {
                return Err(format!(
                    "visible sideboard {label} differs from retained pixels"
                ));
            }
        }
    }
    for card in &sideboard.cards {
        let actual =
            visible_frame_region_content_sha256_v1(canonical_bgra8, size, &card.rect_client_px)
                .map_err(|error| format!("rehash visible sideboard card: {error}"))?;
        if actual != card.content_sha256 {
            return Err("visible sideboard card differs from retained pixels".to_owned());
        }
    }
    Ok(())
}

fn looks_like_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn commitment_parts_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn request_header(
        pixels: &[u8],
        assets: &[u8],
    ) -> MtgoCompetitiveSideboardClassifierRequestHeaderV1 {
        MtgoCompetitiveSideboardClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: "mtgo_visible_competitive_sideboard_v1".to_owned(),
            parser_scope:
                "league_or_challenge_exact_deck_policy_between_game_sideboard_checked_untrusted_v1"
                    .to_owned(),
            frame_id: 7,
            frame_sequence: 11,
            captured_at_unix_millis: 1_786_000_000_000,
            canonical_width: 2,
            canonical_height: 1,
            canonical_stride: 8,
            canonical_byte_length: 8,
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: digest('1'),
            source_frame_profile_binding_sha256: digest('2'),
            source_navigation_classification_result_commitment_sha256: digest('3'),
            source_lifecycle_snapshot_commitment_sha256: digest('4'),
            navigation_profile_commitment_sha256: digest('5'),
            navigation_profile_admission_commitment_sha256: digest('6'),
            approved_account_alias_sha256: digest('7'),
            runtime_identity_commitment_sha256: digest('8'),
            classifier_binary_sha256: digest('9'),
            classifier_assets_manifest_sha256: sha256_hex_v1(assets),
            deck_list_sha256: digest('a'),
            deck_manifest_commitment_sha256: digest('b'),
            deck_format_sha256: digest('c'),
            policy_deployment_commitment_sha256: digest('d'),
        }
    }

    #[test]
    fn request_checker_binds_pixels_assets_and_all_identity_layers() {
        let pixels = [1_u8, 2, 3, 255, 4, 5, 6, 255];
        let assets = b"sideboard-assets";
        let header = request_header(&pixels, assets);
        let json = serde_json::to_vec(&header).unwrap();
        let checked =
            check_untrusted_competitive_sideboard_classifier_request_v1(&json, assets, &pixels)
                .unwrap();
        assert_eq!(checked.header_v1(), &header);
        assert_eq!(checked.request_commitment_sha256_v1().len(), 64);
        assert!(!checked.safe_for_live_classification_v1());
        assert!(!checked.safe_for_input_v1());
    }

    #[test]
    fn request_checker_rejects_pixel_asset_and_identity_substitution() {
        let pixels = [1_u8, 2, 3, 255, 4, 5, 6, 255];
        let assets = b"sideboard-assets";
        let baseline = request_header(&pixels, assets);
        let mut changed_pixels = pixels;
        changed_pixels[0] ^= 1;
        assert!(check_untrusted_competitive_sideboard_classifier_request_v1(
            &serde_json::to_vec(&baseline).unwrap(),
            assets,
            &changed_pixels,
        )
        .is_err());
        assert!(check_untrusted_competitive_sideboard_classifier_request_v1(
            &serde_json::to_vec(&baseline).unwrap(),
            b"different-assets",
            &pixels,
        )
        .is_err());

        for mutation in 0..5 {
            let mut changed = baseline.clone();
            match mutation {
                0 => changed.source_lifecycle_snapshot_commitment_sha256 = "BAD".to_owned(),
                1 => changed.deck_list_sha256 = changed.deck_format_sha256.clone(),
                2 => {
                    changed.deck_manifest_commitment_sha256 =
                        changed.policy_deployment_commitment_sha256.clone()
                }
                3 => changed.frame_sequence = 0,
                4 => changed.parser_scope = "navigation_only".to_owned(),
                _ => unreachable!(),
            }
            assert!(check_untrusted_competitive_sideboard_classifier_request_v1(
                &serde_json::to_vec(&changed).unwrap(),
                assets,
                &pixels,
            )
            .is_err());
        }
    }

    #[test]
    fn request_checker_rejects_unknown_or_noncanonical_json() {
        let pixels = [1_u8, 2, 3, 255, 4, 5, 6, 255];
        let assets = b"sideboard-assets";
        let header = request_header(&pixels, assets);
        let mut json = serde_json::to_vec(&header).unwrap();
        json.insert(1, b' ');
        assert!(check_untrusted_competitive_sideboard_classifier_request_v1(
            &json, assets, &pixels,
        )
        .is_err());

        let json = serde_json::to_string(&header)
            .unwrap()
            .replacen('{', "{\"x\":1,", 1);
        assert!(check_untrusted_competitive_sideboard_classifier_request_v1(
            json.as_bytes(),
            assets,
            &pixels,
        )
        .is_err());
    }
}
