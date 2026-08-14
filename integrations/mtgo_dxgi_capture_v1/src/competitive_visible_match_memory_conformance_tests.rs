use super::*;
use mtgo_blackbox_v1::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, VecDeque};

const DEPLOYMENT_V1: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TRANSCRIPT_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-ongoing-history-combat-conformance-transcript-v1";
const FIXTURE_JSON_V1: &str =
    include_str!("../fixtures/player_visible_ongoing_history_combat_conformance_source_v1.json");
const FIXTURE_FILE_SHA256_V1: &str =
    "ab56c215161770318d4fc1ccad1e92ce8c83311abb4a1e57970013c8cd8b809a";
const ORDINARY_AUXILIARY_FIXTURE_BYTES_V1: &[u8] = include_bytes!(
    "../fixtures/player_visible_competitive_auxiliary_heads_conformance_source_v1.json"
);
const ORDINARY_AUXILIARY_FIXTURE_SHA256_V1: &str =
    "960bee515dec946e399390f74b6ae6037af92e372fefc3a2441072244065eeca";

struct ScriptedAttackerScorerV1 {
    selected_indices: VecDeque<usize>,
}

impl MtgoPlayerVisibleAttackerScorerV1 for ScriptedAttackerScorerV1 {
    fn score_player_visible_attacker_inclusion_v1(
        &mut self,
        model_input: &MtgoPlayerVisibleAttackerInclusionDecisionV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
        let selected = self
            .selected_indices
            .pop_front()
            .ok_or_else(|| "missing scripted attacker score".to_owned())?;
        let mut logits = vec![0.0f32.to_bits(); model_input.ordered_legal_actions.len()];
        logits[selected] = 1.0f32.to_bits();
        Ok(MtgoPlayerVisibleDuelScoreResponseV1 {
            schema_version: MTGO_PLAYER_VISIBLE_DUEL_SCORING_SCHEMA_V1,
            ordered_action_logits_f32_bits: logits,
            value_f32_bits: 0.25f32.to_bits(),
        })
    }
}

impl MtgoPlayerVisibleSingleAttackerBlockerScorerV1 for ScriptedAttackerScorerV1 {
    fn score_player_visible_single_attacker_blocker_inclusion_v1(
        &mut self,
        _model_input: &MtgoPlayerVisibleSingleAttackerBlockerInclusionDecisionV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
        Err("single-attacker blocker scoring is outside this fixture".to_owned())
    }
}

impl MtgoPlayerVisibleMultiAttackerBlockerScorerV1 for ScriptedAttackerScorerV1 {
    fn score_player_visible_multi_attacker_blocker_v1(
        &mut self,
        _model_input: &MtgoPlayerVisibleMultiAttackerBlockerModelDecisionV1,
    ) -> Result<MtgoPlayerVisibleDuelScoreResponseV1, String> {
        Err("multi-attacker blocker scoring is outside this fixture".to_owned())
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
        active_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
        priority_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
        initiative: None,
        life_totals: [20, 20],
        mana_pools: [[0; 6], [0; 6]],
        hand_counts: [0, 0],
        library_counts: [49, 48],
        battlefield: [
            vec![card_v1(0, "Attacker A"), card_v1(1, "Attacker B")],
            Vec::new(),
        ],
        graveyards: [Vec::new(), Vec::new()],
        exile: Vec::new(),
        stack: Vec::new(),
        combat: MtgoPlayerVisibleCombatStateV1 {
            attackers_declared: !declaring_attackers,
            blockers_declared: false,
            ordered_attackers: Vec::new(),
            blocker_assignments: Vec::new(),
        },
        visible_object_relations: Vec::new(),
        own_hand: Vec::new(),
        known_library_cards: [Vec::new(), Vec::new()],
        known_hand_cards: [Vec::new(), Vec::new()],
    }
}

fn attacker_selection_bytes_v1() -> Vec<u8> {
    serde_json::to_vec(
        &MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection {
            selection: Box::new(MtgoPlayerVisibleAttackerSelectionInputV1 {
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
            }),
        },
    )
    .unwrap()
}

fn attacker_state_bytes_v1(attacking: &[u32]) -> Vec<u8> {
    let mut selection =
        match parse_and_validate_visible_duel_producer_result_v1(&attacker_selection_bytes_v1())
            .unwrap()
        {
            MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection { selection } => {
                *selection
            }
            _ => unreachable!(),
        };
    for candidate in &mut selection.ordered_candidates {
        candidate.currently_attacking = attacking.contains(&candidate.attacker.visible_ordinal);
        candidate.attack_opponent_action_visible = !candidate.currently_attacking;
        candidate.dont_attack_action_visible = candidate.currently_attacking;
    }
    selection.current_state.combat.ordered_attackers =
        attacking.iter().copied().map(object_v1).collect();
    serde_json::to_vec(
        &MtgoVisibleDuelViewModelBrokerResultV1::VisibleAttackerSelection {
            selection: Box::new(selection),
        },
    )
    .unwrap()
}

