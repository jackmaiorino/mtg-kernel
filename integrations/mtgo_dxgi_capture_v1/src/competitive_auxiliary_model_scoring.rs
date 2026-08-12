use crate::{
    competitive_native_pregame_model_input_commitment_v1,
    competitive_native_sideboard_model_input_commitment_v1,
    competitive_native_sideboard_model_selection_commitment_v1,
    validate_competitive_native_pregame_model_input_v1,
    validate_competitive_native_sideboard_model_input_v1,
    validate_competitive_native_sideboard_model_selection_v1, MtgoCompetitiveNativePregameActionV1,
    MtgoCompetitiveNativePregameModelInputV1, MtgoCompetitiveNativeSideboardModelInputV1,
    MtgoCompetitiveNativeSideboardModelSelectionV1, OpaqueMtgoCompetitiveNativePregameRequestV1,
    OpaqueMtgoCompetitiveNativeSideboardRequestV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const NATIVE_PREGAME_CHECKED_SELECTION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-native-pregame-checked-untrusted-selection-v1";
const NATIVE_SIDEBOARD_CHECKED_SELECTION_DOMAIN_V1: &[u8] =
    b"mtgo-competitive-native-sideboard-checked-untrusted-selection-v1";

pub const MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1: u32 = 1;

/// Coordinate-free response for one exact player-visible pregame input.
///
/// This is a transport contract, not proof that the response came from a
/// loaded MTG-kernel checkpoint. The checked wrapper produced below therefore
/// cannot recover an event session or reach an input path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativePregameScoreResponseV1 {
    pub schema_version: u32,
    pub model_input_commitment_sha256: String,
    pub deployment_commitment_sha256: String,
    pub ordered_action_logits_f32_bits: Vec<u32>,
    pub value_f32_bits: u32,
}

/// Minimal semantic scorer seam for a future kernel pregame head or an
/// offline test double. Implementing this trait supplies no live authority.
pub trait MtgoCompetitiveNativePregameScorerV1 {
    fn score_pregame_v1(
        &mut self,
        model_input: &MtgoCompetitiveNativePregameModelInputV1,
        model_input_commitment_sha256: &str,
        deployment_commitment_sha256: &str,
    ) -> Result<MtgoCompetitiveNativePregameScoreResponseV1, String>;
}

