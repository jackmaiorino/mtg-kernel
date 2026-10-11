use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::rl_session::FastActorResponseV1;
use crate::state::{GameState, Zone};

fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(e) = session.current_response() else {
        panic!("registered fixture must reach a decision");
    };
    e
}

fn basic(actor: PlayerId, reversed: bool) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    let mut cards = vec![
        (actor, "Mountain", Zone::Hand),
        (actor, "Forest", Zone::Hand),
        (actor, "Mountain", Zone::Battlefield),
        (
            actor.opponent(),
            if reversed {
                "Fireblast"
            } else {
                "Flaring Pain"
            },
            Zone::Hand,
        ),
    ];
    if reversed {
        cards.reverse();
    }
    for (owner, name, zone) in cards {
        let id = put(&mut state, owner, name, zone);
        if reversed {
            state.objects.get_mut(id).zone_change_count = id.0 * 17 + 3;
        }
    }
    state
}

fn assert_safe(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                assert!(
                    !matches!(
                        key.as_str(),
                        "arena_id"
                            | "zone_change_count"
                            | "zone_change_generation"
                            | "visible_projection_hash"
                            | "physical_decision_id"
                            | "environment_revision"
                            | "episode_id"
                            | "seed"
                            | "timestamp"
                            | "binding"
                            | "stable_id"
                            | "engine_action_indexes"
                            | "logits"
                            | "value"
                    ),
                    "private key {key}"
                );
                assert_safe(value);
            }
        }
        serde_json::Value::Array(array) => {
            for value in array {
                assert_safe(value);
            }
        }
        _ => {}
    }
}

#[test]
fn ordinary_human_v8_preserves_legacy_choices_hidden_invariance_and_retry() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut bytes = Vec::new();
        for reversed in [false, true] {
            let mut legacy = FastActorSessionV1::from_v3_fixture_state(basic(actor, reversed));
            let mut session = legacy.clone().into_human_v8().unwrap();
            let e = expected(&session);
            let mut old = HumanDecisionProjectorV1::new_v4(actor.into());
            let old_prompt = old.project_current(&legacy, e).unwrap();
            let mut adapter = HumanDecisionProjectorV8::new(actor.into());
            let before = session.privileged_core_environment_hash();
            let prompt = adapter.project_current(&session, e).unwrap();
            assert_eq!(prompt, adapter.project_current(&session, e).unwrap());
            assert_eq!(session.privileged_core_environment_hash(), before);
            assert_eq!(prompt.actions, old_prompt.actions);
            assert_eq!(prompt.state.public, old_prompt.state.public);
            let safe = serde_json::to_value(&prompt).unwrap();
            assert_safe(&safe);
            let encoded = serde_json::to_vec(&prompt).unwrap();
            assert!(!String::from_utf8_lossy(&encoded).contains("Flaring Pain"));
            assert!(!String::from_utf8_lossy(&encoded).contains("Fireblast"));
            bytes.push(encoded);
            let action = prompt
                .actions
                .iter()
                .find(|a| a.label.starts_with("Play Mountain"))
                .unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: action.action_index,
            };
            let receipt = adapter.submit(&mut session, request.clone()).unwrap();
            assert_eq!(receipt, old.submit(&mut legacy, request.clone()).unwrap());
            let after = session.privileged_core_environment_hash();
            assert_eq!(after, legacy.privileged_core_environment_hash());
            assert_eq!(
                adapter.submit(&mut session, request.clone()).unwrap(),
                receipt
            );
            assert_eq!(
                adapter.submit(
                    &mut session,
                    HumanActionRequestV1 {
                        action_index: u32::MAX,
                        ..request
                    }
                ),
                Err(Error::ConflictingRetry)
            );
            assert_eq!(session.privileged_core_environment_hash(), after);
        }
        assert_eq!(bytes[0], bytes[1]);
    }
}

#[test]
fn human_v8_invalid_stale_and_wrong_seat_requests_are_read_only() {
    let mut session = FastActorSessionV1::from_human_v8_fixture_state(basic(PlayerId::P0, false));
    let e = expected(&session);
    let before = session.privileged_core_environment_hash();
    assert_eq!(
        HumanDecisionProjectorV8::new(PlayerSeatV1::P1).project_current(&session, e),
        Err(Error::NotHumanTurn)
    );
    let mut adapter = HumanDecisionProjectorV8::new(e.acting_player);
    let prompt = adapter.project_current(&session, e).unwrap();
    for request in [
        HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: u32::MAX,
        },
        HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq + 1,
            action_index: 0,
        },
    ] {
        assert!(adapter.submit(&mut session, request).is_err());
    }
    let mut stale = e;
    stale.environment_revision += 1;
    assert_eq!(
        adapter.project_current(&session, stale),
        Err(Error::StaleDecision)
    );
    assert_eq!(session.privileged_core_environment_hash(), before);
    session.step(e.episode_id, e.step, 0).unwrap();
    let changed = session.privileged_core_environment_hash();
    assert_eq!(
        adapter.submit(
            &mut session,
            HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: 0
            }
        ),
        Err(Error::StaleDecision)
    );
    assert_eq!(session.privileged_core_environment_hash(), changed);
}

