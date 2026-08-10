use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(target_os = "windows")]
mod actuator;

#[cfg(target_os = "windows")]
mod probe;

#[cfg(target_os = "windows")]
pub use actuator::{
    confirm_pending_pregame_keep_to_bottom_six_v3, confirm_pending_pregame_keep_to_first_main_v3,
    confirm_pending_pregame_mulligan_v3, execute_authorized_private_match_pregame_action_v3,
    pregame_input_gate_status_v3, ratify_private_match_authorization_v3,
    MtgoPregameInputGateStatusV3, OpaqueMtgoPendingPregameInputV3,
    RatifiedMtgoPrivateMatchAuthorizationV3,
};

#[cfg(target_os = "windows")]
pub use probe::{
    build_card_aware_bottoming_action_plan_v5, build_card_aware_bottoming_scoring_request_v5,
    build_card_aware_pregame_action_plan_v4, build_card_aware_pregame_scoring_request_v4,
    build_pinned_current_solitaire_pregame_action_plan_v1, build_pregame_action_plan_v3,
    build_pregame_scoring_request_v3, capture_mtgo_dxgi_frame_candidate_v3,
    capture_pinned_current_solitaire_visible_frame_v1,
    card_aware_bottoming_scoring_request_commitment_v5,
    card_aware_pregame_scoring_request_commitment_v4, confirm_card_aware_bottom_selection_v5,
    confirm_card_aware_bottoming_cancel_plan_v5, confirm_card_aware_bottoming_selection_plan_v5,
    confirm_card_aware_bottoming_submit_plan_v5, confirm_pregame_keep_to_bottom_six_transition_v3,
    confirm_pregame_keep_to_first_main_transition_v3, confirm_pregame_mulligan_transition_v3,
    measure_mtgo_dxgi_bottom_six_initial_candidate_v3,
    measure_mtgo_dxgi_bottom_six_reflow_candidate_v3,
    measure_mtgo_dxgi_bottom_six_state_candidate_v3,
    measure_mtgo_dxgi_bottom_six_visible_card_identities_candidate_v3,
    measure_mtgo_dxgi_first_main_candidate_v3, measure_mtgo_dxgi_mulligan_ladder_candidate_v3,
    measure_mtgo_dxgi_mulligan_visible_hand_candidate_v3,
    measure_pinned_current_solitaire_first_main_v1,
    measure_pinned_current_solitaire_mulligan_ladder_v1,
    measure_pinned_current_solitaire_mulligan_visible_hand_v1,
    non_model_pregame_heuristic_profile_commitment_v1, pregame_scoring_request_commitment_v3,
    run_cli_v3, score_and_select_card_aware_bottoming_model_v5,
    score_and_select_card_aware_pregame_model_v4,
    score_and_select_pinned_current_solitaire_pregame_v1, score_and_select_pregame_model_v3,
    start_card_aware_bottoming_session_v5, validate_card_aware_bottoming_score_response_v5,
    validate_card_aware_pregame_score_response_v4, validate_pregame_score_response_v3,
    MtgoBottomingCardIdentitySourceV5, MtgoBottomingConfirmedCardV5, MtgoBottomingVisibleCardV5,
    MtgoCardAwareBottomingScoreResponseV5, MtgoCardAwareBottomingScoringRequestV5,
    MtgoCardAwarePregameScoreResponseV4, MtgoCardAwarePregameScoringRequestV4,
    MtgoDxgiCaptureRequestV3, MtgoDxgiFrameCommitmentsV3, MtgoExternalCardAwareBottomingScorerV5,
    MtgoExternalCardAwarePregameScorerV4, MtgoExternalPregameScorerV3, MtgoHeuristicCardFeatureV1,
    MtgoHeuristicCardKindV1, MtgoNonModelPregameHeuristicProfileV1, MtgoNonModelPregameHeuristicV1,
    MtgoPinnedSolitaireVisibleFrameCommitmentsV1, MtgoPlannedBottomingPostconditionV5,
    MtgoPlannedPregamePostconditionV3, MtgoPregameScoreResponseV3, MtgoPregameScoringRequestV3,
    OpaqueMtgoBottomingActionPlanV5, OpaqueMtgoCardAwareBottomingModelSelectionV5,
    OpaqueMtgoCardAwareBottomingSessionV5, OpaqueMtgoCardAwarePregameModelSelectionV4,
    OpaqueMtgoConfirmedBottomingSubmitV5, OpaqueMtgoConfirmedKeepToBottomSixTransitionV3,
    OpaqueMtgoConfirmedKeepToFirstMainTransitionV3, OpaqueMtgoConfirmedMulliganTransitionV3,
    OpaqueMtgoDxgiBottomSixInitialMeasurementV3, OpaqueMtgoDxgiBottomSixReflowMeasurementV3,
    OpaqueMtgoDxgiBottomSixStateMeasurementV3,
    OpaqueMtgoDxgiBottomSixVisibleCardIdentityMeasurementV3, OpaqueMtgoDxgiFirstMainMeasurementV3,
    OpaqueMtgoDxgiFrameCandidateV3, OpaqueMtgoDxgiMulliganMeasurementV3,
    OpaqueMtgoDxgiMulliganVisibleHandMeasurementV3,
    OpaqueMtgoPinnedSolitaireFirstMainMeasurementV1, OpaqueMtgoPinnedSolitaireMulliganMeasurementV1,
    OpaqueMtgoPinnedSolitaireMulliganVisibleHandV1, OpaqueMtgoPinnedSolitairePregameActionPlanV1,
    OpaqueMtgoPinnedSolitairePregameSelectionV1, OpaqueMtgoPinnedSolitaireVisibleFrameV1,
    OpaqueMtgoPregameActionPlanV3, OpaqueMtgoPregameModelSelectionV3,
    MTGO_BOTTOMING_CARD_AWARE_SCORING_SCHEMA_V5, MTGO_HEURISTIC_COLORLESS_V1,
    MTGO_HEURISTIC_COLOR_BLACK_V1, MTGO_HEURISTIC_COLOR_BLUE_V1, MTGO_HEURISTIC_COLOR_GREEN_V1,
    MTGO_HEURISTIC_COLOR_RED_V1, MTGO_HEURISTIC_COLOR_WHITE_V1,
    MTGO_NON_MODEL_PREGAME_HEURISTIC_SCHEMA_V1, MTGO_PREGAME_CARD_AWARE_SCORING_SCHEMA_V4,
    MTGO_PREGAME_EXTERNAL_SCORING_SCHEMA_V3,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureWindowModeV2 {
    MainClient,
    SolitaireGame,
    SpectatorGame,
}

impl CaptureWindowModeV2 {
    pub fn manifest_name(self) -> &'static str {
        match self {
            Self::MainClient => "main_client",
            Self::SolitaireGame => "solitaire_game",
            Self::SpectatorGame => "spectator_game",
        }
    }

    pub fn capture_role(self) -> &'static str {
        match self {
            Self::MainClient => "navigation",
            Self::SolitaireGame => "acting_player_solitaire",
            Self::SpectatorGame => "spectator",
        }
    }
}

