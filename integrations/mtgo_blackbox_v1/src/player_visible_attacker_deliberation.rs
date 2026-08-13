use crate::{
    validate_player_visible_duel_decision_input_strict_v1, MtgoContractErrorV1,
    MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleDuelDecisionInputV1,
    MtgoPlayerVisibleDuelStateV1, MtgoPlayerVisibleObjectRefV1, ZoneIndependentStepV1,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_VISIBLE_ATTACKER_CANDIDATES_V1: usize = 64;

/// One MTGO attacker toggle reduced to facts available from the rendered
/// battlefield and visible action presentation. Client action and victim IDs
/// may be used to establish this record, but cannot be stored in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleAttackerCandidateV1 {
    pub attacker: MtgoPlayerVisibleObjectRefV1,
    pub currently_attacking: bool,
    pub attack_opponent_action_visible: bool,
    pub dont_attack_action_visible: bool,
}

/// Complete visible attacker-selection moment before any model choice is sent
/// back to MTGO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleAttackerSelectionInputV1 {
    pub current_state: MtgoPlayerVisibleDuelStateV1,
    pub ordered_candidates: Vec<MtgoPlayerVisibleAttackerCandidateV1>,
    pub unique_visible_enabled_done_control: bool,
}

/// One model subdecision. The context is derived only from the complete
/// visible candidate order and choices already made by the model in this
/// local deliberation. It is not MTGO rules state or client metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgoPlayerVisibleAttackerInclusionDecisionV1 {
    pub current_state: MtgoPlayerVisibleDuelStateV1,
    pub ordered_candidate_attackers: Vec<MtgoPlayerVisibleObjectRefV1>,
    pub selected_before_current: Vec<MtgoPlayerVisibleObjectRefV1>,
    pub current_candidate_index: u32,
    pub current_candidate: MtgoPlayerVisibleObjectRefV1,
    pub remaining_after_current: Vec<MtgoPlayerVisibleObjectRefV1>,
    pub ordered_legal_actions: [MtgoPlayerVisibleDuelActionV1; 2],
}

impl MtgoPlayerVisibleAttackerInclusionDecisionV1 {
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

/// Coordinate-free difference between the visible selection at capture time
/// and the attack set chosen by the model. A later sealed execution layer must
/// reobserve and bind each entry to the exact current visible client action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MtgoPlayerVisibleAttackerToggleV1 {
    pub attacker: MtgoPlayerVisibleObjectRefV1,
    pub select_for_attack: bool,
}

/// Completed pure deliberation. This owns no client action, control, input
/// method, coordinate, process object, event entry, or spending authority.
#[derive(Debug, PartialEq, Eq)]
pub struct MtgoPlayerVisibleAttackerPlanV1 {
    desired_attackers: Vec<MtgoPlayerVisibleObjectRefV1>,
    required_toggles: Vec<MtgoPlayerVisibleAttackerToggleV1>,
}

impl MtgoPlayerVisibleAttackerPlanV1 {
    pub fn desired_attackers_v1(&self) -> &[MtgoPlayerVisibleObjectRefV1] {
        &self.desired_attackers
    }

    pub fn required_toggles_v1(&self) -> &[MtgoPlayerVisibleAttackerToggleV1] {
        &self.required_toggles
    }

