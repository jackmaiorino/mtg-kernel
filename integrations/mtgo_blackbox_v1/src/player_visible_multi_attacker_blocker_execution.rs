use crate::{
    parse_and_validate_visible_duel_producer_result_v1,
    CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerChoiceV1, MtgoContractErrorV1,
    MtgoPlayerVisibleMultiAttackerBlockerChoiceV1, MtgoPlayerVisibleObjectRefV1,
    MtgoVisibleDuelViewModelBrokerResultV1,
};
use sha2::{Digest, Sha256};

const BLOCKER_STEP_COMMITMENT_DOMAIN_V1: &[u8] =
    b"mtgo-visible-multi-attacker-blocker-execution-step-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1 {
    FinishBlocking,
    ChooseBlocker {
        blocker: MtgoPlayerVisibleObjectRefV1,
    },
    ChooseAttackerForBlocker {
        blocker: MtgoPlayerVisibleObjectRefV1,
        attacker: MtgoPlayerVisibleObjectRefV1,
    },
}

/// A coordinate-free, one-step execution description bound to the exact
/// sanitized producer bytes and the checked model selection. The Windows
/// transaction owner must rebuild those same visible bytes before dispatch.
/// It exposes no client object, process handle, command, event-entry route, or
/// spending authority.
///
/// ```compile_fail
/// # use mtgo_blackbox_v1::CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerExecutionStepV1;
/// fn cannot_extract_client_input(value: &CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerExecutionStepV1) {
///     let _ = value.client_object();
/// }
/// ```
pub struct CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerExecutionStepV1 {
    source_selection_sha256: String,
    selected_index: usize,
    operation: MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1,
    model_selection_commitment_sha256: String,
    execution_step_commitment_sha256: String,
}

impl CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerExecutionStepV1 {
    pub fn source_selection_sha256_v1(&self) -> &str {
        &self.source_selection_sha256
    }

    pub fn selected_index_v1(&self) -> usize {
        self.selected_index
    }

    pub fn operation_v1(&self) -> MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1 {
        self.operation
    }

    pub fn model_selection_commitment_sha256_v1(&self) -> &str {
        &self.model_selection_commitment_sha256
    }

    pub fn execution_step_commitment_sha256_v1(&self) -> &str {
        &self.execution_step_commitment_sha256
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

pub fn prepare_player_visible_multi_attacker_blocker_execution_step_v1(
    checked_choice: &CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerChoiceV1,
    exact_source_producer_result: &[u8],
) -> Result<CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerExecutionStepV1, MtgoContractErrorV1>
{
    let parsed = parse_and_validate_visible_duel_producer_result_v1(exact_source_producer_result)?;
    let selected_index = checked_choice.selected_index_v1();
    let operation = match (parsed, checked_choice.selected_choice_v1()) {
        (
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
                selection,
            },
            MtgoPlayerVisibleMultiAttackerBlockerChoiceV1::FinishBlocking,
        ) if selected_index == 0 => {
            let _ = selection;
            MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::FinishBlocking
        }
        (
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
                selection,
            },
            MtgoPlayerVisibleMultiAttackerBlockerChoiceV1::ChooseBlocker { blocker },
        ) => {
            let expected = selected_index
                .checked_sub(1)
                .and_then(|index| selection.ordered_available_blockers.get(index).copied());
            if expected != Some(*blocker) {
                return Err(error_v1(
                    "visible_multi_blocker_execution_choice",
                    "the checked blocker choice must occupy the same exact visible source index",
                ));
            }
            MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::ChooseBlocker {
                blocker: *blocker,
            }
        }
        (
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection { selection },
            MtgoPlayerVisibleMultiAttackerBlockerChoiceV1::ChooseAttackerForBlocker {
                blocker,
                attacker,
            },
        ) => {
            if selection.blocker != *blocker
                || selection
                    .ordered_visible_targetable_attackers
                    .get(selected_index)
                    .copied()
                    != Some(*attacker)
            {
                return Err(error_v1(
                    "visible_multi_blocker_execution_target_choice",
                    "the checked blocker and attacker must occupy the same exact visible target source index",
                ));
            }
            MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::ChooseAttackerForBlocker {
                blocker: *blocker,
                attacker: *attacker,
            }
        }
        _ => {
            return Err(error_v1(
                "visible_multi_blocker_execution_stage",
                "the checked choice kind must match the exact visible blocker stage",
            ));
        }
    };

