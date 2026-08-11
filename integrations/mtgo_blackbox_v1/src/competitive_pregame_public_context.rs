use crate::{
    visible_frame_region_content_sha256_v1, CheckedUntrustedMtgoCompetitivePregameClassificationV1,
    MtgoContractErrorV1, MtgoRectPxV1, MtgoSizePxV1, MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_SCHEMA_V1: u32 = 1;

const COMPETITIVE_PREGAME_PUBLIC_CONTEXT_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-pregame-public-context-v1";
const COMPETITIVE_PREGAME_PUBLIC_CONTEXT_BINDING_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-pregame-public-context-binding-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitivePregamePlayDrawV1 {
    OnPlay,
    OnDraw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoCompetitivePregamePublicContextFactKindV1 {
    PlayDrawIndicator,
    ActingPlayerMatchScore,
    OpponentMatchScore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregamePublicContextFactV1 {
    pub kind: MtgoCompetitivePregamePublicContextFactKindV1,
    pub rect_client_px: MtgoRectPxV1,
    pub visible_content_sha256: String,
    pub confidence_bps: u16,
}

/// Checked-untrusted classifier output for public match context needed by a
/// future pregame model. Every fact is rehashed from the supplied frame, but
/// this structural check does not prove the classifier interpreted the pixels
/// correctly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitivePregamePublicContextCandidateV1 {
    pub schema_version: u32,
    pub source_capture_commitment_sha256: String,
    pub source_frame_profile_binding_sha256: String,
    pub pregame_classification_commitment_sha256: String,
    pub visible_interaction_commitment_sha256: String,
    pub frame_id: u64,
    pub frame_sequence: u64,
    pub game_number: u8,
    pub play_draw: MtgoCompetitivePregamePlayDrawV1,
    pub acting_player_games_won: u8,
    pub opponent_games_won: u8,
    pub visible_facts: Vec<MtgoCompetitivePregamePublicContextFactV1>,
    pub public_context_commitment_sha256: String,
}

/// Pixel-checked public context with no classifier admission, model, event,
/// or input authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoCompetitivePregamePublicContextV1;
/// fn cannot_extract_rects(value: &CheckedUntrustedMtgoCompetitivePregamePublicContextV1) {
///     let _ = value.visible_facts();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitivePregamePublicContextV1 {
    candidate: MtgoCompetitivePregamePublicContextCandidateV1,
}

impl CheckedUntrustedMtgoCompetitivePregamePublicContextV1 {
    pub fn public_context_commitment_sha256(&self) -> &str {
        &self.candidate.public_context_commitment_sha256
    }

    pub fn game_number(&self) -> u8 {
        self.candidate.game_number
    }

    pub fn play_draw(&self) -> MtgoCompetitivePregamePlayDrawV1 {
        self.candidate.play_draw
    }

    pub fn acting_player_games_won(&self) -> u8 {
        self.candidate.acting_player_games_won
    }

    pub fn opponent_games_won(&self) -> u8 {
        self.candidate.opponent_games_won
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

/// Exact-frame binding between the already checked pregame classification and
/// its separately pixel-checked public match context. This remains untrusted
/// model input because neither classifier has a production accuracy admission.
pub struct CheckedUntrustedMtgoCompetitivePregameModelContextV1 {
    _classification: CheckedUntrustedMtgoCompetitivePregameClassificationV1,
    _public_context: CheckedUntrustedMtgoCompetitivePregamePublicContextV1,
    binding_commitment_sha256: String,
    game_number: u8,
    play_draw: MtgoCompetitivePregamePlayDrawV1,
    acting_player_games_won: u8,
    opponent_games_won: u8,
}

impl CheckedUntrustedMtgoCompetitivePregameModelContextV1 {
    pub fn binding_commitment_sha256(&self) -> &str {
        &self.binding_commitment_sha256
    }

    pub fn game_number(&self) -> u8 {
        self.game_number
    }

    pub fn play_draw(&self) -> MtgoCompetitivePregamePlayDrawV1 {
        self.play_draw
    }

    pub fn match_score(&self) -> (u8, u8) {
        (self.acting_player_games_won, self.opponent_games_won)
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn competitive_pregame_public_context_commitment_v1(
    candidate: &MtgoCompetitivePregamePublicContextCandidateV1,
) -> Result<String, MtgoContractErrorV1> {
    let mut unsigned = candidate.clone();
    unsigned.public_context_commitment_sha256.clear();
    let bytes = serde_json::to_vec(&unsigned).map_err(|error| {
        error_v1(
            "competitive_pregame_public_context_serialization_failed",
            error.to_string(),
        )
    })?;
    Ok(commitment_v1(
        COMPETITIVE_PREGAME_PUBLIC_CONTEXT_DOMAIN_V1,
        &[&bytes],
    ))
}

pub fn check_untrusted_competitive_pregame_public_context_v1(
    candidate: MtgoCompetitivePregamePublicContextCandidateV1,
    canonical_bgra8: &[u8],
    client_size_px: MtgoSizePxV1,
) -> Result<CheckedUntrustedMtgoCompetitivePregamePublicContextV1, MtgoContractErrorV1> {
    if candidate.schema_version != MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_SCHEMA_V1
        || candidate.frame_id == 0
        || candidate.frame_sequence == 0
        || client_size_px.width == 0
        || client_size_px.height == 0
    {
        return Err(error_v1(
            "competitive_pregame_public_context_identity_invalid",
            "schema, frame identity, or client size is invalid",
        ));
    }
    let expected_pixel_length = usize::try_from(client_size_px.width)
        .ok()
        .and_then(|width| {
            usize::try_from(client_size_px.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            error_v1(
                "competitive_pregame_public_context_geometry_invalid",
                "pixel length overflow",
            )
        })?;
    if canonical_bgra8.len() != expected_pixel_length {
        return Err(error_v1(
            "competitive_pregame_public_context_pixels_invalid",
            "canonical pixel length differs from the client size",
        ));
    }
    for digest in [
        candidate.source_capture_commitment_sha256.as_str(),
        candidate.source_frame_profile_binding_sha256.as_str(),
        candidate.pregame_classification_commitment_sha256.as_str(),
        candidate.visible_interaction_commitment_sha256.as_str(),
        candidate.public_context_commitment_sha256.as_str(),
    ] {
        validate_lower_sha256_v1(digest)?;
    }
    validate_match_score_v1(
        candidate.game_number,
        candidate.acting_player_games_won,
        candidate.opponent_games_won,
    )?;
    validate_visible_facts_v1(&candidate.visible_facts, canonical_bgra8, &client_size_px)?;
    let expected_commitment = competitive_pregame_public_context_commitment_v1(&candidate)?;
    if candidate.public_context_commitment_sha256 != expected_commitment {
        return Err(error_v1(
            "competitive_pregame_public_context_commitment_mismatch",
            "public context changed after commitment",
        ));
    }
    Ok(CheckedUntrustedMtgoCompetitivePregamePublicContextV1 { candidate })
}

pub fn bind_untrusted_competitive_pregame_model_context_v1(
    classification: CheckedUntrustedMtgoCompetitivePregameClassificationV1,
    public_context: CheckedUntrustedMtgoCompetitivePregamePublicContextV1,
) -> Result<CheckedUntrustedMtgoCompetitivePregameModelContextV1, MtgoContractErrorV1> {
    let context = &public_context.candidate;
    if context.source_capture_commitment_sha256 != classification.source_capture_commitment_sha256()
        || context.source_frame_profile_binding_sha256
            != classification.source_frame_profile_binding_sha256()
        || context.pregame_classification_commitment_sha256
            != classification.classification_commitment_sha256()
        || context.visible_interaction_commitment_sha256
            != classification.visible_interaction_commitment_sha256()
        || context.frame_id != classification.frame_id()
        || context.frame_sequence != classification.frame_sequence()
    {
        return Err(error_v1(
            "competitive_pregame_public_context_source_mismatch",
            "public context and pregame classification do not share one exact frame",
        ));
    }
    let binding_commitment_sha256 = commitment_v1(
        COMPETITIVE_PREGAME_PUBLIC_CONTEXT_BINDING_DOMAIN_V1,
        &[
            classification.classification_commitment_sha256().as_bytes(),
            classification
                .visible_interaction_commitment_sha256()
                .as_bytes(),
            context.public_context_commitment_sha256.as_bytes(),
            &[context.game_number],
            &[context.acting_player_games_won],
            &[context.opponent_games_won],
            match context.play_draw {
                MtgoCompetitivePregamePlayDrawV1::OnPlay => b"on_play",
                MtgoCompetitivePregamePlayDrawV1::OnDraw => b"on_draw",
            },
            b"checked_untrusted_same_frame_no_model_or_input_authority",
        ],
    );
    let game_number = context.game_number;
    let play_draw = context.play_draw;
    let acting_player_games_won = context.acting_player_games_won;
    let opponent_games_won = context.opponent_games_won;
    Ok(CheckedUntrustedMtgoCompetitivePregameModelContextV1 {
        _classification: classification,
        _public_context: public_context,
        binding_commitment_sha256,
        game_number,
        play_draw,
        acting_player_games_won,
        opponent_games_won,
    })
}

fn validate_match_score_v1(
    game_number: u8,
    acting_player_games_won: u8,
    opponent_games_won: u8,
) -> Result<(), MtgoContractErrorV1> {
    let valid = match game_number {
        1 => acting_player_games_won == 0 && opponent_games_won == 0,
        2 => {
            acting_player_games_won <= 1
                && opponent_games_won <= 1
                && acting_player_games_won + opponent_games_won == 1
        }
        3 => acting_player_games_won == 1 && opponent_games_won == 1,
        _ => false,
    };
    if !valid {
        return Err(error_v1(
            "competitive_pregame_public_match_score_invalid",
            "game number and public best-of-three score are inconsistent",
        ));
    }
    Ok(())
}

fn validate_visible_facts_v1(
    facts: &[MtgoCompetitivePregamePublicContextFactV1],
    canonical_bgra8: &[u8],
    client_size_px: &MtgoSizePxV1,
) -> Result<(), MtgoContractErrorV1> {
    let expected = [
        MtgoCompetitivePregamePublicContextFactKindV1::PlayDrawIndicator,
        MtgoCompetitivePregamePublicContextFactKindV1::ActingPlayerMatchScore,
        MtgoCompetitivePregamePublicContextFactKindV1::OpponentMatchScore,
    ];
    if facts.len() != expected.len()
        || facts
            .iter()
            .map(|fact| fact.kind)
            .ne(expected.iter().copied())
    {
        return Err(error_v1(
            "competitive_pregame_public_context_fact_set_invalid",
            "public context facts must use the exact canonical order",
        ));
    }
    for (index, fact) in facts.iter().enumerate() {
        validate_lower_sha256_v1(&fact.visible_content_sha256)?;
        if fact.confidence_bps < MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1
            || fact.confidence_bps > 10_000
            || fact.rect_client_px.width < 2
            || fact.rect_client_px.height < 2
        {
            return Err(error_v1(
                "competitive_pregame_public_context_fact_invalid",
                index.to_string(),
            ));
        }
        let observed = visible_frame_region_content_sha256_v1(
            canonical_bgra8,
            client_size_px,
            &fact.rect_client_px,
        )?;
        if observed != fact.visible_content_sha256 {
            return Err(error_v1(
                "competitive_pregame_public_context_fact_pixels_mismatch",
                index.to_string(),
            ));
        }
    }
    for left in 0..facts.len() {
        for right in (left + 1)..facts.len() {
            if rectangles_overlap_v1(&facts[left].rect_client_px, &facts[right].rect_client_px)? {
                return Err(error_v1(
                    "competitive_pregame_public_context_fact_overlap",
                    format!("{left}:{right}"),
                ));
            }
        }
    }
    Ok(())
}

fn rectangles_overlap_v1(
    left: &MtgoRectPxV1,
    right: &MtgoRectPxV1,
) -> Result<bool, MtgoContractErrorV1> {
    let left_right = left.x.checked_add(left.width).ok_or_else(|| {
        error_v1(
            "competitive_pregame_public_context_geometry_invalid",
            "left rectangle x overflow",
        )
    })?;
    let left_bottom = left.y.checked_add(left.height).ok_or_else(|| {
        error_v1(
            "competitive_pregame_public_context_geometry_invalid",
            "left rectangle y overflow",
        )
    })?;
    let right_right = right.x.checked_add(right.width).ok_or_else(|| {
        error_v1(
            "competitive_pregame_public_context_geometry_invalid",
            "right rectangle x overflow",
        )
    })?;
    let right_bottom = right.y.checked_add(right.height).ok_or_else(|| {
        error_v1(
            "competitive_pregame_public_context_geometry_invalid",
            "right rectangle y overflow",
        )
    })?;
    Ok(left.x < right_right
        && right.x < left_right
        && left.y < right_bottom
        && right.y < left_bottom)
}

fn validate_lower_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(error_v1(
            "competitive_pregame_public_context_digest_invalid",
            value,
        ));
    }
    Ok(())
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
mod tests {
    use super::*;
    use crate::checked_untrusted_competitive_pregame_classification_for_context_test_v1;

    fn pixels_v1() -> (MtgoSizePxV1, Vec<u8>) {
        let size = MtgoSizePxV1 {
            width: 12,
            height: 4,
        };
        let pixels = (0..size.width * size.height * 4)
            .map(|index| (index % 251) as u8)
            .collect();
        (size, pixels)
    }

    fn fact_v1(
        kind: MtgoCompetitivePregamePublicContextFactKindV1,
        x: u32,
        pixels: &[u8],
        size: &MtgoSizePxV1,
    ) -> MtgoCompetitivePregamePublicContextFactV1 {
        let rect_client_px = MtgoRectPxV1 {
            x,
            y: 1,
            width: 2,
            height: 2,
        };
        MtgoCompetitivePregamePublicContextFactV1 {
            kind,
            visible_content_sha256: visible_frame_region_content_sha256_v1(
                pixels,
                size,
                &rect_client_px,
            )
            .unwrap(),
            rect_client_px,
            confidence_bps: MIN_GAME_INFORMATION_CONFIDENCE_BPS_V1,
        }
    }

    fn candidate_v1(
        pixels: &[u8],
        size: &MtgoSizePxV1,
    ) -> MtgoCompetitivePregamePublicContextCandidateV1 {
        let mut candidate = MtgoCompetitivePregamePublicContextCandidateV1 {
            schema_version: MTGO_COMPETITIVE_PREGAME_PUBLIC_CONTEXT_SCHEMA_V1,
            source_capture_commitment_sha256: "a".repeat(64),
            source_frame_profile_binding_sha256: "b".repeat(64),
            pregame_classification_commitment_sha256: "c".repeat(64),
            visible_interaction_commitment_sha256: "d".repeat(64),
            frame_id: 9,
            frame_sequence: 11,
            game_number: 2,
            play_draw: MtgoCompetitivePregamePlayDrawV1::OnDraw,
            acting_player_games_won: 1,
            opponent_games_won: 0,
            visible_facts: vec![
                fact_v1(
                    MtgoCompetitivePregamePublicContextFactKindV1::PlayDrawIndicator,
                    0,
                    pixels,
                    size,
                ),
                fact_v1(
                    MtgoCompetitivePregamePublicContextFactKindV1::ActingPlayerMatchScore,
                    4,
                    pixels,
                    size,
                ),
                fact_v1(
                    MtgoCompetitivePregamePublicContextFactKindV1::OpponentMatchScore,
                    8,
                    pixels,
                    size,
                ),
            ],
            public_context_commitment_sha256: String::new(),
        };
        candidate.public_context_commitment_sha256 =
            competitive_pregame_public_context_commitment_v1(&candidate).unwrap();
        candidate
    }

    #[test]
    fn exact_pixels_play_draw_and_best_of_three_score_validate_without_authority() {
        let (size, pixels) = pixels_v1();
        let checked = check_untrusted_competitive_pregame_public_context_v1(
            candidate_v1(&pixels, &size),
            &pixels,
            size,
        )
        .unwrap();
        assert_eq!(checked.game_number(), 2);
        assert_eq!(
            checked.play_draw(),
            MtgoCompetitivePregamePlayDrawV1::OnDraw
        );
        assert_eq!(checked.acting_player_games_won(), 1);
        assert_eq!(checked.opponent_games_won(), 0);
        assert!(!checked.safe_for_model_scoring_v1());
        assert!(!checked.safe_for_input_v1());
    }

    #[test]
    fn score_stage_commitment_pixels_and_regions_fail_closed() {
        let (size, pixels) = pixels_v1();
        let base = candidate_v1(&pixels, &size);

        let mut wrong_score = base.clone();
        wrong_score.game_number = 3;
        wrong_score.public_context_commitment_sha256 =
            competitive_pregame_public_context_commitment_v1(&wrong_score).unwrap();
        assert!(check_untrusted_competitive_pregame_public_context_v1(
            wrong_score,
            &pixels,
            size.clone(),
        )
        .is_err());

        let mut wrong_commitment = base.clone();
        wrong_commitment.play_draw = MtgoCompetitivePregamePlayDrawV1::OnPlay;
        assert!(check_untrusted_competitive_pregame_public_context_v1(
            wrong_commitment,
            &pixels,
            size.clone(),
        )
        .is_err());

        let mut changed_pixels = pixels.clone();
        changed_pixels[4 * size.width as usize] ^= 1;
        assert!(check_untrusted_competitive_pregame_public_context_v1(
            base.clone(),
            &changed_pixels,
            size.clone(),
        )
        .is_err());

        let mut overlapping = base;
        overlapping.visible_facts[1].rect_client_px.x = 1;
        overlapping.visible_facts[1].visible_content_sha256 =
            visible_frame_region_content_sha256_v1(
                &pixels,
                &size,
                &overlapping.visible_facts[1].rect_client_px,
            )
            .unwrap();
        overlapping.public_context_commitment_sha256 =
            competitive_pregame_public_context_commitment_v1(&overlapping).unwrap();
        assert!(
            check_untrusted_competitive_pregame_public_context_v1(overlapping, &pixels, size,)
                .is_err()
        );
    }

    #[test]
    fn exact_classification_frame_binds_and_crossed_frame_is_rejected() {
        let (size, pixels) = pixels_v1();
        let candidate = candidate_v1(&pixels, &size);
        let classification =
            checked_untrusted_competitive_pregame_classification_for_context_test_v1(
                candidate.source_capture_commitment_sha256.clone(),
                candidate.source_frame_profile_binding_sha256.clone(),
                candidate.pregame_classification_commitment_sha256.clone(),
                candidate.visible_interaction_commitment_sha256.clone(),
                candidate.frame_id,
                candidate.frame_sequence,
            );
        let checked = check_untrusted_competitive_pregame_public_context_v1(
            candidate.clone(),
            &pixels,
            size.clone(),
        )
        .unwrap();
        let bound =
            bind_untrusted_competitive_pregame_model_context_v1(classification, checked).unwrap();
        assert_eq!(bound.game_number(), 2);
        assert_eq!(bound.match_score(), (1, 0));
        assert_eq!(bound.play_draw(), MtgoCompetitivePregamePlayDrawV1::OnDraw);
        assert!(!bound.safe_for_model_scoring_v1());
        assert!(!bound.safe_for_input_v1());

        let crossed = checked_untrusted_competitive_pregame_classification_for_context_test_v1(
            candidate.source_capture_commitment_sha256.clone(),
            candidate.source_frame_profile_binding_sha256.clone(),
            candidate.pregame_classification_commitment_sha256.clone(),
            candidate.visible_interaction_commitment_sha256.clone(),
            candidate.frame_id,
            candidate.frame_sequence + 1,
        );
        let checked =
            check_untrusted_competitive_pregame_public_context_v1(candidate, &pixels, size)
                .unwrap();
        assert!(bind_untrusted_competitive_pregame_model_context_v1(crossed, checked).is_err());
    }
}
