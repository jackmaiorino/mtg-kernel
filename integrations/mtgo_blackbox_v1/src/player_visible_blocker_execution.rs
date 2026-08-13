use crate::{
    parse_and_validate_visible_duel_producer_result_v1, MtgoContractErrorV1,
    MtgoPlayerVisibleObjectRefV1, MtgoPlayerVisibleSingleAttackerBlockerPlanV1,
    MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1, MtgoVisibleDuelViewModelBrokerResultV1,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const SINGLE_BLOCKER_PLAN_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-visible-single-attacker-blocker-execution-plan-v1";

/// Coordinate-free monotonic execution description for a blocker plan that
/// began with an empty rendered block lane. It exposes no command, client
/// object, target identifier, process handle, event entry, or spending path.
pub struct MtgoPlayerVisibleSingleAttackerBlockerExecutionPlanV1 {
    source_selection: MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
    source_selection_sha256: String,
    candidate_count: usize,
    desired_mask: u64,
    desired_mask_hex: String,
    plan_commitment_sha256: String,
}

impl MtgoPlayerVisibleSingleAttackerBlockerExecutionPlanV1 {
    pub fn source_selection_sha256_v1(&self) -> &str {
        &self.source_selection_sha256
    }

    pub fn candidate_count_v1(&self) -> usize {
        self.candidate_count
    }

    pub fn desired_blocker_mask_hex_v1(&self) -> &str {
        &self.desired_mask_hex
    }

    pub fn plan_commitment_sha256_v1(&self) -> &str {
        &self.plan_commitment_sha256
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1 {
    AddBlocker {
        blocker: MtgoPlayerVisibleObjectRefV1,
    },
    Done,
}

/// One operation derived from a fresh complete player-visible blocker state.
/// The Windows transaction owner must rebuild those exact bytes before
/// dispatch and must observe the rendered assignment before another step.
pub struct CheckedUntrustedMtgoPlayerVisibleSingleAttackerBlockerExecutionStepV1 {
    current_selection_sha256: String,
    plan_commitment_sha256: String,
    operation: MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1,
}

impl CheckedUntrustedMtgoPlayerVisibleSingleAttackerBlockerExecutionStepV1 {
    pub fn current_selection_sha256_v1(&self) -> &str {
        &self.current_selection_sha256
    }

    pub fn plan_commitment_sha256_v1(&self) -> &str {
        &self.plan_commitment_sha256
    }

    pub fn operation_v1(&self) -> MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1 {
        self.operation
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

pub fn prepare_player_visible_single_attacker_blocker_execution_plan_v1(
    plan: MtgoPlayerVisibleSingleAttackerBlockerPlanV1,
    exact_source_producer_result: &[u8],
) -> Result<MtgoPlayerVisibleSingleAttackerBlockerExecutionPlanV1, MtgoContractErrorV1> {
    let source_selection = match parse_and_validate_visible_duel_producer_result_v1(
        exact_source_producer_result,
    )? {
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
            selection,
        } => *selection,
        _ => {
            return Err(error_v1(
                "visible_single_blocker_execution_source_kind",
                "execution planning requires the exact initial visible single-attacker blocker selection",
            ));
        }
    };
    if &source_selection != plan.source_selection_v1() {
        return Err(error_v1(
            "visible_single_blocker_execution_source",
            "execution planning must bind the exact sanitized selection used for model deliberation",
        ));
    }

    let candidate_count = source_selection.ordered_candidates.len();
    let positions = source_selection
        .ordered_candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.blocker.visible_ordinal, index))
        .collect::<HashMap<_, _>>();
    let mut desired_mask = 0u64;
    let mut seen = HashSet::new();
    for blocker in plan.desired_blockers_v1() {
        let index = *positions.get(&blocker.visible_ordinal).ok_or_else(|| {
            error_v1(
                "visible_single_blocker_execution_desired_set",
                "every desired blocker must belong to the exact source candidate set",
            )
        })?;
        if !seen.insert(blocker.visible_ordinal) {
            return Err(error_v1(
                "visible_single_blocker_execution_desired_set",
                "the desired visible blocker set must not contain duplicates",
            ));
        }
        desired_mask |= 1u64 << index;
    }

    let source_selection_sha256 = sha256_v1(exact_source_producer_result);
    let desired_mask_hex = format!("{desired_mask:016x}");
    let plan_commitment_sha256 = commitment_v1(
        SINGLE_BLOCKER_PLAN_COMMITMENT_DOMAIN_V1,
        &[
            source_selection_sha256.as_bytes(),
            candidate_count.to_string().as_bytes(),
            desired_mask_hex.as_bytes(),
        ],
    );
    Ok(MtgoPlayerVisibleSingleAttackerBlockerExecutionPlanV1 {
        source_selection,
        source_selection_sha256,
        candidate_count,
        desired_mask,
        desired_mask_hex,
        plan_commitment_sha256,
    })
}

pub fn reobserve_player_visible_single_attacker_blocker_execution_step_v1(
    plan: &MtgoPlayerVisibleSingleAttackerBlockerExecutionPlanV1,
    exact_current_producer_result: &[u8],
) -> Result<
    CheckedUntrustedMtgoPlayerVisibleSingleAttackerBlockerExecutionStepV1,
    MtgoContractErrorV1,
> {
    let current = match parse_and_validate_visible_duel_producer_result_v1(
        exact_current_producer_result,
    )? {
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
            selection,
        }
        | MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
            selection,
        } => *selection,
        _ => {
            return Err(error_v1(
                "visible_single_blocker_execution_result_kind",
                "blocker execution requires a complete sanitized single-attacker blocker state",
            ));
        }
    };
    validate_same_visible_single_blocker_universe_v1(&plan.source_selection, &current)?;

    let operation = current
        .ordered_candidates
        .iter()
        .enumerate()
        .find_map(|(index, candidate)| {
            let desired = ((plan.desired_mask >> index) & 1) != 0;
            if candidate.currently_blocking && !desired {
                return Some(Err(error_v1(
                    "visible_single_blocker_execution_nonmonotonic",
                    "an unexpected rendered blocker cannot be removed by the monotonic empty-lane execution slice",
                )));
            }
            if desired && !candidate.currently_blocking {
                return Some(if candidate.block_action_visible {
                    Ok(MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1::AddBlocker {
                        blocker: candidate.blocker,
                    })
                } else {
                    Err(error_v1(
                        "visible_single_blocker_execution_action_missing",
                        "the next desired unassigned blocker must still expose its visible Block action",
                    ))
                });
            }
            None
        })
        .transpose()?
        .unwrap_or(MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1::Done);

    Ok(
        CheckedUntrustedMtgoPlayerVisibleSingleAttackerBlockerExecutionStepV1 {
            current_selection_sha256: sha256_v1(exact_current_producer_result),
            plan_commitment_sha256: plan.plan_commitment_sha256.clone(),
            operation,
        },
    )
}