/// Deterministic checked selection from a structurally valid pregame scorer
/// response. It contains game information and commitments only.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1) {
///     let _ = value.input_command();
///     let _ = value.event_session();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1 {
    response: MtgoCompetitiveNativePregameScoreResponseV1,
    selected_index: usize,
    selected_action: MtgoCompetitiveNativePregameActionV1,
    selection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1 {
    pub fn selected_index_v1(&self) -> usize {
        self.selected_index
    }

    pub fn selected_action_v1(&self) -> &MtgoCompetitiveNativePregameActionV1 {
        &self.selected_action
    }

    pub fn selected_logit_f32_bits_v1(&self) -> u32 {
        self.response.ordered_action_logits_f32_bits[self.selected_index]
    }

    pub fn value_f32_bits_v1(&self) -> u32 {
        self.response.value_f32_bits
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        &self.response.model_input_commitment_sha256
    }

    pub fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.response.deployment_commitment_sha256
    }

    pub fn selection_commitment_sha256_v1(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Scores and deterministically selects one legal action from the exact
/// player-visible pregame payload. NaNs, infinities, response substitution,
/// action-count drift, and deployment substitution fail closed. Equal logits
/// select the earliest action in the adapter's canonical order.
pub fn score_checked_untrusted_competitive_native_pregame_v1<
    S: MtgoCompetitiveNativePregameScorerV1,
>(
    model_input: &MtgoCompetitiveNativePregameModelInputV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1, String> {
    validate_competitive_native_pregame_model_input_v1(model_input)?;
    require_sha256_v1(
        deployment_commitment_sha256,
        "native pregame deployment commitment",
    )?;
    let model_input_commitment_sha256 =
        competitive_native_pregame_model_input_commitment_v1(model_input)?;
    let response = scorer.score_pregame_v1(
        model_input,
        &model_input_commitment_sha256,
        deployment_commitment_sha256,
    )?;
    if response.schema_version != MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1
        || response.model_input_commitment_sha256 != model_input_commitment_sha256
        || response.deployment_commitment_sha256 != deployment_commitment_sha256
        || response.ordered_action_logits_f32_bits.len() != model_input.ordered_actions.len()
        || response.ordered_action_logits_f32_bits.is_empty()
    {
        return Err(
            "native pregame score response changed the exact input, deployment, or action set"
                .to_owned(),
        );
    }
    let logits = response
        .ordered_action_logits_f32_bits
        .iter()
        .copied()
        .map(f32::from_bits)
        .collect::<Vec<_>>();
    if logits.iter().any(|value| !value.is_finite())
        || !f32::from_bits(response.value_f32_bits).is_finite()
    {
        return Err("native pregame score response contains a non-finite value".to_owned());
    }
    let mut selected_index = 0_usize;
    for index in 1..logits.len() {
        if logits[index].total_cmp(&logits[selected_index]).is_gt() {
            selected_index = index;
        }
    }
    let selected_action = model_input.ordered_actions[selected_index].clone();
    let response_json = serde_json::to_vec(&response)
        .map_err(|error| format!("serialize native pregame score response: {error}"))?;
    let selected_action_json = serde_json::to_vec(&selected_action)
        .map_err(|error| format!("serialize native pregame selected action: {error}"))?;
    let selection_commitment_sha256 = commitment_v1(
        NATIVE_PREGAME_CHECKED_SELECTION_DOMAIN_V1,
        &[
            model_input_commitment_sha256.as_bytes(),
            deployment_commitment_sha256.as_bytes(),
            &response_json,
            &(selected_index as u64).to_be_bytes(),
            &selected_action_json,
            b"checked_untrusted_semantic_selection_no_event_session_no_input",
        ],
    );
    Ok(
        CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1 {
            response,
            selected_index,
            selected_action,
            selection_commitment_sha256,
        },
    )
}

/// Move-only offline scoring result that retains the exact source-bound
/// pregame request. Neither the retained paid-event session nor its visible
/// control target can be recovered through this API.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoScoredCompetitiveNativePregameRequestV1;
/// fn cannot_resume(value: OpaqueMtgoScoredCompetitiveNativePregameRequestV1) {
///     let _ = value.into_event_session();
///     let _ = value.input_command();
/// }
/// ```
pub struct OpaqueMtgoScoredCompetitiveNativePregameRequestV1 {
    _request: OpaqueMtgoCompetitiveNativePregameRequestV1,
    selection: CheckedUntrustedMtgoCompetitiveNativePregameModelSelectionV1,
}

impl OpaqueMtgoScoredCompetitiveNativePregameRequestV1 {
    pub fn selected_index_v1(&self) -> usize {
        self.selection.selected_index_v1()
    }

    pub fn selected_action_v1(&self) -> &MtgoCompetitiveNativePregameActionV1 {
        self.selection.selected_action_v1()
    }

    pub fn selection_commitment_sha256_v1(&self) -> &str {
        self.selection.selection_commitment_sha256_v1()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }
}

/// Consumes the actual move-only adapter request through the offline scorer
/// seam. This proves request ownership survives scoring, but deliberately
/// withholds the session pending a concrete kernel-owned response type.
pub fn score_checked_untrusted_competitive_native_pregame_request_v1<
    S: MtgoCompetitiveNativePregameScorerV1,
>(
    request: OpaqueMtgoCompetitiveNativePregameRequestV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<OpaqueMtgoScoredCompetitiveNativePregameRequestV1, String> {
    let selection = score_checked_untrusted_competitive_native_pregame_v1(
        request.model_input_v1(),
        deployment_commitment_sha256,
        scorer,
    )?;
    if selection.model_input_commitment_sha256_v1() != request.model_input_commitment_sha256_v1() {
        return Err("native pregame scoring lost the exact opaque request".to_owned());
    }
    Ok(OpaqueMtgoScoredCompetitiveNativePregameRequestV1 {
        _request: request,
        selection,
    })
}

/// Coordinate-free target response for one exact player-visible sideboard
/// input. This response is structurally checkable but does not prove kernel
/// checkpoint origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoCompetitiveNativeSideboardScoreResponseV1 {
    pub schema_version: u32,
    pub model_input_commitment_sha256: String,
    pub deployment_commitment_sha256: String,
    pub selection: MtgoCompetitiveNativeSideboardModelSelectionV1,
    pub value_f32_bits: u32,
}

/// Minimal semantic scorer seam for a future kernel sideboard head or an
/// offline test double. Implementing this trait supplies no live authority.
pub trait MtgoCompetitiveNativeSideboardScorerV1 {
    fn score_sideboard_v1(
        &mut self,
        model_input: &MtgoCompetitiveNativeSideboardModelInputV1,
        model_input_commitment_sha256: &str,
        deployment_commitment_sha256: &str,
    ) -> Result<MtgoCompetitiveNativeSideboardScoreResponseV1, String>;
}

/// Checked inventory-conserving sideboard target. The wrapper retains no
/// event session, classified frame, card database IDs, rectangles, or input
/// capability.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1;
/// fn cannot_act(value: &CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1) {
///     let _ = value.submit_sideboard();
///     let _ = value.event_session();
/// }
/// ```
pub struct CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1 {
    response: MtgoCompetitiveNativeSideboardScoreResponseV1,
    model_selection_commitment_sha256: String,
    checked_selection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1 {
    pub fn selection_v1(&self) -> &MtgoCompetitiveNativeSideboardModelSelectionV1 {
        &self.response.selection
    }

    pub fn value_f32_bits_v1(&self) -> u32 {
        self.response.value_f32_bits
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        &self.response.model_input_commitment_sha256
    }

    pub fn deployment_commitment_sha256_v1(&self) -> &str {
        &self.response.deployment_commitment_sha256
    }

    pub fn model_selection_commitment_sha256_v1(&self) -> &str {
        &self.model_selection_commitment_sha256
    }

    pub fn checked_selection_commitment_sha256_v1(&self) -> &str {
        &self.checked_selection_commitment_sha256
    }

    pub fn no_changes_selected_v1(
        &self,
        model_input: &MtgoCompetitiveNativeSideboardModelInputV1,
    ) -> bool {
        self.response.selection.target_configuration == model_input.current_configuration
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Checks one scorer-selected target against the exact player-visible current
/// deck. Both changed and unchanged targets are explicit valid responses, but
/// neither can recover the retained event request or reach submission.
pub fn score_checked_untrusted_competitive_native_sideboard_v1<
    S: MtgoCompetitiveNativeSideboardScorerV1,
>(
    model_input: &MtgoCompetitiveNativeSideboardModelInputV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1, String> {
    validate_competitive_native_sideboard_model_input_v1(model_input)?;
    require_sha256_v1(
        deployment_commitment_sha256,
        "native sideboard deployment commitment",
    )?;
    let model_input_commitment_sha256 =
        competitive_native_sideboard_model_input_commitment_v1(model_input)?;
    let response = scorer.score_sideboard_v1(
        model_input,
        &model_input_commitment_sha256,
        deployment_commitment_sha256,
    )?;
    if response.schema_version != MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1
        || response.model_input_commitment_sha256 != model_input_commitment_sha256
        || response.deployment_commitment_sha256 != deployment_commitment_sha256
    {
        return Err(
            "native sideboard score response changed the exact input or deployment".to_owned(),
        );
    }
    if !f32::from_bits(response.value_f32_bits).is_finite() {
        return Err("native sideboard score response contains a non-finite value".to_owned());
    }
    validate_competitive_native_sideboard_model_selection_v1(model_input, &response.selection)?;
    let model_selection_commitment_sha256 =
        competitive_native_sideboard_model_selection_commitment_v1(
            model_input,
            &response.selection,
        )?;
    let response_json = serde_json::to_vec(&response)
        .map_err(|error| format!("serialize native sideboard score response: {error}"))?;
    let checked_selection_commitment_sha256 = commitment_v1(
        NATIVE_SIDEBOARD_CHECKED_SELECTION_DOMAIN_V1,
        &[
            model_input_commitment_sha256.as_bytes(),
            deployment_commitment_sha256.as_bytes(),
            model_selection_commitment_sha256.as_bytes(),
            &response_json,
            b"checked_untrusted_semantic_target_no_event_session_no_input_no_submit",
        ],
    );
    Ok(
        CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1 {
            response,
            model_selection_commitment_sha256,
            checked_selection_commitment_sha256,
        },
    )
}

/// Move-only offline scoring result that retains the exact source-bound
/// sideboard request. It exposes only the checked target and commitments.
///
/// ```compile_fail
/// use mtgo_dxgi_capture_v1::OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1;
/// fn cannot_resume(value: OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1) {
///     let _ = value.into_event_session();
///     let _ = value.submit_sideboard();
/// }
/// ```
pub struct OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1 {
    _request: OpaqueMtgoCompetitiveNativeSideboardRequestV1,
    selection: CheckedUntrustedMtgoCompetitiveNativeSideboardModelSelectionV1,
}

impl OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1 {
    pub fn selection_v1(&self) -> &MtgoCompetitiveNativeSideboardModelSelectionV1 {
        self.selection.selection_v1()
    }

    pub fn checked_selection_commitment_sha256_v1(&self) -> &str {
        self.selection.checked_selection_commitment_sha256_v1()
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_session_recovery_v1(&self) -> bool {
        false
    }

    pub fn permits_sideboard_submission_v1(&self) -> bool {
        false
    }
}

/// Consumes the actual move-only adapter request through the offline scorer
/// seam. The retained event session stays inaccessible until a future native
/// checkpoint response type is implemented in MTG-kernel.
pub fn score_checked_untrusted_competitive_native_sideboard_request_v1<
    S: MtgoCompetitiveNativeSideboardScorerV1,
>(
    request: OpaqueMtgoCompetitiveNativeSideboardRequestV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1, String> {
    let selection = score_checked_untrusted_competitive_native_sideboard_v1(
        request.model_input_v1(),
        deployment_commitment_sha256,
        scorer,
    )?;
    if selection.model_input_commitment_sha256_v1() != request.model_input_commitment_sha256_v1() {
        return Err("native sideboard scoring lost the exact opaque request".to_owned());
    }
    Ok(OpaqueMtgoScoredCompetitiveNativeSideboardRequestV1 {
        _request: request,
        selection,
    })
}

fn require_sha256_v1(value: &str, label: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{label} is not a lowercase SHA-256"));
    }
    Ok(())
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
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
    use crate::{
        MtgoCompetitiveNativePregameCardV1, MtgoCompetitiveNativeSideboardCardCountV1,
        MtgoCompetitiveNativeSideboardConfigurationV1, MtgoCompetitivePregameStageV1,
    };
    use mtgo_blackbox_v1::MtgoCompetitivePregamePlayDrawV1;

    fn deck_v1() -> MtgoCompetitiveNativeSideboardConfigurationV1 {
        MtgoCompetitiveNativeSideboardConfigurationV1 {
            mainboard: vec![
                MtgoCompetitiveNativeSideboardCardCountV1 {
                    visible_card_name: "Lightning Bolt".to_owned(),
                    count: 4,
                },
                MtgoCompetitiveNativeSideboardCardCountV1 {
                    visible_card_name: "Mountain".to_owned(),
                    count: 56,
                },
            ],
            sideboard: vec![MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: "Lightning Bolt".to_owned(),
                count: 15,
            }],
        }
    }

    fn pregame_input_v1() -> MtgoCompetitiveNativePregameModelInputV1 {
        MtgoCompetitiveNativePregameModelInputV1 {
            game_number: 1,
            play_draw: MtgoCompetitivePregamePlayDrawV1::OnPlay,
            acting_player_games_won: 0,
            opponent_games_won: 0,
            player_known_deck_configuration: deck_v1(),
            stage: MtgoCompetitivePregameStageV1::MulliganChoice {
                prospective_keep_size: 7,
            },
            prospective_keep_size: Some(7),
            required_bottom_count: 0,
            selected_bottom_count: 0,
            ordered_visible_cards: (0_u8..7)
                .map(|card_slot| MtgoCompetitiveNativePregameCardV1 {
                    card_slot,
                    visible_card_name: format!("Visible Card {card_slot}"),
                    selected_for_bottom: false,
                })
                .collect(),
            ordered_confirmed_bottom_slots: Vec::new(),
            ordered_actions: vec![
                MtgoCompetitiveNativePregameActionV1::KeepOpeningHand,
                MtgoCompetitiveNativePregameActionV1::Mulligan { next_hand_size: 6 },
            ],
        }
    }

    fn sideboard_input_v1() -> MtgoCompetitiveNativeSideboardModelInputV1 {
        MtgoCompetitiveNativeSideboardModelInputV1 {
            next_game_number: 2,
            acting_player_games_won: 1,
            opponent_games_won: 0,
            current_configuration: deck_v1(),
        }
    }

    struct PregameScorerV1 {
        logits: Vec<f32>,
        corrupt_input: bool,
        corrupt_deployment: bool,
    }

    impl MtgoCompetitiveNativePregameScorerV1 for PregameScorerV1 {
        fn score_pregame_v1(
            &mut self,
            _model_input: &MtgoCompetitiveNativePregameModelInputV1,
            model_input_commitment_sha256: &str,
            deployment_commitment_sha256: &str,
        ) -> Result<MtgoCompetitiveNativePregameScoreResponseV1, String> {
            Ok(MtgoCompetitiveNativePregameScoreResponseV1 {
                schema_version: MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
                model_input_commitment_sha256: if self.corrupt_input {
                    "a".repeat(64)
                } else {
                    model_input_commitment_sha256.to_owned()
                },
                deployment_commitment_sha256: if self.corrupt_deployment {
                    "b".repeat(64)
                } else {
                    deployment_commitment_sha256.to_owned()
                },
                ordered_action_logits_f32_bits: self
                    .logits
                    .iter()
                    .copied()
                    .map(f32::to_bits)
                    .collect(),
                value_f32_bits: 0.25_f32.to_bits(),
            })
        }
    }

    struct SideboardScorerV1 {
        selection: MtgoCompetitiveNativeSideboardModelSelectionV1,
        corrupt_input: bool,
    }

    impl MtgoCompetitiveNativeSideboardScorerV1 for SideboardScorerV1 {
        fn score_sideboard_v1(
            &mut self,
            _model_input: &MtgoCompetitiveNativeSideboardModelInputV1,
            model_input_commitment_sha256: &str,
            deployment_commitment_sha256: &str,
        ) -> Result<MtgoCompetitiveNativeSideboardScoreResponseV1, String> {
            Ok(MtgoCompetitiveNativeSideboardScoreResponseV1 {
                schema_version: MTGO_COMPETITIVE_AUXILIARY_MODEL_SCORING_SCHEMA_V1,
                model_input_commitment_sha256: if self.corrupt_input {
                    "a".repeat(64)
                } else {
                    model_input_commitment_sha256.to_owned()
                },
                deployment_commitment_sha256: deployment_commitment_sha256.to_owned(),
                selection: self.selection.clone(),
                value_f32_bits: 0.5_f32.to_bits(),
            })
        }
    }

    #[test]
    fn pregame_score_selects_canonical_argmax_without_authority() {
        let input = pregame_input_v1();
        let mut scorer = PregameScorerV1 {
            logits: vec![0.0, 2.0],
            corrupt_input: false,
            corrupt_deployment: false,
        };
        let selected = score_checked_untrusted_competitive_native_pregame_v1(
            &input,
            &"d".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert_eq!(selected.selected_index_v1(), 1);
        assert_eq!(
            selected.selected_action_v1(),
            &MtgoCompetitiveNativePregameActionV1::Mulligan { next_hand_size: 6 }
        );
        assert_eq!(f32::from_bits(selected.selected_logit_f32_bits_v1()), 2.0);
        assert_eq!(selected.selection_commitment_sha256_v1().len(), 64);
        assert!(!selected.safe_for_live_input_v1());
        assert!(!selected.permits_event_session_recovery_v1());
    }

    #[test]
    fn pregame_score_rejects_substitution_shape_and_nonfinite_values() {
        let input = pregame_input_v1();
        for mut scorer in [
            PregameScorerV1 {
                logits: vec![0.0, 1.0],
                corrupt_input: true,
                corrupt_deployment: false,
            },
            PregameScorerV1 {
                logits: vec![0.0, 1.0],
                corrupt_input: false,
                corrupt_deployment: true,
            },
            PregameScorerV1 {
                logits: vec![0.0],
                corrupt_input: false,
                corrupt_deployment: false,
            },
            PregameScorerV1 {
                logits: vec![0.0, f32::NAN],
                corrupt_input: false,
                corrupt_deployment: false,
            },
        ] {
            assert!(score_checked_untrusted_competitive_native_pregame_v1(
                &input,
                &"d".repeat(64),
                &mut scorer,
            )
            .is_err());
        }
    }

    #[test]
    fn pregame_ties_select_first_canonical_action() {
        let input = pregame_input_v1();
        let mut scorer = PregameScorerV1 {
            logits: vec![1.0, 1.0],
            corrupt_input: false,
            corrupt_deployment: false,
        };
        let selected = score_checked_untrusted_competitive_native_pregame_v1(
            &input,
            &"d".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert_eq!(selected.selected_index_v1(), 0);
        assert_eq!(
            selected.selected_action_v1(),
            &MtgoCompetitiveNativePregameActionV1::KeepOpeningHand
        );
    }

    #[test]
    fn sideboard_score_accepts_changed_and_unchanged_inventory_conserving_targets() {
        let input = sideboard_input_v1();
        let unchanged = MtgoCompetitiveNativeSideboardModelSelectionV1 {
            target_configuration: input.current_configuration.clone(),
        };
        let mut scorer = SideboardScorerV1 {
            selection: unchanged,
            corrupt_input: false,
        };
        let selected = score_checked_untrusted_competitive_native_sideboard_v1(
            &input,
            &"d".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert!(selected.no_changes_selected_v1(&input));
        assert_eq!(selected.checked_selection_commitment_sha256_v1().len(), 64);
        assert!(!selected.safe_for_live_input_v1());
        assert!(!selected.permits_sideboard_submission_v1());

        let mut changed = input.current_configuration.clone();
        changed.mainboard[0].count += 1;
        changed.sideboard[0].count -= 1;
        let mut scorer = SideboardScorerV1 {
            selection: MtgoCompetitiveNativeSideboardModelSelectionV1 {
                target_configuration: changed,
            },
            corrupt_input: false,
        };
        let selected = score_checked_untrusted_competitive_native_sideboard_v1(
            &input,
            &"d".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert!(!selected.no_changes_selected_v1(&input));
    }

    #[test]
    fn sideboard_score_rejects_response_or_inventory_substitution() {
        let input = sideboard_input_v1();
        let unchanged = MtgoCompetitiveNativeSideboardModelSelectionV1 {
            target_configuration: input.current_configuration.clone(),
        };
        let mut corrupt_response = SideboardScorerV1 {
            selection: unchanged,
            corrupt_input: true,
        };
        assert!(score_checked_untrusted_competitive_native_sideboard_v1(
            &input,
            &"d".repeat(64),
            &mut corrupt_response,
        )
        .is_err());

        let mut invalid = input.current_configuration.clone();
        invalid.mainboard[0].count -= 1;
        let mut corrupt_inventory = SideboardScorerV1 {
            selection: MtgoCompetitiveNativeSideboardModelSelectionV1 {
                target_configuration: invalid,
            },
            corrupt_input: false,
        };
        assert!(score_checked_untrusted_competitive_native_sideboard_v1(
            &input,
            &"d".repeat(64),
            &mut corrupt_inventory,
        )
        .is_err());
    }
}