    let source_selection_sha256 = sha256_v1(exact_source_producer_result);
    let (kind, blocker, attacker) = operation_fields_v1(operation);
    let selected_index_text = selected_index.to_string();
    let execution_step_commitment_sha256 = commitment_v1(
        BLOCKER_STEP_COMMITMENT_DOMAIN_V1,
        &[
            source_selection_sha256.as_bytes(),
            selected_index_text.as_bytes(),
            kind.as_bytes(),
            blocker.as_bytes(),
            attacker.as_bytes(),
            checked_choice.selection_commitment_sha256_v1().as_bytes(),
        ],
    );
    Ok(
        CheckedUntrustedMtgoPlayerVisibleMultiAttackerBlockerExecutionStepV1 {
            source_selection_sha256,
            selected_index,
            operation,
            model_selection_commitment_sha256: checked_choice
                .selection_commitment_sha256_v1()
                .to_owned(),
            execution_step_commitment_sha256,
        },
    )
}

fn operation_fields_v1(
    operation: MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1,
) -> (String, String, String) {
    match operation {
        MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::FinishBlocking => {
            ("f".to_owned(), "-".to_owned(), "-".to_owned())
        }
        MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::ChooseBlocker { blocker } => (
            "b".to_owned(),
            blocker.visible_ordinal.to_string(),
            "-".to_owned(),
        ),
        MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::ChooseAttackerForBlocker {
            blocker,
            attacker,
        } => (
            "t".to_owned(),
            blocker.visible_ordinal.to_string(),
            attacker.visible_ordinal.to_string(),
        ),
    }
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
        score_and_select_player_visible_blocker_target_v1,
        score_and_select_player_visible_multi_attacker_blocker_v1, MtgoPlayerRelativeRoleV1,
        MtgoPlayerVisibleBattlefieldCardV1, MtgoPlayerVisibleBlockerAssignmentV1,
        MtgoPlayerVisibleBlockerTargetSelectionInputV1, MtgoPlayerVisibleCombatStateV1,
        MtgoPlayerVisibleCounterStateV1, MtgoPlayerVisibleDuelScoreResponseV1,
        MtgoPlayerVisibleDuelStateV1, MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1,
        MtgoPlayerVisibleMultiAttackerBlockerScorerV1,
        MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1, ZoneIndependentStepV1,
        MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
    };

    struct SelectIndexV1(usize);

    impl MtgoPlayerVisibleMultiAttackerBlockerScorerV1 for SelectIndexV1 {
        fn score_player_visible_multi_attacker_blocker_v1(
            &mut self,
            model_input: &MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1,
        ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
            let count = match model_input {
                MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1::SelectBlocker {
                    ordered_legal_choices,
                    ..
                }
                | MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1::SelectAttackerForBlocker {
                    ordered_legal_choices,
                    ..
                } => ordered_legal_choices.len(),
            };
            let mut logits = vec![0.0f32.to_bits(); count];
            logits[self.0] = 1.0f32.to_bits();
            Ok(MtgoPlayerVisibleDuelScoreResponseV1 {
                schema_version: MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
                ordered_action_logits_f32_bits: logits,
                value_f32_bits: 0.0f32.to_bits(),
            })
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

    fn state_v1() -> MtgoPlayerVisibleDuelStateV1 {
        MtgoPlayerVisibleDuelStateV1 {
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
                vec![card_v1(2, "Attacker A"), card_v1(3, "Attacker B")],
            ],
            graveyards: [Vec::new(), Vec::new()],
            exile: Vec::new(),
            stack: Vec::new(),
            combat: MtgoPlayerVisibleCombatStateV1 {
                attackers_declared: true,
                blockers_declared: false,
                ordered_attackers: vec![object_ref_v1(2), object_ref_v1(3)],
                blocker_assignments: Vec::<MtgoPlayerVisibleBlockerAssignmentV1>::new(),
            },
            visible_object_relations: Vec::new(),
            own_hand: Vec::new(),
            known_library_cards: [Vec::new(), Vec::new()],
            known_hand_cards: [Vec::new(), Vec::new()],
        }
    }

    fn blocker_selection_v1() -> MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1 {
        MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1 {
            current_state: state_v1(),
            ordered_available_blockers: vec![object_ref_v1(0), object_ref_v1(1)],
            unique_visible_enabled_done_control: true,
        }
    }

    fn target_selection_v1() -> MtgoPlayerVisibleBlockerTargetSelectionInputV1 {
        MtgoPlayerVisibleBlockerTargetSelectionInputV1 {
            current_state: state_v1(),
            blocker: object_ref_v1(0),
            ordered_visible_targetable_attackers: vec![object_ref_v1(2), object_ref_v1(3)],
        }
    }

    #[test]
    fn blocker_and_target_steps_bind_exact_visible_source_and_choice() {
        let blocker_input = blocker_selection_v1();
        let blocker_bytes = serde_json::to_vec(
            &MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
                selection: Box::new(blocker_input.clone()),
            },
        )
        .unwrap();
        let blocker_choice = score_and_select_player_visible_multi_attacker_blocker_v1(
            blocker_input,
            &"1".repeat(64),
            &mut SelectIndexV1(1),
        )
        .unwrap();
        let blocker_step = prepare_player_visible_multi_attacker_blocker_execution_step_v1(
            &blocker_choice,
            &blocker_bytes,
        )
        .unwrap();
        assert_eq!(
            blocker_step.operation_v1(),
            MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::ChooseBlocker {
                blocker: object_ref_v1(0),
            }
        );
        assert_eq!(blocker_step.execution_step_commitment_sha256_v1().len(), 64);
        assert!(!blocker_step.safe_for_live_input_v1());
        assert!(!blocker_step.permits_event_entry_v1());
        assert!(!blocker_step.permits_spending_v1());

        let target_input = target_selection_v1();
        let target_bytes = serde_json::to_vec(
            &MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection {
                selection: Box::new(target_input.clone()),
            },
        )
        .unwrap();
        let target_choice = score_and_select_player_visible_blocker_target_v1(
            target_input,
            &"2".repeat(64),
            &mut SelectIndexV1(1),
        )
        .unwrap();
        let target_step = prepare_player_visible_multi_attacker_blocker_execution_step_v1(
            &target_choice,
            &target_bytes,
        )
        .unwrap();
        assert_eq!(
            target_step.operation_v1(),
            MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::ChooseAttackerForBlocker {
                blocker: object_ref_v1(0),
                attacker: object_ref_v1(3),
            }
        );
        assert_ne!(
            blocker_step.execution_step_commitment_sha256_v1(),
            target_step.execution_step_commitment_sha256_v1()
        );
    }

    #[test]
    fn source_stage_index_and_bytes_substitution_reject() {
        let blocker_input = blocker_selection_v1();
        let blocker_choice = score_and_select_player_visible_multi_attacker_blocker_v1(
            blocker_input.clone(),
            &"3".repeat(64),
            &mut SelectIndexV1(1),
        )
        .unwrap();
        let target_bytes = serde_json::to_vec(
            &MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection {
                selection: Box::new(target_selection_v1()),
            },
        )
        .unwrap();
        assert!(
            prepare_player_visible_multi_attacker_blocker_execution_step_v1(
                &blocker_choice,
                &target_bytes,
            )
            .is_err()
        );

        let mut reordered = blocker_input;
        reordered.ordered_available_blockers.swap(0, 1);
        let reordered_bytes = serde_json::to_vec(
            &MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
                selection: Box::new(reordered),
            },
        )
        .unwrap();
        assert!(
            prepare_player_visible_multi_attacker_blocker_execution_step_v1(
                &blocker_choice,
                &reordered_bytes,
            )
            .is_err()
        );
    }
}
