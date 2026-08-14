use crate::{
    validate_player_visible_duel_decision_input_strict_v1, MtgoContractErrorV1,
    MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleDuelDecisionInputV1,
    MtgoPlayerVisibleDuelScoreResponseV1, MtgoPlayerVisibleDuelStateV1,
    MtgoPlayerVisibleObjectRefV1, ZoneIndependentStepV1,
    MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const MAX_VISIBLE_COMBAT_CHOICES_V1: usize = 64;
const MODEL_INPUT_DOMAIN_V1: &[u8] = b"mtgo-player-visible-multi-attacker-blocker-model-input-v1";
const MODEL_SELECTION_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-multi-attacker-blocker-model-selection-v1";

/// The rendered declare-blockers state before another blocker is selected.
/// Attacker legality is deliberately absent here because MTGO has not yet
/// presented the target highlights for a particular blocker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1 {
    pub current_state: MtgoPlayerVisibleDuelStateV1,
    pub ordered_available_blockers: Vec<MtgoPlayerVisibleObjectRefV1>,
    pub unique_visible_enabled_done_control: bool,
}

/// The later rendered target-selection moment for one visible blocker. Every
/// candidate is a card MTGO currently presents as targetable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleBlockerTargetSelectionInputV1 {
    pub current_state: MtgoPlayerVisibleDuelStateV1,
    pub blocker: MtgoPlayerVisibleObjectRefV1,
    pub ordered_visible_targetable_attackers: Vec<MtgoPlayerVisibleObjectRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "choice_kind", rename_all = "snake_case")]