    pub fn visible_done_control_required_v1(&self) -> bool {
        true
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

/// Move-only local scan state. Advancing consumes the prior state, so one
/// model response cannot be applied twice.
#[derive(Debug, PartialEq, Eq)]
pub struct MtgoPlayerVisibleAttackerDeliberationV1 {
    input: MtgoPlayerVisibleAttackerSelectionInputV1,
    cursor: usize,
    selected: Vec<MtgoPlayerVisibleObjectRefV1>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MtgoPlayerVisibleAttackerDeliberationProgressV1 {
    Pending(MtgoPlayerVisibleAttackerDeliberationV1),
    Complete(MtgoPlayerVisibleAttackerPlanV1),
}

impl MtgoPlayerVisibleAttackerDeliberationV1 {
    pub fn current_model_decision_v1(&self) -> MtgoPlayerVisibleAttackerInclusionDecisionV1 {
        let ordered_candidate_attackers = self
            .input
            .ordered_candidates
            .iter()
            .map(|candidate| candidate.attacker)
            .collect::<Vec<_>>();
        let current_candidate = ordered_candidate_attackers[self.cursor];
        MtgoPlayerVisibleAttackerInclusionDecisionV1 {
            current_state: self.input.current_state.clone(),
            ordered_candidate_attackers: ordered_candidate_attackers.clone(),
            selected_before_current: self.selected.clone(),
            current_candidate_index: self.cursor as u32,
            current_candidate,
            remaining_after_current: ordered_candidate_attackers[(self.cursor + 1)..].to_vec(),
            ordered_legal_actions: [
                MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                    attacker: current_candidate,
                    include: false,
                },
                MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                    attacker: current_candidate,
                    include: true,
                },
            ],
        }
    }

    pub fn advance_v1(
        mut self,
        selected_action_index: usize,
    ) -> Result<MtgoPlayerVisibleAttackerDeliberationProgressV1, MtgoContractErrorV1> {
        match selected_action_index {
            0 => {}
            1 => self
                .selected
                .push(self.input.ordered_candidates[self.cursor].attacker),
            _ => {
                return Err(error_v1(
                    "visible_attacker_model_action_index",
                    "attacker inclusion accepts only visible action index zero or one",
                ));
            }
        }
        self.cursor += 1;
        if self.cursor < self.input.ordered_candidates.len() {
            Ok(MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(
                self,
            ))
        } else {
            Ok(MtgoPlayerVisibleAttackerDeliberationProgressV1::Complete(
                finish_plan_v1(self.input, self.selected),
            ))
        }
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

pub fn begin_player_visible_attacker_deliberation_v1(
    input: MtgoPlayerVisibleAttackerSelectionInputV1,
) -> Result<MtgoPlayerVisibleAttackerDeliberationProgressV1, MtgoContractErrorV1> {
    validate_attacker_selection_input_v1(&input)?;
    if input.ordered_candidates.is_empty() {
        return Ok(MtgoPlayerVisibleAttackerDeliberationProgressV1::Complete(
            finish_plan_v1(input, Vec::new()),
        ));
    }
    Ok(MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(
        MtgoPlayerVisibleAttackerDeliberationV1 {
            input,
            cursor: 0,
            selected: Vec::new(),
        },
    ))
}

fn validate_attacker_selection_input_v1(
    input: &MtgoPlayerVisibleAttackerSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    validate_player_visible_duel_decision_input_strict_v1(&MtgoPlayerVisibleDuelDecisionInputV1 {
        current_state: input.current_state.clone(),
        ordered_legal_actions: Vec::new(),
    })?;
    let state = &input.current_state;
    if state.phase != ZoneIndependentStepV1::DeclareAttackers
        || state.acting_player != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || state.active_player != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || state.priority_player != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || state.combat.attackers_declared
        || state.combat.blockers_declared
        || !state.combat.blocker_assignments.is_empty()
        || !state.stack.is_empty()
    {
        return Err(error_v1(
            "visible_attacker_selection_state",
            "the first attacker slice requires the seated player's uncomplicated declare-attackers presentation",
        ));
    }
    if !input.unique_visible_enabled_done_control {
        return Err(error_v1(
            "visible_attacker_done_control",
            "attacker selection requires exactly one visible enabled completion control",
        ));
    }
    if input.ordered_candidates.len() > MAX_VISIBLE_ATTACKER_CANDIDATES_V1 {
        return Err(error_v1(
            "visible_attacker_candidate_count",
            "the visible attacker candidate count exceeds the supported bound",
        ));
    }

    let battlefield_positions = state.battlefield[0]
        .iter()
        .enumerate()
        .map(|(position, card)| (card.object_ref.visible_ordinal, position))
        .collect::<HashMap<_, _>>();
    let mut candidates = HashSet::new();
    let mut prior_position = None;
    for candidate in &input.ordered_candidates {
        let position = *battlefield_positions
            .get(&candidate.attacker.visible_ordinal)
            .ok_or_else(|| {
                error_v1(
                    "visible_attacker_candidate_zone",
                    "every attacker candidate must be a seated-player battlefield object",
                )
            })?;
        if !candidates.insert(candidate.attacker.visible_ordinal)
            || prior_position.is_some_and(|prior| position <= prior)
        {
            return Err(error_v1(
                "visible_attacker_candidate_order",
                "attacker candidates must be unique and preserve visible battlefield order",
            ));
        }
        prior_position = Some(position);
        let expected_action_shape = if candidate.currently_attacking {
            !candidate.attack_opponent_action_visible && candidate.dont_attack_action_visible
        } else {
            candidate.attack_opponent_action_visible && !candidate.dont_attack_action_visible
        };
        if !expected_action_shape {
            return Err(error_v1(
                "visible_attacker_toggle_shape",
                "each candidate must have exactly the visible toggle action opposite its current presentation state",
            ));
        }
    }

    let selected_from_candidates = input
        .ordered_candidates
        .iter()
        .filter(|candidate| candidate.currently_attacking)
        .map(|candidate| candidate.attacker)
        .collect::<Vec<_>>();
    if selected_from_candidates != state.combat.ordered_attackers {
        return Err(error_v1(
            "visible_attacker_current_selection",
            "the candidate toggle state must exactly match the visible ordered attack lane",
        ));
    }
    Ok(())
}

fn finish_plan_v1(
    input: MtgoPlayerVisibleAttackerSelectionInputV1,
    desired_attackers: Vec<MtgoPlayerVisibleObjectRefV1>,
) -> MtgoPlayerVisibleAttackerPlanV1 {
    let desired = desired_attackers
        .iter()
        .map(|attacker| attacker.visible_ordinal)
        .collect::<HashSet<_>>();
    let required_toggles = input
        .ordered_candidates
        .iter()
        .filter_map(|candidate| {
            let select_for_attack = desired.contains(&candidate.attacker.visible_ordinal);
            (select_for_attack != candidate.currently_attacking).then_some(
                MtgoPlayerVisibleAttackerToggleV1 {
                    attacker: candidate.attacker,
                    select_for_attack,
                },
            )
        })
        .collect();
    MtgoPlayerVisibleAttackerPlanV1 {
        desired_attackers,
        required_toggles,
    }
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MtgoPlayerVisibleBattlefieldCardV1, MtgoPlayerVisibleCombatStateV1,
        MtgoPlayerVisibleCounterStateV1,
    };

    fn object_ref_v1(visible_ordinal: u32) -> MtgoPlayerVisibleObjectRefV1 {
        MtgoPlayerVisibleObjectRefV1 { visible_ordinal }
    }

    fn battlefield_card_v1(
        visible_ordinal: u32,
        card_name: &str,
    ) -> MtgoPlayerVisibleBattlefieldCardV1 {
        MtgoPlayerVisibleBattlefieldCardV1 {
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
        }
    }

    fn input_v1(selected: &[u32]) -> MtgoPlayerVisibleAttackerSelectionInputV1 {
        let selected_set = selected.iter().copied().collect::<HashSet<_>>();
        let candidates = [0, 1]
            .into_iter()
            .map(|visible_ordinal| {
                let currently_attacking = selected_set.contains(&visible_ordinal);
                MtgoPlayerVisibleAttackerCandidateV1 {
                    attacker: object_ref_v1(visible_ordinal),
                    currently_attacking,
                    attack_opponent_action_visible: !currently_attacking,
                    dont_attack_action_visible: currently_attacking,
                }
            })
            .collect();
        MtgoPlayerVisibleAttackerSelectionInputV1 {
            current_state: MtgoPlayerVisibleDuelStateV1 {
                acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                turn: 3,
                phase: ZoneIndependentStepV1::DeclareAttackers,
                active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6], [0; 6]],
                hand_counts: [0, 0],
                library_counts: [50, 50],
                battlefield: [
                    vec![
                        battlefield_card_v1(0, "Grizzly Bears"),
                        battlefield_card_v1(1, "Runeclaw Bear"),
                    ],
                    Vec::new(),
                ],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: MtgoPlayerVisibleCombatStateV1 {
                    attackers_declared: false,
                    blockers_declared: false,
                    ordered_attackers: selected.iter().copied().map(object_ref_v1).collect(),
                    blocker_assignments: Vec::new(),
                },
                visible_object_relations: Vec::new(),
                own_hand: Vec::new(),
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            ordered_candidates: candidates,
            unique_visible_enabled_done_control: true,
        }
    }

    #[test]
    fn scans_false_then_true_and_builds_only_required_visible_toggles() {
        let progress = begin_player_visible_attacker_deliberation_v1(input_v1(&[1])).unwrap();
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(scan) = progress else {
            panic!("expected first attacker subdecision");
        };
        let decision = scan.current_model_decision_v1();
        assert_eq!(decision.current_candidate_index, 0);
        assert_eq!(decision.current_candidate, object_ref_v1(0));
        assert!(decision.selected_before_current.is_empty());
        assert_eq!(
            decision.ordered_legal_actions,
            [
                MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                    attacker: object_ref_v1(0),
                    include: false,
                },
                MtgoPlayerVisibleDuelActionV1::ChooseAttackerInclusion {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                    attacker: object_ref_v1(0),
                    include: true,
                },
            ]
        );

        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(scan) =
            scan.advance_v1(1).unwrap()
        else {
            panic!("expected second attacker subdecision");
        };
        let decision = scan.current_model_decision_v1();
        assert_eq!(decision.current_candidate_index, 1);
        assert_eq!(decision.selected_before_current, vec![object_ref_v1(0)]);
        assert_eq!(decision.remaining_after_current, Vec::new());

        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Complete(plan) =
            scan.advance_v1(0).unwrap()
        else {
            panic!("expected completed attacker plan");
        };
        assert_eq!(plan.desired_attackers_v1(), &[object_ref_v1(0)]);
        assert_eq!(
            plan.required_toggles_v1(),
            &[
                MtgoPlayerVisibleAttackerToggleV1 {
                    attacker: object_ref_v1(0),
                    select_for_attack: true,
                },
                MtgoPlayerVisibleAttackerToggleV1 {
                    attacker: object_ref_v1(1),
                    select_for_attack: false,
                },
            ]
        );
        assert!(plan.visible_done_control_required_v1());
        assert!(!plan.safe_for_live_input_v1());
        assert!(!plan.permits_event_entry_v1());
        assert!(!plan.permits_spending_v1());
    }

