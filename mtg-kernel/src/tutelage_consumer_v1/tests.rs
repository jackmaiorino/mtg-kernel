use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    effect::sphinxs_tutelage_v1_tests as native,
    engine::{self, Action as NativeAction, Decision as NativeDecision},
    event::CommittedEvent as E,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{GameState, Target},
};

fn card(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let mut mainboards = [vec![card("Island"); 40], vec![card("Island"); 40]];
    mainboards[actor.index()][..6].fill(PRINTING);
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_466,
        starting_player: actor.into(),
        deck_ids: ["tutelage-left".into(), "tutelage-right".into()],
        mainboards,
        max_physical_decisions: 2_000,
        max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Tutelage decision absent: {:?}", session.current_response());
    };
    session.encode(expected).unwrap()
}

fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: serial.to_string(),
        command,
    });
    let ReplyBodyV1::State { payload } = reply.body else {
        panic!("Tutelage wire request rejected: {reply:?}");
    };
    payload.body
}

#[test]
fn declared_forty_card_v3_reset_pays_tutelage_draw_and_mills_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::omniscience_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut invalid = config.clone();
        invalid.mainboards[actor.index()][0] = 465;
        assert!(reset_numeric_v1(&invalid).is_err());

        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(
            &mut wire,
            1,
            CommandV1::Reset {
                config: config.clone(),
            },
        ) else {
            panic!()
        };
        let State::Decision {
            decision: wire_opening,
            ..
        } = *state
        else {
            panic!()
        };
        assert_eq!(
            wire_opening.schema,
            "mtg-kernel-limited-tutelage-decision/v1"
        );
        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let opening = decision(&numeric);
        let expected = opening.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-tutelage-decision/v1");
        assert_eq!(prompt.actions, wire_opening.actions);

        let mut source = None;
        let mut activated = false;
        let mut discarded = false;
        let mut completed = false;
        for _ in 0..500 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation().tutelage.as_ref().unwrap();
            if discarded
                && facts
                    .draws
                    .iter()
                    .flat_map(|draw| &draw.observers)
                    .any(|observer| {
                        observer.stage == StageV1::MillEnded
                            && observer.batches.iter().any(|batch| {
                                batch.visible_milled_in_move_order.len() == 2 && !batch.repeated
                            })
                    })
            {
                let input = numeric.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture()
                    .forward(&input)
                    .unwrap()
                    .iter()
                    .all(|value| value.is_finite()));
                completed = true;
                break;
            }
            let rows = &backend.candidates_v75().candidates;
            let choose = rows
                .iter()
                .position(|row| {
                    matches!(
                        row.tutelage_action,
                        Some(
                            ActionV1::TargetOpponent { .. }
                                | ActionV1::OrderMilledCard { .. }
                                | ActionV1::DiscardAfterDraw
                        )
                    ) || !activated && row.tutelage_action == Some(ActionV1::ActivateDrawDiscard)
                        || source.is_none()
                            && matches!(&row.action, Action::CastSpell { source, .. }
                        if source.card_db_id == PRINTING)
                })
                .or_else(|| {
                    rows.iter()
                        .position(|row| matches!(row.action, Action::PlayLand { .. }))
                })
                .or_else(|| {
                    rows.iter().position(|row| {
                        matches!(
                            row.action,
                            Action::ChooseLondonMulligan {
                                mulligan: false,
                                ..
                            } | Action::ChooseAttackerInclusion { include: false, .. }
                                | Action::ChooseBlockerInclusion { include: false, .. }
                                | Action::ChooseTriggerOrderNext { .. }
                                | Action::OrderTriggers { .. }
                                | Action::Pass { .. }
                                | Action::Discard { .. }
                        )
                    })
                })
                .expect("missing native Tutelage fixture action");
            let before = native::taps(numeric.fixture_state(), actor);
            let row = &rows[choose];
            numeric.apply(&d, choose as u32).unwrap();
            match &row.action {
                Action::CastSpell { source: spell, .. } if spell.card_db_id == PRINTING => {
                    let id = ObjectId(spell.arena_id);
                    assert_eq!(numeric.fixture_state().objects.get(id).zone, Zone::Stack);
                    assert_eq!(native::taps(numeric.fixture_state(), actor) - before, 3);
                    source = Some(id);
                }
                _ if row.tutelage_action == Some(ActionV1::ActivateDrawDiscard) => {
                    assert_eq!(native::taps(numeric.fixture_state(), actor) - before, 6);
                    activated = true;
                }
                _ if row.tutelage_action == Some(ActionV1::DiscardAfterDraw) => discarded = true,
                _ => {}
            }
        }
        assert!(
            completed,
            "declared reset did not finish the paid draw and mill"
        );
    }
}

