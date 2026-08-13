use crate::{
    MtgoContractErrorV1, MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleDuelScoreResponseV1,
    MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1,
    MtgoPlayerVisibleSingleAttackerBlockerDeliberationV1,
    MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1,
    MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
};
use sha2::{Digest, Sha256};

const BLOCKER_MODEL_INPUT_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-single-attacker-blocker-model-input-v1";
const BLOCKER_MODEL_SELECTION_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-single-attacker-blocker-model-selection-v1";

/// Player-visible-only scoring boundary for one single-attacker blocker
/// inclusion choice. It cannot request aggregate kernel observations, client
/// objects, target IDs, or simulator pending-combat state.
pub trait MtgoPlayerVisibleSingleAttackerBlockerScorerV1 {
    fn score_player_visible_single_attacker_blocker_inclusion_v1(
        &mut self,
        model_input: &MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String>;
}

pub struct CheckedUntrustedMtgoPlayerVisibleSingleAttackerBlockerScoredStepV1 {
    progress: MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1,
    response: MtgoPlayerVisibleDuelScoreResponseV1,
    selected_index: usize,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    model_input_commitment_sha256: String,
    selection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleSingleAttackerBlockerScoredStepV1 {
    pub fn selected_index_v1(&self) -> usize {
        self.selected_index
    }

    pub fn selected_action_v1(&self) -> &MtgoPlayerVisibleDuelActionV1 {
        &self.selected_action
    }

    pub fn selected_logit_f32_bits_v1(&self) -> u32 {
        self.response.ordered_action_logits_f32_bits[self.selected_index]
    }

    pub fn value_f32_bits_v1(&self) -> u32 {
        self.response.value_f32_bits
    }

    pub fn model_input_commitment_sha256_v1(&self) -> &str {
        &self.model_input_commitment_sha256
    }

    pub fn selection_commitment_sha256_v1(&self) -> &str {
        &self.selection_commitment_sha256
    }

    pub fn into_progress_v1(self) -> MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1 {
        self.progress
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

pub fn score_and_advance_player_visible_single_attacker_blocker_deliberation_v1<
    S: MtgoPlayerVisibleSingleAttackerBlockerScorerV1,
>(
    deliberation: MtgoPlayerVisibleSingleAttackerBlockerDeliberationV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoPlayerVisibleSingleAttackerBlockerScoredStepV1, MtgoContractErrorV1>
{
    require_sha256_v1(deployment_commitment_sha256)?;
    let model_input = deliberation.current_model_decision_v1();
    if !matches!(
        model_input.ordered_legal_actions.as_slice(),
        [
            MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion { include: false, .. },
            MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion { include: true, .. }
        ]
    ) {
        return Err(error_v1(
            "visible_blocker_scoring_action_order",
            "blocker scoring requires the exact ordered false-then-true visible choice pair",
        ));
    }
    let model_input_json = serde_json::to_vec(&model_input).map_err(|error| {
        error_v1(
            "visible_blocker_scoring_input_serialization",
            error.to_string(),
        )
    })?;
    let model_input_commitment_sha256 =
        commitment_v1(BLOCKER_MODEL_INPUT_DOMAIN_V1, &[&model_input_json]);
    let response = scorer
        .score_player_visible_single_attacker_blocker_inclusion_v1(&model_input)
        .map_err(|error| error_v1("visible_blocker_scorer_failed", error))?;
    if response.schema_version != MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1
        || response.ordered_action_logits_f32_bits.len() != 2
    {
        return Err(error_v1(
            "visible_blocker_score_shape",
            "blocker score must contain exactly the two ordered visible inclusion logits",
        ));
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
        return Err(error_v1(
            "visible_blocker_score_nonfinite",
            "both blocker logits and the value must be finite",
        ));
    }
    let selected_index = usize::from(logits[1].total_cmp(&logits[0]).is_gt());
    let selected_action = model_input.ordered_legal_actions[selected_index].clone();
    let response_json = serde_json::to_vec(&response).map_err(|error| {
        error_v1(
            "visible_blocker_scoring_response_serialization",
            error.to_string(),
        )
    })?;
    let selected_action_json = serde_json::to_vec(&selected_action).map_err(|error| {
        error_v1(
            "visible_blocker_scoring_action_serialization",
            error.to_string(),
        )
    })?;
    let selection_commitment_sha256 = commitment_v1(
        BLOCKER_MODEL_SELECTION_DOMAIN_V1,
        &[
            model_input_commitment_sha256.as_bytes(),
            deployment_commitment_sha256.as_bytes(),
            &response_json,
            &(selected_index as u64).to_be_bytes(),
            &selected_action_json,
            b"visible_only_local_deliberation_no_client_or_input_authority",
        ],
    );
    let progress = deliberation.advance_v1(selected_index)?;
    Ok(
        CheckedUntrustedMtgoPlayerVisibleSingleAttackerBlockerScoredStepV1 {
            progress,
            response,
            selected_index,
            selected_action,
            model_input_commitment_sha256,
            selection_commitment_sha256,
        },
    )
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

fn require_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            "visible_blocker_scoring_deployment",
            "deployment commitment must be one lowercase SHA-256 digest",
        ));
    }
    Ok(())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        begin_player_visible_single_attacker_blocker_deliberation_v1, MtgoPlayerRelativeRoleV1,
        MtgoPlayerVisibleBattlefieldCardV1, MtgoPlayerVisibleCombatStateV1,
        MtgoPlayerVisibleCounterStateV1, MtgoPlayerVisibleDuelStateV1,
        MtgoPlayerVisibleObjectRefV1, MtgoPlayerVisibleSingleAttackerBlockerCandidateV1,
        MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1, ZoneIndependentStepV1,
    };

