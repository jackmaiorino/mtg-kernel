use super::{
    competitive_entry_window_continuity_commitment_for_frame_v1,
    competitive_navigation_classifier_assets_manifest_bytes_v1,
    confirm_classified_competitive_deck_selection_transition_v1, deck_control_target_v1,
    invoke_verified_competitive_deck_chooser_classifier_process_v1, sha256_hex_v1,
    verify_runtime_identity_now_v1, MtgoCompetitiveDeckControlTargetV1,
    MtgoCompetitiveNavigationFrameIdentityV1, OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    OpaqueMtgoClassifiedCompetitiveDeckGateV1,
    OpaqueMtgoConfirmedCompetitiveDeckSelectionTransitionV1,
    OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
};
use mtgo_blackbox_v1::{
    competitive_event_listing_target_commitment_v1, visible_frame_region_content_sha256_v1,
    MtgoCompetitiveDeckGateStateV1, MtgoCompetitiveEventKindV1,
    MtgoCompetitiveEventListingTargetV1, MtgoRectPxV1, MtgoSizePxV1,
    ValidatedMtgoCompetitiveDeckManifestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

const DECK_CHOOSER_REQUEST_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-deck-chooser-classifier-request-v1";
const DECK_CHOOSER_RESULT_DOMAIN_V1: &[u8] = b"mtgo-competitive-deck-chooser-classifier-result-v1";
const MAX_DECK_CHOOSER_HEADER_BYTES_V1: usize = 1024 * 1024;
const MAX_DECK_CHOOSER_ASSETS_BYTES_V1: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitiveDeckChooserStateV1 {
    AwaitingExactDeckSelection,
    ExactDeckSelected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveDeckChooserClassifierRequestHeaderV1 {
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
    pub source_deck_gate_observation_commitment_sha256: String,
    pub source_deck_gate_result_commitment_sha256: String,
    pub source_window_continuity_commitment_sha256: String,
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
pub struct MtgoVisibleCompetitiveDeckChooserV1 {
    pub schema_version: u32,
    pub observation_id: String,
    pub target_commitment_sha256: String,
    pub source_deck_gate_observation_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub frame_sha256: String,
    pub client_bounds: MtgoRectPxV1,
    pub state: MtgoCompetitiveDeckChooserStateV1,
    pub chooser_title_label_sha256: String,
    pub chooser_title_rect_client_px: MtgoRectPxV1,
    pub chooser_title_region_sha256: String,
    pub selected_deck_label_sha256: String,
    pub selected_deck_label_rect_client_px: MtgoRectPxV1,
    pub selected_deck_label_region_sha256: String,
    pub deck_row_control_rect_client_px: MtgoRectPxV1,
    pub deck_row_control_region_sha256: String,
    pub deck_row_selected: bool,
    pub submit_control_rect_client_px: MtgoRectPxV1,
    pub submit_control_region_sha256: String,
    pub submit_control_enabled: bool,
    pub confidence_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveDeckChooserClassifierProcessResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub chooser: MtgoVisibleCompetitiveDeckChooserV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedUntrustedMtgoCompetitiveDeckChooserClassifierRequestV1 {
    header: MtgoCompetitiveDeckChooserClassifierRequestHeaderV1,
    request_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveDeckChooserClassifierRequestV1 {
    pub fn header_v1(&self) -> &MtgoCompetitiveDeckChooserClassifierRequestHeaderV1 {
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
pub struct MtgoClassifiedCompetitiveDeckChooserCommitmentsV1 {
    pub source_deck_gate_observation_commitment_sha256: String,
    pub source_deck_gate_result_commitment_sha256: String,
    pub target_commitment_sha256: String,
    pub runtime_identity_commitment_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_window_continuity_commitment_sha256: String,
    pub request_commitment_sha256: String,
    pub classifier_response_sha256: String,
    pub classification_result_commitment_sha256: String,
    pub event_kind: MtgoCompetitiveEventKindV1,
    pub event_identity_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub captured_at_unix_millis: u128,
    pub state: MtgoCompetitiveDeckChooserStateV1,
}

/// One exact visible deck-chooser state bound to the original missing-deck
/// gate and all prior chooser frames. It is move-only and exposes no pixels,
/// coordinates, deck selector, input method, event-entry operation, or spending
/// operation.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoClassifiedCompetitiveDeckChooserV1;
/// fn require_clone<T: Clone>() {}
/// require_clone::<OpaqueMtgoClassifiedCompetitiveDeckChooserV1>();
/// ```
pub struct OpaqueMtgoClassifiedCompetitiveDeckChooserV1 {
    source_gate: OpaqueMtgoClassifiedCompetitiveDeckGateV1,
    prior_frames: Vec<OpaqueMtgoAdmittedCompetitiveNavigationFrameV1>,
    current_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    target: MtgoCompetitiveEventListingTargetV1,
    raw: MtgoVisibleCompetitiveDeckChooserV1,
    commitments: MtgoClassifiedCompetitiveDeckChooserCommitmentsV1,
}

impl OpaqueMtgoClassifiedCompetitiveDeckChooserV1 {
    pub fn commitments_v1(&self) -> MtgoClassifiedCompetitiveDeckChooserCommitmentsV1 {
        self.commitments.clone()
    }

    pub fn state_v1(&self) -> MtgoCompetitiveDeckChooserStateV1 {
        self.commitments.state
    }

    pub fn exact_deck_selected_v1(&self) -> bool {
        self.commitments.state == MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected
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

    pub(crate) fn exact_deck_row_control_target_v1(
        &self,
    ) -> Result<MtgoCompetitiveDeckControlTargetV1, String> {
        if self.commitments.state != MtgoCompetitiveDeckChooserStateV1::AwaitingExactDeckSelection
            || self.raw.deck_row_selected
            || self.raw.submit_control_enabled
        {
            return Err(
                "exact deck-row input requires the unselected reviewed chooser state".to_owned(),
            );
        }
        deck_control_target_v1(
            &self.current_frame,
            &self.raw.deck_row_control_rect_client_px,
            &self.raw.deck_row_control_region_sha256,
            &self.commitments.classification_result_commitment_sha256,
            self.raw.frame_id,
            self.raw.frame_sequence,
            b"exact_deck_row",
        )
    }

    pub(crate) fn submit_control_target_v1(
        &self,
    ) -> Result<MtgoCompetitiveDeckControlTargetV1, String> {
        if self.commitments.state != MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected
            || !self.raw.deck_row_selected
            || !self.raw.submit_control_enabled
        {
            return Err(
                "deck Submit requires the exact selected reviewed chooser state".to_owned(),
            );
        }
        deck_control_target_v1(
            &self.current_frame,
            &self.raw.submit_control_rect_client_px,
            &self.raw.submit_control_region_sha256,
            &self.commitments.classification_result_commitment_sha256,
            self.raw.frame_id,
            self.raw.frame_sequence,
            b"submit_selected_deck",
        )
    }
}

pub(crate) struct OpaqueMtgoConfirmedCompetitiveDeckSubmitVisibleV1 {
    _prior_chooser_frames: Vec<OpaqueMtgoAdmittedCompetitiveNavigationFrameV1>,
    _selected_chooser_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    _target: MtgoCompetitiveEventListingTargetV1,
    _selected_raw: MtgoVisibleCompetitiveDeckChooserV1,
    _selected_commitments: MtgoClassifiedCompetitiveDeckChooserCommitmentsV1,
    gate_transition: OpaqueMtgoConfirmedCompetitiveDeckSelectionTransitionV1,
}

impl OpaqueMtgoConfirmedCompetitiveDeckSubmitVisibleV1 {
    pub(crate) fn transition_commitments_v1(
        &self,
    ) -> mtgo_blackbox_v1::MtgoCompetitiveDeckSelectionTransitionCommitmentsV1 {
        self.gate_transition.commitments_v1()
    }

    pub(crate) fn after_captured_at_unix_millis_v1(&self) -> u128 {
        self.gate_transition.after_captured_at_unix_millis_v1()
    }

    pub(crate) fn into_after_gate_v1(self) -> OpaqueMtgoClassifiedCompetitiveDeckGateV1 {
        self.gate_transition.into_after_gate_v1()
    }
}

pub(crate) fn confirm_competitive_deck_submit_visible_v1(
    selected: OpaqueMtgoClassifiedCompetitiveDeckChooserV1,
    after: OpaqueMtgoClassifiedCompetitiveDeckGateV1,
) -> Result<OpaqueMtgoConfirmedCompetitiveDeckSubmitVisibleV1, String> {
    if selected.state_v1() != MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected {
        return Err("deck Submit confirmation requires the exact selected chooser".to_owned());
    }
    let OpaqueMtgoClassifiedCompetitiveDeckChooserV1 {
        source_gate,
        prior_frames,
        current_frame,
        target,
        raw,
        commitments,
    } = selected;
    let gate_transition =
        confirm_classified_competitive_deck_selection_transition_v1(source_gate, after)?;
    Ok(OpaqueMtgoConfirmedCompetitiveDeckSubmitVisibleV1 {
        _prior_chooser_frames: prior_frames,
        _selected_chooser_frame: current_frame,
        _target: target,
        _selected_raw: raw,
        _selected_commitments: commitments,
        gate_transition,
    })
}

pub fn check_untrusted_competitive_deck_chooser_classifier_request_v1(
    canonical_header_json: &[u8],
    classifier_assets_manifest: &[u8],
    canonical_bgra8: &[u8],
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
) -> Result<CheckedUntrustedMtgoCompetitiveDeckChooserClassifierRequestV1, String> {
    if canonical_header_json.is_empty()
        || canonical_header_json.len() > MAX_DECK_CHOOSER_HEADER_BYTES_V1
        || classifier_assets_manifest.is_empty()
        || classifier_assets_manifest.len() > MAX_DECK_CHOOSER_ASSETS_BYTES_V1
    {
        return Err("deck-chooser request header or assets are outside bounds".to_owned());
    }
    let header: MtgoCompetitiveDeckChooserClassifierRequestHeaderV1 =
        serde_json::from_slice(canonical_header_json)
            .map_err(|error| format!("parse deck-chooser request header: {error}"))?;
    if serde_json::to_vec(&header)
        .map_err(|error| format!("serialize deck-chooser request header: {error}"))?
        != canonical_header_json
    {
        return Err("deck-chooser request header is not canonical JSON".to_owned());
    }
    if header.schema_version != 1
        || header.protocol != "mtgo_visible_competitive_deck_chooser_v1"
        || header.parser_scope != "league_and_challenge_exact_deck_chooser_checked_untrusted_v1"
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
    let stride = header
        .canonical_width
        .checked_mul(4)
        .ok_or("deck-chooser stride overflow")?;
    let byte_length = u64::from(stride)
        .checked_mul(u64::from(header.canonical_height))
        .ok_or("deck-chooser byte length overflow")?;
    if header.canonical_stride != stride
        || header.canonical_byte_length != byte_length
        || usize::try_from(byte_length).ok() != Some(canonical_bgra8.len())
        || header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
        || header.classifier_assets_manifest_sha256 != sha256_hex_v1(classifier_assets_manifest)
    {
        return Err("deck-chooser pixels or assets differ from the header".to_owned());
    }
    for digest in deck_chooser_header_digests_v1(&header) {
        if !looks_like_lower_sha256_v1(digest) {
            return Err("deck-chooser request contains an invalid commitment".to_owned());
        }
    }
    let target_commitment = competitive_event_listing_target_commitment_v1(&header.target, deck)
        .map_err(|error| format!("validate deck-chooser target: {error}"))?;
    if header.target_commitment_sha256 != target_commitment
        || header.target.approved_account_alias_sha256 != header.approved_account_alias_sha256
        || header.target.deck_list_sha256 != deck.deck_list_sha256()
        || header.target.deck_manifest_commitment_sha256 != deck.manifest_commitment_sha256()
        || header.target.deck_format_sha256 != deck.format_sha256()
    {
        return Err("deck-chooser request changed the exact account, target, or deck".to_owned());
    }
    let request_commitment_sha256 = commitment_v1(
        DECK_CHOOSER_REQUEST_DOMAIN_V1,
        &[
            canonical_header_json,
            classifier_assets_manifest,
            canonical_bgra8,
        ],
    );
    Ok(
        CheckedUntrustedMtgoCompetitiveDeckChooserClassifierRequestV1 {
            header,
            request_commitment_sha256,
        },
    )
}

pub fn check_untrusted_competitive_deck_chooser_pixels_v1(
    header: &MtgoCompetitiveDeckChooserClassifierRequestHeaderV1,
    raw: &MtgoVisibleCompetitiveDeckChooserV1,
    canonical_bgra8: &[u8],
) -> Result<String, String> {
    let size = MtgoSizePxV1 {
        width: header.canonical_width,
        height: header.canonical_height,
    };
    if raw.schema_version != 1
        || raw.observation_id.is_empty()
        || raw.observation_id.len() > 128
        || raw.observation_id.trim() != raw.observation_id
        || raw.observation_id.chars().any(char::is_control)
        || raw.target_commitment_sha256 != header.target_commitment_sha256
        || raw.source_deck_gate_observation_commitment_sha256
            != header.source_deck_gate_observation_commitment_sha256
        || raw.event_kind != header.target.event_kind
        || raw.event_identity_sha256 != header.target.event_identity_sha256
        || raw.frame_id != header.frame_id
        || raw.frame_sequence != header.frame_sequence
        || raw.frame_sha256 != header.canonical_bgra8_sha256
        || raw.client_bounds
            != (MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: header.canonical_width,
                height: header.canonical_height,
            })
        || raw.selected_deck_label_sha256 != header.target.deck_display_label_sha256
        || !(9_500..=10_000).contains(&raw.confidence_bps)
    {
        return Err("deck-chooser response identity, state, or confidence differs".to_owned());
    }
    for digest in [
        raw.target_commitment_sha256.as_str(),
        raw.source_deck_gate_observation_commitment_sha256.as_str(),
        raw.event_identity_sha256.as_str(),
        raw.frame_sha256.as_str(),
        raw.chooser_title_label_sha256.as_str(),
        raw.chooser_title_region_sha256.as_str(),
        raw.selected_deck_label_sha256.as_str(),
        raw.selected_deck_label_region_sha256.as_str(),
        raw.deck_row_control_region_sha256.as_str(),
        raw.submit_control_region_sha256.as_str(),
    ] {
        if !looks_like_lower_sha256_v1(digest) {
            return Err("deck-chooser response contains an invalid digest".to_owned());
        }
    }
    let bounds = &raw.client_bounds;
    for rect in [
        &raw.chooser_title_rect_client_px,
        &raw.selected_deck_label_rect_client_px,
        &raw.deck_row_control_rect_client_px,
        &raw.submit_control_rect_client_px,
    ] {
        if !rect_inside_v1(rect, bounds) {
            return Err("deck-chooser response rectangle is outside the client".to_owned());
        }
    }
    if !rect_inside_v1(
        &raw.selected_deck_label_rect_client_px,
        &raw.deck_row_control_rect_client_px,
    ) || rects_overlap_v1(
        &raw.chooser_title_rect_client_px,
        &raw.deck_row_control_rect_client_px,
    ) || rects_overlap_v1(
        &raw.chooser_title_rect_client_px,
        &raw.submit_control_rect_client_px,
    ) || rects_overlap_v1(
        &raw.deck_row_control_rect_client_px,
        &raw.submit_control_rect_client_px,
    ) {
        return Err("deck-chooser evidence and controls have invalid geometry".to_owned());
    }
    match raw.state {
        MtgoCompetitiveDeckChooserStateV1::AwaitingExactDeckSelection => {
            if raw.deck_row_selected || raw.submit_control_enabled {
                return Err("awaiting deck-chooser state cannot expose enabled Submit".to_owned());
            }
        }
        MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected => {
            if !raw.deck_row_selected || !raw.submit_control_enabled {
                return Err(
                    "selected deck-chooser state requires selected row and Submit".to_owned(),
                );
            }
        }
    }
    for (rect, expected, label) in [
        (
            &raw.chooser_title_rect_client_px,
            raw.chooser_title_region_sha256.as_str(),
            "chooser title",
        ),
        (
            &raw.selected_deck_label_rect_client_px,
            raw.selected_deck_label_region_sha256.as_str(),
            "deck label",
        ),
        (
            &raw.deck_row_control_rect_client_px,
            raw.deck_row_control_region_sha256.as_str(),
            "deck row",
        ),
        (
            &raw.submit_control_rect_client_px,
            raw.submit_control_region_sha256.as_str(),
            "Submit control",
        ),
    ] {
        let actual = visible_frame_region_content_sha256_v1(canonical_bgra8, &size, rect)
            .map_err(|error| format!("rehash deck-chooser {label}: {error}"))?;
        if actual != expected {
            return Err(format!("deck-chooser {label} differs from supplied pixels"));
        }
    }
    let raw_json = serde_json::to_vec(raw)
        .map_err(|error| format!("serialize checked deck-chooser response: {error}"))?;
    Ok(commitment_v1(
        DECK_CHOOSER_RESULT_DOMAIN_V1,
        &[
            header
                .source_deck_gate_observation_commitment_sha256
                .as_bytes(),
            header.target_commitment_sha256.as_bytes(),
            header.source_capture_commitment_sha256.as_bytes(),
            raw_json.as_slice(),
            b"checked_untrusted_visible_chooser_no_input_no_entry_no_spending",
        ],
    ))
}

pub fn classify_competitive_deck_chooser_open_state_v1(
    source_gate: OpaqueMtgoClassifiedCompetitiveDeckGateV1,
    current_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    identity: MtgoCompetitiveNavigationFrameIdentityV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitiveDeckChooserV1, String> {
    if source_gate.state_v1() != MtgoCompetitiveDeckGateStateV1::AwaitingCompatibleDeckSelection {
        return Err("deck chooser can open only from the exact missing-deck state".to_owned());
    }
    let classified = classify_deck_chooser_frame_v1(
        source_gate,
        Vec::new(),
        current_frame,
        deck,
        target,
        runtime,
        identity,
        timeout_ms,
    )?;
    if classified.state_v1() != MtgoCompetitiveDeckChooserStateV1::AwaitingExactDeckSelection {
        return Err("initial deck chooser is not awaiting exact deck selection".to_owned());
    }
    Ok(classified)
}

pub fn classify_competitive_deck_chooser_selected_successor_v1(
    prior: OpaqueMtgoClassifiedCompetitiveDeckChooserV1,
    current_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    identity: MtgoCompetitiveNavigationFrameIdentityV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitiveDeckChooserV1, String> {
    if prior.state_v1() != MtgoCompetitiveDeckChooserStateV1::AwaitingExactDeckSelection {
        return Err("deck chooser successor requires the exact awaiting state".to_owned());
    }
    let prior_commitments = prior.commitments_v1();
    let OpaqueMtgoClassifiedCompetitiveDeckChooserV1 {
        source_gate,
        mut prior_frames,
        current_frame: prior_frame,
        target,
        ..
    } = prior;
    let current_commitments = current_frame.commitments_v1();
    if identity.frame_sequence <= prior_commitments.frame_sequence
        || identity.frame_id == prior_commitments.frame_id
        || current_commitments.source_capture.captured_at_unix_millis
            <= prior_commitments.captured_at_unix_millis
        || current_commitments.source_capture.capture_commitment_sha256
            == prior_commitments.source_capture_commitment_sha256
        || competitive_entry_window_continuity_commitment_for_frame_v1(&prior_frame.source_frame)?
            != competitive_entry_window_continuity_commitment_for_frame_v1(
                &current_frame.source_frame,
            )?
    {
        return Err("selected deck chooser is not a distinct newer continuous frame".to_owned());
    }
    prior_frames.push(prior_frame);
    let classified = classify_deck_chooser_frame_v1(
        source_gate,
        prior_frames,
        current_frame,
        deck,
        target,
        runtime,
        identity,
        timeout_ms,
    )?;
    if classified.state_v1() != MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected {
        return Err("deck chooser successor did not select the exact expected deck".to_owned());
    }
    Ok(classified)
}

#[allow(clippy::too_many_arguments)]
fn classify_deck_chooser_frame_v1(
    source_gate: OpaqueMtgoClassifiedCompetitiveDeckGateV1,
    prior_frames: Vec<OpaqueMtgoAdmittedCompetitiveNavigationFrameV1>,
    current_frame: OpaqueMtgoAdmittedCompetitiveNavigationFrameV1,
    deck: &ValidatedMtgoCompetitiveDeckManifestV1,
    target: MtgoCompetitiveEventListingTargetV1,
    runtime: &OpaqueMtgoVerifiedCompetitiveNavigationClassifierRuntimeV1,
    identity: MtgoCompetitiveNavigationFrameIdentityV1,
    timeout_ms: u32,
) -> Result<OpaqueMtgoClassifiedCompetitiveDeckChooserV1, String> {
    if !(100..=60_000).contains(&timeout_ms)
        || identity.frame_id == 0
        || identity.frame_sequence == 0
    {
        return Err("deck-chooser identity or timeout is invalid".to_owned());
    }
    let gate_commitments = source_gate.commitments_v1();
    let runtime_commitments = runtime.commitments_v1();
    let frame_commitments = current_frame.commitments_v1();
    if gate_commitments.target_commitment_sha256
        != competitive_event_listing_target_commitment_v1(&target, deck)
            .map_err(|error| format!("validate deck-chooser target: {error}"))?
        || target.approved_account_alias_sha256
            != gate_commitments
                .source_navigation
                .source_frame
                .approved_account_alias_sha256
        || target.event_kind != gate_commitments.source_navigation.event_kind
        || frame_commitments.profile_commitment_sha256
            != runtime_commitments.navigation_profile_commitment_sha256
        || frame_commitments.profile_admission_commitment_sha256
            != runtime_commitments.navigation_profile_admission_commitment_sha256
        || frame_commitments.approved_account_alias_sha256
            != runtime_commitments.approved_account_alias_sha256
        || frame_commitments.approved_account_alias_sha256 != target.approved_account_alias_sha256
        || identity.frame_sequence <= gate_commitments.source_navigation.frame_sequence
        || identity.frame_id == gate_commitments.source_navigation.frame_id
        || frame_commitments.source_capture.captured_at_unix_millis
            <= source_gate
                .source_frame_v1()
                .source_frame
                .manifest
                .captured_at_unix_millis
        || competitive_entry_window_continuity_commitment_for_frame_v1(
            &source_gate.source_frame_v1().source_frame,
        )? != competitive_entry_window_continuity_commitment_for_frame_v1(
            &current_frame.source_frame,
        )?
    {
        return Err("deck-chooser gate, frame, runtime, account, or target differs".to_owned());
    }
    verify_runtime_identity_now_v1(runtime)?;
    let source = &current_frame.source_frame;
    let width = source.manifest.frame.canonical_width;
    let height = source.manifest.frame.canonical_height;
    let stride = width
        .checked_mul(4)
        .ok_or("deck-chooser canonical stride overflow")?;
    let byte_length = u64::try_from(source.canonical_bgra8.len())
        .map_err(|_| "deck-chooser byte length overflow")?;
    if source.manifest.frame.canonical_stride != stride
        || source.manifest.frame.canonical_byte_length != source.canonical_bgra8.len()
        || source.manifest.frame.canonical_bgra8_sha256 != sha256_hex_v1(&source.canonical_bgra8)
    {
        return Err("deck-chooser source pixels differ from capture metadata".to_owned());
    }
    let assets = competitive_navigation_classifier_assets_manifest_bytes_v1(runtime);
    let window_continuity_commitment_sha256 =
        competitive_entry_window_continuity_commitment_for_frame_v1(source)?;
    let header = MtgoCompetitiveDeckChooserClassifierRequestHeaderV1 {
        schema_version: 1,
        protocol: "mtgo_visible_competitive_deck_chooser_v1".to_owned(),
        parser_scope: "league_and_challenge_exact_deck_chooser_checked_untrusted_v1".to_owned(),
        frame_id: identity.frame_id,
        frame_sequence: identity.frame_sequence,
        captured_at_unix_millis: source.manifest.captured_at_unix_millis,
        canonical_width: width,
        canonical_height: height,
        canonical_stride: stride,
        canonical_byte_length: byte_length,
        canonical_bgra8_sha256: source.manifest.frame.canonical_bgra8_sha256.clone(),
        source_capture_commitment_sha256: source.capture_commitment_sha256.clone(),
        source_frame_profile_binding_sha256: frame_commitments.frame_profile_binding_sha256.clone(),
        source_deck_gate_observation_commitment_sha256: gate_commitments
            .observation_commitment_sha256
            .clone(),
        source_deck_gate_result_commitment_sha256: gate_commitments
            .classification_result_commitment_sha256
            .clone(),
        source_window_continuity_commitment_sha256: window_continuity_commitment_sha256.clone(),
        navigation_profile_commitment_sha256: frame_commitments.profile_commitment_sha256.clone(),
        navigation_profile_admission_commitment_sha256: frame_commitments
            .profile_admission_commitment_sha256
            .clone(),
        approved_account_alias_sha256: frame_commitments.approved_account_alias_sha256.clone(),
        runtime_identity_commitment_sha256: runtime_commitments
            .runtime_identity_commitment_sha256
            .clone(),
        classifier_binary_sha256: runtime_commitments.classifier_binary_sha256.clone(),
        classifier_assets_manifest_sha256: runtime_commitments
            .classifier_assets_manifest_sha256
            .clone(),
        target_commitment_sha256: gate_commitments.target_commitment_sha256.clone(),
        target: target.clone(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|error| format!("serialize deck-chooser request: {error}"))?;
    let checked_request = check_untrusted_competitive_deck_chooser_classifier_request_v1(
        &header_json,
        assets,
        &source.canonical_bgra8,
        deck,
    )?;
    let request_commitment_sha256 = checked_request.request_commitment_sha256_v1().to_owned();
    let response_bytes = invoke_verified_competitive_deck_chooser_classifier_process_v1(
        runtime,
        &header_json,
        assets,
        &source.canonical_bgra8,
        Duration::from_millis(u64::from(timeout_ms)),
    )?;
    verify_runtime_identity_now_v1(runtime)?;
    let response = parse_deck_chooser_response_v1(&response_bytes, &request_commitment_sha256)?;
    let observation_commitment_sha256 = check_untrusted_competitive_deck_chooser_pixels_v1(
        &header,
        &response.chooser,
        &source.canonical_bgra8,
    )?;
    let classifier_response_sha256 = sha256_hex_v1(&response_bytes);
    let state_json = serde_json::to_vec(&response.chooser.state)
        .map_err(|error| format!("serialize deck-chooser state: {error}"))?;
    let classification_result_commitment_sha256 = commitment_v1(
        DECK_CHOOSER_RESULT_DOMAIN_V1,
        &[
            gate_commitments
                .classification_result_commitment_sha256
                .as_bytes(),
            gate_commitments.observation_commitment_sha256.as_bytes(),
            request_commitment_sha256.as_bytes(),
            classifier_response_sha256.as_bytes(),
            observation_commitment_sha256.as_bytes(),
            state_json.as_slice(),
            b"exact_deck_chooser_checked_untrusted_no_input_no_entry_no_spending",
        ],
    );
    let raw = response.chooser;
    let commitments = MtgoClassifiedCompetitiveDeckChooserCommitmentsV1 {
        source_deck_gate_observation_commitment_sha256: gate_commitments
            .observation_commitment_sha256,
        source_deck_gate_result_commitment_sha256: gate_commitments
            .classification_result_commitment_sha256,
        target_commitment_sha256: gate_commitments.target_commitment_sha256,
        runtime_identity_commitment_sha256: runtime_commitments.runtime_identity_commitment_sha256,
        source_capture_commitment_sha256: source.capture_commitment_sha256.clone(),
        source_window_continuity_commitment_sha256: window_continuity_commitment_sha256,
        request_commitment_sha256,
        classifier_response_sha256,
        classification_result_commitment_sha256,
        event_kind: raw.event_kind,
        event_identity_sha256: raw.event_identity_sha256.clone(),
        frame_id: raw.frame_id,
        frame_sequence: raw.frame_sequence,
        captured_at_unix_millis: source.manifest.captured_at_unix_millis,
        state: raw.state,
    };
    Ok(OpaqueMtgoClassifiedCompetitiveDeckChooserV1 {
        source_gate,
        prior_frames,
        current_frame,
        target,
        raw,
        commitments,
    })
}

fn parse_deck_chooser_response_v1(
    response_bytes: &[u8],
    request_commitment_sha256: &str,
) -> Result<MtgoCompetitiveDeckChooserClassifierProcessResponseV1, String> {
    let response: MtgoCompetitiveDeckChooserClassifierProcessResponseV1 =
        serde_json::from_slice(response_bytes).map_err(|error| {
            format!("deck-chooser response is not one strict JSON value: {error}")
        })?;
    if serde_json::to_vec(&response)
        .map_err(|error| format!("serialize deck-chooser response: {error}"))?
        != response_bytes
        || response.schema_version != 1
        || response.request_commitment_sha256 != request_commitment_sha256
    {
        return Err("deck-chooser response does not bind the exact request".to_owned());
    }
    Ok(response)
}

fn deck_chooser_header_digests_v1(
    header: &MtgoCompetitiveDeckChooserClassifierRequestHeaderV1,
) -> [&str; 15] {
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
        &header.target.event_identity_sha256,
        &header.target.deck_display_label_sha256,
    ]
}

fn rect_inside_v1(inner: &MtgoRectPxV1, outer: &MtgoRectPxV1) -> bool {
    let Some(inner_right) = inner.x.checked_add(inner.width) else {
        return false;
    };
    let Some(inner_bottom) = inner.y.checked_add(inner.height) else {
        return false;
    };
    let Some(outer_right) = outer.x.checked_add(outer.width) else {
        return false;
    };
    let Some(outer_bottom) = outer.y.checked_add(outer.height) else {
        return false;
    };
    inner.width > 0
        && inner.height > 0
        && inner.x >= outer.x
        && inner.y >= outer.y
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom
}

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
        validate_competitive_deck_manifest_v1, MtgoCompetitiveDeckCardCountV1,
        MtgoCompetitiveDeckConfigurationV1, MtgoCompetitiveDeckManifestV1,
        MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1, MTGO_COMPETITIVE_SIDEBOARD_SCHEMA_V1,
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
        deck: ValidatedMtgoCompetitiveDeckManifestV1,
        header: MtgoCompetitiveDeckChooserClassifierRequestHeaderV1,
        raw: MtgoVisibleCompetitiveDeckChooserV1,
        assets: Vec<u8>,
        pixels: Vec<u8>,
    }

    fn fixture_v1(state: MtgoCompetitiveDeckChooserStateV1) -> FixtureV1 {
        let pixels = (0..32 * 16 * 4)
            .map(|index| ((index * 17 + 5) % 251) as u8)
            .collect::<Vec<_>>();
        let size = MtgoSizePxV1 {
            width: 32,
            height: 16,
        };
        let title_rect = MtgoRectPxV1 {
            x: 0,
            y: 0,
            width: 12,
            height: 3,
        };
        let row_rect = MtgoRectPxV1 {
            x: 0,
            y: 4,
            width: 20,
            height: 5,
        };
        let label_rect = MtgoRectPxV1 {
            x: 1,
            y: 5,
            width: 16,
            height: 2,
        };
        let submit_rect = MtgoRectPxV1 {
            x: 22,
            y: 10,
            width: 8,
            height: 4,
        };
        let region_hash = |rect: &MtgoRectPxV1| {
            visible_frame_region_content_sha256_v1(&pixels, &size, rect).unwrap()
        };
        let deck = deck_v1();
        let target = MtgoCompetitiveEventListingTargetV1 {
            schema_version: MTGO_COMPETITIVE_EVENT_LISTING_SCHEMA_V1,
            target_id: "runtime-deck-chooser-v1".to_owned(),
            event_kind: MtgoCompetitiveEventKindV1::Challenge,
            approved_account_alias_sha256: digest('a'),
            event_identity_sha256: digest('3'),
            event_display_label_sha256: digest('4'),
            deck_display_label_sha256: digest('5'),
            deck_list_sha256: deck.deck_list_sha256().to_owned(),
            deck_manifest_commitment_sha256: deck.manifest_commitment_sha256().to_owned(),
            deck_format_sha256: deck.format_sha256().to_owned(),
            policy_deployment_commitment_sha256: digest('6'),
        };
        let target_commitment_sha256 =
            competitive_event_listing_target_commitment_v1(&target, &deck).unwrap();
        let assets = br#"{"schema_version":1}"#.to_vec();
        let header = MtgoCompetitiveDeckChooserClassifierRequestHeaderV1 {
            schema_version: 1,
            protocol: "mtgo_visible_competitive_deck_chooser_v1".to_owned(),
            parser_scope: "league_and_challenge_exact_deck_chooser_checked_untrusted_v1".to_owned(),
            frame_id: 11,
            frame_sequence: 13,
            captured_at_unix_millis: 1_786_400_000_000,
            canonical_width: 32,
            canonical_height: 16,
            canonical_stride: 128,
            canonical_byte_length: pixels.len() as u64,
            canonical_bgra8_sha256: sha256_hex_v1(&pixels),
            source_capture_commitment_sha256: digest('7'),
            source_frame_profile_binding_sha256: digest('8'),
            source_deck_gate_observation_commitment_sha256: digest('9'),
            source_deck_gate_result_commitment_sha256: digest('b'),
            source_window_continuity_commitment_sha256: digest('c'),
            navigation_profile_commitment_sha256: digest('d'),
            navigation_profile_admission_commitment_sha256: digest('e'),
            approved_account_alias_sha256: target.approved_account_alias_sha256.clone(),
            runtime_identity_commitment_sha256: digest('f'),
            classifier_binary_sha256: digest('0'),
            classifier_assets_manifest_sha256: sha256_hex_v1(&assets),
            target_commitment_sha256: target_commitment_sha256.clone(),
            target: target.clone(),
        };
        let selected = state == MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected;
        let raw = MtgoVisibleCompetitiveDeckChooserV1 {
            schema_version: 1,
            observation_id: "runtime-visible-deck-chooser-v1".to_owned(),
            target_commitment_sha256,
            source_deck_gate_observation_commitment_sha256: header
                .source_deck_gate_observation_commitment_sha256
                .clone(),
            event_kind: target.event_kind,
            event_identity_sha256: target.event_identity_sha256,
            frame_id: header.frame_id,
            frame_sequence: header.frame_sequence,
            frame_sha256: header.canonical_bgra8_sha256.clone(),
            client_bounds: MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 32,
                height: 16,
            },
            state,
            chooser_title_label_sha256: digest('1'),
            chooser_title_rect_client_px: title_rect.clone(),
            chooser_title_region_sha256: region_hash(&title_rect),
            selected_deck_label_sha256: header.target.deck_display_label_sha256.clone(),
            selected_deck_label_rect_client_px: label_rect.clone(),
            selected_deck_label_region_sha256: region_hash(&label_rect),
            deck_row_control_rect_client_px: row_rect.clone(),
            deck_row_control_region_sha256: region_hash(&row_rect),
            deck_row_selected: selected,
            submit_control_rect_client_px: submit_rect.clone(),
            submit_control_region_sha256: region_hash(&submit_rect),
            submit_control_enabled: selected,
            confidence_bps: 10_000,
        };
        FixtureV1 {
            deck,
            header,
            raw,
            assets,
            pixels,
        }
    }

    #[test]
    fn exact_request_and_visible_state_remain_non_authorizing() {
        for state in [
            MtgoCompetitiveDeckChooserStateV1::AwaitingExactDeckSelection,
            MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected,
        ] {
            let fixture = fixture_v1(state);
            let header_json = serde_json::to_vec(&fixture.header).unwrap();
            let request = check_untrusted_competitive_deck_chooser_classifier_request_v1(
                &header_json,
                &fixture.assets,
                &fixture.pixels,
                &fixture.deck,
            )
            .unwrap();
            assert!(!request.safe_for_input_v1());
            assert!(!request.permits_event_entry_v1());
            assert!(!request.permits_spending_v1());
            assert!(check_untrusted_competitive_deck_chooser_pixels_v1(
                &fixture.header,
                &fixture.raw,
                &fixture.pixels,
            )
            .is_ok());
        }
    }

    #[test]
    fn request_target_asset_and_pixel_substitution_fail_closed() {
        let fixture = fixture_v1(MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected);
        let header_json = serde_json::to_vec(&fixture.header).unwrap();
        assert!(
            check_untrusted_competitive_deck_chooser_classifier_request_v1(
                &header_json,
                b"{}",
                &fixture.pixels,
                &fixture.deck,
            )
            .is_err()
        );
        let mut crossed = fixture.header.clone();
        crossed.target.deck_display_label_sha256 = digest('d');
        let crossed_json = serde_json::to_vec(&crossed).unwrap();
        assert!(
            check_untrusted_competitive_deck_chooser_classifier_request_v1(
                &crossed_json,
                &fixture.assets,
                &fixture.pixels,
                &fixture.deck,
            )
            .is_err()
        );
        let mut changed_pixels = fixture.pixels.clone();
        changed_pixels[0] ^= 1;
        assert!(check_untrusted_competitive_deck_chooser_pixels_v1(
            &fixture.header,
            &fixture.raw,
            &changed_pixels,
        )
        .is_err());
    }

    #[test]
    fn contradictory_state_region_hash_and_geometry_fail_closed() {
        let mut fixture = fixture_v1(MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected);
        fixture.raw.submit_control_enabled = false;
        assert!(check_untrusted_competitive_deck_chooser_pixels_v1(
            &fixture.header,
            &fixture.raw,
            &fixture.pixels,
        )
        .is_err());

        let mut fixture = fixture_v1(MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected);
        fixture.raw.deck_row_control_region_sha256 = digest('e');
        assert!(check_untrusted_competitive_deck_chooser_pixels_v1(
            &fixture.header,
            &fixture.raw,
            &fixture.pixels,
        )
        .is_err());

        let mut fixture = fixture_v1(MtgoCompetitiveDeckChooserStateV1::ExactDeckSelected);
        fixture.raw.submit_control_rect_client_px =
            fixture.raw.deck_row_control_rect_client_px.clone();
        assert!(check_untrusted_competitive_deck_chooser_pixels_v1(
            &fixture.header,
            &fixture.raw,
            &fixture.pixels,
        )
        .is_err());
    }
}