fn fixture(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state.clone(),
        LimitedAppendixV1::Tutelage,
    )
}

fn paid_order(actor: PlayerId) -> GameState {
    let mut state = native::ready(
        actor,
        &[
            "Elvish Mystic",
            "Llanowar Elves",
            "Think Twice",
            "Think Twice",
            "Forest",
            "Island",
        ],
    );
    let source = native::paid_tutelage(&mut state, actor);
    native::activate_to_discard(&mut state, actor, source);
    let facts = public::capture(&state, actor).unwrap();
    assert_eq!(facts.draws.len(), 1);
    assert!(facts.activation_discard.is_some());
    let discard = engine::advance_until_decision(&mut state);
    native::respond(&mut state, discard, false);
    for _ in 0..12 {
        let d = engine::advance_until_decision(&mut state);
        if matches!(d, NativeDecision::CastSpellOrPass { player, .. } if player == actor.opponent())
        {
            break;
        }
        native::respond(&mut state, d, false);
    }
    let wings = put(&mut state, actor.opponent(), "Broken Wings", Zone::Hand);
    let before = native::taps(&state, actor.opponent());
    engine::step(&mut state, NativeAction::CastSpell(wings)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(&mut state), NativeDecision::ChooseTargets {
        ref legal_targets, ..
    } if legal_targets.contains(&Target::Object(source)))
    );
    engine::step(
        &mut state,
        NativeAction::ChooseTarget(Target::Object(source)),
    )
    .unwrap();
    engine::advance_until_decision(&mut state);
    assert_eq!(native::taps(&state, actor.opponent()) - before, 3);
    native::to_mill_order(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    state
}

