use crate::{
    parse_and_validate_visible_duel_producer_result_v1, MtgoContractErrorV1,
    MtgoPlayerVisibleAttackerPlanV1, MtgoPlayerVisibleAttackerSelectionInputV1,
    MtgoPlayerVisibleObjectRefV1, MtgoVisibleDuelViewModelBrokerResultV1,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const ATTACKER_PLAN_COMMITMENT_DOMAIN_V1: &[u8] = b"mtgo-visible-attacker-execution-plan-v1";

/// A coordinate-free execution description bound to the exact sanitized
/// attacker-selection bytes that were deliberated. It exposes no command,
/// client object, process handle, input method, event entry, or spending path.
///
/// ```compile_fail
/// # use mtgo_blackbox_v1::MtgoPlayerVisibleAttackerExecutionPlanV1;
/// fn cannot_extract_input(value: &MtgoPlayerVisibleAttackerExecutionPlanV1) {
///     let _ = value.input_command();
/// }
/// ```
pub struct MtgoPlayerVisibleAttackerExecutionPlanV1 {
    source_selection: MtgoPlayerVisibleAttackerSelectionInputV1,
    source_selection_sha256: String,
    candidate_count: usize,
    desired_mask: u64,
    desired_mask_hex: String,
    plan_commitment_sha256: String,
}

impl MtgoPlayerVisibleAttackerExecutionPlanV1 {
    pub fn source_selection_sha256_v1(&self) -> &str {
        &self.source_selection_sha256
    }

    pub fn candidate_count_v1(&self) -> usize {
        self.candidate_count
    }

    pub fn desired_attacker_mask_hex_v1(&self) -> &str {
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
pub enum MtgoPlayerVisibleAttackerExecutionOperationV1 {
    Toggle {
        attacker: MtgoPlayerVisibleObjectRefV1,
        select_for_attack: bool,
    },
    Done,
}

/// One operation selected from a freshly parsed complete visible selection.
/// The Windows transaction owner must send it through the in-process sealed
/// producer and then acquire a new complete visible selection before asking
/// for another operation.
pub struct CheckedUntrustedMtgoPlayerVisibleAttackerExecutionStepV1 {
    current_selection_sha256: String,
    plan_commitment_sha256: String,
    operation: MtgoPlayerVisibleAttackerExecutionOperationV1,
}

impl CheckedUntrustedMtgoPlayerVisibleAttackerExecutionStepV1 {
    pub fn current_selection_sha256_v1(&self) -> &str {
        &self.current_selection_sha256
    }

    pub fn plan_commitment_sha256_v1(&self) -> &str {
        &self.plan_commitment_sha256
    }

    pub fn operation_v1(&self) -> MtgoPlayerVisibleAttackerExecutionOperationV1 {
        self.operation
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }
}

pub fn prepare_player_visible_attacker_execution_plan_v1(
    plan: MtgoPlayerVisibleAttackerPlanV1,
    exact_source_producer_result: &[u8],
) -> Result<MtgoPlayerVisibleAttackerExecutionPlanV1, MtgoContractErrorV1> {
    let source_selection = exact_attacker_selection_v1(exact_source_producer_result)?;
    if &source_selection != plan.source_selection_v1() {
        return Err(error_v1(
            "visible_attacker_execution_source",
            "execution planning must bind the exact sanitized selection used for model deliberation",
        ));
    }
    let candidate_count = source_selection.ordered_candidates.len();
    let candidate_positions = source_selection
        .ordered_candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.attacker.visible_ordinal, index))
        .collect::<std::collections::HashMap<_, _>>();
    let mut desired_mask = 0u64;
    let mut seen = HashSet::new();
    for attacker in plan.desired_attackers_v1() {
        let index = *candidate_positions
            .get(&attacker.visible_ordinal)
            .ok_or_else(|| {
                error_v1(
                    "visible_attacker_execution_desired_set",
                    "every desired attacker must belong to the exact source candidate set",
                )
            })?;
        if !seen.insert(attacker.visible_ordinal) {
            return Err(error_v1(
                "visible_attacker_execution_desired_set",
                "the desired visible attacker set must not contain duplicates",
            ));
        }
        desired_mask |= 1u64 << index;
    }
    let source_selection_sha256 = sha256_v1(exact_source_producer_result);
    let desired_mask_hex = format!("{desired_mask:016x}");
    let plan_commitment_sha256 = commitment_v1(
        ATTACKER_PLAN_COMMITMENT_DOMAIN_V1,
        &[
            source_selection_sha256.as_bytes(),
            candidate_count.to_string().as_bytes(),
            desired_mask_hex.as_bytes(),
        ],
    );
    Ok(MtgoPlayerVisibleAttackerExecutionPlanV1 {
        source_selection,
        source_selection_sha256,
        candidate_count,
        desired_mask,
        desired_mask_hex,
        plan_commitment_sha256,
    })
}

pub fn reobserve_player_visible_attacker_execution_step_v1(
    plan: &MtgoPlayerVisibleAttackerExecutionPlanV1,
    exact_current_producer_result: &[u8],
) -> Result<CheckedUntrustedMtgoPlayerVisibleAttackerExecutionStepV1, MtgoContractErrorV1> {
    let current = exact_attacker_selection_v1(exact_current_producer_result)?;
    validate_same_visible_attacker_universe_v1(&plan.source_selection, &current)?;
    let operation = current
        .ordered_candidates
        .iter()
        .enumerate()
        .find_map(|(index, candidate)| {
            let desired = ((plan.desired_mask >> index) & 1) != 0;
            (candidate.currently_attacking != desired).then_some(
                MtgoPlayerVisibleAttackerExecutionOperationV1::Toggle {
                    attacker: candidate.attacker,
                    select_for_attack: desired,
                },
            )
        })
        .unwrap_or(MtgoPlayerVisibleAttackerExecutionOperationV1::Done);
    Ok(CheckedUntrustedMtgoPlayerVisibleAttackerExecutionStepV1 {
        current_selection_sha256: sha256_v1(exact_current_producer_result),
        plan_commitment_sha256: plan.plan_commitment_sha256.clone(),
        operation,
    })
}

fn exact_attacker_selection_v1(
    exact_producer_result: &[u8],
) -> Result<MtgoPlayerVisibleAttackerSelectionInputV1, MtgoContractErrorV1> {
    match parse_and_validate_visible_duel_producer_result_v1(exact_producer_result)? {
        MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { selection } => {
            Ok(*selection)
        }
        _ => Err(error_v1(
            "visible_attacker_execution_result_kind",
            "attacker execution requires a complete sanitized attacker-selection result",
        )),
    }
}

fn validate_same_visible_attacker_universe_v1(
    source: &MtgoPlayerVisibleAttackerSelectionInputV1,
    current: &MtgoPlayerVisibleAttackerSelectionInputV1,
) -> Result<(), MtgoContractErrorV1> {
    if source.ordered_candidates.len() != current.ordered_candidates.len()
        || source
            .ordered_candidates
            .iter()
            .zip(&current.ordered_candidates)
            .any(|(left, right)| left.attacker != right.attacker)
    {
        return Err(error_v1(
            "visible_attacker_execution_candidate_universe",
            "candidate identities and visible order must remain exact throughout one attack plan",
        ));
    }
    let mut source_state = source.current_state.clone();
    let mut current_state = current.current_state.clone();
    source_state.combat.ordered_attackers.clear();
    current_state.combat.ordered_attackers.clear();
    if source_state != current_state {
        return Err(error_v1(
            "visible_attacker_execution_state_drift",
            "only the visible attacker selection may change while executing one attack plan",
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
        begin_player_visible_attacker_deliberation_v1, MtgoPlayerRelativeRoleV1,
        MtgoPlayerVisibleAttackerCandidateV1, MtgoPlayerVisibleAttackerDeliberationProgressV1,
        MtgoPlayerVisibleBattlefieldCardV1, MtgoPlayerVisibleCombatStateV1,
        MtgoPlayerVisibleCounterStateV1, MtgoPlayerVisibleDuelStateV1,
        MtgoVisibleDuelViewModelBrokerResultV1, ZoneIndependentStepV1,
    };

    fn object_ref_v1(visible_ordinal: u32) -> MtgoPlayerVisibleObjectRefV1 {
        MtgoPlayerVisibleObjectRefV1 { visible_ordinal }
    }

    fn selection_v1(selected: &[u32]) -> MtgoPlayerVisibleAttackerSelectionInputV1 {
        let selected = selected.iter().copied().collect::<HashSet<_>>();
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
                turn: 3,
                phase: ZoneIndependentStepV1::DeclareAttackers,
                active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
                initiative: None,
                life_totals: [20, 20],
                mana_pools: [[0; 6], [0; 6]],
                hand_counts: [0, 0],
                library_counts: [50, 50],
                battlefield: [[card(0, "Bear A"), card(1, "Bear B")].to_vec(), Vec::new()],
                graveyards: [Vec::new(), Vec::new()],
                exile: Vec::new(),
                stack: Vec::new(),
                combat: MtgoPlayerVisibleCombatStateV1 {
                    attackers_declared: false,
                    blockers_declared: false,
                    ordered_attackers: [0, 1]
                        .into_iter()
                        .filter(|value| selected.contains(value))
                        .map(object_ref_v1)
                        .collect(),
                    blocker_assignments: Vec::new(),
                },
                visible_object_relations: Vec::new(),
                own_hand: Vec::new(),
                known_library_cards: [Vec::new(), Vec::new()],
                known_hand_cards: [Vec::new(), Vec::new()],
            },
            ordered_candidates: [0, 1]
                .into_iter()
                .map(|visible_ordinal| {
                    let attacking = selected.contains(&visible_ordinal);
                    MtgoPlayerVisibleAttackerCandidateV1 {
                        attacker: object_ref_v1(visible_ordinal),
                        currently_attacking: attacking,
                        attack_opponent_action_visible: !attacking,
                        dont_attack_action_visible: attacking,
                    }
                })
                .collect(),
            unique_visible_enabled_done_control: true,
        }
    }

    fn bytes_v1(selection: MtgoPlayerVisibleAttackerSelectionInputV1) -> Vec<u8> {
        serde_json::to_vec(
            &MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection {
                selection: Box::new(selection),
            },
        )
        .unwrap()
    }

    fn execution_plan_v1() -> MtgoPlayerVisibleAttackerExecutionPlanV1 {
        let source = selection_v1(&[1]);
        let source_bytes = bytes_v1(source.clone());
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(scan) =
            begin_player_visible_attacker_deliberation_v1(source).unwrap()
        else {
            panic!("expected first attacker");
        };
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(scan) =
            scan.advance_v1(1).unwrap()
        else {
            panic!("expected second attacker");
        };
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Complete(plan) =
            scan.advance_v1(0).unwrap()
        else {
            panic!("expected complete plan");
        };
        prepare_player_visible_attacker_execution_plan_v1(plan, &source_bytes).unwrap()
    }

    #[test]
    fn exact_plan_binds_source_count_mask_and_commitment_without_authority() {
        let plan = execution_plan_v1();
        assert_eq!(plan.candidate_count_v1(), 2);
        assert_eq!(plan.desired_attacker_mask_hex_v1(), "0000000000000001");
        assert_eq!(plan.source_selection_sha256_v1().len(), 64);
        assert_eq!(plan.plan_commitment_sha256_v1().len(), 64);
        assert!(!plan.safe_for_live_input_v1());
        assert!(!plan.permits_event_entry_v1());
        assert!(!plan.permits_spending_v1());
    }

    #[test]
    fn fresh_visible_states_choose_each_toggle_then_done() {
        let plan = execution_plan_v1();
        let first = reobserve_player_visible_attacker_execution_step_v1(
            &plan,
            &bytes_v1(selection_v1(&[1])),
        )
        .unwrap();
        assert_eq!(
            first.operation_v1(),
            MtgoPlayerVisibleAttackerExecutionOperationV1::Toggle {
                attacker: object_ref_v1(0),
                select_for_attack: true,
            }
        );
        let second = reobserve_player_visible_attacker_execution_step_v1(
            &plan,
            &bytes_v1(selection_v1(&[0, 1])),
        )
        .unwrap();
        assert_eq!(
            second.operation_v1(),
            MtgoPlayerVisibleAttackerExecutionOperationV1::Toggle {
                attacker: object_ref_v1(1),
                select_for_attack: false,
            }
        );
        let done = reobserve_player_visible_attacker_execution_step_v1(
            &plan,
            &bytes_v1(selection_v1(&[0])),
        )
        .unwrap();
        assert_eq!(
            done.operation_v1(),
            MtgoPlayerVisibleAttackerExecutionOperationV1::Done
        );
        assert!(!done.safe_for_live_input_v1());
    }

    #[test]
    fn changed_candidate_identity_order_or_public_state_rejects() {
        let plan = execution_plan_v1();
        let mut changed = selection_v1(&[1]);
        changed.ordered_candidates.swap(0, 1);
        assert!(
            reobserve_player_visible_attacker_execution_step_v1(&plan, &bytes_v1(changed)).is_err()
        );

        let mut changed = selection_v1(&[1]);
        changed.current_state.life_totals[0] = 19;
        assert!(
            reobserve_player_visible_attacker_execution_step_v1(&plan, &bytes_v1(changed)).is_err()
        );
    }

    #[test]
    fn source_substitution_and_non_attacker_results_reject() {
        let source = selection_v1(&[1]);
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(scan) =
            begin_player_visible_attacker_deliberation_v1(source).unwrap()
        else {
            panic!("expected scan");
        };
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Pending(scan) =
            scan.advance_v1(1).unwrap()
        else {
            panic!("expected scan");
        };
        let MtgoPlayerVisibleAttackerDeliberationProgressV1::Complete(plan) =
            scan.advance_v1(0).unwrap()
        else {
            panic!("expected plan");
        };
        assert!(prepare_player_visible_attacker_execution_plan_v1(
            plan,
            &bytes_v1(selection_v1(&[]))
        )
        .is_err());
        assert!(reobserve_player_visible_attacker_execution_step_v1(
            &execution_plan_v1(),
            br#"{\"result_kind\":\"abstained\",\"reason\":\"projection_incomplete\"}"#,
        )
        .is_err());
    }
}
