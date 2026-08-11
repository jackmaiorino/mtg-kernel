use crate::{
    canonical_competitive_pregame_states_v1, visible_frame_region_content_sha256_v1,
    AdmittedMtgoCompetitivePregameProfileV1, AdmittedMtgoDuelPerceptionProfileV1,
    MtgoCompetitivePregameStageLabelV1, MtgoContractErrorV1, MtgoRectPxV1, MtgoSizePxV1,
    MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const MTGO_COMPETITIVE_PREGAME_CLASSIFIER_SCHEMA_V1: u32 = 1;
pub const MTGO_COMPETITIVE_PREGAME_CLASSIFIER_PROTOCOL_V1: &str =
    "mtgo_visible_competitive_pregame_v1";

const COMPETITIVE_PREGAME_CLASSIFIER_REQUEST_DOMAIN_V1: &[u8] =
    b"mtgo-visible-competitive-pregame-classifier-request-v1";
const COMPETITIVE_PREGAME_CLASSIFICATION_DOMAIN_V1: &[u8] =
    b"mtgo-visible-competitive-pregame-classification-v1";
const COMPETITIVE_PREGAME_VISIBLE_INTERACTION_DOMAIN_V1: &[u8] =
    b"mtgo-visible-competitive-pregame-interaction-v1";

/// Canonical metadata sent before one tightly packed BGRA8 acting-player
/// duel frame. League or Challenge identity is intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregameClassifierRequestHeaderV1 {
    pub schema_version: u32,
    pub protocol: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub canonical_width: u32,
    pub canonical_height: u32,
    pub canonical_stride: u32,
    pub canonical_byte_length: usize,
    pub canonical_bgra8_sha256: String,
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub duel_perception_profile_commitment_sha256: String,
    pub duel_perception_profile_admission_commitment_sha256: String,
    pub pregame_evaluation_commitment_sha256: String,
    pub pregame_profile_admission_commitment_sha256: String,
    pub classifier_runtime_identity_commitment_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitivePregameVisibleFactKindV1 {
    Prompt,
    Hand,
    MulliganControls,
    BottomingControls,
    GameplaySurface,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregameVisibleFactV1 {
    pub kind: MtgoCompetitivePregameVisibleFactKindV1,
    pub rect_client_px: MtgoRectPxV1,
    pub visible_content_sha256: String,
    pub confidence_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregameVisibleCardV1 {
    pub card_slot: u8,
    pub visible_card_name: String,
    pub rect_client_px: MtgoRectPxV1,
    pub visible_content_sha256: String,
    pub confidence_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "control_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoCompetitivePregameVisibleControlSemanticV1 {
    KeepOpeningHand,
    Mulligan { next_hand_size: u8 },
    SelectForBottom { card_slot: u8, selected: bool },
    SubmitBottoming,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregameVisibleControlV1 {
    pub control_id: String,
    pub semantic: MtgoCompetitivePregameVisibleControlSemanticV1,
    pub rect_client_px: MtgoRectPxV1,
    pub visible_content_sha256: String,
    pub confidence_bps: u16,
    pub visibly_enabled: bool,
}

/// Strict classifier output. The response remains untrusted until every
/// region is rehashed from the exact request pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregameClassifierResponseV1 {
    pub schema_version: u32,
    pub request_commitment_sha256: String,
    pub stage: MtgoCompetitivePregameStageLabelV1,
    pub visible_facts: Vec<MtgoCompetitivePregameVisibleFactV1>,
    pub visible_cards: Vec<MtgoCompetitivePregameVisibleCardV1>,
    pub visible_controls: Vec<MtgoCompetitivePregameVisibleControlV1>,
    pub visible_interaction_commitment_sha256: String,
}

/// Canonical request metadata checked against the exact admitted profiles and
/// following BGRA8 bytes. It retains no pixels.
pub struct CheckedUntrustedMtgoCompetitivePregameClassifierRequestV1 {
    header: MtgoCompetitivePregameClassifierRequestHeaderV1,
    request_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitivePregameClassifierRequestV1 {
    pub fn request_commitment_sha256(&self) -> &str {
        &self.request_commitment_sha256
    }

    pub fn frame_id(&self) -> u64 {
        self.header.frame_id
    }

    pub fn frame_sequence(&self) -> u64 {
        self.header.frame_sequence
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }
}

/// Structurally and pixel checked pregame classification. It retains no
/// pixels, rectangles, event identity, action target, or input method.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitivePregameClassificationV1;
/// fn cannot_extract_target(value: &CheckedUntrustedMtgoCompetitivePregameClassificationV1) {
///     let _ = value.visible_facts();
///     let _ = value.input_point();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitivePregameClassificationV1 {
    source_capture_commitment_sha256: String,
    source_frame_profile_binding_sha256: String,
    classifier_runtime_identity_commitment_sha256: String,
    pregame_evaluation_commitment_sha256: String,
    pregame_profile_admission_commitment_sha256: String,
    request_commitment_sha256: String,
    classification_commitment_sha256: String,
    visible_interaction_commitment_sha256: String,
    frame_id: u64,
    frame_sequence: u64,
    stage: MtgoCompetitivePregameStageLabelV1,
}

impl CheckedUntrustedMtgoCompetitivePregameClassificationV1 {
    pub fn source_capture_commitment_sha256(&self) -> &str {
        &self.source_capture_commitment_sha256
    }

    pub fn source_frame_profile_binding_sha256(&self) -> &str {
        &self.source_frame_profile_binding_sha256
    }

    pub fn classifier_runtime_identity_commitment_sha256(&self) -> &str {
        &self.classifier_runtime_identity_commitment_sha256
    }

    pub fn pregame_evaluation_commitment_sha256(&self) -> &str {
        &self.pregame_evaluation_commitment_sha256
    }

    pub fn pregame_profile_admission_commitment_sha256(&self) -> &str {
        &self.pregame_profile_admission_commitment_sha256
    }

    pub fn request_commitment_sha256(&self) -> &str {
        &self.request_commitment_sha256
    }

    pub fn classification_commitment_sha256(&self) -> &str {
        &self.classification_commitment_sha256
    }

    pub fn visible_interaction_commitment_sha256(&self) -> &str {
        &self.visible_interaction_commitment_sha256
    }

    pub fn frame_id(&self) -> u64 {
        self.frame_id
    }

    pub fn frame_sequence(&self) -> u64 {
        self.frame_sequence
    }

    pub fn stage(&self) -> MtgoCompetitivePregameStageLabelV1 {
        self.stage
    }

    pub fn safe_for_live_classification_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }
}

pub fn check_untrusted_competitive_pregame_classifier_exchange_v1(
    duel_profile: &AdmittedMtgoDuelPerceptionProfileV1,
    pregame_profile: &AdmittedMtgoCompetitivePregameProfileV1,
    canonical_request_header_json: &[u8],
    canonical_bgra8: &[u8],
    canonical_response_json: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitivePregameClassificationV1, MtgoContractErrorV1> {
    let request = check_untrusted_competitive_pregame_classifier_request_v1(
        duel_profile,
        pregame_profile,
        canonical_request_header_json,
        canonical_bgra8,
    )?;
    check_untrusted_competitive_pregame_classifier_response_v1(
        pregame_profile,
        &request,
        canonical_bgra8,
        canonical_response_json,
    )
}

pub fn check_untrusted_competitive_pregame_classifier_request_v1(
    duel_profile: &AdmittedMtgoDuelPerceptionProfileV1,
    pregame_profile: &AdmittedMtgoCompetitivePregameProfileV1,
    canonical_request_header_json: &[u8],
    canonical_bgra8: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitivePregameClassifierRequestV1, MtgoContractErrorV1> {
    let header: MtgoCompetitivePregameClassifierRequestHeaderV1 =
        parse_canonical_json_v1(canonical_request_header_json, "request")?;
    validate_header_v1(&header, canonical_bgra8, duel_profile, pregame_profile)?;
    let request_commitment_sha256 = commitment_v1(
        COMPETITIVE_PREGAME_CLASSIFIER_REQUEST_DOMAIN_V1,
        &[canonical_request_header_json, canonical_bgra8],
    );
    Ok(CheckedUntrustedMtgoCompetitivePregameClassifierRequestV1 {
        header,
        request_commitment_sha256,
    })
}

pub fn check_untrusted_competitive_pregame_classifier_response_v1(
    pregame_profile: &AdmittedMtgoCompetitivePregameProfileV1,
    request: &CheckedUntrustedMtgoCompetitivePregameClassifierRequestV1,
    canonical_bgra8: &[u8],
    canonical_response_json: &[u8],
) -> Result<CheckedUntrustedMtgoCompetitivePregameClassificationV1, MtgoContractErrorV1> {
    if request.header.canonical_byte_length != canonical_bgra8.len()
        || request.header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
        || request.header.pregame_evaluation_commitment_sha256
            != pregame_profile.evaluation_commitment_sha256()
        || request.header.pregame_profile_admission_commitment_sha256
            != pregame_profile.admission_commitment_sha256()
    {
        return Err(error_v1(
            "competitive_pregame_classifier_response_source_mismatch",
            "response validation source differs from the checked request or pregame profile",
        ));
    }
    let response: MtgoCompetitivePregameClassifierResponseV1 =
        parse_canonical_json_v1(canonical_response_json, "response")?;
    if response.schema_version != MTGO_COMPETITIVE_PREGAME_CLASSIFIER_SCHEMA_V1
        || response.request_commitment_sha256 != request.request_commitment_sha256
        || !canonical_competitive_pregame_states_v1().contains(&response.stage)
    {
        return Err(error_v1(
            "competitive_pregame_classifier_response_invalid",
            "response schema, request binding, or stage is invalid",
        ));
    }
    validate_visible_facts_v1(
        response.stage,
        &response.visible_facts,
        canonical_bgra8,
        &MtgoSizePxV1 {
            width: request.header.canonical_width,
            height: request.header.canonical_height,
        },
    )?;
    validate_visible_cards_v1(
        response.stage,
        &response.visible_cards,
        canonical_bgra8,
        &MtgoSizePxV1 {
            width: request.header.canonical_width,
            height: request.header.canonical_height,
        },
    )?;
    validate_visible_controls_v1(
        response.stage,
        &response.visible_controls,
        canonical_bgra8,
        &MtgoSizePxV1 {
            width: request.header.canonical_width,
            height: request.header.canonical_height,
        },
    )?;
    let visible_interaction_commitment_sha256 =
        competitive_pregame_visible_interaction_commitment_v1(
            response.stage,
            &response.visible_cards,
            &response.visible_controls,
        )?;
    if response.visible_interaction_commitment_sha256 != visible_interaction_commitment_sha256 {
        return Err(error_v1(
            "competitive_pregame_visible_interaction_mismatch",
            "visible interaction commitment differs from the checked stage and controls",
        ));
    }

    let classification_commitment_sha256 = commitment_v1(
        COMPETITIVE_PREGAME_CLASSIFICATION_DOMAIN_V1,
        &[
            request.request_commitment_sha256.as_bytes(),
            canonical_response_json,
            pregame_profile.evaluation_commitment_sha256().as_bytes(),
            pregame_profile.admission_commitment_sha256().as_bytes(),
            b"pixel_checked_mode_independent_stage_and_interaction_no_live_or_input_authority",
        ],
    );
    Ok(CheckedUntrustedMtgoCompetitivePregameClassificationV1 {
        source_capture_commitment_sha256: request.header.source_capture_commitment_sha256.clone(),
        source_frame_profile_binding_sha256: request
            .header
            .source_frame_profile_binding_sha256
            .clone(),
        classifier_runtime_identity_commitment_sha256: request
            .header
            .classifier_runtime_identity_commitment_sha256
            .clone(),
        pregame_evaluation_commitment_sha256: request
            .header
            .pregame_evaluation_commitment_sha256
            .clone(),
        pregame_profile_admission_commitment_sha256: request
            .header
            .pregame_profile_admission_commitment_sha256
            .clone(),
        request_commitment_sha256: request.request_commitment_sha256.clone(),
        classification_commitment_sha256,
        visible_interaction_commitment_sha256,
        frame_id: request.header.frame_id,
        frame_sequence: request.header.frame_sequence,
        stage: response.stage,
    })
}

/// Commits the complete mode-independent visible pregame interaction surface.
/// The ordered controls include pixel hashes and card labels, but this digest
/// grants no live-classification, scoring, event-entry, or input authority.
pub fn competitive_pregame_visible_interaction_commitment_v1(
    stage: MtgoCompetitivePregameStageLabelV1,
    visible_cards: &[MtgoCompetitivePregameVisibleCardV1],
    controls: &[MtgoCompetitivePregameVisibleControlV1],
) -> Result<String, MtgoContractErrorV1> {
    let stage_json = serde_json::to_vec(&stage).map_err(|error| {
        error_v1(
            "competitive_pregame_visible_interaction_serialization_failed",
            error.to_string(),
        )
    })?;
    let controls_json = serde_json::to_vec(controls).map_err(|error| {
        error_v1(
            "competitive_pregame_visible_interaction_serialization_failed",
            error.to_string(),
        )
    })?;
    let visible_cards_json = serde_json::to_vec(visible_cards).map_err(|error| {
        error_v1(
            "competitive_pregame_visible_interaction_serialization_failed",
            error.to_string(),
        )
    })?;
    Ok(commitment_v1(
        COMPETITIVE_PREGAME_VISIBLE_INTERACTION_DOMAIN_V1,
        &[&stage_json, &visible_cards_json, &controls_json],
    ))
}

fn validate_header_v1(
    header: &MtgoCompetitivePregameClassifierRequestHeaderV1,
    canonical_bgra8: &[u8],
    duel_profile: &AdmittedMtgoDuelPerceptionProfileV1,
    pregame_profile: &AdmittedMtgoCompetitivePregameProfileV1,
) -> Result<(), MtgoContractErrorV1> {
    let expected_stride = header.canonical_width.checked_mul(4).ok_or_else(|| {
        error_v1(
            "competitive_pregame_classifier_geometry_invalid",
            "canonical stride overflow",
        )
    })?;
    let expected_length = usize::try_from(header.canonical_width)
        .ok()
        .and_then(|width| {
            usize::try_from(header.canonical_height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            error_v1(
                "competitive_pregame_classifier_geometry_invalid",
                "canonical byte length overflow",
            )
        })?;
    if header.schema_version != MTGO_COMPETITIVE_PREGAME_CLASSIFIER_SCHEMA_V1
        || header.protocol != MTGO_COMPETITIVE_PREGAME_CLASSIFIER_PROTOCOL_V1
        || header.frame_id == 0
        || header.frame_sequence == 0
        || header.canonical_width == 0
        || header.canonical_height == 0
        || header.canonical_width > 16_384
        || header.canonical_height > 16_384
        || header.canonical_width != duel_profile.client_size_px().width
        || header.canonical_height != duel_profile.client_size_px().height
        || header.canonical_stride != expected_stride
        || header.canonical_byte_length != expected_length
        || canonical_bgra8.len() != expected_length
        || header.canonical_bgra8_sha256 != sha256_hex_v1(canonical_bgra8)
    {
        return Err(error_v1(
            "competitive_pregame_classifier_request_invalid",
            "request identity, geometry, or pixels are invalid",
        ));
    }
    for digest in [
        header.canonical_bgra8_sha256.as_str(),
        header.source_capture_commitment_sha256.as_str(),
        header.source_frame_profile_binding_sha256.as_str(),
        header.duel_perception_profile_commitment_sha256.as_str(),
        header
            .duel_perception_profile_admission_commitment_sha256
            .as_str(),
        header.pregame_evaluation_commitment_sha256.as_str(),
        header.pregame_profile_admission_commitment_sha256.as_str(),
        header
            .classifier_runtime_identity_commitment_sha256
            .as_str(),
    ] {
        validate_lower_hex_sha256_v1(digest)?;
    }
    if pregame_profile.duel_perception_profile_commitment_sha256()
        != duel_profile.perception_profile_commitment_sha256()
        || pregame_profile.duel_perception_profile_admission_commitment_sha256()
            != duel_profile.admission_commitment_sha256()
        || header.duel_perception_profile_commitment_sha256
            != duel_profile.perception_profile_commitment_sha256()
        || header.duel_perception_profile_admission_commitment_sha256
            != duel_profile.admission_commitment_sha256()
        || header.pregame_evaluation_commitment_sha256
            != pregame_profile.evaluation_commitment_sha256()
        || header.pregame_profile_admission_commitment_sha256
            != pregame_profile.admission_commitment_sha256()
    {
        return Err(error_v1(
            "competitive_pregame_classifier_profile_mismatch",
            "request does not bind the exact duel and pregame profile admissions",
        ));
    }
    Ok(())
}

fn validate_visible_facts_v1(
    stage: MtgoCompetitivePregameStageLabelV1,
    facts: &[MtgoCompetitivePregameVisibleFactV1],
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), MtgoContractErrorV1> {
    let required = required_fact_kinds_v1(stage);
    let actual: Vec<_> = facts.iter().map(|fact| fact.kind).collect();
    if actual != required {
        return Err(error_v1(
            "competitive_pregame_visible_fact_set_invalid",
            "visible facts must be the exact canonical set for the stage",
        ));
    }
    let mut distinct_rects = HashSet::new();
    for fact in facts {
        validate_lower_hex_sha256_v1(&fact.visible_content_sha256)?;
        if fact.confidence_bps < MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1
            || fact.confidence_bps > 10_000
        {
            return Err(error_v1(
                "competitive_pregame_visible_fact_confidence_invalid",
                format!("{:?}", fact.kind),
            ));
        }
        if fact.rect_client_px.width < 2
            || fact.rect_client_px.height < 2
            || !distinct_rects.insert((
                fact.rect_client_px.x,
                fact.rect_client_px.y,
                fact.rect_client_px.width,
                fact.rect_client_px.height,
            ))
        {
            return Err(error_v1(
                "competitive_pregame_visible_fact_geometry_invalid",
                format!("{:?}", fact.kind),
            ));
        }
        let observed =
            visible_frame_region_content_sha256_v1(canonical_bgra8, size, &fact.rect_client_px)?;
        if observed != fact.visible_content_sha256 {
            return Err(error_v1(
                "competitive_pregame_visible_fact_pixels_mismatch",
                format!("{:?}", fact.kind),
            ));
        }
    }
    Ok(())
}

fn required_fact_kinds_v1(
    stage: MtgoCompetitivePregameStageLabelV1,
) -> Vec<MtgoCompetitivePregameVisibleFactKindV1> {
    use MtgoCompetitivePregameVisibleFactKindV1::*;
    match stage {
        MtgoCompetitivePregameStageLabelV1::MulliganChoice { .. } => {
            vec![Prompt, Hand, MulliganControls]
        }
        MtgoCompetitivePregameStageLabelV1::LondonBottoming { .. } => {
            vec![Prompt, Hand, BottomingControls]
        }
        MtgoCompetitivePregameStageLabelV1::GameplayReady => vec![Prompt, GameplaySurface],
    }
}

fn validate_visible_cards_v1(
    stage: MtgoCompetitivePregameStageLabelV1,
    cards: &[MtgoCompetitivePregameVisibleCardV1],
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), MtgoContractErrorV1> {
    let expected_count = usize::from(!matches!(
        stage,
        MtgoCompetitivePregameStageLabelV1::GameplayReady
    )) * 7;
    if cards.len() != expected_count {
        return Err(error_v1(
            "competitive_pregame_visible_card_set_invalid",
            "pregame decisions require exactly seven ordered visible cards",
        ));
    }
    let mut rectangles = HashSet::new();
    for (slot, card) in cards.iter().enumerate() {
        validate_lower_hex_sha256_v1(&card.visible_content_sha256)?;
        if usize::from(card.card_slot) != slot
            || !valid_visible_card_name_v1(&card.visible_card_name)
            || card.confidence_bps < MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1
            || card.confidence_bps > 10_000
            || card.rect_client_px.width < 2
            || card.rect_client_px.height < 2
            || !rectangles.insert((
                card.rect_client_px.x,
                card.rect_client_px.y,
                card.rect_client_px.width,
                card.rect_client_px.height,
            ))
        {
            return Err(error_v1(
                "competitive_pregame_visible_card_invalid",
                slot.to_string(),
            ));
        }
        let observed =
            visible_frame_region_content_sha256_v1(canonical_bgra8, size, &card.rect_client_px)?;
        if observed != card.visible_content_sha256 {
            return Err(error_v1(
                "competitive_pregame_visible_card_pixels_mismatch",
                slot.to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_visible_controls_v1(
    stage: MtgoCompetitivePregameStageLabelV1,
    controls: &[MtgoCompetitivePregameVisibleControlV1],
    canonical_bgra8: &[u8],
    size: &MtgoSizePxV1,
) -> Result<(), MtgoContractErrorV1> {
    let mut control_ids = HashSet::new();
    let mut rectangles = HashSet::new();
    let mut centers = HashSet::new();
    for control in controls {
        validate_safe_control_id_v1(&control.control_id)?;
        validate_lower_hex_sha256_v1(&control.visible_content_sha256)?;
        if !control_ids.insert(control.control_id.as_str())
            || !control.visibly_enabled
            || control.confidence_bps < MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1
            || control.confidence_bps > 10_000
            || control.rect_client_px.width < 2
            || control.rect_client_px.height < 2
            || !rectangles.insert((
                control.rect_client_px.x,
                control.rect_client_px.y,
                control.rect_client_px.width,
                control.rect_client_px.height,
            ))
        {
            return Err(error_v1(
                "competitive_pregame_visible_control_invalid",
                &control.control_id,
            ));
        }
        let center = control_center_v1(&control.rect_client_px)?;
        if !centers.insert(center) {
            return Err(error_v1(
                "competitive_pregame_visible_control_target_ambiguous",
                &control.control_id,
            ));
        }
        let observed =
            visible_frame_region_content_sha256_v1(canonical_bgra8, size, &control.rect_client_px)?;
        if observed != control.visible_content_sha256 {
            return Err(error_v1(
                "competitive_pregame_visible_control_pixels_mismatch",
                &control.control_id,
            ));
        }
    }
    for (index, control) in controls.iter().enumerate() {
        let center = control_center_v1(&control.rect_client_px)?;
        if controls.iter().enumerate().any(|(other_index, other)| {
            other_index != index && rect_contains_point_v1(&other.rect_client_px, center)
        }) {
            return Err(error_v1(
                "competitive_pregame_visible_control_target_ambiguous",
                &control.control_id,
            ));
        }
    }
    validate_stage_control_set_v1(stage, controls)
}

fn validate_stage_control_set_v1(
    stage: MtgoCompetitivePregameStageLabelV1,
    controls: &[MtgoCompetitivePregameVisibleControlV1],
) -> Result<(), MtgoContractErrorV1> {
    match stage {
        MtgoCompetitivePregameStageLabelV1::MulliganChoice {
            prospective_keep_size,
        } => {
            if prospective_keep_size > 7 {
                return Err(error_v1(
                    "competitive_pregame_stage_invalid",
                    "prospective keep size must be at most seven",
                ));
            }
            let expected_len = if prospective_keep_size == 0 { 1 } else { 2 };
            if controls.len() != expected_len
                || controls[0].control_id != "keep_opening_hand"
                || controls[0].semantic
                    != MtgoCompetitivePregameVisibleControlSemanticV1::KeepOpeningHand
                || (prospective_keep_size > 0
                    && (controls[1].control_id != "mulligan"
                        || controls[1].semantic
                            != MtgoCompetitivePregameVisibleControlSemanticV1::Mulligan {
                                next_hand_size: prospective_keep_size - 1,
                            }))
            {
                return Err(error_v1(
                    "competitive_pregame_visible_control_set_invalid",
                    "mulligan choice controls",
                ));
            }
        }
        MtgoCompetitivePregameStageLabelV1::LondonBottoming {
            required_bottom_count,
            selected_bottom_count,
        } => {
            if !(1..=7).contains(&required_bottom_count)
                || selected_bottom_count > required_bottom_count
            {
                return Err(error_v1(
                    "competitive_pregame_stage_invalid",
                    "London bottom count must be one through seven and selected cannot exceed required",
                ));
            }
            let expected_len =
                7_usize + usize::from(selected_bottom_count == required_bottom_count);
            if controls.len() != expected_len {
                return Err(error_v1(
                    "competitive_pregame_visible_control_set_invalid",
                    "London bottoming control count",
                ));
            }
            let mut observed_selected_count = 0_u8;
            for (slot, control) in controls.iter().take(7).enumerate() {
                let expected_id = format!("bottom_card_{slot}");
                let MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
                    card_slot,
                    selected,
                } = &control.semantic
                else {
                    return Err(error_v1(
                        "competitive_pregame_visible_control_set_invalid",
                        "London bottoming card semantic",
                    ));
                };
                if control.control_id != expected_id || usize::from(*card_slot) != slot {
                    return Err(error_v1(
                        "competitive_pregame_visible_control_set_invalid",
                        "London bottoming card identity",
                    ));
                }
                observed_selected_count = observed_selected_count
                    .checked_add(u8::from(*selected))
                    .ok_or_else(|| {
                        error_v1(
                            "competitive_pregame_visible_control_set_invalid",
                            "selected card count overflow",
                        )
                    })?;
            }
            if observed_selected_count != selected_bottom_count
                || (selected_bottom_count == required_bottom_count
                    && (controls[7].control_id != "submit_bottoming"
                        || controls[7].semantic
                            != MtgoCompetitivePregameVisibleControlSemanticV1::SubmitBottoming))
            {
                return Err(error_v1(
                    "competitive_pregame_visible_control_set_invalid",
                    "London bottoming selected count or Submit control",
                ));
            }
        }
        MtgoCompetitivePregameStageLabelV1::GameplayReady if controls.is_empty() => {}
        MtgoCompetitivePregameStageLabelV1::GameplayReady => {
            return Err(error_v1(
                "competitive_pregame_visible_control_set_invalid",
                "GameplayReady must expose no pregame controls",
            ));
        }
    }
    Ok(())
}

fn control_center_v1(rect: &MtgoRectPxV1) -> Result<(u32, u32), MtgoContractErrorV1> {
    let x = rect
        .x
        .checked_add(rect.width / 2)
        .ok_or_else(|| error_v1("competitive_pregame_visible_control_invalid", "center x"))?;
    let y = rect
        .y
        .checked_add(rect.height / 2)
        .ok_or_else(|| error_v1("competitive_pregame_visible_control_invalid", "center y"))?;
    Ok((x, y))
}

fn rect_contains_point_v1(rect: &MtgoRectPxV1, point: (u32, u32)) -> bool {
    rect.x
        .checked_add(rect.width)
        .zip(rect.y.checked_add(rect.height))
        .is_some_and(|(right, bottom)| {
            point.0 >= rect.x && point.0 < right && point.1 >= rect.y && point.1 < bottom
        })
}

fn validate_safe_control_id_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(error_v1(
            "competitive_pregame_visible_control_id_invalid",
            value,
        ));
    }
    Ok(())
}

fn valid_visible_card_name_v1(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn parse_canonical_json_v1<T>(bytes: &[u8], label: &str) -> Result<T, MtgoContractErrorV1>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    let value: T = serde_json::from_slice(bytes).map_err(|error| {
        error_v1(
            "competitive_pregame_classifier_json_invalid",
            format!("parse {label}: {error}"),
        )
    })?;
    let reencoded = serde_json::to_vec(&value).map_err(|error| {
        error_v1(
            "competitive_pregame_classifier_json_invalid",
            format!("serialize {label}: {error}"),
        )
    })?;
    if reencoded != bytes {
        return Err(error_v1(
            "competitive_pregame_classifier_json_noncanonical",
            label,
        ));
    }
    Ok(value)
}

fn validate_lower_hex_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            "competitive_pregame_classifier_sha256_invalid",
            value,
        ));
    }
    Ok(())
}

fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
pub(crate) fn checked_untrusted_competitive_pregame_classification_for_context_test_v1(
    source_capture_commitment_sha256: String,
    source_frame_profile_binding_sha256: String,
    classification_commitment_sha256: String,
    visible_interaction_commitment_sha256: String,
    frame_id: u64,
    frame_sequence: u64,
) -> CheckedUntrustedMtgoCompetitivePregameClassificationV1 {
    CheckedUntrustedMtgoCompetitivePregameClassificationV1 {
        source_capture_commitment_sha256,
        source_frame_profile_binding_sha256,
        classifier_runtime_identity_commitment_sha256: "e".repeat(64),
        pregame_evaluation_commitment_sha256: "f".repeat(64),
        pregame_profile_admission_commitment_sha256: "1".repeat(64),
        request_commitment_sha256: "2".repeat(64),
        classification_commitment_sha256,
        visible_interaction_commitment_sha256,
        frame_id,
        frame_sequence,
        stage: MtgoCompetitivePregameStageLabelV1::MulliganChoice {
            prospective_keep_size: 7,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        check_untrusted_duel_perception_runtime_profile_v1,
        competitive_pregame_profile_admitted_for_test_v1,
        duel_perception_profile_admitted_for_test_v1,
        duel_perception_runtime_profile_payload_for_test_v1,
    };

    fn profiles_v1() -> (
        AdmittedMtgoDuelPerceptionProfileV1,
        AdmittedMtgoCompetitivePregameProfileV1,
    ) {
        let mut payload = duel_perception_runtime_profile_payload_for_test_v1();
        payload.client_size_px = MtgoSizePxV1 {
            width: 16,
            height: 16,
        };
        let duel = duel_perception_profile_admitted_for_test_v1(
            check_untrusted_duel_perception_runtime_profile_v1(payload).unwrap(),
        );
        let pregame = competitive_pregame_profile_admitted_for_test_v1(&duel);
        (duel, pregame)
    }

    fn pixels_v1() -> Vec<u8> {
        (0_u16..1_024).map(|value| value as u8).collect()
    }

    fn rect_v1(kind: MtgoCompetitivePregameVisibleFactKindV1) -> MtgoRectPxV1 {
        use MtgoCompetitivePregameVisibleFactKindV1::*;
        match kind {
            Prompt => MtgoRectPxV1 {
                x: 0,
                y: 0,
                width: 4,
                height: 2,
            },
            Hand => MtgoRectPxV1 {
                x: 0,
                y: 4,
                width: 16,
                height: 8,
            },
            MulliganControls => MtgoRectPxV1 {
                x: 8,
                y: 0,
                width: 4,
                height: 2,
            },
            BottomingControls => MtgoRectPxV1 {
                x: 0,
                y: 14,
                width: 4,
                height: 2,
            },
            GameplaySurface => MtgoRectPxV1 {
                x: 8,
                y: 14,
                width: 4,
                height: 2,
            },
        }
    }

    fn header_v1(
        duel: &AdmittedMtgoDuelPerceptionProfileV1,
        pregame: &AdmittedMtgoCompetitivePregameProfileV1,
        pixels: &[u8],
    ) -> MtgoCompetitivePregameClassifierRequestHeaderV1 {
        MtgoCompetitivePregameClassifierRequestHeaderV1 {
            schema_version: MTGO_COMPETITIVE_PREGAME_CLASSIFIER_SCHEMA_V1,
            protocol: MTGO_COMPETITIVE_PREGAME_CLASSIFIER_PROTOCOL_V1.to_owned(),
            frame_id: 10,
            frame_sequence: 20,
            canonical_width: 16,
            canonical_height: 16,
            canonical_stride: 64,
            canonical_byte_length: pixels.len(),
            canonical_bgra8_sha256: sha256_hex_v1(pixels),
            source_capture_commitment_sha256: "1".repeat(64),
            source_frame_profile_binding_sha256: "2".repeat(64),
            duel_perception_profile_commitment_sha256: duel
                .perception_profile_commitment_sha256()
                .to_owned(),
            duel_perception_profile_admission_commitment_sha256: duel
                .admission_commitment_sha256()
                .to_owned(),
            pregame_evaluation_commitment_sha256: pregame.evaluation_commitment_sha256().to_owned(),
            pregame_profile_admission_commitment_sha256: pregame
                .admission_commitment_sha256()
                .to_owned(),
            classifier_runtime_identity_commitment_sha256: "3".repeat(64),
        }
    }

    fn response_v1(
        header_json: &[u8],
        pixels: &[u8],
        stage: MtgoCompetitivePregameStageLabelV1,
    ) -> MtgoCompetitivePregameClassifierResponseV1 {
        let request = commitment_v1(
            COMPETITIVE_PREGAME_CLASSIFIER_REQUEST_DOMAIN_V1,
            &[header_json, pixels],
        );
        let size = MtgoSizePxV1 {
            width: 16,
            height: 16,
        };
        let visible_facts = required_fact_kinds_v1(stage)
            .into_iter()
            .map(|kind| {
                let rect_client_px = rect_v1(kind);
                MtgoCompetitivePregameVisibleFactV1 {
                    kind,
                    visible_content_sha256: visible_frame_region_content_sha256_v1(
                        pixels,
                        &size,
                        &rect_client_px,
                    )
                    .unwrap(),
                    rect_client_px,
                    confidence_bps: 10_000,
                }
            })
            .collect();
        let visible_controls = controls_v1(stage, pixels, &size);
        let visible_cards = cards_v1(stage, pixels, &size);
        let visible_interaction_commitment_sha256 =
            competitive_pregame_visible_interaction_commitment_v1(
                stage,
                &visible_cards,
                &visible_controls,
            )
            .unwrap();
        MtgoCompetitivePregameClassifierResponseV1 {
            schema_version: MTGO_COMPETITIVE_PREGAME_CLASSIFIER_SCHEMA_V1,
            request_commitment_sha256: request,
            stage,
            visible_facts,
            visible_cards,
            visible_controls,
            visible_interaction_commitment_sha256,
        }
    }

    fn cards_v1(
        stage: MtgoCompetitivePregameStageLabelV1,
        pixels: &[u8],
        size: &MtgoSizePxV1,
    ) -> Vec<MtgoCompetitivePregameVisibleCardV1> {
        if matches!(stage, MtgoCompetitivePregameStageLabelV1::GameplayReady) {
            return Vec::new();
        }
        (0_u8..7)
            .map(|card_slot| {
                let rect_client_px = MtgoRectPxV1 {
                    x: u32::from(card_slot) * 2,
                    y: 8,
                    width: 2,
                    height: 2,
                };
                MtgoCompetitivePregameVisibleCardV1 {
                    card_slot,
                    visible_card_name: format!("Card {card_slot}"),
                    visible_content_sha256: visible_frame_region_content_sha256_v1(
                        pixels,
                        size,
                        &rect_client_px,
                    )
                    .unwrap(),
                    rect_client_px,
                    confidence_bps: 10_000,
                }
            })
            .collect()
    }

    fn controls_v1(
        stage: MtgoCompetitivePregameStageLabelV1,
        pixels: &[u8],
        size: &MtgoSizePxV1,
    ) -> Vec<MtgoCompetitivePregameVisibleControlV1> {
        let semantics = match stage {
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size,
            } => {
                let mut values = vec![(
                    "keep_opening_hand".to_owned(),
                    MtgoCompetitivePregameVisibleControlSemanticV1::KeepOpeningHand,
                )];
                if prospective_keep_size > 0 {
                    values.push((
                        "mulligan".to_owned(),
                        MtgoCompetitivePregameVisibleControlSemanticV1::Mulligan {
                            next_hand_size: prospective_keep_size - 1,
                        },
                    ));
                }
                values
            }
            MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count,
                selected_bottom_count,
            } => {
                let mut values: Vec<_> = (0_u8..7)
                    .map(|card_slot| {
                        (
                            format!("bottom_card_{card_slot}"),
                            MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom {
                                card_slot,
                                selected: card_slot < selected_bottom_count,
                            },
                        )
                    })
                    .collect();
                if selected_bottom_count == required_bottom_count {
                    values.push((
                        "submit_bottoming".to_owned(),
                        MtgoCompetitivePregameVisibleControlSemanticV1::SubmitBottoming,
                    ));
                }
                values
            }
            MtgoCompetitivePregameStageLabelV1::GameplayReady => Vec::new(),
        };
        semantics
            .into_iter()
            .enumerate()
            .map(|(index, (control_id, semantic))| {
                let rect_client_px = MtgoRectPxV1 {
                    x: u32::try_from(index).unwrap() * 2,
                    y: 12,
                    width: 2,
                    height: 2,
                };
                MtgoCompetitivePregameVisibleControlV1 {
                    control_id,
                    semantic,
                    visible_content_sha256: visible_frame_region_content_sha256_v1(
                        pixels,
                        size,
                        &rect_client_px,
                    )
                    .unwrap(),
                    rect_client_px,
                    confidence_bps: 10_000,
                    visibly_enabled: true,
                }
            })
            .collect()
    }

    fn check_stage_v1(
        stage: MtgoCompetitivePregameStageLabelV1,
    ) -> CheckedUntrustedMtgoCompetitivePregameClassificationV1 {
        let (duel, pregame) = profiles_v1();
        let pixels = pixels_v1();
        let header_json = serde_json::to_vec(&header_v1(&duel, &pregame, &pixels)).unwrap();
        let response_json = serde_json::to_vec(&response_v1(&header_json, &pixels, stage)).unwrap();
        check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &response_json,
        )
        .unwrap()
    }

    #[test]
    fn exact_mulligan_bottoming_and_ready_exchanges_bind_stage_without_authority() {
        for stage in [
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size: 6,
            },
            MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count: 2,
                selected_bottom_count: 1,
            },
            MtgoCompetitivePregameStageLabelV1::GameplayReady,
        ] {
            let checked = check_stage_v1(stage);
            assert_eq!(checked.stage(), stage);
            assert_eq!(checked.frame_id(), 10);
            assert_eq!(checked.frame_sequence(), 20);
            assert!(
                validate_lower_hex_sha256_v1(checked.visible_interaction_commitment_sha256())
                    .is_ok()
            );
            assert!(!checked.safe_for_live_classification_v1());
            assert!(!checked.safe_for_input_v1());
            assert!(!checked.permits_event_entry_v1());
        }
    }

    #[test]
    fn pixel_request_or_fact_substitution_is_rejected() {
        let (duel, pregame) = profiles_v1();
        let pixels = pixels_v1();
        let header_json = serde_json::to_vec(&header_v1(&duel, &pregame, &pixels)).unwrap();
        let mut response = response_v1(
            &header_json,
            &pixels,
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size: 7,
            },
        );
        response.visible_facts[0].visible_content_sha256 = "9".repeat(64);
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &serde_json::to_vec(&response).unwrap(),
        )
        .is_err());

        let mut changed_pixels = pixels.clone();
        changed_pixels[0] ^= 1;
        let response_json = serde_json::to_vec(&response_v1(
            &header_json,
            &pixels,
            MtgoCompetitivePregameStageLabelV1::GameplayReady,
        ))
        .unwrap();
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &changed_pixels,
            &response_json,
        )
        .is_err());
    }

    #[test]
    fn exact_stage_fact_set_and_confidence_are_required() {
        let (duel, pregame) = profiles_v1();
        let pixels = pixels_v1();
        let header_json = serde_json::to_vec(&header_v1(&duel, &pregame, &pixels)).unwrap();
        let mut response = response_v1(
            &header_json,
            &pixels,
            MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count: 1,
                selected_bottom_count: 0,
            },
        );
        response.visible_facts.pop();
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &serde_json::to_vec(&response).unwrap(),
        )
        .is_err());

        let mut low_confidence = response_v1(
            &header_json,
            &pixels,
            MtgoCompetitivePregameStageLabelV1::GameplayReady,
        );
        low_confidence.visible_facts[0].confidence_bps = MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1 - 1;
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &serde_json::to_vec(&low_confidence).unwrap(),
        )
        .is_err());
    }

    #[test]
    fn exact_stage_control_surface_and_pixels_are_required() {
        let (duel, pregame) = profiles_v1();
        let pixels = pixels_v1();
        let header_json = serde_json::to_vec(&header_v1(&duel, &pregame, &pixels)).unwrap();
        let stage = MtgoCompetitivePregameStageLabelV1::LondonBottoming {
            required_bottom_count: 2,
            selected_bottom_count: 1,
        };
        let mut bad_card = response_v1(&header_json, &pixels, stage);
        bad_card.visible_cards[0].visible_content_sha256 = "8".repeat(64);
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &serde_json::to_vec(&bad_card).unwrap(),
        )
        .is_err());

        let mut response = response_v1(&header_json, &pixels, stage);
        response.visible_controls[1].visible_content_sha256 = "9".repeat(64);
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &serde_json::to_vec(&response).unwrap(),
        )
        .is_err());

        let mut wrong_interaction = response_v1(&header_json, &pixels, stage);
        wrong_interaction.visible_interaction_commitment_sha256 = "7".repeat(64);
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &serde_json::to_vec(&wrong_interaction).unwrap(),
        )
        .is_err());

        let mut wrong_selection_count = response_v1(&header_json, &pixels, stage);
        let MtgoCompetitivePregameVisibleControlSemanticV1::SelectForBottom { selected, .. } =
            &mut wrong_selection_count.visible_controls[1].semantic
        else {
            unreachable!();
        };
        *selected = true;
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &serde_json::to_vec(&wrong_selection_count).unwrap(),
        )
        .is_err());

        let mut ambiguous = response_v1(
            &header_json,
            &pixels,
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size: 7,
            },
        );
        ambiguous.visible_controls[1].rect_client_px =
            ambiguous.visible_controls[0].rect_client_px.clone();
        ambiguous.visible_controls[1].visible_content_sha256 =
            ambiguous.visible_controls[0].visible_content_sha256.clone();
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &serde_json::to_vec(&ambiguous).unwrap(),
        )
        .is_err());

        let mut ready_has_control = response_v1(
            &header_json,
            &pixels,
            MtgoCompetitivePregameStageLabelV1::GameplayReady,
        );
        ready_has_control.visible_controls = controls_v1(
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size: 0,
            },
            &pixels,
            &MtgoSizePxV1 {
                width: 16,
                height: 16,
            },
        );
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &serde_json::to_vec(&ready_has_control).unwrap(),
        )
        .is_err());
    }

    #[test]
    fn impossible_mulligan_and_bottoming_counts_are_rejected() {
        let (duel, pregame) = profiles_v1();
        let pixels = pixels_v1();
        let header_json = serde_json::to_vec(&header_v1(&duel, &pregame, &pixels)).unwrap();
        for stage in [
            MtgoCompetitivePregameStageLabelV1::MulliganChoice {
                prospective_keep_size: 8,
            },
            MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count: 0,
                selected_bottom_count: 0,
            },
            MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count: 8,
                selected_bottom_count: 0,
            },
            MtgoCompetitivePregameStageLabelV1::LondonBottoming {
                required_bottom_count: 2,
                selected_bottom_count: 3,
            },
        ] {
            let response = response_v1(&header_json, &pixels, stage);
            assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
                &duel,
                &pregame,
                &header_json,
                &pixels,
                &serde_json::to_vec(&response).unwrap(),
            )
            .is_err());
        }
    }

    #[test]
    fn crossed_profile_or_response_request_is_rejected() {
        let (duel, pregame) = profiles_v1();
        let pixels = pixels_v1();
        let mut header = header_v1(&duel, &pregame, &pixels);
        header.pregame_profile_admission_commitment_sha256 = "8".repeat(64);
        let header_json = serde_json::to_vec(&header).unwrap();
        let response_json = serde_json::to_vec(&response_v1(
            &header_json,
            &pixels,
            MtgoCompetitivePregameStageLabelV1::GameplayReady,
        ))
        .unwrap();
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &header_json,
            &pixels,
            &response_json,
        )
        .is_err());

        let good_header_json = serde_json::to_vec(&header_v1(&duel, &pregame, &pixels)).unwrap();
        let mut response = response_v1(
            &good_header_json,
            &pixels,
            MtgoCompetitivePregameStageLabelV1::GameplayReady,
        );
        response.request_commitment_sha256 = "7".repeat(64);
        assert!(check_untrusted_competitive_pregame_classifier_exchange_v1(
            &duel,
            &pregame,
            &good_header_json,
            &pixels,
            &serde_json::to_vec(&response).unwrap(),
        )
        .is_err());
    }
}