#[test]
#[cfg(feature = "standard-magezero-fixtures")]
fn registered_printed_life_traits_are_safe_and_arena_invariant() {
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::state::DayNightV1;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut prompts = Vec::new();
        for reversed in [false, true] {
            let mut state = basic(actor, reversed);
            state.day_night_v1 = Some(DayNightV1::Night);
            let mut sources = Vec::new();
            for _ in 0..2 {
                sources.push(put(
                    &mut state,
                    actor.opponent(),
                    "Brutal Cathar",
                    Zone::Battlefield,
                ));
            }
            for source in sources {
                propose_and_commit(&mut state, ProposedEvent::transform_in_place(source, 1));
            }
            let hidden = put(
                &mut state,
                actor.opponent(),
                if reversed { "Forest" } else { "Brutal Cathar" },
                Zone::Hand,
            );
            state.objects.get_mut(hidden).zone_change_count = if reversed { 83 } else { 0 };
            let session = FastActorSessionV1::from_human_v8_fixture_state(state);
            let prompt = HumanDecisionProjectorV8::new(actor.into())
                .project_current(&session, expected(&session))
                .unwrap();
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            let traits = &prompt.state.extensions.ward_life_card_traits;
            assert_eq!(traits.len(), 2);
            assert!(traits.iter().all(|row| row.life == 3
                && row.effective
                && row.printed_face == WardLifePrintedFaceV8::Back));
            assert_ne!(traits[0].source.handle, traits[1].source.handle);
            assert!(prompt.state.extensions.pending_ward_life_payment.is_none());
            prompts.push(serde_json::to_vec(&prompt).unwrap());
        }
        assert_eq!(prompts[0], prompts[1]);
    }
}

// Synthetic typed projection fixture only. The registered Generic Ward
// state supplies authenticated visible references and public stack ordinals;
// replacing its transport fee tests safe DTO/labels, never life execution.
fn synthetic_life_input() -> (ObservationV8, Vec<ActionSemanticV1>) {
    let (mut state, _, _) = crate::policy_observation_v6::tests::ward_multi_targeter_state();
    crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
    let session = FastActorSessionV1::from_human_v8_fixture_state(state);
    let e = expected(&session);
    let (mut observation, actions, _) = session
        .human_current_decision_input_v8(e, e.acting_player)
        .unwrap();
    let generic = observation.extensions.pending_ward_payment.take().unwrap();
    observation.extensions.pending_ward_life_payment = Some(WardLifePaymentV8 {
        targeting_stack_index: generic.targeting_stack_index,
        payer: generic.payer,
        life: 7,
    });
    (observation, actions)
}

#[test]
fn synthetic_life_projection_labels_complete_menu_and_refuses_wrong_context() {
    let (observation, mut actions) = synthetic_life_input();
    let human = observation.acting_player;
    let (prompt, _) = project_decision(&observation, &actions, human, 1).unwrap();
    assert_safe(&serde_json::to_value(&prompt).unwrap());
    assert_eq!(prompt.actions.len(), 2);
    assert!(prompt
        .actions
        .iter()
        .any(|row| row.label.starts_with("Pay 7 life")));
    assert!(prompt
        .actions
        .iter()
        .any(|row| row.label.starts_with("Decline to pay 7 life")));
    let payment = prompt
        .state
        .extensions
        .pending_ward_life_payment
        .as_ref()
        .unwrap();
    assert_eq!(
        (payment.payer, payment.payer_life, payment.life),
        (human, 20, 7)
    );
    let mut wrong = observation.clone();
    wrong
        .extensions
        .pending_ward_life_payment
        .as_mut()
        .unwrap()
        .payer = PlayerSeatV1::P1;
    assert!(project_decision(&wrong, &actions, human, 1).is_err());
    wrong = observation.clone();
    wrong
        .extensions
        .pending_ward_life_payment
        .as_mut()
        .unwrap()
        .targeting_stack_index = u32::MAX;
    assert!(project_decision(&wrong, &actions, human, 1).is_err());
    actions.push(ActionSemanticV1::Pass { actor: human });
    assert_eq!(
        project_decision(&observation, &actions, human, 1),
        Err(Error::UnsupportedPrompt)
    );
}

#[test]
fn synthetic_different_traits_on_identical_cards_canonicalize_without_arena_order() {
    // Pure typed DTO test: these synthetic traits are not a card definition
    // or gameplay admission. They isolate scalar traits as a canonical key.
    let mut prompts = Vec::new();
    for reversed in [false, true] {
        let mut state = ready_state();
        put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
        let mut sources = Vec::new();
        for life in if reversed { [7, 3] } else { [3, 7] } {
            sources.push((
                life,
                put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield),
            ));
        }
        let session = FastActorSessionV1::from_human_v8_fixture_state(state);
        let e = expected(&session);
        let (mut observation, actions, _) = session
            .human_current_decision_input_v8(e, e.acting_player)
            .unwrap();
        for (life, source) in sources {
            let stable = observation.projection.surface.battlefield[0]
                .iter()
                .find(|row| row.stable.arena_id == source.0)
                .unwrap()
                .stable
                .clone();
            observation.extensions.ward_life_card_traits.push(
                crate::policy_observation_v8::WardLifeCardTraitV8 {
                    source: stable,
                    life,
                    printed_face: WardLifePrintedFaceV8::Front,
                    effective: true,
                },
            );
        }
        let (prompt, _) = project_decision(&observation, &actions, e.acting_player, 1).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let traits = &prompt.state.extensions.ward_life_card_traits;
        assert_eq!(traits.len(), 2);
        assert_ne!(traits[0].source.handle, traits[1].source.handle);
        prompts.push(serde_json::to_vec(&prompt).unwrap());
    }
    assert_eq!(prompts[0], prompts[1]);
}