    #[test]
    fn rejects_an_out_of_range_model_choice() {
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(scan) =
            begin_player_visible_attacker_deliberation_v1(input_v1(&[])).unwrap()
        else {
            panic!("expected attacker subdecision");
        };
        assert_eq!(
            scan.advance_v1(2).unwrap_err().code(),
            "visible_attacker_model_action_index"
        );
    }

    #[test]
    fn zero_candidates_complete_without_model_scoring_but_still_require_done() {
        let mut input = input_v1(&[]);
        input.ordered_candidates.clear();
        input.current_state.battlefield[0].clear();
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Complete(plan) =
            begin_player_visible_attacker_deliberation_v1(input).unwrap()
        else {
            panic!("zero candidates should not produce a model subdecision");
        };
        assert!(plan.desired_attackers_v1().is_empty());
        assert!(plan.required_toggles_v1().is_empty());
        assert!(plan.visible_done_control_required_v1());
    }

    #[test]
    fn rejects_noncanonical_candidates_and_unqualified_visible_controls() {
        let mut cases = Vec::new();

        let mut wrong_phase = input_v1(&[]);
        wrong_phase.current_state.phase = ZoneIndependentStepV1::BeginCombat;
        cases.push((wrong_phase, "visible_attacker_selection_state"));

        let mut no_done = input_v1(&[]);
        no_done.unique_visible_enabled_done_control = false;
        cases.push((no_done, "visible_attacker_done_control"));

        let mut duplicate = input_v1(&[]);
        duplicate.ordered_candidates[1].attacker = object_ref_v1(0);
        cases.push((duplicate, "visible_attacker_candidate_order"));

        let mut reversed = input_v1(&[]);
        reversed.ordered_candidates.swap(0, 1);
        cases.push((reversed, "visible_attacker_candidate_order"));

        let mut absent = input_v1(&[]);
        absent.ordered_candidates[1].attacker = object_ref_v1(9);
        cases.push((absent, "visible_attacker_candidate_zone"));

        let mut bad_toggle = input_v1(&[]);
        bad_toggle.ordered_candidates[0].dont_attack_action_visible = true;
        cases.push((bad_toggle, "visible_attacker_toggle_shape"));

        let mut mismatched_lane = input_v1(&[1]);
        mismatched_lane
            .current_state
            .combat
            .ordered_attackers
            .clear();
        cases.push((mismatched_lane, "visible_attacker_current_selection"));

        for (input, expected_code) in cases {
            assert_eq!(
                begin_player_visible_attacker_deliberation_v1(input)
                    .unwrap_err()
                    .code(),
                expected_code
            );
        }
    }

    #[test]
    fn serde_rejects_private_attack_victim_identity() {
        let mut value = serde_json::to_value(input_v1(&[])).unwrap();
        value["ordered_candidates"][0]["attack_victim_id"] = serde_json::json!(42);
        assert!(
            serde_json::from_value::<MtgoPlayerVisibleAttackerSelectionInputV1>(value).is_err()
        );
    }
}