pub fn validate_visible_mtgo_title_v2(
    mode: CaptureWindowModeV2,
    expected_game_format: Option<&str>,
    title: &str,
) -> Result<(), &'static str> {
    if title.is_empty() || title.len() > 1_024 || title.chars().any(char::is_control) {
        return Err("window title is empty, too long, or contains control characters");
    }
    match mode {
        CaptureWindowModeV2::MainClient => {
            if expected_game_format.is_some() {
                return Err("main-client mode cannot declare a game format");
            }
            if !title.contains("Magic: The Gathering Online") {
                return Err("main-client title does not identify Magic: The Gathering Online");
            }
        }
        CaptureWindowModeV2::SolitaireGame => {
            let format = validate_game_format_v2(expected_game_format)?;
            let prefix = format!("(Solitaire): {format}: Vs. ");
            let participant = title
                .strip_prefix(&prefix)
                .ok_or("Solitaire title does not match the exact format prefix")?;
            validate_participant_text_v2(participant, false)?;
        }
        CaptureWindowModeV2::SpectatorGame => {
            let format = validate_game_format_v2(expected_game_format)?;
            let prefix = format!("(1-on-1): {format}: Vs. ");
            let participants = title
                .strip_prefix(&prefix)
                .ok_or("spectator title does not match the exact format prefix")?;
            validate_participant_text_v2(participants, true)?;
        }
    }
    Ok(())
}

