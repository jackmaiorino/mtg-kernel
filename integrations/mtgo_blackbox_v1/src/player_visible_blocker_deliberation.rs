use crate::{
    validate_player_visible_duel_decision_input_strict_v1, MtgoContractErrorV1,
    MtgoPlayerRelativeRoleV1, MtgoPlayerVisibleDuelActionV1, MtgoPlayerVisibleDuelDecisionInputV1,
    MtgoPlayerVisibleDuelStateV1, MtgoPlayerVisibleObjectRefV1, ZoneIndependentStepV1,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_VISIBLE_BLOCKER_CANDIDATES_V1: usize = 64;

/// One blocker candidate for the first unambiguous MTGO blocking slice.
/// The slice admits exactly one visible attacking creature. MTGO's rendered
/// `Block` action therefore has only one possible combat destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleSingleAttackerBlockerCandidateV1 {
    pub blocker: MtgoPlayerVisibleObjectRefV1,
    pub currently_blocking: bool,
    pub block_action_visible: bool,
}

/// Complete visible single-attacker blocker-selection moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1 {
    pub current_state: MtgoPlayerVisibleDuelStateV1,
    pub attacker: MtgoPlayerVisibleObjectRefV1,
    pub ordered_candidates: Vec<MtgoPlayerVisibleSingleAttackerBlockerCandidateV1>,
    pub unique_visible_enabled_done_control: bool,
}

/// One model subdecision derived only from the visible state, visible
/// battlefield order, and choices already made by the model in this scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1 {
    pub current_state: MtgoPlayerVisibleDuelStateV1,
    pub attacker: MtgoPlayerVisibleObjectRefV1,
    pub ordered_candidate_blockers: Vec<MtgoPlayerVisibleObjectRefV1>,
    pub selected_before_current: Vec<MtgoPlayerVisibleObjectRefV1>,
    pub current_candidate_index: u32,
    pub current_candidate: MtgoPlayerVisibleObjectRefV1,
    pub remaining_after_current: Vec<MtgoPlayerVisibleObjectRefV1>,
    pub ordered_legal_actions: [MtgoPlayerVisibleDuelActionV1; 2],
}

impl MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1 {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MtgoPlayerVisibleSingleAttackerBlockerToggleV1 {
    pub blocker: MtgoPlayerVisibleObjectRefV1,
    pub select_for_block: bool,
}

/// Completed pure model deliberation. It contains no MTGO action object,
/// target identifier, input method, event-entry path, or spending authority.
#[derive(Debug, PartialEq, Eq)]
pub struct MtgoPlayerVisibleSingleAttackerBlockerPlanV1 {
    source_selection: MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
    desired_blockers: Vec<MtgoPlayerVisibleObjectRefV1>,
    required_toggles: Vec<MtgoPlayerVisibleSingleAttackerBlockerToggleV1>,
}

impl MtgoPlayerVisibleSingleAttackerBlockerPlanV1 {
    pub fn attacker_v1(&self) -> MtgoPlayerVisibleObjectRefV1 {
        self.source_selection.attacker
    }

    pub fn desired_blockers_v1(&self) -> &[MtgoPlayerVisibleObjectRefV1] {
        &self.desired_blockers
    }