fn visible_state_bytes_v1(state: MtgoPlayerVisibleDuelStateV1) -> Vec<u8> {
    serde_json::to_vec(&MtgoVisibleDuelViewModelBrokerResultV1::VisibleDecision {
        decision: Box::new(MtgoPlayerVisibleDuelDecisionInputV1 {
            current_state: state,
            ordered_legal_actions: vec![MtgoPlayerVisibleDuelActionV1::Pass {
                actor: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            }],
        }),
    })
    .unwrap()
}

fn combat_history_v1() -> CheckedUntrustedMtgoCompetitivePlayerVisibleGameHistoryV1 {
    let initial = attacker_selection_bytes_v1();
    let mut scorer = ScriptedAttackerScorerV1 {
        selected_indices: [1usize, 0usize].into_iter().collect(),
    };
    let prepared = match score_and_prepare_strict_visible_combat_producer_result_v1(
        &initial,
        DEPLOYMENT_V1,
        &mut scorer,
    )
    .unwrap()
    {
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Prepared(prepared) => prepared,
        CheckedUntrustedMtgoPlayerVisibleCombatScoringOutcomeV1::Abstained { .. } => {
            panic!("expected prepared combat")
        }
    };
    let first = prepare_player_visible_combat_execution_step_v1(prepared, &initial).unwrap();
    let selected = attacker_state_bytes_v1(&[0]);
    let confirmed =
        confirm_player_visible_combat_execution_transition_v1(first, &selected).unwrap();
    let prepared = confirmed.into_prepared_continuation_v1().unwrap();
    let done = prepare_player_visible_combat_execution_step_v1(prepared, &selected).unwrap();
    let mut final_state = state_v1(ZoneIndependentStepV1::DeclareBlockers);
    final_state.combat.attackers_declared = true;
    final_state.combat.ordered_attackers = vec![object_v1(0)];
    let complete = confirm_player_visible_combat_execution_transition_v1(
        done,
        &visible_state_bytes_v1(final_state),
    )
    .unwrap();
    let confirmed = complete.into_confirmed_combat_decision_v1().unwrap();
    begin_checked_untrusted_competitive_player_visible_game_history_from_combat_v1(
        "ongoing-combat-conformance-v1",
        MtgoCompetitivePlayerVisibleCombatHistoryContextV1 {
            event_kind: MtgoCompetitiveEventKindV1::League,
            event_identity_sha256: "b".repeat(64),
            match_identity_sha256: "c".repeat(64),
            game_number: 1,
            policy_deployment_commitment_sha256: DEPLOYMENT_V1.to_owned(),
            source_frame_id: 11,
            source_frame_sequence: 101,
            after_frame_id: 12,
            after_frame_sequence: 102,
            visible_postcondition_commitment_sha256: "d".repeat(64),
        },
        confirmed,
    )
    .unwrap()
}

#[derive(Default)]
struct TranscriptConsumerV1 {
    header: Option<Value>,
    confirmed_decisions: Vec<Value>,
    public_game_log_events: Vec<Value>,
    callback_order: Vec<String>,
}

impl MtgoCompetitiveExternalPublicHistoryConsumerV1 for TranscriptConsumerV1 {
    type Output = Value;

    fn begin_public_history_v1(
        &mut self,
        header: MtgoCompetitiveExternalPublicHistoryHeaderV1,
    ) -> Result<(), String> {
        self.callback_order.push("begin_public_history".to_owned());
        self.header = Some(json!({
            "event_kind": header.event_kind,
            "game_number": header.game_number,
            "confirmed_decision_count": header.confirmed_decision_count,
            "public_event_count": header.public_event_count,
        }));
        Ok(())
    }

    fn consume_confirmed_decision_v1(
        &mut self,
        decision: MtgoCompetitiveExternalConfirmedDecisionV1,
    ) -> Result<(), String> {
        self.callback_order.push(format!(
            "consume_confirmed_decision:{}",
            decision.within_source_position_v1()
        ));
        self.confirmed_decisions.push(json!({
            "entry_kind": "ordinary_decision",
            "within_source_position": decision.within_source_position_v1(),
            "player_visible_decision": decision.player_visible_decision_v1(),
        }));
        Ok(())
    }