    struct FixtureScorerV1 {
        response: MtgoPlayerVisibleDuelScoreResponseV1,
        seen: usize,
    }

    impl MtgoPlayerVisibleSingleAttackerBlockerScorerV1 for FixtureScorerV1 {
        fn score_player_visible_single_attacker_blocker_inclusion_v1(
            &mut self,
            model_input: &MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1,
        ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
            assert_eq!(model_input.ordered_legal_actions.len(), 2);
            self.seen += 1;
            Ok(self.response.clone())
        }
    }

    fn object_ref_v1(visible_ordinal: u32) -> MtgoPlayerVisibleObjectRefV1 {
        MtgoPlayerVisibleObjectRefV1 { visible_ordinal }
    }

    fn card_v1(visible_ordinal: u32, name: &str) -> MtgoPlayerVisibleBattlefieldCardV1 {
        MtgoPlayerVisibleBattlefieldCardV1 {
            object_ref: object_ref_v1(visible_ordinal),
            card_name: name.to_owned(),
            tapped: false,
            marked_damage: 0,
            counters: MtgoPlayerVisibleCounterStateV1 {
                plus_one_plus_one: 0,
                minus_one_minus_one: 0,
                minus_zero_minus_one: 0,
                stun: 0,
                lore: 0,
            },
            is_token: false,
            visible_effective_power: Some(2),
            visible_effective_toughness: Some(2),
        }
    }

    fn input_v1() -> MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1 {
        let attacker = object_ref_v1(1);
        MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1 {
            current_state: MtgoPlayerVisibleDuelStateV1 {
                acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                turn: 4,
                phase: ZoneIndependentStepV1::DeclareBlockers,
                active_player: MtgoPlayerRelativeRoleV1::Opponent,
                priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6], [0; 6]],
                hand_counts: [0, 0],
                library_counts: [50, 50],
                battlefield: [vec![card_v1(0, "Blocker")], vec![card_v1(1, "Attacker")]],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: MtgoPlayerVisibleCombatStateV1 {
                    attackers_declared: true,
                    blockers_declared: false,
                    ordered_attackers: vec![attacker],
                    blocker_assignments: Vec::new(),
                },
                visible_object_relations: Vec::new(),
                own_hand: Vec::new(),
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            attacker,
            ordered_candidates: vec![MtgoPlayerVisibleSingleAttackerBlockerCandidateV1 {
                blocker: object_ref_v1(0),
                currently_blocking: false,
                block_action_visible: true,
            }],
            unique_visible_enabled_done_control: true,
        }
    }

    fn response_v1(logits: [f32; 2]) -> MtgoPlayerVisibleDuelScoreResponseV1 {
        MtgoPlayerVisibleDuelScoreResponseV1 {
            schema_version: MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
            ordered_action_logits_f32_bits: logits.map(f32::to_bits).to_vec(),
            value_f32_bits: 0.25f32.to_bits(),
        }
    }

    #[test]
    fn scores_only_visible_input_and_advances_move_only_scan() {
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(scan) =
            begin_player_visible_single_attacker_blocker_deliberation_v1(input_v1()).unwrap()
        else {
            panic!("expected blocker subdecision");
        };
        let mut scorer = FixtureScorerV1 {
            response: response_v1([-1.0, 2.0]),
            seen: 0,
        };
        let scored = score_and_advance_player_visible_single_attacker_blocker_deliberation_v1(
            scan,
            &"a".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert_eq!(scorer.seen, 1);
        assert_eq!(scored.selected_index_v1(), 1);
        assert!(matches!(
            scored.selected_action_v1(),
            MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion { include: true, .. }
        ));
        assert_eq!(scored.model_input_commitment_sha256_v1().len(), 64);
        assert_eq!(scored.selection_commitment_sha256_v1().len(), 64);
        assert!(matches!(
            scored.into_progress_v1(),
            MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Complete(_)
        ));
    }

    #[test]
    fn rejects_bad_deployment_score_shape_and_nonfinite_outputs() {
        let make_scan = || {
            let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(scan) =
                begin_player_visible_single_attacker_blocker_deliberation_v1(input_v1()).unwrap()
            else {
                panic!("expected blocker subdecision");
            };
            scan
        };
        let mut scorer = FixtureScorerV1 {
            response: response_v1([0.0, 1.0]),
            seen: 0,
        };
        let error = match score_and_advance_player_visible_single_attacker_blocker_deliberation_v1(
            make_scan(),
            "bad",
            &mut scorer,
        ) {
            Err(error) => error,
            Ok(_) => panic!("invalid deployment commitment must reject"),
        };
        assert_eq!(error.code(), "visible_blocker_scoring_deployment");

        scorer.response.ordered_action_logits_f32_bits.pop();
        let error = match score_and_advance_player_visible_single_attacker_blocker_deliberation_v1(
            make_scan(),
            &"b".repeat(64),
            &mut scorer,
        ) {
            Err(error) => error,
            Ok(_) => panic!("wrong score width must reject"),
        };
        assert_eq!(error.code(), "visible_blocker_score_shape");

        scorer.response = response_v1([0.0, f32::NAN]);
        let error = match score_and_advance_player_visible_single_attacker_blocker_deliberation_v1(
            make_scan(),
            &"c".repeat(64),
            &mut scorer,
        ) {
            Err(error) => error,
            Ok(_) => panic!("nonfinite score must reject"),
        };
        assert_eq!(error.code(), "visible_blocker_score_nonfinite");
    }
}