fn validate_same_visible_single_blocker_universe_v1(
    source: &MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
    current: &MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    if source.attacker != current.attacker
        || source.ordered_candidates.len() != current.ordered_candidates.len()
        || source
            .ordered_candidates
            .iter()
            .zip(&current.ordered_candidates)
            .any(|(left, right)| left.blocker != right.blocker)
    {
        return Err(error_v1(
            "visible_single_blocker_execution_candidate_universe",
            "the attacker plus blocker identities and visible order must remain exact throughout one plan",
        ));
    }
    let mut source_state = source.current_state.clone();
    let mut current_state = current.current_state.clone();
    source_state.combat.blocker_assignments.clear();
    current_state.combat.blocker_assignments.clear();
    if source_state != current_state {
        return Err(error_v1(
            "visible_single_blocker_execution_state_drift",
            "only the rendered blocker assignments may change while executing one single-attacker plan",
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

fn sha256_v1(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        begin_player_visible_single_attacker_blocker_deliberation_v1, MtgoPlayerRelativeRoleV1,
        MtgoPlayerVisibleBattlefieldCardV1, MtgoPlayerVisibleBlockerAssignmentV1,
        MtgoPlayerVisibleCombatStateV1, MtgoPlayerVisibleCounterStateV1,
        MtgoPlayerVisibleDuelStateV1, MtgoPlayerVisibleSingleAttackerBlockerCandidateV1,
        MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1, ZoneIndependentStepV1,
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

    fn selection_v1(blocking: &[u32]) -> MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1 {
        let blocking = blocking.iter().copied().collect::<HashSet<_>>();
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
                    vec![card_v1(0, "Blocker A"), card_v1(1, "Blocker B")],
                    vec![card_v1(2, "Attacker")],
                ],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: MtgoPlayerVisibleCombatStateV1 {
                    attackers_declared: true,
                    blockers_declared: false,
                    ordered_attackers: vec![attacker],
                    blocker_assignments: if blocking.is_empty() {
                        Vec::new()
                    } else {
                        vec![MtgoPlayerVisibleBlockerAssignmentV1 {
                            attacker,
                            ordered_blockers: [0, 1]
                                .into_iter()
                                .filter(|ordinal| blocking.contains(ordinal))
                                .map(object_ref_v1)
                                .collect(),
                        }]
                    },
                },
                visible_object_relations: Vec::new(),
                own_hand: Vec::new(),
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            attacker,
            ordered_candidates: [0, 1]
                .into_iter()
                .map(|visible_ordinal| {
                    let currently_blocking = blocking.contains(&visible_ordinal);
                    MtgoPlayerVisibleSingleAttackerBlockerCandidateV1 {
                        blocker: object_ref_v1(visible_ordinal),
                        currently_blocking,
                        block_action_visible: !currently_blocking,
                    }
                })
                .collect(),
            unique_visible_enabled_done_control: true,
        }
    }

    fn initial_bytes_v1() -> Vec<u8> {
        serde_json::to_vec(
            &MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
                selection: Box::new(selection_v1(&[])),
            },
        )
        .unwrap()
    }

    fn current_bytes_v1(blocking: &[u32]) -> Vec<u8> {
        serde_json::to_vec(
            &MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
                selection: Box::new(selection_v1(blocking)),
            },
        )
        .unwrap()
    }

    fn execution_plan_v1() -> MtgoPlayerVisibleSingleAttackerBlockerExecutionPlanV1 {
        let source = selection_v1(&[]);
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(scan) =
            begin_player_visible_single_attacker_blocker_deliberation_v1(source).unwrap()
        else {
            panic!("expected first blocker");
        };
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(scan) =
            scan.advance_v1(1).unwrap()
        else {
            panic!("expected second blocker");
        };
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Complete(plan) =
            scan.advance_v1(1).unwrap()
        else {
            panic!("expected complete plan");
        };
        prepare_player_visible_single_attacker_blocker_execution_plan_v1(plan, &initial_bytes_v1())
            .unwrap()
    }

    #[test]
    fn exact_plan_and_fresh_visible_states_choose_each_addition_then_done() {
        let plan = execution_plan_v1();
        assert_eq!(plan.candidate_count_v1(), 2);
        assert_eq!(plan.desired_blocker_mask_hex_v1(), "0000000000000003");
        assert_eq!(plan.source_selection_sha256_v1().len(), 64);
        assert_eq!(plan.plan_commitment_sha256_v1().len(), 64);
        assert!(!plan.safe_for_live_input_v1());
        assert!(!plan.permits_event_entry_v1());
        assert!(!plan.permits_spending_v1());

        let first = reobserve_player_visible_single_attacker_blocker_execution_step_v1(
            &plan,
            &initial_bytes_v1(),
        )
        .unwrap();
        assert_eq!(
            first.operation_v1(),
            MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1::AddBlocker {
                blocker: object_ref_v1(0),
            }
        );
        let second = reobserve_player_visible_single_attacker_blocker_execution_step_v1(
            &plan,
            &current_bytes_v1(&[0]),
        )
        .unwrap();
        assert_eq!(
            second.operation_v1(),
            MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1::AddBlocker {
                blocker: object_ref_v1(1),
            }
        );
        let done = reobserve_player_visible_single_attacker_blocker_execution_step_v1(
            &plan,
            &current_bytes_v1(&[0, 1]),
        )
        .unwrap();
        assert_eq!(
            done.operation_v1(),
            MtgoPlayerVisibleSingleAttackerBlockerExecutionOperationV1::Done
        );
        assert!(!done.safe_for_live_input_v1());
    }

    #[test]
    fn source_substitution_state_drift_and_unwanted_assignment_reject() {
        let plan = execution_plan_v1();
        let mut changed = selection_v1(&[0]);
        changed.current_state.life_totals[0] = 19;
        let changed = serde_json::to_vec(
            &MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
                selection: Box::new(changed),
            },
        )
        .unwrap();
        assert!(
            reobserve_player_visible_single_attacker_blocker_execution_step_v1(&plan, &changed)
                .is_err()
        );

        let source = selection_v1(&[]);
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(scan) =
            begin_player_visible_single_attacker_blocker_deliberation_v1(source).unwrap()
        else {
            panic!("expected first blocker");
        };
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Pending(scan) =
            scan.advance_v1(0).unwrap()
        else {
            panic!("expected second blocker");
        };
        let MtgoPlayerVisibleSingleAttackerBlockerDeliberationProgressV1::Complete(plan) =
            scan.advance_v1(1).unwrap()
        else {
            panic!("expected complete plan");
        };
        let one_desired = prepare_player_visible_single_attacker_blocker_execution_plan_v1(
            plan,
            &initial_bytes_v1(),
        )
        .unwrap();
        assert!(
            reobserve_player_visible_single_attacker_blocker_execution_step_v1(
                &one_desired,
                &current_bytes_v1(&[0]),
            )
            .is_err()
        );
    }
}