    fn consume_confirmed_combat_decision_v1(
        &mut self,
        decision: MtgoCompetitiveExternalConfirmedCombatDecisionV1,
    ) -> Result<(), String> {
        let model_decisions = decision
            .model_decisions_v1()
            .iter()
            .map(|model_decision| match model_decision {
                MtgoCompetitiveExternalCombatModelDecisionV1::AttackerInclusion {
                    model_input,
                    selected_index,
                    selected_action,
                } => json!({
                    "decision_kind": "attacker_inclusion",
                    "model_input": model_input,
                    "selected_index": selected_index,
                    "selected_action": selected_action,
                }),
                MtgoCompetitiveExternalCombatModelDecisionV1::SingleAttackerBlockerInclusion {
                    model_input,
                    selected_index,
                    selected_action,
                } => json!({
                    "decision_kind": "single_attacker_blocker_inclusion",
                    "model_input": model_input,
                    "selected_index": selected_index,
                    "selected_action": selected_action,
                }),
                MtgoCompetitiveExternalCombatModelDecisionV1::MultiAttackerBlockerChoice {
                    model_input,
                    selected_index,
                    selected_choice,
                } => json!({
                    "decision_kind": "multi_attacker_blocker_choice",
                    "model_input": model_input,
                    "selected_index": selected_index,
                    "selected_choice": selected_choice,
                }),
            })
            .collect::<Vec<_>>();
        self.callback_order.push(format!(
            "consume_confirmed_combat_decision:{}",
            decision.within_source_position_v1()
        ));
        self.confirmed_decisions.push(json!({
            "entry_kind": "combat_decision",
            "within_source_position": decision.within_source_position_v1(),
            "combat_kind": decision.combat_kind_v1(),
            "source_visible_state": decision.source_visible_state_v1(),
            "final_visible_state": decision.final_visible_state_v1(),
            "model_decisions": model_decisions,
        }));
        Ok(())
    }

    fn finish_confirmed_decision_stream_v1(&mut self) -> Result<(), String> {
        self.callback_order
            .push("finish_confirmed_decision_stream".to_owned());
        Ok(())
    }