pub enum MtgoPlayerVisibleMultiAttackerBlockerChoiceV1 {
    FinishBlocking,
    ChooseBlocker {
        blocker: MtgoPlayerVisibleObjectRefV1,
    },
    ChooseAttackerForBlocker {
        blocker: MtgoPlayerVisibleObjectRefV1,
        attacker: MtgoPlayerVisibleObjectRefV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision_kind", rename_all = "snake_case")]
pub enum MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1 {
    SelectBlocker {
        current_state: MtgoPlayerVisibleDuelStateV1,
        ordered_legal_choices: Vec<MtgoPlayerVisibleMultiAttackerBlockerChoiceV1>,
    },
    SelectAttackerForBlocker {
        current_state: MtgoPlayerVisibleDuelStateV1,
        blocker: MtgoPlayerVisibleObjectRefV1,
        ordered_legal_choices: Vec<MtgoPlayerVisibleMultiAttackerBlockerChoiceV1>,
    },
}

impl MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1 {
    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

pub trait MtgoPlayerVisibleMultiAttackerBlockerScorerV1 {
    fn score_player_visible_multi_attacker_blocker_v1(
        &mut self,
        model_input: &MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String>;
}

pub struct CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerChoiceV1 {
    model_input: MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1,
    selected_index: usize,
    selected_choice: MtgoPlayerVisibleMultiAttackerBlockerChoiceV1,
    response: MtgoPlayerVisibleDuelScoreResponseV1,
    model_input_commitment_sha256: String,
    selection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerChoiceV1 {
    pub fn model_input_v1(&self) -> &MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1 {
        &self.model_input
    }

    pub fn selected_index_v1(&self) -> usize {
        self.selected_index
    }

    pub fn selected_choice_v1(&self) -> &MtgoPlayerVisibleMultiAttackerBlockerChoiceV1 {
        &self.selected_choice
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

pub fn validate_player_visible_multi_attacker_blocker_selection_v1(
    input: &MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    let assigned = validate_common_blocking_state_v1(&input.current_state)?;
    if !input.unique_visible_enabled_done_control {
        return Err(error_v1(
            "visible_multi_blocker_done_control",
            "the visible blocker stage requires exactly one enabled completion control",
        ));
    }
    validate_ordered_battlefield_subset_v1(
        &input.current_state.battlefield[0],
        &input.ordered_available_blockers,
        &assigned,
        false,
        "visible_multi_blocker_candidates",
    )
}

pub fn validate_player_visible_blocker_target_selection_v1(
    input: &MtgoPlayerVisibleBlockerTargetSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    let assigned = validate_common_blocking_state_v1(&input.current_state)?;
    if assigned.contains(&input.blocker.visible_ordinal)
        || !input.current_state.battlefield[0]
            .iter()
            .any(|card| card.object_ref == input.blocker)
    {
        return Err(error_v1(
            "visible_blocker_target_source",
            "the targeting blocker must be one unassigned seated-player battlefield card",
        ));
    }
    validate_ordered_attacker_subset_v1(
        &input.current_state,
        &input.ordered_visible_targetable_attackers,
    )
}

pub fn score_and_select_player_visible_multi_attacker_blocker_v1<
    S: MtgoPlayerVisibleMultiAttackerBlockerScorerV1,
>(
    input: MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerChoiceV1, MtgoContractErrorV1> {
    validate_player_visible_multi_attacker_blocker_selection_v1(&input)?;
    let mut ordered_legal_choices =
        vec![MtgoPlayerVisibleMultiAttackerBlockerChoiceV1::FinishBlocking];
    ordered_legal_choices.extend(
        input
            .ordered_available_blockers
            .iter()
            .copied()
            .map(
                |blocker| MtgoPlayerVisibleMultiAttackerBlockerChoiceV1::ChooseBlocker { blocker },
            ),
    );
    score_model_decision_v1(
        MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1::SelectBlocker {
            current_state: input.current_state,
            ordered_legal_choices,
        },
        deployment_commitment_sha256,
        scorer,
    )
}

pub fn score_and_select_player_visible_blocker_target_v1<
    S: MtgoPlayerVisibleMultiAttackerBlockerScorerV1,
>(
    input: MtgoPlayerVisibleBlockerTargetSelectionInputV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerChoiceV1, MtgoContractErrorV1> {
    validate_player_visible_blocker_target_selection_v1(&input)?;
    let ordered_legal_choices = input
        .ordered_visible_targetable_attackers
        .iter()
        .copied()
        .map(
            |attacker| MtgoPlayerVisibleMultiAttackerBlockerChoiceV1::ChooseAttackerForBlocker {
                blocker: input.blocker,
                attacker,
            },
        )
        .collect();
    score_model_decision_v1(
        MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1::SelectAttackerForBlocker {
            current_state: input.current_state,
            blocker: input.blocker,
            ordered_legal_choices,
        },
        deployment_commitment_sha256,
        scorer,
    )
}

fn score_model_decision_v1<S: MtgoPlayerVisibleMultiAttackerBlockerScorerV1>(
    model_input: MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1,
    deployment_commitment_sha256: &str,
    scorer: &mut S,
) -> Result<CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerChoiceV1, MtgoContractErrorV1> {
    require_sha256_v1(deployment_commitment_sha256)?;
    let choices = match &model_input {
        MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1::SelectBlocker {
            ordered_legal_choices,
            ..
        }
        | MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1::SelectAttackerForBlocker {
            ordered_legal_choices,
            ..
        } => ordered_legal_choices,
    };
    let model_input_json = serde_json::to_vec(&model_input).map_err(|error| {
        error_v1(
            "visible_multi_blocker_model_input_serialization",
            error.to_string(),
        )
    })?;
    let model_input_commitment_sha256 = commitment_v1(MODEL_INPUT_DOMAIN_V1, &[&model_input_json]);
    let response = scorer
        .score_player_visible_multi_attacker_blocker_v1(&model_input)
        .map_err(|error| error_v1("visible_multi_blocker_scorer_failed", error))?;
    if response.schema_version != MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1
        || response.ordered_action_logits_f32_bits.len() != choices.len()
        || choices.is_empty()
    {
        return Err(error_v1(
            "visible_multi_blocker_score_shape",
            "the score must preserve the exact nonempty visible staged-choice count",
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
            "visible_multi_blocker_score_nonfinite",
            "all staged blocker logits and the value must be finite",
        ));
    }
    let selected_index = logits
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1).then_with(|| right.0.cmp(&left.0)))
        .map(|(index, _)| index)
        .expect("nonempty visible choices");
    let selected_choice = choices[selected_index].clone();
    let response_json = serde_json::to_vec(&response).map_err(|error| {
        error_v1(
            "visible_multi_blocker_score_serialization",
            error.to_string(),
        )
    })?;
    let selected_json = serde_json::to_vec(&selected_choice).map_err(|error| {
        error_v1(
            "visible_multi_blocker_choice_serialization",
            error.to_string(),
        )
    })?;
    let selection_commitment_sha256 = commitment_v1(
        MODEL_SELECTION_DOMAIN_V1,
        &[
            model_input_commitment_sha256.as_bytes(),
            deployment_commitment_sha256.as_bytes(),
            &response_json,
            &(selected_index as u64).to_be_bytes(),
            &selected_json,
            b"visible_only_staged_blocking_no_client_or_input_authority",
        ],
    );
    Ok(
        CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerChoiceV1 {
            model_input,
            selected_index,
            selected_choice,
            response,
            model_input_commitment_sha256,
            selection_commitment_sha256,
        },
    )
}

fn validate_common_blocking_state_v1(
    state: &MtgoPlayerVisibleDuelStateV1,
) -> Result<HashSet<u32>, MtgoContractErrorV1> {
    validate_player_visible_duel_decision_input_strict_v1(&MtgoPlayerVisibleDuelDecisionInputV1 {
        current_state: state.clone(),
        ordered_legal_actions: Vec::new(),
    })?;
    if state.phase != ZoneIndependentStepV1::DeclareBlockers
        || state.acting_player != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || state.active_player != MtgoPlayerRelativeRoleV1::Opponent
        || state.priority_player != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || !state.combat.attackers_declared
        || state.combat.blockers_declared
        || !state.stack.is_empty()
        || state.combat.ordered_attackers.len() < 2
        || state.combat.ordered_attackers.len() > MAX_VISIBLE_COMBAT_CHOICES_V1
    {
        return Err(error_v1(
            "visible_multi_blocker_state",
            "multi-attacker blocking requires the uncomplicated visible declare-blockers state",
        ));
    }
    validate_ordered_attacker_subset_v1(state, &state.combat.ordered_attackers)?;
    let attacker_positions = state
        .combat
        .ordered_attackers
        .iter()
        .enumerate()
        .map(|(position, attacker)| (attacker.visible_ordinal, position))
        .collect::<HashMap<_, _>>();
    let seated_refs = state.battlefield[0]
        .iter()
        .map(|card| card.object_ref.visible_ordinal)
        .collect::<HashSet<_>>();
    let mut prior_assignment_position = None;
    let mut assigned_blockers = HashSet::new();
    for assignment in &state.combat.blocker_assignments {
        let position = *attacker_positions
            .get(&assignment.attacker.visible_ordinal)
            .ok_or_else(|| {
                error_v1(
                    "visible_multi_blocker_assignment",
                    "assignment attacker is not rendered attacking",
                )
            })?;
        if assignment.ordered_blockers.is_empty()
            || prior_assignment_position.is_some_and(|prior| position <= prior)
        {
            return Err(error_v1(
                "visible_multi_blocker_assignment",
                "nonempty blocker assignments must preserve visible attacker order",
            ));
        }
        prior_assignment_position = Some(position);
        for blocker in &assignment.ordered_blockers {
            if !seated_refs.contains(&blocker.visible_ordinal)
                || !assigned_blockers.insert(blocker.visible_ordinal)
            {
                return Err(error_v1(
                    "visible_multi_blocker_assignment",
                    "each rendered blocker may occur once and must be on the seated battlefield",
                ));
            }
        }
    }
    Ok(assigned_blockers)
}

fn validate_ordered_attacker_subset_v1(
    state: &MtgoPlayerVisibleDuelStateV1,
    candidates: &[MtgoPlayerVisibleObjectRefV1],
) -> Result<(), MtgoContractErrorV1> {
    if candidates.is_empty() || candidates.len() > MAX_VISIBLE_COMBAT_CHOICES_V1 {
        return Err(error_v1(
            "visible_blocker_target_candidates",
            "the rendered target modal must present one to 64 attacker cards",
        ));
    }
    let ordered_positions = state
        .combat
        .ordered_attackers
        .iter()
        .enumerate()
        .map(|(position, attacker)| (attacker.visible_ordinal, position))
        .collect::<HashMap<_, _>>();
    let opponent_refs = state.battlefield[1]
        .iter()
        .map(|card| card.object_ref.visible_ordinal)
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut prior = None;
    for candidate in candidates {
        let position = *ordered_positions
            .get(&candidate.visible_ordinal)
            .ok_or_else(|| {
                error_v1(
                    "visible_blocker_target_candidates",
                    "targetable card is not a rendered attacker",
                )
            })?;
        if !opponent_refs.contains(&candidate.visible_ordinal)
            || !seen.insert(candidate.visible_ordinal)
            || prior.is_some_and(|prior_position| position <= prior_position)
        {
            return Err(error_v1(
                "visible_blocker_target_candidates",
                "targetable attackers must be unique opponent cards in rendered attacker order",
            ));
        }
        prior = Some(position);
    }
    Ok(())
}

fn validate_ordered_battlefield_subset_v1(
    battlefield: &[crate::MtgoPlayerVisibleBattlefieldCardV1],
    candidates: &[MtgoPlayerVisibleObjectRefV1],
    forbidden: &HashSet<u32>,
    require_nonempty: bool,
    code: &'static str,
) -> Result<(), MtgoContractErrorV1> {
    if candidates.len() > MAX_VISIBLE_COMBAT_CHOICES_V1
        || (require_nonempty && candidates.is_empty())
    {
        return Err(error_v1(
            code,
            "visible battlefield choice count is outside the supported bound",
        ));
    }
    let positions = battlefield
        .iter()
        .enumerate()
        .map(|(position, card)| (card.object_ref.visible_ordinal, position))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut prior = None;
    for candidate in candidates {
        let position = *positions
            .get(&candidate.visible_ordinal)
            .ok_or_else(|| error_v1(code, "visible choice is outside the seated battlefield"))?;
        if forbidden.contains(&candidate.visible_ordinal)
            || !seen.insert(candidate.visible_ordinal)
            || prior.is_some_and(|prior_position| position <= prior_position)
        {
            return Err(error_v1(
                code,
                "visible choices must be unassigned, unique, and preserve battlefield order",
            ));
        }
        prior = Some(position);
    }
    Ok(())
}

fn require_sha256_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error_v1(
            "visible_multi_blocker_scoring_deployment",
            "deployment commitment must be one lowercase SHA-256 digest",
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
    use crate::{
        MtgoPlayerVisibleBattlefieldCardV1, MtgoPlayerVisibleBlockerAssignmentV1,
        MtgoPlayerVisibleCombatStateV1, MtgoPlayerVisibleCounterStateV1,
    };

    fn r(value: u32) -> MtgoPlayerVisibleObjectRefV1 {
        MtgoPlayerVisibleObjectRefV1 {
            visible_ordinal: value,
        }
    }

    fn card(value: u32) -> MtgoPlayerVisibleBattlefieldCardV1 {
        MtgoPlayerVisibleBattlefieldCardV1 {
            object_ref: r(value),
            card_name: format!("Card {value}"),
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

    fn state() -> MtgoPlayerVisibleDuelStateV1 {
        MtgoPlayerVisibleDuelStateV1 {
            acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            turn: 2,
            phase: ZoneIndependentStepV1::DeclareBlockers,
            active_player: MtgoPlayerRelativeRoleV1::Opponent,
            priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            initiative: None,
            life_totals: [20, 20],
            mana_pools: [[0; 6], [0; 6]],
            hand_counts: [0, 0],
            library_counts: [50, 50],
            battlefield: [vec![card(0), card(1)], vec![card(2), card(3)]],
            graveyards: [Vec::new(), Vec::new()],
            exile: Vec::new(),
            stack: Vec::new(),
            combat: MtgoPlayerVisibleCombatStateV1 {
                attackers_declared: true,
                blockers_declared: false,
                ordered_attackers: vec![r(2), r(3)],
                blocker_assignments: Vec::new(),
            },
            visible_object_relations: Vec::new(),
            own_hand: Vec::new(),
            known_library_cards: [Vec::new(), Vec::new()],
            known_hand_cards: [Vec::new(), Vec::new()],
        }
    }

    struct Scorer {
        logits: Vec<f32>,
        seen: Option<MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1>,
    }

    impl MtgoPlayerVisibleMultiAttackerBlockerScorerV1 for Scorer {
        fn score_player_visible_multi_attacker_blocker_v1(
            &mut self,
            input: &MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1,
        ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
            self.seen = Some(input.clone());
            Ok(MtgoPlayerVisibleDuelScoreResponseV1 {
                schema_version: MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
                ordered_action_logits_f32_bits: self
                    .logits
                    .iter()
                    .map(|value| value.to_bits())
                    .collect(),
                value_f32_bits: 0.25f32.to_bits(),
            })
        }
    }

    #[test]
    fn model_first_selects_visible_blocker_then_visible_target() {
        let start = MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1 {
            current_state: state(),
            ordered_available_blockers: vec![r(0), r(1)],
            unique_visible_enabled_done_control: true,
        };
        let mut scorer = Scorer {
            logits: vec![-1.0, 3.0, 2.0],
            seen: None,
        };
        let selected = score_and_select_player_visible_multi_attacker_blocker_v1(
            start,
            &"a".repeat(64),
            &mut scorer,
        )
        .unwrap();
        assert_eq!(selected.selected_index_v1(), 1);
        assert!(matches!(selected.selected_choice_v1(),
            MtgoPlayerVisibleMultiAttackerBlockerChoiceV1::ChooseBlocker { blocker } if *blocker == r(0)));
        assert!(!selected.safe_for_live_input_v1());

        scorer.logits = vec![0.0, 4.0];
        let target = MtgoPlayerVisibleBlockerTargetSelectionInputV1 {
            current_state: state(),
            blocker: r(0),
            ordered_visible_targetable_attackers: vec![r(2), r(3)],
        };
        let selected =
            score_and_select_player_visible_blocker_target_v1(target, &"b".repeat(64), &mut scorer)
                .unwrap();
        assert!(matches!(selected.selected_choice_v1(),
            MtgoPlayerVisibleMultiAttackerBlockerChoiceV1::ChooseAttackerForBlocker { blocker, attacker }
                if *blocker == r(0) && *attacker == r(3)));
    }

    #[test]
    fn current_assignments_are_visible_and_cannot_reuse_a_blocker() {
        let mut current = state();
        current.combat.blocker_assignments = vec![MtgoPlayerVisibleBlockerAssignmentV1 {
            attacker: r(2),
            ordered_blockers: vec![r(0)],
        }];
        let valid = MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1 {
            current_state: current.clone(),
            ordered_available_blockers: vec![r(1)],
            unique_visible_enabled_done_control: true,
        };
        validate_player_visible_multi_attacker_blocker_selection_v1(&valid).unwrap();
        let invalid = MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1 {
            ordered_available_blockers: vec![r(0)],
            ..valid
        };
        assert_eq!(
            validate_player_visible_multi_attacker_blocker_selection_v1(&invalid)
                .unwrap_err()
                .code(),
            "visible_multi_blocker_candidates"
        );
    }

    #[test]
    fn rejects_hidden_or_misordered_target_candidates_and_unknown_json() {
        let mut wrong = MtgoPlayerVisibleBlockerTargetSelectionInputV1 {
            current_state: state(),
            blocker: r(0),
            ordered_visible_targetable_attackers: vec![r(3), r(2)],
        };
        assert_eq!(
            validate_player_visible_blocker_target_selection_v1(&wrong)
                .unwrap_err()
                .code(),
            "visible_blocker_target_candidates"
        );
        wrong.ordered_visible_targetable_attackers = vec![r(2), r(1)];
        assert!(validate_player_visible_blocker_target_selection_v1(&wrong).is_err());

        let mut value = serde_json::to_value(MtgoPlayerVisibleBlockerTargetSelectionInputV1 {
            current_state: state(),
            blocker: r(0),
            ordered_visible_targetable_attackers: vec![r(2)],
        })
        .unwrap();
        value["legal_targets"] = serde_json::json!([999]);
        assert!(
            serde_json::from_value::<MtgoPlayerVisibleBlockerTargetSelectionInputV1>(value)
                .is_err()
        );
    }
}