    pub fn required_toggles_v1(&self) -> &[MtgoPlayerVisibleSingleAttackerBlockerToggleV1] {
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

/// Move-only local scan state. Advancing consumes the prior state.
#[derive(Debug, PartialEq, Eq)]
pub struct MtgoPlayerVisibleSingleAttackerBlockerDeliberationV1 {
    input: MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
    cursor: usize,
    selected: Vec<MtgoPlayerVisibleObjectRefV1>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1 {
    Pending(MtgoPlayerVisibleSingleAttackerBlockerDeliberationV1),
    Complete(MtgoPlayerVisibleSingleAttackerBlockerPlanV1),
}

impl MtgoPlayerVisibleSingleAttackerBlockerDeliberationV1 {
    pub fn current_model_decision_v1(
        &self,
    ) -> MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1 {
        let ordered_candidate_blockers = self
            .input
            .ordered_candidates
            .iter()
            .map(|candidate| candidate.blocker)
            .collect::<Vec<_>>();
        let current_candidate = ordered_candidate_blockers[self.cursor];
        MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1 {
            current_state: self.input.current_state.clone(),
            attacker: self.input.attacker,
            ordered_candidate_blockers: ordered_candidate_blockers.clone(),
            selected_before_current: self.selected.clone(),
            current_candidate_index: self.cursor as u32,
            current_candidate,
            remaining_after_current: ordered_candidate_blockers[(self.cursor + 1)..].to_vec(),
            ordered_legal_actions: [
                MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                    attacker: self.input.attacker,
                    blocker: current_candidate,
                    include: false,
                },
                MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion {
                    actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                    attacker: self.input.attacker,
                    blocker: current_candidate,
                    include: true,
                },
            ],
        }
    }

    pub fn advance_v1(
        mut self,
        selected_action_index: usize,
    ) -> Result<MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1, MtgoContractErrorV1>
    {
        match selected_action_index {
            0 => {}
            1 => self
                .selected
                .push(self.input.ordered_candidates[self.cursor].blocker),
            _ => {
                return Err(error_v1(
                    "visible_blocker_model_action_index",
                    "blocker inclusion accepts only visible action index zero or one",
                ));
            }
        }
        self.cursor += 1;
        if self.cursor < self.input.ordered_candidates.len() {
            Ok(MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(self))
        } else {
            Ok(
                MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Complete(
                    finish_plan_v1(self.input, self.selected),
                ),
            )
        }
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

pub fn begin_player_visible_single_attacker_blocker_deliberation_v1(
    input: MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
) -> Result<MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1, MtgoContractErrorV1> {
    validate_blocker_selection_input_v1(&input)?;
    if input.ordered_candidates.is_empty() {
        return Ok(
            MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Complete(finish_plan_v1(
                input,
                Vec::new(),
            )),
        );
    }
    Ok(
        MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(
            MtgoPlayerVisibleSingleAttackerBlockerDeliberationV1 {
                input,
                cursor: 0,
                selected: Vec::new(),
            },
        ),
    )
}

fn validate_blocker_selection_input_v1(
    input: &MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    validate_player_visible_duel_decision_input_strict_v1(&MtgoPlayerVisibleDuelDecisionInputV1 {
        current_state: input.current_state.clone(),
        ordered_legal_actions: Vec::new(),
    })?;
    let state = &input.current_state;
    if state.phase != ZoneIndependentStepV1::DeclareBlockers
        || state.acting_player != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || state.active_player != MtgoPlayerRelativeRoleV1::Opponent
        || state.priority_player != MtgoPlayerRelativeRoleV1::SeatedPlayer
        || !state.combat.attackers_declared
        || state.combat.blockers_declared
        || state.combat.ordered_attackers.as_slice() != [input.attacker]
        || !state.combat.blocker_assignments.is_empty()
        || !state.stack.is_empty()
    {
        return Err(error_v1(
            "visible_blocker_selection_state",
            "the first blocker slice requires one opposing visible attacker and the seated player's uncomplicated declare-blockers presentation",
        ));
    }
    if !state.battlefield[1]
        .iter()
        .any(|card| card.object_ref == input.attacker)
    {
        return Err(error_v1(
            "visible_blocker_attacker_zone",
            "the sole attacker must be an opponent battlefield object",
        ));
    }
    if !input.unique_visible_enabled_done_control {
        return Err(error_v1(
            "visible_blocker_done_control",
            "blocker selection requires exactly one visible enabled completion control",
        ));
    }
    if input.ordered_candidates.len() > MAX_VISIBLE_BLOCKER_CANDIDATES_V1 {
        return Err(error_v1(
            "visible_blocker_candidate_count",
            "the visible blocker candidate count exceeds the supported bound",
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
            .get(&candidate.blocker.visible_ordinal)
            .ok_or_else(|| {
                error_v1(
                    "visible_blocker_candidate_zone",
                    "every blocker candidate must be a seated-player battlefield object",
                )
            })?;
        if !candidates.insert(candidate.blocker.visible_ordinal)
            || prior_position.is_some_and(|prior| position <= prior)
        {
            return Err(error_v1(
                "visible_blocker_candidate_order",
                "blocker candidates must be unique and preserve visible battlefield order",
            ));
        }
        prior_position = Some(position);
        if !candidate.block_action_visible || candidate.currently_blocking {
            return Err(error_v1(
                "visible_blocker_candidate_shape",
                "the first blocker slice requires one visible Block action per candidate and an initially empty visible block lane",
            ));
        }
    }
    Ok(())
}

fn finish_plan_v1(
    input: MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
    desired_blockers: Vec<MtgoPlayerVisibleObjectRefV1>,
) -> MtgoPlayerVisibleSingleAttackerBlockerPlanV1 {
    let desired = desired_blockers
        .iter()
        .map(|blocker| blocker.visible_ordinal)
        .collect::<HashSet<_>>();
    let required_toggles = input
        .ordered_candidates
        .iter()
        .filter_map(|candidate| {
            let select_for_block = desired.contains(&candidate.blocker.visible_ordinal);
            (select_for_block != candidate.currently_blocking).then_some(
                MtgoPlayerVisibleSingleAttackerBlockerToggleV1 {
                    blocker: candidate.blocker,
                    select_for_block,
                },
            )
        })
        .collect();
    MtgoPlayerVisibleSingleAttackerBlockerPlanV1 {
        source_selection: input,
        desired_blockers,
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
        MtgoPlayerVisibleBattlefieldCardV1, MtgoPlayerVisibleBlockerAssignmentV1,
        MtgoPlayerVisibleCombatStateV1, MtgoPlayerVisibleCounterStateV1,
    };

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

    fn input_v1(_selected: &[u32]) -> MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1 {
        let attacker = object_ref_v1(2);
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
                battlefield: [
                    vec![card_v1(0, "Bear A"), card_v1(1, "Bear B")],
                    vec![card_v1(2, "Attacker")],
                ],
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
            ordered_candidates: [0, 1]
                .into_iter()
                .map(
                    |visible_ordinal| MtgoPlayerVisibleSingleAttackerBlockerCandidateV1 {
                        blocker: object_ref_v1(visible_ordinal),
                        currently_blocking: false,
                        block_action_visible: true,
                    },
                )
                .collect(),
            unique_visible_enabled_done_control: true,
        }
    }

    #[test]
    fn scans_false_then_true_and_builds_visible_blocker_plan() {
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(scan) =
            begin_player_visible_single_attacker_blocker_deliberation_v1(input_v1(&[])).unwrap()
        else {
            panic!("expected first blocker subdecision");
        };
        let decision = scan.current_model_decision_v1();
        assert_eq!(decision.current_candidate, object_ref_v1(0));
        assert!(matches!(
            decision.ordered_legal_actions[0],
            MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion { include: false, .. }
        ));
        assert!(matches!(
            decision.ordered_legal_actions[1],
            MtgoPlayerVisibleDuelActionV1::ChooseBlockerInclusion { include: true, .. }
        ));
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(scan) =
            scan.advance_v1(1).unwrap()
        else {
            panic!("expected second blocker subdecision");
        };
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Complete(plan) =
            scan.advance_v1(0).unwrap()
        else {
            panic!("expected completed blocker plan");
        };
        assert_eq!(plan.attacker_v1(), object_ref_v1(2));
        assert_eq!(plan.desired_blockers_v1(), &[object_ref_v1(0)]);
        assert_eq!(
            plan.required_toggles_v1(),
            &[MtgoPlayerVisibleSingleAttackerBlockerToggleV1 {
                blocker: object_ref_v1(0),
                select_for_block: true,
            }]
        );
        assert!(plan.visible_done_control_required_v1());
        assert!(!plan.safe_for_live_input_v1());
    }

    #[test]
    fn rejects_ambiguous_or_incomplete_visible_blocker_shapes() {
        let mut cases = Vec::new();
        let mut two_attackers = input_v1(&[]);
        two_attackers.current_state.battlefield[1].push(card_v1(3, "Second Attacker"));
        two_attackers
            .current_state
            .combat
            .ordered_attackers
            .push(object_ref_v1(3));
        cases.push((two_attackers, "visible_blocker_selection_state"));

        let mut missing_action = input_v1(&[]);
        missing_action.ordered_candidates[0].block_action_visible = false;
        cases.push((missing_action, "visible_blocker_candidate_shape"));

        let mut wrong_assignment = input_v1(&[]);
        wrong_assignment.current_state.combat.blocker_assignments =
            vec![MtgoPlayerVisibleBlockerAssignmentV1 {
                attacker: object_ref_v1(2),
                ordered_blockers: vec![object_ref_v1(1)],
            }];
        cases.push((wrong_assignment, "visible_blocker_selection_state"));

        let mut no_done = input_v1(&[]);
        no_done.unique_visible_enabled_done_control = false;
        cases.push((no_done, "visible_blocker_done_control"));

        for (input, code) in cases {
            assert_eq!(
                begin_player_visible_single_attacker_blocker_deliberation_v1(input)
                    .unwrap_err()
                    .code(),
                code
            );
        }
    }

    #[test]
    fn zero_candidates_complete_and_private_target_ids_are_unrepresentable() {
        let mut input = input_v1(&[]);
        input.ordered_candidates.clear();
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Complete(plan) =
            begin_player_visible_single_attacker_blocker_deliberation_v1(input).unwrap()
        else {
            panic!("zero candidates should complete");
        };
        assert!(plan.desired_blockers_v1().is_empty());

        let mut value = serde_json::to_value(input_v1(&[])).unwrap();
        value["ordered_candidates"][0]["target_id"] = serde_json::json!(77);
        assert!(
            serde_json::from_value::<MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1>(value)
                .is_err()
        );
    }
}
