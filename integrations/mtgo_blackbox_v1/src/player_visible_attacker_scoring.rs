use crate::{
    MtgoContractErrorV1, MtgoPlayerVisibleAttackerDeliberationProgressV1,
    MtgoPlayerVisibleAttackerDeliberationV1, MtgoPlayerVisibleAttackerInclusionDecisionV1,
    MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleDuelScoreResponseV1,
    MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
};
use sha2::{Digest, Sha256};

const ATTACKER_MODEL_INPUT_DOMAIN_V1: &[u8] = b"mtgo-player-visible-attacker-model-input-v1";
const ATTACKER_MODEL_SELECTION_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-attacker-model-selection-v1";

/// Player-visible-only scoring boundary for one local attacker inclusion
/// choice. A future native implementation must consume only this input. It
/// cannot request `ObservationV5`, `ActionSemanticV1`, client objects, or
/// simulator pending-combat state through this interface.
pub trait MtgoPlayerVisibleAttackerScorerV1 {
    fn score_player_visible_attacker_inclusion_v1(
        &mut self,
        model_input: &MtgoPlayerVisibleAttackerInclusionDecisionV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String>;
}

/// One model-scored attacker inclusion plus the move-only continuation of the
/// local deliberation. This wrapper contains no MTGO action object, command,
/// process handle, input method, event-entry authority, or spending authority.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleAttackerScoredStepV1;
/// fn cannot_extract_client_input(value: &CheckedUntrustedMtgoPlayerVisibleAttackerScoredStepV1) {
///     let _ = value.client_action();
///     let _ = value.input_command();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleAttackerScoredStepV1 {
    progress: MtgoPlayerVisibleAttackerDeliberationProgressV1,
    response: MtgoPlayerVisibleDuelScoreResponseV1,
    selected_index: usize,
    selected_action: MtgoPlayerVisibleDuelActionV1,
    model_input_commitment_sha256: String,
    selection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleAttackerScoredStepV1 {
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

    pub fn into_progress_v1(self) -> MtgoPlayerVisibleAttackerDeliberationProgressV1 {
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

/// Scores the current visible attacker candidate, chooses the greatest finite
/// logit with lower-index tie breaking, and consumes the deliberation into its
/// next state. The scorer sees only the player-visible state and adapter-local
/// scan context produced by `current_model_decision_v1`.
pub fn score_and_advance_player_visible_attacker_deliberation_v1<
    S: MtgoPlayerVisibleAttackerScorerV1,
>(
    deliberation: MtgoPlayerVisibleAttackerDeliberationV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoPlayerVisibleAttackerScoredStepV1, MtgoContractErrorV1> {
    require_sha256_v1(deployment_commitment_sha256)?;
    let model_input = deliberation.current_model_decision_v1();
    if !matches!(
        model_input.ordered_legal_actions.as_slice(),
        [
            MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion { include: false, .. },
            MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion { include: true, .. }
        ]
    ) {
        return Err(error_v1(
            "visible_attacker_scoring_action_order",
            "attacker scoring requires the exact ordered false-then-true visible choice pair",
        ));
    }
    let model_input_json = serde_json::to_vec(&model_input).map_err(|error| {
        error_v1(
            "visible_attacker_scoring_input_serialization",
            error.to_string(),
        )
    })?;
    let model_input_commitment_sha256 =
        commitment_v1(ATTACKER_MODEL_INPUT_DOMAIN_V1, &[&model_input_json]);
    let response = scorer
        .score_player_visible_attacker_inclusion_v1(&model_input)
        .map_err(|error| error_v1("visible_attacker_scorer_failed", error))?;
    if response.schema_version != MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1
        || response.ordered_action_logits_f32_bits.len() != 2
    {
        return Err(error_v1(
            "visible_attacker_score_shape",
            "attacker score must contain exactly the two ordered visible inclusion logits",
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
            "visible_attacker_score_nonfinite",
            "both attacker logits and the value must be finite",
        ));
    }
    let selected_index = usize::from(logits[1].total_cmp(&logits[0]).is_gt());
    let selected_action = model_input.ordered_legal_actions[selected_index].clone();
    let response_json = serde_json::to_vec(&response).map_err(|error| {
        error_v1(
            "visible_attacker_scoring_response_serialization",
            error.to_string(),
        )
    })?;
    let selected_action_json = serde_json::to_vec(&selected_action).map_err(|error| {
        error_v1(
            "visible_attacker_scoring_action_serialization",
            error.to_string(),
        )
    })?;
    let selection_commitment_sha256 = commitment_v1(
        ATTACKER_MODEL_SELECTION_DOMAIN_V1,
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
    Ok(CheckedUntrustedMtgoPlayerVisibleAttackerScoredStepV1 {
        progress,
        response,
        selected_index,
        selected_action,
        model_input_commitment_sha256,
        selection_commitment_sha256,
    })
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
            "visible_attacker_scoring_deployment",
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
        begin_player_visible_attacker_deliberation_v1, MtgoPlayerRelativeRoleV1,
        MtgoPlayerVisibleAttackerCandidateV1, MtgoPlayerVisibleAttackerSelectionInputV1,
        MtgoPlayerVisibleBattlefieldCardV1, MtgoPlayerVisibleCombatStateV1,
        MtgoPlayerVisibleCounterStateV1, MtgoPlayerVisibleDuelStateV1,
        MtgoPlayerVisibleObjectRefV1, ZoneIndependentStepV1,
    };
    use std::collections::HashSet;

    fn object_ref_v1(visible_ordinal: u32) -> MtgoPlayerVisibleObjectRefV1 {
        MtgoPlayerVisibleObjectRefV1 { visible_ordinal }
    }

    fn input_v1() -> MtgoPlayerVisibleAttackerSelectionInputV1 {
        let card = |visible_ordinal, card_name: &str| MtgoPlayerVisibleBattlefieldCardV1 {
            object_ref: object_ref_v1(visible_ordinal),
            card_name: card_name.to_owned(),
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
        };
        MtgoPlayerVisibleAttackerSelectionInputV1 {
            current_state: MtgoPlayerVisibleDuelStateV1 {
                acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                turn: 4,
                phase: ZoneIndependentStepV1::DeclareAttackers,
                active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6], [0; 6]],
                hand_counts: [0, 3],
                library_counts: [48, 47],
                battlefield: [[card(0, "Bear A"), card(1, "Bear B")].to_vec(), Vec::new()],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: MtgoPlayerVisibleCombatStateV1 {
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
            ordered_candidates: [0, 1]
                .into_iter()
                .map(|visible_ordinal| MtgoPlayerVisibleAttackerCandidateV1 {
                    attacker: object_ref_v1(visible_ordinal),
                    currently_attacking: false,
                    attack_opponent_action_visible: true,
                    dont_attack_action_visible: false,
                })
                .collect(),
            unique_visible_enabled_done_control: true,
        }
    }

    struct RecordingScorerV1 {
        logits: Vec<u32>,
        value: u32,
        observed_json: Vec<String>,
    }

    impl MtgoPlayerVisibleAttackerScorerV1 for RecordingScorerV1 {
        fn score_player_visible_attacker_inclusion_v1(
            &mut self,
            model_input: &MtgoPlayerVisibleAttackerInclusionDecisionV1,
        ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
            self.observed_json
                .push(serde_json::to_string(model_input).unwrap());
            Ok(MtgoPlayerVisibleDuelScoreResponseV1 {
                schema_version: MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
                ordered_action_logits_f32_bits: self.logits.clone(),
                value_f32_bits: self.value,
            })
        }
    }

    fn pending_v1() -> MtgoPlayerVisibleAttackerDeliberationV1 {
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(scan) =
            begin_player_visible_attacker_deliberation_v1(input_v1()).unwrap()
        else {
            panic!("expected pending attacker scan");
        };
        scan
    }

    #[test]
    fn scorer_receives_only_visible_and_local_context_then_advances_move_only_scan() {
        let mut scorer = RecordingScorerV1 {
            logits: vec![0.0_f32.to_bits(), 1.0_f32.to_bits()],
            value: 0.25_f32.to_bits(),
            observed_json: Vec::new(),
        };
        let first = score_and_advance_player_visible_attacker_deliberation_v1(
            pending_v1(),
            &"a".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert_eq!(first.selected_index_v1(), 1);
        assert!(matches!(
            first.selected_action_v1(),
            MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion {
                attacker,
                include: true,
                ..
            } if *attacker == object_ref_v1(0)
        ));
        assert_eq!(first.model_input_commitment_sha256_v1().len(), 64);
        assert_eq!(first.selection_commitment_sha256_v1().len(), 64);
        assert!(!first.safe_for_live_input_v1());
        assert!(!first.permits_event_entry_v1());
        assert!(!first.permits_spending_v1());

        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(next) =
            first.into_progress_v1()
        else {
            panic!("expected second attacker");
        };
        scorer.logits = vec![1.0_f32.to_bits(), 0.0_f32.to_bits()];
        let second = score_and_advance_player_visible_attacker_deliberation_v1(
            next,
            &"a".repeat(64),
            &mut scorer,
        )
        .unwrap();
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Complete(plan) =
            second.into_progress_v1()
        else {
            panic!("expected complete attacker plan");
        };
        assert_eq!(plan.desired_attackers_v1(), &[object_ref_v1(0)]);

        let first_json: serde_json::Value = serde_json::from_str(&scorer.observed_json[0]).unwrap();
        let second_json: serde_json::Value =
            serde_json::from_str(&scorer.observed_json[1]).unwrap();
        assert_eq!(
            first_json["selected_before_current"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            second_json["selected_before_current"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        for forbidden in [
            "arena_id",
            "card_db_id",
            "zone_change_count",
            "adapter_object_id",
            "engine_context",
            "surface_context",
            "policy_surface_context",
            "client_action",
            "attack_victim",
        ] {
            assert!(!scorer.observed_json.join("").contains(forbidden));
        }
    }

    #[test]
    fn ties_choose_false_and_local_choice_history_changes_the_next_input_commitment() {
        let mut scorer = RecordingScorerV1 {
            logits: vec![1.0_f32.to_bits(); 2],
            value: 0.0_f32.to_bits(),
            observed_json: Vec::new(),
        };
        let first = score_and_advance_player_visible_attacker_deliberation_v1(
            pending_v1(),
            &"b".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert_eq!(first.selected_index_v1(), 0);
        let first_commitment = first.model_input_commitment_sha256_v1().to_owned();
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(next) =
            first.into_progress_v1()
        else {
            panic!("expected pending");
        };
        let second = score_and_advance_player_visible_attacker_deliberation_v1(
            next,
            &"b".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert_ne!(first_commitment, second.model_input_commitment_sha256_v1());
    }

    #[test]
    fn wrong_shape_nonfinite_output_and_bad_deployment_reject_without_progress() {
        for (logits, value) in [
            (vec![0.0_f32.to_bits()], 0.0_f32.to_bits()),
            (
                vec![0.0_f32.to_bits(), f32::NAN.to_bits()],
                0.0_f32.to_bits(),
            ),
            (vec![0.0_f32.to_bits(); 2], f32::INFINITY.to_bits()),
        ] {
            let mut scorer = RecordingScorerV1 {
                logits,
                value,
                observed_json: Vec::new(),
            };
            assert!(score_and_advance_player_visible_attacker_deliberation_v1(
                pending_v1(),
                &"c".repeat(64),
                &mut scorer,
            )
            .is_err());
        }
        let mut scorer = RecordingScorerV1 {
            logits: vec![0.0_f32.to_bits(); 2],
            value: 0.0_f32.to_bits(),
            observed_json: Vec::new(),
        };
        assert!(score_and_advance_player_visible_attacker_deliberation_v1(
            pending_v1(),
            "not-a-digest",
            &mut scorer,
        )
        .is_err());
        assert!(scorer.observed_json.is_empty());
    }

    #[test]
    fn desired_set_remains_unique_after_model_choices() {
        let mut scorer = RecordingScorerV1 {
            logits: vec![0.0_f32.to_bits(), 1.0_f32.to_bits()],
            value: 0.0_f32.to_bits(),
            observed_json: Vec::new(),
        };
        let first = score_and_advance_player_visible_attacker_deliberation_v1(
            pending_v1(),
            &"d".repeat(64),
            &mut scorer,
        )
        .unwrap();
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(next) =
            first.into_progress_v1()
        else {
            panic!("expected pending");
        };
        let second = score_and_advance_player_visible_attacker_deliberation_v1(
            next,
            &"d".repeat(64),
            &mut scorer,
        )
        .unwrap();
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Complete(plan) =
            second.into_progress_v1()
        else {
            panic!("expected complete");
        };
        assert_eq!(
            plan.desired_attackers_v1()
                .iter()
                .map(|value| value.visible_ordinal)
                .collect::<HashSet<_>>()
                .len(),
            2
        );
    }
}