fn validate_game_format_v2(value: Option<&str>) -> Result<&str, &'static str> {
    let value = value.ok_or("game mode requires an expected format")?;
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'-'))
    {
        return Err("expected game format is not a safe visible label");
    }
    Ok(value)
}

fn validate_participant_text_v2(value: &str, require_comma: bool) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > 512 {
        return Err("participant title text is empty or too long");
    }
    let participant_text = if let Some((participants, identity)) = value.split_once(" Match #") {
        let (match_id, game_id) = identity
            .split_once(" - Game #")
            .ok_or("visible match title suffix is malformed")?;
        if match_id.is_empty()
            || game_id.is_empty()
            || !match_id.bytes().all(|byte| byte.is_ascii_digit())
            || !game_id.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("visible match and game IDs must be decimal integers");
        }
        participants
    } else {
        if value.contains('#') {
            return Err("visible match title suffix is malformed");
        }
        value
    };
    if participant_text.trim() != participant_text || participant_text.is_empty() {
        return Err("participant title text has invalid surrounding whitespace");
    }
    if require_comma {
        let (left, right) = participant_text
            .split_once(',')
            .ok_or("spectator title must visibly identify two participants")?;
        if left.trim().is_empty() || right.trim().is_empty() || right.contains(',') {
            return Err("spectator title must contain exactly two visible participants");
        }
    } else if participant_text.contains(',') {
        return Err("Solitaire title must identify one visible participant");
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRectV1 {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl SignedRectV1 {
    pub fn width(self) -> Result<u32, &'static str> {
        u32::try_from(
            self.right
                .checked_sub(self.left)
                .ok_or("rectangle width overflow")?,
        )
        .map_err(|_| "rectangle width is not positive")
        .and_then(|value| {
            if value == 0 {
                Err("rectangle width is zero")
            } else {
                Ok(value)
            }
        })
    }

    pub fn height(self) -> Result<u32, &'static str> {
        u32::try_from(
            self.bottom
                .checked_sub(self.top)
                .ok_or("rectangle height overflow")?,
        )
        .map_err(|_| "rectangle height is not positive")
        .and_then(|value| {
            if value == 0 {
                Err("rectangle height is zero")
            } else {
                Ok(value)
            }
        })
    }

    pub fn contains(self, other: Self) -> bool {
        self.width().is_ok()
            && self.height().is_ok()
            && other.width().is_ok()
            && other.height().is_ok()
            && other.left >= self.left
            && other.top >= self.top
            && other.right <= self.right
            && other.bottom <= self.bottom
    }

    pub fn intersects(self, other: Self) -> bool {
        self.width().is_ok()
            && self.height().is_ok()
            && other.width().is_ok()
            && other.height().is_ok()
            && self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }

    pub fn contains_point(self, x: i32, y: i32) -> bool {
        self.width().is_ok()
            && self.height().is_ok()
            && x >= self.left
            && x < self.right
            && y >= self.top
            && y < self.bottom
    }

    pub fn crop_box_within(self, output: Self) -> Result<CropBoxV1, &'static str> {
        if !output.contains(self) {
            return Err("crop is not wholly contained in the output");
        }
        Ok(CropBoxV1 {
            left: u32::try_from(self.left - output.left).map_err(|_| "crop left is negative")?,
            top: u32::try_from(self.top - output.top).map_err(|_| "crop top is negative")?,
            width: self.width()?,
            height: self.height()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropBoxV1 {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

pub fn copy_tightly_packed_bgra8_v1(
    mapped: &[u8],
    row_pitch: usize,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, &'static str> {
    let tight_row = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or("tight row size overflow")?;
    let height = usize::try_from(height).map_err(|_| "height does not fit usize")?;
    if tight_row == 0 || height == 0 || row_pitch < tight_row {
        return Err("mapped texture geometry is invalid");
    }
    let required = row_pitch
        .checked_mul(height)
        .ok_or("mapped byte length overflow")?;
    if mapped.len() < required {
        return Err("mapped texture is shorter than its declared rows");
    }
    let output_len = tight_row
        .checked_mul(height)
        .ok_or("canonical byte length overflow")?;
    let mut output = Vec::with_capacity(output_len);
    for row in 0..height {
        let start = row * row_pitch;
        output.extend_from_slice(&mapped[start..start + tight_row]);
    }
    Ok(output)
}

pub fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_origin_crop_is_exact() {
        let output = SignedRectV1 {
            left: -1_920,
            top: -200,
            right: 0,
            bottom: 880,
        };
        let client = SignedRectV1 {
            left: -1_800,
            top: -100,
            right: -600,
            bottom: 700,
        };
        assert_eq!(
            client.crop_box_within(output).unwrap(),
            CropBoxV1 {
                left: 120,
                top: 100,
                width: 1_200,
                height: 800,
            }
        );
    }

    #[test]
    fn offscreen_edge_and_invalid_rectangles_fail() {
        let output = SignedRectV1 {
            left: 0,
            top: 0,
            right: 1_920,
            bottom: 1_080,
        };
        assert!(SignedRectV1 {
            left: -1,
            top: 0,
            right: 100,
            bottom: 100,
        }
        .crop_box_within(output)
        .is_err());
        assert!(SignedRectV1 {
            left: 10,
            top: 10,
            right: 10,
            bottom: 20,
        }
        .crop_box_within(output)
        .is_err());
    }

    #[test]
    fn touching_edges_do_not_intersect() {
        let left = SignedRectV1 {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let right = SignedRectV1 {
            left: 100,
            top: 0,
            right: 200,
            bottom: 100,
        };
        assert!(!left.intersects(right));
    }

    #[test]
    fn canonical_copy_ignores_staging_padding() {
        let mapped = [
            1, 2, 3, 4, 5, 6, 7, 8, 90, 91, 92, 93, 9, 10, 11, 12, 13, 14, 15, 16, 94, 95, 96, 97,
        ];
        assert_eq!(
            copy_tightly_packed_bgra8_v1(&mapped, 12, 2, 2).unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn main_client_title_has_no_game_role() {
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::MainClient,
            None,
            "Magic: The Gathering Online"
        )
        .is_ok());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::MainClient,
            Some("Freeform"),
            "Magic: The Gathering Online"
        )
        .is_err());
    }

    #[test]
    fn solitaire_title_requires_exact_format_and_one_participant() {
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::SolitaireGame,
            Some("Freeform"),
            "(Solitaire): Freeform: Vs. local-player Match #123 - Game #456"
        )
        .is_ok());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::SolitaireGame,
            Some("Standard"),
            "(Solitaire): Freeform: Vs. local-player"
        )
        .is_err());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::SolitaireGame,
            Some("Freeform"),
            "(Solitaire): Freeform: Vs. one, two"
        )
        .is_err());
    }

    #[test]
    fn spectator_title_requires_exact_format_and_two_participants() {
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::SpectatorGame,
            Some("Standard"),
            "(1-on-1): Standard: Vs. player-one, player-two"
        )
        .is_ok());
        assert!(validate_visible_mtgo_title_v2(
            CaptureWindowModeV2::SpectatorGame,
            Some("Standard"),
            "(1-on-1): Standard: Vs. player-one"
        )
        .is_err());
    }
}
