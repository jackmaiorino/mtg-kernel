use mtgo_blackbox_v1::*;
use std::collections::VecDeque;

const DEPLOYMENT_V1: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct ScriptedCombatScorerV1 {
    attacker_indices: VecDeque<usize>,
    single_blocker_indices: VecDeque<usize>,
    multi_blocker_indices: VecDeque<usize>,
    calls: usize,
}

impl ScriptedCombatScorerV1 {
    fn new(attacker: &[usize], single_blocker: &[usize], multi_blocker: &[usize]) -> Self {
        Self {
            attacker_indices: attacker.iter().copied().collect(),
            single_blocker_indices: single_blocker.iter().copied().collect(),
            multi_blocker_indices: multi_blocker.iter().copied().collect(),
            calls: 0,
        }
    }

    fn response_v1(count: usize, selected: usize) -> MtgoPlayerVisibleDuelScoreResponseV1 {
        let mut logits = vec![0.0f32.to_bits(); count];
        logits[selected] = 1.0f32.to_bits();
        MtgoPlayerVisibleDuelScoreResponseV1 {
            schema_version: MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
            ordered_action_logits_f32_bits: logits,
            value_f32_bits: 0.25f32.to_bits(),
        }
    }
}

impl MtgoPlayerVisibleAttackerScorerV1 for ScriptedCombatScorerV1 {
    fn score_player_visible_attacker_inclusion_v1(
        &mut self,
        model_input: &MtgoPlayerVisibleAttackerInclusionDecisionV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
        self.calls += 1;
        assert_eq!(model_input.ordered_legal_actions.len(), 2);
        Ok(Self::response_v1(
            2,
            self.attacker_indices
                .pop_front()
                .ok_or_else(|| "missing attacker score".to_owned())?,
        ))
    }
}

impl MtgoPlayerVisibleSingleAttackerBlockerScorerV1 for ScriptedCombatScorerV1 {
    fn score_player_visible_single_attacker_blocker_inclusion_v1(
        &mut self,
        model_input: &MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
        self.calls += 1;
        assert_eq!(model_input.ordered_legal_actions.len(), 2);
        Ok(Self::response_v1(
            2,
            self.single_blocker_indices
                .pop_front()
                .ok_or_else(|| "missing single-blocker score".to_owned())?,
        ))
    }
}

impl MtgoPlayerVisibleMultiAttackerBlockerScorerV1 for ScriptedCombatScorerV1 {
    fn score_player_visible_multi_attacker_blocker_v1(
        &mut self,
        model_input: &MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
        self.calls += 1;
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
        Ok(Self::response_v1(
            count,
            self.multi_blocker_indices
                .pop_front()
                .ok_or_else(|| "missing multi-blocker score".to_owned())?,
        ))
    }
}

fn object_v1(visible_ordinal: u32) -> MtgoPlayerVisibleObjectRefV1 {
    MtgoPlayerVisibleObjectRefV1 { visible_ordinal }
}