    fn consume_public_game_log_event_v1(
        &mut self,
        event: MtgoCompetitiveExternalPublicGameLogEventV1<'_>,
    ) -> Result<(), String> {
        self.callback_order.push(format!(
            "consume_public_game_log_event:{}",
            event.within_source_position_v1()
        ));
        let visible_card_names = (0..event.visible_card_name_count_v1())
            .map(|index| {
                event
                    .visible_card_name_v1(index)
                    .ok_or_else(|| "visible card-name stream changed".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.public_game_log_events.push(json!({
            "within_source_position": event.within_source_position_v1(),
            "kind": event.kind_v1(),
            "actor_role": event.actor_role_v1(),
            "turn_number": event.turn_number_v1(),
            "primary_count": event.primary_count_v1(),
            "secondary_count": event.secondary_count_v1(),
            "visible_card_names": visible_card_names,
        }));
        Ok(())
    }

    fn finish_public_game_log_stream_v1(&mut self) -> Result<(), String> {
        self.callback_order
            .push("finish_public_game_log_stream".to_owned());
        Ok(())
    }

    fn finish_public_history_v1(&mut self) -> Result<Self::Output, String> {
        self.callback_order.push("finish_public_history".to_owned());
        Ok(json!({
            "ordering": MtgoCompetitiveExternalPublicHistoryOrderingV1::SeparateOrderedStreamsNoCrossSourceTotalOrder,
            "header": self.header.take().ok_or_else(|| "missing history header".to_owned())?,
            "confirmed_decision_stream": std::mem::take(&mut self.confirmed_decisions),
            "public_game_log_stream": std::mem::take(&mut self.public_game_log_events),
            "callback_order": std::mem::take(&mut self.callback_order),
        }))
    }
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

fn reject_forbidden_keys_v1(value: &Value) {
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                assert!(
                    !matches!(
                        key.as_str(),
                        "event_identity"
                            | "match_identity"
                            | "account"
                            | "capture"
                            | "frame_id"
                            | "frame_sequence"
                            | "rect"
                            | "card_db_id"
                            | "arena_id"
                            | "zone_change_count"
                            | "adapter_object_id"
                            | "client_object_id"
                            | "source_visible_text_sha256"
                            | "raw_game_log_text"
                            | "deployment_commitment_sha256"
                            | "decision_commitment_sha256"
                            | "confirmation_commitment_sha256"
                    ),
                    "forbidden history fixture field leaked: {key}"
                );
                reject_forbidden_keys_v1(child);
            }
        }
        Value::Array(values) => values.iter().for_each(reject_forbidden_keys_v1),
        _ => {}
    }
}

#[test]
fn ongoing_combat_history_projection_matches_cross_crate_fixture_v1() {
    let history = combat_history_v1();
    let mut consumer = TranscriptConsumerV1::default();
    consumer
        .begin_public_history_v1(MtgoCompetitiveExternalPublicHistoryHeaderV1 {
            event_kind: history.event_kind_v1(),
            game_number: history.game_number_v1(),
            confirmed_decision_count: history.decision_count_v1(),
            public_event_count: 0,
        })
        .unwrap();
    for index in 0..history.decision_count_v1() {
        consume_ongoing_history_entry_v1(&mut consumer, history.entry_v1(index).unwrap()).unwrap();
    }
    consumer.finish_confirmed_decision_stream_v1().unwrap();
    consumer.finish_public_game_log_stream_v1().unwrap();
    let transcript = consumer.finish_public_history_v1().unwrap();
    reject_forbidden_keys_v1(&transcript);

    let fixture: Value = serde_json::from_str(FIXTURE_JSON_V1).unwrap();
    let fixture_fields = fixture
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        fixture_fields,
        BTreeSet::from([
            "authority",
            "expected_transcript_commitment_sha256",
            "fixture_id",
            "information_boundary",
            "ordinary_auxiliary_fixture_sha256",
            "schema_version",
            "transcript",
        ])
    );
    assert_eq!(
        sha256_hex_v1(FIXTURE_JSON_V1.as_bytes()),
        FIXTURE_FILE_SHA256_V1
    );
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(
        fixture["fixture_id"],
        "player-visible-ongoing-history-combat-v1"
    );
    assert_eq!(
        fixture["information_boundary"],
        "rendered_mtgo_ui_or_rendered_game_log_player_visible_facts_only"
    );
    assert_eq!(
        sha256_hex_v1(ORDINARY_AUXILIARY_FIXTURE_BYTES_V1),
        ORDINARY_AUXILIARY_FIXTURE_SHA256_V1
    );
    assert_eq!(
        fixture["ordinary_auxiliary_fixture_sha256"],
        ORDINARY_AUXILIARY_FIXTURE_SHA256_V1
    );
    assert_eq!(fixture["transcript"], transcript);
    let canonical = serde_json::to_vec(&transcript).unwrap();
    let expected_commitment = commitment_v1(
        TRANSCRIPT_DOMAIN_V1,
        &[
            &canonical,
            ORDINARY_AUXILIARY_FIXTURE_SHA256_V1.as_bytes(),
            b"exact_combat_callback_projection_plus_existing_ordinary_auxiliary_fixture_no_authority",
        ],
    );
    assert_eq!(
        fixture["expected_transcript_commitment_sha256"],
        expected_commitment
    );
    let authority_fields = fixture["authority"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        authority_fields,
        BTreeSet::from([
            "permits_event_entry",
            "permits_spending",
            "safe_for_input",
            "safe_for_live_scoring",
        ])
    );
    for field in [
        "safe_for_live_scoring",
        "safe_for_input",
        "permits_event_entry",
        "permits_spending",
    ] {
        assert_eq!(
            fixture["authority"][field], false,
            "authority field {field}"
        );
    }
}

fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn ongoing_combat_history_fixture_rejects_semantic_or_hidden_field_mutation_v1() {
    let fixture: Value = serde_json::from_str(FIXTURE_JSON_V1).unwrap();
    let expected = fixture["expected_transcript_commitment_sha256"]
        .as_str()
        .unwrap();
    let mut changed_selection = fixture["transcript"].clone();
    changed_selection["confirmed_decision_stream"][0]["model_decisions"][0]["selected_index"] =
        json!(0);
    let canonical = serde_json::to_vec(&changed_selection).unwrap();
    assert_ne!(
        commitment_v1(
            TRANSCRIPT_DOMAIN_V1,
            &[
                &canonical,
                ORDINARY_AUXILIARY_FIXTURE_SHA256_V1.as_bytes(),
                b"exact_combat_callback_projection_plus_existing_ordinary_auxiliary_fixture_no_authority",
            ],
        ),
        expected
    );

    let mut hidden_field = fixture["transcript"].clone();
    hidden_field["confirmed_decision_stream"][0]["client_object_id"] = json!(123);
    assert!(std::panic::catch_unwind(|| reject_forbidden_keys_v1(&hidden_field)).is_err());
}
