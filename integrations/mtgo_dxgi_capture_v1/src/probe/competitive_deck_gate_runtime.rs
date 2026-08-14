use super::{
    bind_checked_competitive_event_listing_parts_v1,
    competitive_navigation_classifier_assets_manifest_bytes_v1,
    invoke_verified_competitive_deck_gate_classifier_process_v1, sha256_hex_v1,
    verify_runtime_identity_now_v1, MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
    OpaqueMtgoSourceBoundCompetitiveEventListingV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
};
use mtgo_blackbox_v1::{
    competitive_event_listing_target_commitment_v1,
    promote_open_entry_review_available_deck_gate_v1, validate_visible_competitive_deck_gate_v1,
    visible_frame_region_content_sha256_v1, CheckedUntrustedMtgoCompetitiveDeckGateV1,
    CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1, MtgoCompetitiveDeckGateStateV1,
    MtgoCompetitiveEventListingTargetV1, MtgoCompetitiveLifecyclePhaseV1, MtgoRectPxV1,
    MtgoSizePxV1, MtgoVisibleCompetitiveDeckGateV1, ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

const DECK_GATE_CLASSIFIER_REQUEST_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-gate-classifier-request-v1";
const DECK_GATE_CLASSIFIER_RESULT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-gate-classifier-result-v1";
const MAX_DECK_GATE_REQUEST_HEADER_BYTES_V1: usize = 1024 * 1024;
const MAX_DECK_GATE_ASSETS_MANIFEST_BYTES_V1: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveDeckGateClassifierRequestHeaderV1 {
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
    pub target_commitment_sha256: String,
    pub target: MtgoCompetitiveEventListingTargetV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveDeckGateClassifierProcessResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub gate: MtgoVisibleCompetitiveDeckGateV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUntrustedMtgoCompetitiveDeckGateClassifierRequestV1 {
    header: MtgoCompetitiveDeckGateClassifierRequestHeaderV1,
    request_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveDeckGateClassifierRequestV1 {
    pub fn header_v1(&self) -> &MtgoCompetitiveDeckGateClassifierRequestHeaderV1 {
        &self.header
    }

    pub fn request_commitment_sha256_v1(&self) -> &str {
        &self.request_commitment_sha256
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoClassifiedCompetitiveDeckGateCommitmentsV1 {
    pub source_navigation: MtgoClassifiedCompetitiveNavigationFrameCommitmentsV1,
    pub target_commitment_sha256: String,
    pub observation_commitment_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub request_commitment_sha256: String,
    pub classifier_response_sha256: String,
    pub classification_result_commitment_sha256: String,
    pub state: MtgoCompetitiveDeckGateStateV1,
}

/// One exact three-state pre-entry deck-gate interpretation retained with its
/// opaque source frame. It is move-only and exposes commitments and state only.
/// It has no pixel, coordinate, chooser, entry, spending, or input accessor.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitiveDeckGateV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoClassifiedCompetitiveDeckGateV1>();
/// ```
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitiveDeckGateV1;
/// fn cannot_extract_or_control(value: &OpaqueMtgoClassifiedCompetitiveDeckGateV1) {
///     let _ = value.canonical_bgra8_v1();
///     let _ = value.open_entry_review_control_rect_client_px();
///     value.click();
/// }
/// ```
pub struct OpaqueMtgoClassifiedCompetitiveDeckGateV1 {
    source_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    navigation_classification: OpaqueMtgoRetainedCompetitiveNavigationClassificationV1,
    gate: CheckedUntrustedMtgoCompetitiveDeckGateV1,
    raw: MtgoVisibleCompetitiveDeckGateV1,
    commitments: MtgoClassifiedCompetitiveDeckGateCommitmentsV1,
}

impl OpaqueMtgoClassifiedCompetitiveDeckGateV1 {
    pub fn commitments_v1(&self) -> MtgoClassifiedCompetitiveDeckGateCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn state_v1(&self) -> MtgoCompetitiveDeckGateStateV1 {
        self.commitments.state
    }

    pub fn ready_for_open_entry_review_v1(&self) -> bool {
        self.commitments.state == MtgoCompetitiveDeckGateStateV1::OpenEntryReviewAvailable
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }

    pub(super) fn source_frame_v1(&self) -> &OpaqueMtgoAdmittedCompetitiveNavigationFrameV1 {
        &self.source_frame
    }
}

pub fn check_untrusted_competitive_deck_gate_classifier_request_v1(
    canonical_header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> Result<CheckedUntrustedMtgoCompetitiveDeckGateClassifierRequestV1, String> {
    if canonical_header_json.is_empty()
        || canonical_header_json.len() > MAX_DECK_GATE_REQUEST_HEADER_BYTES_V1
        || classifier_assets_manifest.is_empty()
        || classifier_assets_manifest.len() > MAX_DECK_GATE_ASSETS_MANIFEST_BYTES_V1
    {
        return Err("deck-gate classifier request header or assets are outside bounds".to_owned());
    }
    let header: MtgoCompetitiveDeckGateClassifierRequestHeaderV1 =
        serde_json::from_slice(canonical_header_json)
            .map_err(|error| format!("parse deck-gate classifier request header: {error}"))?;
    let canonical = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize deck-gate classifier request header: {error}"))?;
    if canonical != canonical_header_json {
        return Err("deck-gate classifier request header is not canonical JSON".to_owned());
    }
    if header.schema_version != 1
        || header.protocol != "mtgo_visible_competitive_deck_gate_v1"
        || header.parser_scope != "league_and_challenge_three_state_deck_gate_checked_untrusted_v1"
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.captured_at_unix_millis == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
    {
        return Err("deck-gate classifier request identity is invalid".to_owned());
    }
    let expected_stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("deck-gate classifier stride overflow")?;
    let expected_length = u64::from(expected_stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("deck-gate classifier byte length overflow")?;
    if header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || usize::try_from(expected_length).ok() != Some(canonical_bgra8.len())
        || header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(classifier_assets_manifest)
    {
        return Err("deck-gate classifier pixels or assets differ from the header".to_owned());
    }
    for value in deck_gate_header_digests_v1(&header) {
        if !looks_like_lower_sha256_v1(value) {
            return Err("deck-gate classifier request contains an invalid commitment".to_owned());
        }
    }
    let target_commitment_sha256 =
        competitive_event_listing_target_commitment_v1(&header.target, deck)
            .map_err(|error| format!("validate deck-gate classifier target: {error}"))?;
    if header.target_commitment_sha256 != target_commitment_sha256
        || header.target.approved_account_alias_sha256 != header.approved_account_alias_sha256
        || header.target.deck_list_sha256 != deck.deck_list_sha256()
        || header.target.deck_manifest_commitment_sha256 != deck.manifest_commitment_sha256()
        || header.target.deck_format_sha256 != deck.format_sha256()
    {
        return Err(
            "deck-gate classifier target differs from the exact account or deck".to_owned(),
        );
    }
    let request_commitment_sha256 = commitment_v1(
        DECK_GATE_CLASSIFIER_REQUEST_DOMAIN_V1,
        &[
            canonical_header_json,
            classifier_assets_manifest,
            canonical_bgra8,
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitiveDeckGateClassifierRequestV1 {
        header,
        request_commitment_sha256,
    })
}

pub fn check_untrusted_competitive_deck_gate_pixels_v1(
    lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    raw: MtgoVisibleCompetitiveDeckGateV1,
    expected_approved_account_alias_sha256: &str,
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitiveDeckGateV1, String> {
    let bounds = lifecycle.client_bounds_v1();
    let size = MtgoSizePxV1 {
        width: bounds.width,
        height: bounds.height,
    };
    let expected_length = usize::try_from(
        u64::from(size.width)
            .checked_mul(u64::from(size.height))
            .and_then(|value| value.checked_mul(4))
            .ok_or("deck-gate pixel length overflow")?,
    )
    .map_err(|_| "deck-gate pixel length does not fit this process")?;
    if bounds.x != 0
        || bounds.y != 0
        || canonical_bgra8.len() != expected_length
        || sha256_hex_v1(canonical_bgra8) != lifecycle.frame_sha256_v1()
    {
        return Err("deck-gate pixels do not match the exact lifecycle frame".to_owned());
    }
    for fact in lifecycle.visible_facts_v1() {
        let actual =
            visible_frame_region_content_sha256_v1(canonical_bgra8, &size, &fact.rect_client_px)
                .map_err(|error| format!("rehash deck-gate lifecycle fact: {error}"))?;
        if actual != fact.content_sha256 {
            return Err("deck-gate lifecycle fact differs from supplied pixels".to_owned());
        }
    }
    rehash_required_region_v1(
        canonical_bgra8,
        &size,
        &raw.event_label_rect_client_px,
        &raw.event_label_region_sha256,
        "event label",
    )?;
    rehash_required_region_v1(
        canonical_bgra8,
        &size,
        &raw.select_deck_control_rect_client_px,
        &raw.select_deck_control_region_sha256,
        "deck control",
    )?;
    rehash_optional_region_v1(
        canonical_bgra8,
        &size,
        raw.missing_deck_prompt_rect_client_px.as_ref(),
        raw.missing_deck_prompt_region_sha256.as_deref(),
        "missing-deck prompt",
    )?;
    rehash_optional_region_v1(
        canonical_bgra8,
        &size,
        raw.selected_deck_rect_client_px.as_ref(),
        raw.selected_deck_region_sha256.as_deref(),
        "selected deck",
    )?;
    rehash_optional_region_v1(
        canonical_bgra8,
        &size,
        raw.open_entry_review_control_rect_client_px.as_ref(),
        raw.open_entry_review_control_region_sha256.as_deref(),
        "Open Entry Review control",
    )?;
    if target.approved_account_alias_sha256 != expected_approved_account_alias_sha256 {
        return Err("deck-gate target does not bind the approved account".to_owned());
    }
    let checked = validate_visible_competitive_deck_gate_v1(lifecycle, deck, target, raw)
        .map_err(|error| format!("validate visible competitive deck gate: {error}"))?;
    Ok(checked)
}

pub fn classify_checked_untrusted_competitive_deck_gate_v1(
    source: OpaqueMtgoClassifiedCompetitiveNavigationFrameV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitiveDeckGateV1, String> {
    if !(100..=60_000).contains(&timeout_ms) {
        return Err("deck-gate classifier timeout must be between 100 and 60000 ms".to_owned());
    }
    let source_commitments = source.commitments_v1();
    let runtime_commitments = runtime.commitments_v1();
    if source.phase_v1() != MtgoCompetitiveLifecyclePhaseV1::EventBrowser
        || source.lifecycle_snapshot_v1().phase() != MtgoCompetitiveLifecyclePhaseV1::EventBrowser
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
        || target.approved_account_alias_sha256
            != source_commitments
                .source_frame
                .approved_account_alias_sha256
        || target.event_kind != source_commitments.event_kind
    {
        return Err("deck-gate source, runtime, account, target, or phase differs".to_owned());
    }
    let target_commitment_sha256 = competitive_event_listing_target_commitment_v1(&target, deck)
        .map_err(|error| format!("validate deck-gate target: {error}"))?;
    verify_runtime_identity_now_v1(runtime)?;

    let raw_source = &source._source_frame.source_frame;
    let width = raw_source.manifest.frame.canonical_width;
    let height = raw_source.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("deck-gate classifier canonical stride overflow")?;
    let byte_length = u64::try_from(raw_source.canonical_bgra8.len())
        .map_err(|_| "deck-gate classifier canonical byte length overflow")?;
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
        return Err("opaque deck-gate source pixels no longer match capture metadata".to_owned());
    }
    let assets = competitive_navigation_classifier_assets_manifest_bytes_v1(runtime);
    let header = MtgoCompetitiveDeckGateClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_deck_gate_v1".to_owned(),
        parser_scope: "league_and_challenge_three_state_deck_gate_checked_untrusted_v1".to_owned(),
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
        target_commitment_sha256,
        target: target.clone(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize deck-gate classifier request: {error}"))?;
    let checked_request = check_untrusted_competitive_deck_gate_classifier_request_v1(
        &header_json,
        assets,
        &raw_source.canonical_bgra8,
        deck,
    )?;
    let request_commitment_sha256 = checked_request.request_commitment_sha256_v1().to_owned();
    let response_bytes = invoke_verified_competitive_deck_gate_classifier_process_v1(
        runtime,
        &header_json,
        assets,
        &raw_source.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_runtime_identity_now_v1(runtime)?;
    let response =
        parse_deck_gate_classifier_response_v1(&response_bytes, &request_commitment_sha256)?;
    if response.gate.frame_id != source_commitments.frame_id
        || response.gate.frame_sequence != source_commitments.frame_sequence
        || response.gate.frame_sha256
            != source_commitments
                .source_frame
                .source_capture
                .canonical_bgra8_sha256
        || response.gate.source_lifecycle_snapshot_commitment_sha256
            != source_commitments.lifecycle_snapshot_commitment_sha256
        || response.gate.target_commitment_sha256 != header.target_commitment_sha256
    {
        return Err("deck-gate classifier response changed the exact source or target".to_owned());
    }
    let raw = response.gate;
    let raw_for_checked = raw.clone();
    let classifier_response_sha256 = sha256_hex_v1(&response_bytes);
    let (source_frame, lifecycle, navigation_classification) = source.into_event_listing_parts_v1();
    if source_commitments.source_frame != source_frame.commitments_v1() {
        return Err("deck-gate source-frame lineage changed".to_owned());
    }
    let gate = check_untrusted_competitive_deck_gate_pixels_v1(
        lifecycle,
        deck,
        target,
        raw_for_checked,
        &source_frame.approved_account_alias_sha256,
        &source_frame.source_frame.canonical_bgra8,
    )?;
    let observation_commitment_sha256 = gate.observation_commitment_sha256_v1().to_owned();
    let state = gate.state_v1();
    let state_json = serde_json::to_vec(&state)
        .map_err(|error| format!("serialize deck-gate state: {error}"))?;
    let classification_result_commitment_sha256 = commitment_v1(
        DECK_GATE_CLASSIFIER_RESULT_DOMAIN_V1,
        &[
            source_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            source_commitments
                .lifecycle_snapshot_commitment_sha256
                .as_bytes(),
            header.target_commitment_sha256.as_bytes(),
            observation_commitment_sha256.as_bytes(),
            runtime_commitments
                .runtime_identity_commitment_sha256
                .as_bytes(),
            request_commitment_sha256.as_bytes(),
            classifier_response_sha256.as_bytes(),
            state_json.as_slice(),
            b"three_state_deck_gate_unratified_no_deck_choice_no_entry_no_spending_no_input",
        ],
    );
    Ok(OpaqueMtgoClassifiedCompetitiveDeckGateV1 {
        source_frame,
        navigation_classification,
        gate,
        raw,
        commitments: MtgoClassifiedCompetitiveDeckGateCommitmentsV1 {
            source_navigation: source_commitments,
            target_commitment_sha256: header.target_commitment_sha256,
            observation_commitment_sha256,
            runtime_identity_commitment_sha256: runtime_commitments
                .runtime_identity_commitment_sha256,
            request_commitment_sha256,
            classifier_response_sha256,
            classification_result_commitment_sha256,
            state,
        },
    })
}

pub fn promote_classified_competitive_deck_gate_to_event_listing_v1(
    classified: OpaqueMtgoClassifiedCompetitiveDeckGateV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> Result<OpaqueMtgoSourceBoundCompetitiveEventListingV1, String> {
    let OpaqueMtgoClassifiedCompetitiveDeckGateV1 {
        source_frame,
        navigation_classification,
        gate,
        raw,
        commitments,
    } = classified;
    let open_entry_review_control_rect_client_px = raw
        .open_entry_review_control_rect_client_px
        .clone()
        .ok_or("classified deck gate has no visible Open Entry Review control")?;
    let selection = promote_open_entry_review_available_deck_gate_v1(gate, deck)
        .map_err(|error| format!("promote classified deck gate: {error}"))?;
    bind_checked_competitive_event_listing_parts_v1(
        commitments.source_navigation,
        source_frame,
        navigation_classification,
        selection,
        open_entry_review_control_rect_client_px,
    )
}

fn parse_deck_gate_classifier_response_v1(
    response_bytes: &[u8],
    request_commitment_sha256: &str,
) -> Result<MtgoCompetitiveDeckGateClassifierProcessResponseV1, String> {
    let response: MtgoCompetitiveDeckGateClassifierProcessResponseV1 =
        serde_json::from_slice(response_bytes).map_err(|error| {
            format!("deck-gate classifier response is not one strict JSON value: {error}")
        })?;
    let canonical = serde_json::to_vec(&response)
        .map_err(|error| format!("serialize deck-gate classifier response: {error}"))?;
    if canonical != response_bytes {
        return Err("deck-gate classifier response is not canonical JSON".to_owned());
    }
    if response.schema_version != 1
        || response.request_commitment_sha256 != request_commitment_sha256
    {
        return Err("deck-gate classifier response does not bind the exact request".to_owned());
    }
    Ok(response)
}

fn rehash_required_region_v1(
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
    rect: &MtgoRectPxV1,
    expected_sha256: &str,
    label: &str,
) -> Result<(), String> {
    let actual = visible_frame_region_content_sha256_v1(canonical_bgra8, size, rect)
        .map_err(|error| format!("rehash deck-gate {label}: {error}"))?;
    if actual != expected_sha256 {
        return Err(format!("deck-gate {label} differs from supplied pixels"));
    }
    Ok(())
}

fn rehash_optional_region_v1(
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
    rect: Option<&MtgoRectPxV1>,
    expected_sha256: Option<&str>,
    label: &str,
) -> Result<(), String> {
    match (rect, expected_sha256) {
        (None, None) => Ok(()),
        (Some(rect), Some(expected)) => {
            rehash_required_region_v1(canonical_bgra8, size, rect, expected, label)
        }
        _ => Err(format!(
            "deck-gate {label} rectangle and hash must appear together"
        )),
    }
}

fn deck_gate_header_digests_v1(
    header: &MtgoCompetitiveDeckGateClassifierRequestHeaderV1,
) -> [&str; 13] {
    [
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
        header.target_commitment_sha256.as_str(),
        header.target.deck_display_label_sha256.as_str(),
    ]
}

fn looks_like_lower_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::{
        validate_competitive_deck_manifest_v1, validate_visible_competitive_lifecycle_snapshot_v1,
        MtgoCompetitiveDeckCardCountV1, MtgoCompetitiveDeckConfigurationV1,
        MtgoCompetitiveDeckManifestV1, MtgoLifecycleVisibleFactKindV1, MtgoLifecycleVisibleFactV1,
        MtgoVisibleCompetitiveLifecycleSnapshotV1, MTGO_COMPETITIVE_DECK_GATE_SCHEMA_V1,
        MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1, MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
        MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
    };

    fn digest(value: char) -> String {
        value.to_string().repeat(64)
    }

    fn deck_v1() -> ValidatedMtgoCompetitiveDeckManifestV1 {
        validate_competitive_deck_manifest_v1(MtgoCompetitiveDeckManifestV1 {
            schema_version: MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
            deck_list_sha256: digest('1'),
            format_sha256: digest('2'),
            starting_mainboard_count: 5,
            starting_sideboard_count: 2,
            configuration: MtgoCompetitiveDeckConfigurationV1 {
                mainboard: vec![
                    MtgoCompetitiveDeckCardCountV1 {
                        card_db_id: 66,
                        card_name: "Lightning Bolt".to_owned(),
                        count: 2,
                    },
                    MtgoCompetitiveDeckCardCountV1 {
                        card_db_id: 76,
                        card_name: "Mountain".to_owned(),
                        count: 3,
                    },
                ],
                sideboard: vec![MtgoCompetitiveDeckCardCountV1 {
                    card_db_id: 101,
                    card_name: "Searing Blaze".to_owned(),
                    count: 2,
                }],
            },
        })
        .unwrap()
    }

    struct FixtureV1 {
        lifecycle: CheckedUntrustedMtgoCompetitiveLifecycleSnapshotV1,
        deck: ValidatedMtgoCompetitiveDeckManifestV1,
        target: MtgoCompetitiveEventListingTargetV1,
        raw: MtgoVisibleCompetitiveDeckGateV1,
        pixels: Vec<u8>,
    }

    fn fixture_v1() -> FixtureV1 {
        let pixels = (0..32 * 16 * 4)
            .map(|index| ((index * 19 + 7) % 251) as u8)
            .collect::<Vec<_>>();
        let size = MtgoSizePxV1 {
            width: 32,
            height: 16,
        };
        let client_bounds = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 32,
            height: 16,
        };
        let browser_rect = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 4,
            height: 4,
        };
        let event_rect = MtgoRectPxV1 {
            x: 5,
            y: 0,
            width: 6,
            height: 4,
        };
        let control_rect = MtgoRectPxV1 {
            x: 13,
            y: 0,
            width: 6,
            height: 4,
        };
        let deck_rect = MtgoRectPxV1 {
            x: 21,
            y: 0,
            width: 8,
            height: 4,
        };
        let rect_hash = |rect: &MtgoRectPxV1| {
            visible_frame_region_content_sha256_v1(&pixels, &size, rect).unwrap()
        };
        let lifecycle = validate_visible_competitive_lifecycle_snapshot_v1(
            MtgoVisibleCompetitiveLifecycleSnapshotV1 {
                schema_version: MTGO_COMPETITIVE_LIFECYCLE_SCHEMA_V1,
                snapshot_id: "deck-gate-runtime-event-browser-v1".to_owned(),
                event_kind: mtgo_blackbox_v1::MtgoCompetitiveEventKindV1::Challenge,
                phase: MtgoCompetitiveLifecyclePhaseV1::EventBrowser,
                frame_id: 7,
                frame_sequence: 11,
                frame_sha256: sha256_hex_v1(&pixels),
                client_bounds: client_bounds.clone(),
                event_identity_sha256: None,
                match_identity_sha256: None,
                game_number: None,
                entry_terms: None,
                visible_state_complete: true,
                facts: vec![MtgoLifecycleVisibleFactV1 {
                    kind: MtgoLifecycleVisibleFactKindV1::EventBrowserVisible,
                    rect_client_px: browser_rect.clone(),
                    content_sha256: rect_hash(&browser_rect),
                    confidence_bps: 10_000,
                }],
            },
        )
        .unwrap();
        let deck = deck_v1();
        let target = MtgoCompetitiveEventListingTargetV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
            target_id: "deck-gate-runtime-target-v1".to_owned(),
            event_kind: mtgo_blackbox_v1::MtgoCompetitiveEventKindV1::Challenge,
            approved_account_alias_sha256: digest('a'),
            event_identity_sha256: digest('3'),
            event_display_label_sha256: digest('4'),
            deck_display_label_sha256: digest('6'),
            deck_list_sha256: deck.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: deck.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('5'),
        };
        let raw = MtgoVisibleCompetitiveDeckGateV1 {
            schema_version: MTGO_COMPETITIVE_DECK_GATE_SCHEMA_V1,
            observation_id: "deck-gate-runtime-selected-v1".to_owned(),
            target_commitment_sha256: competitive_event_listing_target_commitment_v1(
                &target, &deck,
            )
            .unwrap(),
            event_kind: target.event_kind,
            event_identity_sha256: target.event_identity_sha256.clone(),
            event_display_label_sha256: target.event_display_label_sha256.clone(),
            source_lifecycle_snapshot_commitment_sha256: lifecycle
                .snapshot_commitment_sha256()
                .to_owned(),
            frame_id: lifecycle.frame_id_v1(),
            frame_sequence: lifecycle.frame_sequence(),
            frame_sha256: lifecycle.frame_sha256_v1().to_owned(),
            client_bounds,
            state: MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected,
            event_label_rect_client_px: event_rect.clone(),
            event_label_region_sha256: rect_hash(&event_rect),
            select_deck_control_rect_client_px: control_rect.clone(),
            select_deck_control_region_sha256: rect_hash(&control_rect),
            select_deck_control_enabled: true,
            missing_deck_prompt_rect_client_px: None,
            missing_deck_prompt_region_sha256: None,
            selected_deck_label_sha256: Some(target.deck_display_label_sha256.clone()),
            selected_deck_rect_client_px: Some(deck_rect.clone()),
            selected_deck_region_sha256: Some(rect_hash(&deck_rect)),
            open_entry_review_control_rect_client_px: None,
            open_entry_review_control_region_sha256: None,
            open_entry_review_control_enabled: false,
            confidence_bps: 10_000,
        };
        FixtureV1 {
            lifecycle,
            deck,
            target,
            raw,
            pixels,
        }
    }

    fn request_header_v1(
        fixture: &FixtureV1,
        assets: &[u8],
    ) -> MtgoCompetitiveDeckGateClassifierRequestHeaderV1 {
        MtgoCompetitiveDeckGateClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: "mtgo_visible_competitive_deck_gate_v1".to_owned(),
            parser_scope: "league_and_challenge_three_state_deck_gate_checked_untrusted_v1"
                .to_owned(),
            frame_id: fixture.raw.frame_id,
            frame_sequence: fixture.raw.frame_sequence,
            captured_at_unix_millis: 1_786_400_000_000,
            canonical_width: 32,
            canonical_height: 16,
            canonical_stride: 128,
            canonical_byte_length: fixture.pixels.len() as u64,
            canonical_bgra8_sha256: sha256_hex_v1(&fixture.pixels),
            source_capture_commitment_sha256: digest('7'),
            source_frame_profile_binding_sha256: digest('8'),
            source_navigation_classification_result_commitment_sha256: digest('9'),
            source_lifecycle_snapshot_commitment_sha256: fixture
                .raw
                .source_lifecycle_snapshot_commitment_sha256
                .clone(),
            navigation_profile_commitment_sha256: digest('b'),
            navigation_profile_admission_commitment_sha256: digest('c'),
            approved_account_alias_sha256: fixture.target.approved_account_alias_sha256.clone(),
            runtime_identity_commitment_sha256: digest('d'),
            classifier_binary_sha256: digest('e'),
            classifier_assets_manifest_sha256: sha256_hex_v1(assets),
            target_commitment_sha256: fixture.raw.target_commitment_sha256.clone(),
            target: fixture.target.clone(),
        }
    }

    #[test]
    fn exact_request_and_current_pixels_create_only_checked_untrusted_values() {
        let fixture = fixture_v1();
        let assets = br#"{"schema_version":1}"#;
        let header = request_header_v1(&fixture, assets);
        let header_json = serde_json::to_vec(&header).unwrap();
        let request = check_untrusted_competitive_deck_gate_classifier_request_v1(
            &header_json,
            assets,
            &fixture.pixels,
            &fixture.deck,
        )
        .unwrap();
        assert!(!request.safe_for_input_v1());
        assert!(!request.permits_event_entry_v1());
        assert!(!request.permits_spending_v1());

        let checked = check_untrusted_competitive_deck_gate_pixels_v1(
            fixture.lifecycle,
            &fixture.deck,
            fixture.target,
            fixture.raw,
            &digest('a'),
            &fixture.pixels,
        )
        .unwrap();
        assert_eq!(
            checked.state_v1(),
            MtgoCompetitiveDeckGateStateV1::CompatibleDeckSelected
        );
        assert!(!checked.ready_for_open_entry_review_v1());
        assert!(!checked.safe_for_input_v1());
    }

    #[test]
    fn request_target_asset_pixel_and_region_substitution_fail_closed() {
        let fixture = fixture_v1();
        let assets = br#"{"schema_version":1}"#;
        let mut header = request_header_v1(&fixture, assets);
        header.target.deck_display_label_sha256 = digest('f');
        let header_json = serde_json::to_vec(&header).unwrap();
        assert!(check_untrusted_competitive_deck_gate_classifier_request_v1(
            &header_json,
            assets,
            &fixture.pixels,
            &fixture.deck,
        )
        .is_err());

        let fixture = fixture_v1();
        let header = request_header_v1(&fixture, assets);
        let header_json = serde_json::to_vec(&header).unwrap();
        assert!(check_untrusted_competitive_deck_gate_classifier_request_v1(
            &header_json,
            b"{}",
            &fixture.pixels,
            &fixture.deck,
        )
        .is_err());

        let mut fixture = fixture_v1();
        fixture.raw.selected_deck_region_sha256 = Some(digest('f'));
        assert!(check_untrusted_competitive_deck_gate_pixels_v1(
            fixture.lifecycle,
            &fixture.deck,
            fixture.target,
            fixture.raw,
            &digest('a'),
            &fixture.pixels,
        )
        .is_err());
    }
}