fn card_v1(visible_ordinal: u32, card_name: &str) -> MtgoPlayerVisibleBattlefieldCardV1 {
    MtgoPlayerVisibleBattlefieldCardV1 {
        object_ref: object_v1(visible_ordinal),
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

fn state_v1(phase: ZoneIndependentStepV1) -> MtgoPlayerVisibleDuelStateV1 {
    let declaring_attackers = phase == ZoneIndependentStepV1::DeclareAttackers;
    MtgoPlayerVisibleDuelStateV1 {
        acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
        turn: 4,
        phase,
        active_player: if declaring_attackers {
            MtgoPlayerRelativeRoleV1::SeatedPlayer
        } else {
            MtgoPlayerRelativeRoleV1::Opponent
        },
        priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
        initiative: None,
        life_totals: [20, 20],
        mana_pools: [[0; 6], [0; 6]],
        hand_counts: [0, 0],
        library_counts: [49, 48],
        battlefield: [
            vec![card_v1(0, "Blocker A"), card_v1(1, "Blocker B")],
            vec![card_v1(2, "Attacker A"), card_v1(3, "Attacker B")],
        ],
        graveyards: [Vec::new(), Vec::new()],
        exile: Vec::new(),
        stack: Vec::new(),
        combat: MtgoPlayerVisibleCombatStateV1 {
            attackers_declared: !declaring_attackers,
            blockers_declared: false,
            ordered_attackers: if declaring_attackers {
                Vec::new()
            } else {
                vec![object_v1(2), object_v1(3)]
            },
            blocker_assignments: Vec::new(),
        },
        visible_object_relations: Vec::new(),
        own_hand: Vec::new(),
        known_library_cards: [Vec::new(), Vec::new()],
        known_hand_cards: [Vec::new(), Vec::new()],
    }
}

fn attacker_bytes_v1() -> Vec<u8> {
    let selection = MtgoPlayerVisibleAttackerSelectionInputV1 {
        current_state: state_v1(ZoneIndependentStepV1::DeclareAttackers),
        ordered_candidates: vec![
            MtgoPlayerVisibleAttackerCandidateV1 {
                attacker: object_v1(0),
                currently_attacking: false,
                attack_opponent_action_visible: true,
                dont_attack_action_visible: false,
            },
            MtgoPlayerVisibleAttackerCandidateV1 {
                attacker: object_v1(1),
                currently_attacking: false,
                attack_opponent_action_visible: true,
                dont_attack_action_visible: false,
            },
        ],
        unique_visible_enabled_done_control: true,
    };
    serde_json::to_vec(
        &MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection {
            selection: Box::new(selection),
        },
    )
    .unwrap()
}

fn single_blocker_selection_v1(
    blocking: &[u32],
) -> MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1 {
    let mut state = state_v1(ZoneIndependentStepV1::DeclareBlockers);
    state.combat.ordered_attackers = vec![object_v1(2)];
    if !blocking.is_empty() {
        state.combat.blocker_assignments = vec![MtgoPlayerVisibleBlockerAssignmentV1 {
            attacker: object_v1(2),
            ordered_blockers: blocking.iter().copied().map(object_v1).collect(),
        }];
    }
    MtgoPlayerVisibleSingleAttackerBlockerSelectionInputV1 {
        current_state: state,
        attacker: object_v1(2),
        ordered_candidates: [0, 1]
            .into_iter()
            .map(
                |visible_ordinal| MtgoPlayerVisibleSingleAttackerBlockerCandidateV1 {
                    blocker: object_v1(visible_ordinal),
                    currently_blocking: blocking.contains(&visible_ordinal),
                    block_action_visible: !blocking.contains(&visible_ordinal),
                },
            )
            .collect(),
        unique_visible_enabled_done_control: true,
    }
}

fn single_blocker_bytes_v1() -> Vec<u8> {
    serde_json::to_vec(
        &MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerSelection {
            selection: Box::new(single_blocker_selection_v1(&[])),
        },
    )
    .unwrap()
}

fn multi_blocker_bytes_v1() -> Vec<u8> {
    serde_json::to_vec(
        &MtgoVisibleDuelViewModelBrokerResultV1::VisibleMultiAttackerBlockerSelection {
            selection: Box::new(MtgoPlayerVisibleMultiAttackerBlockerSelectionInputV1 {
                current_state: state_v1(ZoneIndependentStepV1::DeclareBlockers),
                ordered_available_blockers: vec![object_v1(0), object_v1(1)],
                unique_visible_enabled_done_control: true,
            }),
        },
    )
    .unwrap()
}

fn blocker_target_bytes_v1() -> Vec<u8> {
    serde_json::to_vec(
        &MtgoVisibleDuelViewModelBrokerResultV1::VisibleBlockerTargetSelection {
            selection: Box::new(MtgoPlayerVisibleBlockerTargetSelectionInputV1 {
                current_state: state_v1(ZoneIndependentStepV1::DeclareBlockers),
                blocker: object_v1(0),
                ordered_visible_targetable_attackers: vec![object_v1(2), object_v1(3)],
            }),
        },
    )
    .unwrap()
}

fn prepared_v1(
    bytes: &[u8],
    scorer: &mut ScriptedCombatScorerV1,
) -> CheckedUntrustedMtgoPlayerVisiblePreparedCombatV1 {
    match score_and_prepare_strict_visible_combat_producer_result_v1(bytes, DEPLOYMENT_V1, scorer)
        .unwrap()
    {
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(prepared) => prepared,
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained { .. } => {
            panic!("expected prepared combat")
        }
    }
}

#[test]
fn attacker_scan_binds_every_model_choice_to_exact_bytes_and_plan() {
    let bytes = attacker_bytes_v1();
    let mut scorer = ScriptedCombatScorerV1::new(&[1, 0], &[], &[]);
    let prepared = prepared_v1(&bytes, &mut scorer);
    let plan = prepared.attacker_execution_plan_v1().unwrap();

    assert_eq!(
        prepared.kind_v1(),
        MtgoPlayerVisiblePreparedCombatKindV1::AttackerPlan
    );
    assert_eq!(prepared.model_selection_count_v1(), 2);
    assert_eq!(scorer.calls, 2);
    assert_eq!(plan.desired_attacker_mask_hex_v1(), "0000000000000001");
    assert_eq!(
        prepared.exact_producer_result_sha256_v1(),
        plan.source_selection_sha256_v1()
    );
    assert_eq!(
        prepared.execution_commitment_sha256_v1(),
        plan.plan_commitment_sha256_v1()
    );
    assert_eq!(prepared.bridge_commitment_sha256_v1().len(), 64);
    assert!(prepared
        .single_attacker_blocker_execution_plan_v1()
        .is_none());
    assert!(prepared
        .multi_attacker_blocker_execution_step_v1()
        .is_none());
    assert!(!prepared.safe_for_live_input_v1());
    assert!(!prepared.permits_event_entry_v1());
    assert!(!prepared.permits_spending_v1());
}

#[test]
fn single_attacker_blocker_scan_binds_every_choice_to_monotonic_plan() {
    let bytes = single_blocker_bytes_v1();
    let mut scorer = ScriptedCombatScorerV1::new(&[], &[1, 1], &[]);
    let prepared = prepared_v1(&bytes, &mut scorer);
    let plan = prepared
        .single_attacker_blocker_execution_plan_v1()
        .unwrap();

    assert_eq!(
        prepared.kind_v1(),
        MtgoPlayerVisiblePreparedCombatKindV1::SingleAttackerBlockerPlan
    );
    assert_eq!(prepared.model_selection_count_v1(), 2);
    assert_eq!(scorer.calls, 2);
    assert_eq!(plan.desired_blocker_mask_hex_v1(), "0000000000000003");
    assert_eq!(
        prepared.exact_producer_result_sha256_v1(),
        plan.source_selection_sha256_v1()
    );
    assert_eq!(
        prepared.execution_commitment_sha256_v1(),
        plan.plan_commitment_sha256_v1()
    );
}

#[test]
fn multi_attacker_blocker_and_target_each_prepare_one_exact_step() {
    let blocker_bytes = multi_blocker_bytes_v1();
    let mut blocker_scorer = ScriptedCombatScorerV1::new(&[], &[], &[1]);
    let blocker = prepared_v1(&blocker_bytes, &mut blocker_scorer);
    let blocker_step = blocker.multi_attacker_blocker_execution_step_v1().unwrap();
    assert_eq!(blocker.model_selection_count_v1(), 1);
    assert_eq!(
        blocker_step.operation_v1(),
        MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::ChooseBlocker {
            blocker: object_v1(0)
        }
    );

    let target_bytes = blocker_target_bytes_v1();
    let mut target_scorer = ScriptedCombatScorerV1::new(&[], &[], &[1]);
    let target = prepared_v1(&target_bytes, &mut target_scorer);
    let target_step = target.multi_attacker_blocker_execution_step_v1().unwrap();
    assert_eq!(target.model_selection_count_v1(), 1);
    assert_eq!(
        target_step.operation_v1(),
        MtgoPlayerVisibleMultiAttackerBlockerExecutionOperationV1::ChooseAttackerForBlocker {
            blocker: object_v1(0),
            attacker: object_v1(3),
        }
    );
    assert_ne!(
        blocker.bridge_commitment_sha256_v1(),
        target.bridge_commitment_sha256_v1()
    );
}

#[test]
fn exact_byte_changes_change_source_execution_and_bridge_commitments() {
    let bytes = attacker_bytes_v1();
    let mut with_trailing_space = bytes.clone();
    with_trailing_space.push(b' ');
    let first = prepared_v1(&bytes, &mut ScriptedCombatScorerV1::new(&[1, 0], &[], &[]));
    let second = prepared_v1(
        &with_trailing_space,
        &mut ScriptedCombatScorerV1::new(&[1, 0], &[], &[]),
    );
    assert_ne!(
        first.exact_producer_result_sha256_v1(),
        second.exact_producer_result_sha256_v1()
    );
    assert_ne!(
        first.execution_commitment_sha256_v1(),
        second.execution_commitment_sha256_v1()
    );
    assert_ne!(
        first.bridge_commitment_sha256_v1(),
        second.bridge_commitment_sha256_v1()
    );
}

#[test]
fn hidden_or_unknown_producer_fields_fail_before_the_model_is_called() {
    let mut value: serde_json::Value = serde_json::from_slice(&attacker_bytes_v1()).unwrap();
    value["selection"]["client_object_id"] = serde_json::json!(987654);
    let bytes = serde_json::to_vec(&value).unwrap();
    let mut scorer = ScriptedCombatScorerV1::new(&[1, 0], &[], &[]);
    let error = match score_and_prepare_strict_visible_combat_producer_result_v1(
        &bytes,
        DEPLOYMENT_V1,
        &mut scorer,
    ) {
        Ok(_) => panic!("hidden field must reject"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "visible_duel_producer_result_json");
    assert_eq!(scorer.calls, 0);
}

#[test]
fn abstention_never_calls_the_model_and_grants_no_prepared_execution() {
    let bytes = serde_json::to_vec(&MtgoVisibleDuelViewModelBrokerResultV1::Abstained {
        reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete,
    })
    .unwrap();
    let mut scorer = ScriptedCombatScorerV1::new(&[], &[], &[]);
    let outcome = score_and_prepare_strict_visible_combat_producer_result_v1(
        &bytes,
        DEPLOYMENT_V1,
        &mut scorer,
    )
    .unwrap();
    assert!(matches!(
        outcome,
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained {
            reason: MtgoVisibleDuelViewModelBrokerAbstentionReasonV1::ProjectionIncomplete
        }
    ));
    assert_eq!(scorer.calls, 0);
}

#[test]
fn mid_execution_state_cannot_rescore_or_begin_another_plan() {
    let bytes = serde_json::to_vec(
        &MtgoVisibleDuelViewModelBrokerResultV1::VisibleSingleAttackerBlockerExecutionState {
            selection: Box::new(single_blocker_selection_v1(&[0])),
        },
    )
    .unwrap();
    let mut scorer = ScriptedCombatScorerV1::new(&[], &[1, 1], &[]);
    let error = match score_and_prepare_strict_visible_combat_producer_result_v1(
        &bytes,
        DEPLOYMENT_V1,
        &mut scorer,
    ) {
        Ok(_) => panic!("mid-execution state must not rescore"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "visible_combat_scoring_execution_state");
    assert_eq!(scorer.calls, 0);
}

#[test]
fn invalid_deployment_commitment_rejects_before_parsing_or_scoring() {
    let mut scorer = ScriptedCombatScorerV1::new(&[1, 0], &[], &[]);
    let error = match score_and_prepare_strict_visible_combat_producer_result_v1(
        &attacker_bytes_v1(),
        "not-a-sha256",
        &mut scorer,
    ) {
        Ok(_) => panic!("invalid deployment must reject"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "visible_combat_scoring_deployment_commitment");
    assert_eq!(scorer.calls, 0);
}

#[test]
fn incomplete_sequential_model_scoring_returns_no_execution_plan() {
    let mut scorer = ScriptedCombatScorerV1::new(&[1], &[], &[]);
    let error = match score_and_prepare_strict_visible_combat_producer_result_v1(
        &attacker_bytes_v1(),
        DEPLOYMENT_V1,
        &mut scorer,
    ) {
        Ok(_) => panic!("an incomplete attacker scan must not produce a plan"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "visible_attacker_scorer_failed");
    assert_eq!(scorer.calls, 2);
}