#[test]
fn paid_departed_observer_repeat_receipts_hidden_pair_indices_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let chooser = actor.opponent();
        let state = paid_order(actor);
        let actor_facts = public::capture(&state, actor).unwrap();
        assert!(actor_facts.current_order.is_none());
        assert_eq!(
            actor_facts.draws[0].observers[0].source.captured_controller,
            actor.into()
        );
        assert!(actor_facts.draws[0].observers[0]
            .source
            .current_original_source
            .is_none());
        let mut hidden_order = state.clone();
        let first = hidden_order.players[chooser.index()].library[0];
        hidden_order.objects.get_mut(first).card_def = card("Forest");
        assert_eq!(public::capture(&hidden_order, actor).unwrap(), actor_facts);
        assert_ne!(
            public::capture(&hidden_order, chooser).unwrap(),
            public::capture(&state, chooser).unwrap()
        );
        let mut future = state.clone();
        let later = *future.players[chooser.index()].library.last().unwrap();
        future.objects.get_mut(later).card_def = card("Swamp");

        let mut raw = fixture(&state);
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        let mut other = KaitoSession::from_session(fixture(&future)).unwrap();
        let mut human =
            HumanKaitoSessionV75::from_session(fixture(&state), chooser.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(chooser.into());
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(fixture(&state)).unwrap());
        let mut other_wire = JsonlServerV1::new();
        other_wire
            .0
            .install_fixture_core(KaitoSessionV75::from_session(fixture(&future)).unwrap());

        let d = decision(&numeric);
        assert_ne!(state.state_hash(), future.state_hash());
        assert_eq!(d.packet(), decision(&other).packet());
        assert!(other.training_input_v1(&d).is_err());
        let save = numeric.snapshot();
        assert!(other.restore(&save).is_err());
        let backend = d.fixture_backend_v75();
        let rows = &backend.candidates_v75().candidates;
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| matches!(
            row.tutelage_action,
            Some(ActionV1::OrderMilledCard {
                selected_count: 0,
                ..
            })
        )));
        let expected = backend.expected();
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_save = human.snapshot_v75();
        let current = body(&mut wire, 1, CommandV1::Current);
        assert_eq!(current, body(&mut other_wire, 1, CommandV1::Current));
        let Body::State { state: current, .. } = current else {
            panic!()
        };
        let State::Decision {
            episode_id,
            authority_generation,
            decision: visible,
        } = *current
        else {
            panic!()
        };
        let (projected, indices) =
            project_limited_kaito_v16(backend, expected.acting_player, visible.prompt_seq).unwrap();
        assert_eq!(*visible, projected);
        assert_eq!(prompt.state, visible.state);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
        let index = 1;
        let action_index = indices.iter().position(|&i| i == index).unwrap() as u32;
        let human_action = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index,
        };
        let step = CommandV1::Step {
            episode_id,
            authority_generation,
            actor: chooser.into(),
            prompt_seq: visible.prompt_seq,
            action_index,
        };
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = body(&mut wire, 2, CommandV1::Snapshot)
        else {
            panic!()
        };
        let original = backend.fixture_original_index_v75(index).unwrap();
        let response = raw
            .step(expected.episode_id, expected.step, original)
            .unwrap();
        assert_eq!(numeric.apply(&d, index).unwrap(), response);
        projector.submit(&mut human, human_action.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(
            body(&mut wire, 3, step.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());

        let mut legacy = numeric.fixture_state().clone();
        let batch = legacy
            .engine
            .event_history
            .iter_mut()
            .find(|entry| matches!(entry, E::SphinxsTutelageMillBatchV1 { .. }))
            .unwrap();
        let E::SphinxsTutelageMillBatchV1 { outcome, .. } = batch else {
            panic!()
        };
        outcome.0 = None;
        let value = serde_json::to_value(batch).unwrap();
        assert!(value["SphinxsTutelageMillBatchV1"].get("outcome").is_none());
        crate::effect::validate_pending_effect_choice(&legacy).unwrap();
        assert!(public::require_receipts(&legacy).is_err());
        assert!(crate::optional_trigger_public_v1::validate_state_checked(&legacy).is_err());

        for _ in 0..2 {
            let menu = decision(&numeric);
            assert!(menu
                .fixture_backend_v75()
                .candidates_v75()
                .candidates
                .iter()
                .all(|row| matches!(row.tutelage_action, Some(ActionV1::OrderMilledCard { .. }))));
            numeric.apply(&menu, 0).unwrap();
        }
        let after = decision(&numeric);
        let facts = after
            .fixture_backend_v75()
            .observation()
            .tutelage
            .as_ref()
            .unwrap();
        let observer = &facts.draws[0].observers[0];
        assert_eq!(observer.stage, StageV1::MillEnded);
        assert_eq!(observer.original_target, Some(chooser.into()));
        assert_eq!(
            observer
                .batches
                .iter()
                .map(|batch| batch.repeated)
                .collect::<Vec<_>>(),
            [true, true, false]
        );
        assert!(observer
            .batches
            .iter()
            .all(|batch| batch.visible_milled_in_move_order.len() == 2));

        numeric.restore(&save).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, index).is_err());
        assert!(projector.submit(&mut human, human_action).is_err());
        assert!(matches!(
            body(&mut wire, 4, CommandV1::Restore { snapshot_id }),
            Body::State { .. }
        ));
        assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    }
}
