use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    effect::thousand_year_storm_v1_tests as native,
    engine,
    event::{self, CommittedEvent, ProposedEvent},
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_surface_v5::LimitedAppendixV1,
    rl::TargetRefV1,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{GameState, Target},
};
fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}
fn config(actor: PlayerId) -> ResetV1 {
    let mut mainboards = [vec![id("Island"); 40], vec![id("Island"); 40]];
    mainboards[actor.index()][..4].fill(PRINTING);
    mainboards[actor.index()][4..22].fill(id("Mountain"));
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_485,
        starting_player: actor.into(),
        deck_ids: ["storm-left".into(), "storm-right".into()],
        mainboards,
        max_physical_decisions: 2000,
        max_policy_steps: 2000,
    }
}
fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Storm decision absent: {:?}", session.current_response())
    };
    session.encode(expected).unwrap()
}
fn raw_owner(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state.clone(),
        LimitedAppendixV1::ThousandYearStorm,
    )
}

fn sampled_printings(state: &GameState, replacements: &[(ObjectId, u16)]) -> GameState {
    let mut sampled: GameState =
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(&sampled, state);
    // Qualify the original native root before attaching the same ephemeral
    // custody used by the sealed sampler. Raw restores cannot recreate it.
    KaitoSessionV75::from_session(raw_owner(&sampled))
        .unwrap()
        .decision_v75()
        .unwrap();
    let custody = crate::sampled_history_v1::capture(&sampled).unwrap();
    for &(object, card_def) in replacements {
        let live = sampled.objects.get(object);
        assert_eq!(live.zone, Zone::Library);
        for viewer in [PlayerId::P0, PlayerId::P1] {
            assert!(sampled
                .known_library_cards(viewer, live.owner)
                .iter()
                .all(|row| row.object != object));
        }
        let live = sampled.objects.get_mut(object);
        live.card_def = card_def;
        live.name = crate::card_def::CARD_DEFS[usize::from(card_def)]
            .name
            .into();
        live.v4 = crate::state::ObjectStateV4::from_card_def(card_def);
    }
    assert!(crate::controlled_creature_grants_v1::validate_color_history(&sampled).is_err());
    sampled.sampled_history_v1 = custody;
    crate::controlled_creature_grants_v1::validate_color_history(&sampled).unwrap();
    sampled
}
fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(),
        request_id: serial.to_string(),
        command,
    });
    let ReplyBodyV1::State { payload } = reply.body else {
        panic!("Storm wire refused: {reply:?}")
    };
    payload.body
}
#[test]
fn declared_forty_card_v3_reset_pays_six_for_exact_storm_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut bad = config.clone();
        bad.mainboards[actor.index()][0] = 472;
        assert!(reset_numeric_v1(&bad).is_err());
        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().thousand_year_storm_consumer_v1);
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        assert!(
            crate::effect::thousand_year_storm_v1::refuse_legacy(numeric.fixture_state()).is_err()
        );
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
            decision: opening, ..
        } = *state
        else {
            panic!()
        };
        assert_eq!(
            opening.schema,
            "mtg-kernel-limited-thousand-year-storm-decision/v1"
        );
        let expected = decision(&numeric).fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(
            prompt.schema,
            "mtg-kernel-human-thousand-year-storm-decision/v1"
        );
        assert_eq!(prompt.actions, opening.actions);
        let mut cast = None;
        let mut entered = false;
        for _ in 0..800 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation().thousand_year_storm.as_ref().unwrap();
            if let Some(source) = facts.sources.first() {
                assert_eq!(Some(source.source.object), cast);
                assert_eq!(facts.current_turn_casts, [0, 0]);
                let input = numeric.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture()
                    .forward(&input)
                    .unwrap()
                    .iter()
                    .all(|v| v.is_finite()));
                entered = true;
                break;
            }
            let rows = &backend.candidates_v75().candidates;
            let pick=rows.iter().position(|r|matches!(&r.action,Action::CastSpell{source,..} if source.card_db_id==PRINTING&&cast.is_none()))
                .or_else(||rows.iter().position(|r|matches!(r.action,Action::PlayLand{..})))
                .or_else(||rows.iter().position(|r|matches!(r.action,Action::ChooseLondonMulligan{mulligan:false,..}|Action::Pass{..}|Action::Discard{..}|Action::ChooseAttackerInclusion{include:false,..}|Action::ChooseBlockerInclusion{include:false,..}))).expect("genuine Storm reset action");
            let tapped = |s: &GameState| {
                s.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|&&o| s.objects.get(o).tapped)
                    .count()
            };
            let before = tapped(numeric.fixture_state());
            numeric.apply(&d, pick as u32).unwrap();
            if let Action::CastSpell { source, .. } = &rows[pick].action {
                cast = Some(source.arena_id);
                assert_eq!(tapped(numeric.fixture_state()) - before, 6);
            }
        }
        assert!(
            entered,
            "declared 40-card Storm did not reach real paid entry"
        );
    }
}
fn scenario(actor: PlayerId, hidden: &str) -> (GameState, ObjectId, ObjectId) {
    let mut state = native::ready(actor, hidden);
    state.thousand_year_storm_consumer_v1 = true;
    native::bolt(&mut state, actor);
    let source = native::paid_storm(&mut state, actor);
    let original = native::cast(
        &mut state,
        actor,
        "Lightning Bolt",
        &[Target::Player(actor.opponent())],
        1,
    );
    native::waiting(&mut state);
    (state, source, original)
}
#[test]
fn paid_copy_slot_hidden_pairs_native_index_human_wire_numeric_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source, _) = scenario(actor, "Island");
        let (other, _, _) = scenario(actor, "Mountain");
        assert_ne!(state.state_hash(), other.state_hash());
        assert!(public::capture(&state, actor.opponent())
            .unwrap()
            .triggers
            .iter()
            .all(|r| r.choice.is_none()));
        let mut raw = raw_owner(&state);
        let mut numeric = KaitoSession::from_session(raw_owner(&state)).unwrap();
        let paired = KaitoSession::from_session(raw_owner(&other)).unwrap();
        let d = decision(&numeric);
        let save = numeric.snapshot();
        assert_eq!(d.packet(), decision(&paired).packet());
        assert!(paired.training_input_v1(&d).is_err());
        let backend = d.fixture_backend_v75();
        let facts = backend.observation().thousand_year_storm.as_ref().unwrap();
        validate_facts(facts, actor.into()).unwrap();
        assert_eq!(facts.current_turn_casts[actor.index()], 2);
        let trigger = facts.triggers.iter().find(|r| r.choice.is_some()).unwrap();
        assert_eq!(trigger.source.original_source.object, source.0);
        assert_eq!(trigger.expected_copies, 1);
        let choice = trigger.choice.as_ref().unwrap();
        assert_eq!((choice.copy_ordinal, choice.target_slot), (1, 0));
        assert!(choice.can_keep_original);
        let expected = backend.expected();
        let mut human =
            HumanKaitoSessionV75::from_session(raw_owner(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_save = human.snapshot_v75();
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(raw_owner(&state)).unwrap());
        let mut other_wire = JsonlServerV1::new();
        other_wire
            .0
            .install_fixture_core(KaitoSessionV75::from_session(raw_owner(&other)).unwrap());
        let current = body(&mut wire, 1, CommandV1::Current);
        assert_eq!(current, body(&mut other_wire, 1, CommandV1::Current));
        let Body::State { state: current, .. } = current else {
            panic!()
        };
        let State::Decision {
            episode_id,
            authority_generation,
            decision: wire_prompt,
        } = *current
        else {
            panic!()
        };
        let (projected, indices) =
            project_limited_kaito_v16(backend, actor.into(), wire_prompt.prompt_seq).unwrap();
        assert_eq!(*wire_prompt, projected);
        assert_eq!(prompt.state, wire_prompt.state);
        assert_eq!(prompt.actions, wire_prompt.actions);
        assert_eq!(prompt.action_meanings, wire_prompt.action_meanings);
        assert!(prompt
            .actions
            .iter()
            .any(|action| action.label.ends_with("to you")));
        assert!(prompt
            .actions
            .iter()
            .any(|action| action.label.ends_with("to your opponent")));
        let ui=indices.iter().position(|&index|matches!(&backend.candidates_v75().candidates[index as usize].action,
            Action::ChooseEffectTarget{target:TargetRefV1::Player{player},..} if *player==actor.into())).unwrap();
        let original = backend.fixture_original_index_v75(indices[ui]).unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: ui as u32,
        };
        let step = CommandV1::Step {
            episode_id,
            authority_generation,
            actor: actor.into(),
            prompt_seq: wire_prompt.prompt_seq,
            action_index: ui as u32,
        };
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = body(&mut wire, 2, CommandV1::Snapshot)
        else {
            panic!()
        };
        let response = raw
            .step(expected.episode_id, expected.step, original)
            .unwrap();
        assert_eq!(numeric.apply(&d, indices[ui]).unwrap(), response);
        projector.submit(&mut human, request.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(
            body(&mut wire, 3, step.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());
        let after = public::capture(numeric.fixture_state(), actor).unwrap();
        let copies = &after
            .triggers
            .iter()
            .find(|r| !r.created_copies.is_empty())
            .unwrap()
            .created_copies;
        assert_eq!(copies.len(), 1);
        assert_eq!(
            copies[0].spell.targets,
            vec![public::TargetV1::Player {
                player: actor.into()
            }]
        );
        assert_eq!(after.current_turn_casts, facts.current_turn_casts);
        let input = numeric.training_input_v1(&decision(&numeric)).unwrap();
        assert!(ModelV1::deterministic_fixture()
            .forward(&input)
            .unwrap()
            .iter()
            .all(|v| v.is_finite()));
        numeric.restore(&save).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, indices[ui]).is_err());
        assert!(projector.submit(&mut human, request).is_err());
        body(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    }
}
#[test]
fn departed_observer_and_countered_original_keep_copy_after_hidden_resampling_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, original) = scenario(actor, "Island");
        let original_id = state
            .stack
            .iter()
            .find(|r| r.source == original)
            .unwrap()
            .v4
            .stack_item_id;
        engine::counter_stack_item_by_id(&mut state, original_id).unwrap();
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(original, Zone::Library),
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Library),
        );
        let resolving = state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .resolving_item
            .clone();
        assert!(state.stack.contains(&resolving));
        let mut duplicate = state.clone();
        duplicate.stack.push(resolving);
        assert!(crate::sampled_history_v1::capture(&duplicate).is_err());
        let mut changed_mirror = state.clone();
        changed_mirror
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .resolving_item
            .controller = actor.opponent();
        assert!(crate::sampled_history_v1::capture(&changed_mirror).is_err());
        let mut other = sampled_printings(
            &state,
            &[(original, id("Forest")), (source, id("Mountain"))],
        );
        assert_ne!(state.state_hash(), other.state_hash());
        let mut forged = other.clone();
        let index = forged
            .engine
            .event_history
            .iter()
            .position(|event| {
                matches!(event,
                CommittedEvent::ProtectionSourceColorDepartureV1 { snapshot, .. }
                    if snapshot.source.source == source)
            })
            .unwrap();
        let mut event = serde_json::to_value(&forged.engine.event_history[index]).unwrap();
        let mask = &mut event["ProtectionSourceColorDepartureV1"]["snapshot"]["color_mask"];
        *mask = serde_json::Value::from(mask.as_u64().unwrap() ^ 1);
        forged.engine.event_history[index] = serde_json::from_value(event).unwrap();
        assert!(crate::controlled_creature_grants_v1::validate_color_history(&forged).is_err());
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&other).unwrap()).unwrap();
        assert!(crate::controlled_creature_grants_v1::validate_color_history(&restored).is_err());
        for viewer in [actor, actor.opponent()] {
            assert_eq!(
                public::capture(&state, viewer).unwrap(),
                public::capture(&other, viewer).unwrap()
            );
        }
        let numeric = KaitoSession::from_session(raw_owner(&state)).unwrap();
        let paired = KaitoSession::from_session(raw_owner(&other)).unwrap();
        assert_eq!(decision(&numeric).packet(), decision(&paired).packet());
        for branch in [&mut state, &mut other] {
            native::action(branch, engine::Action::FinishEffectSelection);
            native::finish(branch);
            let facts = public::capture(branch, actor).unwrap();
            validate_facts(&facts, actor.into()).unwrap();
            let row = facts.triggers.last().unwrap();
            assert!(row.source.current_original_source.is_none());
            assert_eq!(row.created_copies.len(), 1);
            assert_eq!(
                row.created_copies[0].spell.original_spell.identity,
                IdentityV1::Known {
                    card_id: id("Lightning Bolt")
                }
            );
        }
        for viewer in [actor, actor.opponent()] {
            assert_eq!(
                public::capture(&state, viewer).unwrap(),
                public::capture(&other, viewer).unwrap()
            );
        }
    }
}
#[test]
fn paid_two_target_slots_and_x_copies_retain_native_choices_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor, "Island");
        state.thousand_year_storm_consumer_v1 = true;
        let a = native::cast(&mut state, actor, "Llanowar Elves", &[], 1);
        native::finish(&mut state);
        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, actor.opponent());
        let b = native::cast(&mut state, actor.opponent(), "Llanowar Elves", &[], 1);
        native::finish(&mut state);
        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, actor);
        native::bolt(&mut state, actor);
        native::paid_storm(&mut state, actor);
        native::cast(
            &mut state,
            actor,
            "Run Away Together",
            &[Target::Object(a), Target::Object(b)],
            2,
        );
        native::waiting(&mut state);
        let first = public::capture(&state, actor).unwrap();
        assert_eq!(
            first
                .triggers
                .last()
                .unwrap()
                .choice
                .as_ref()
                .unwrap()
                .target_slot,
            0
        );
        native::action(&mut state, engine::Action::FinishEffectSelection);
        native::next(&mut state);
        let next = public::capture(&state, actor).unwrap();
        let choice = next.triggers.last().unwrap().choice.as_ref().unwrap();
        assert_eq!((choice.target_slot, choice.selected_prefix.len()), (1, 1));
        let mut malformed = next.clone();
        malformed
            .triggers
            .last_mut()
            .unwrap()
            .choice
            .as_mut()
            .unwrap()
            .target_slot = 0;
        assert!(validate_facts(&malformed, actor.into()).is_err());
        let mut owner = KaitoSession::from_session(raw_owner(&state)).unwrap();
        let d = decision(&owner);
        let selected = d
            .fixture_backend_v75()
            .candidates_v75()
            .candidates
            .iter()
            .position(|r| matches!(r.action, Action::FinishEffectSelection { .. }))
            .unwrap();
        owner.apply(&d, selected as u32).unwrap();
        state = owner.fixture_state().clone();
        native::finish(&mut state);
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            public::capture(&state, actor).unwrap(),
            public::capture(&restored, actor).unwrap()
        );
        assert_eq!(state.objects.get(a).zone, Zone::Hand);
        assert_eq!(state.objects.get(b).zone, Zone::Hand);
        let mut x = native::ready(actor, "Island");
        x.thousand_year_storm_consumer_v1 = true;
        native::bolt(&mut x, actor);
        native::paid_storm(&mut x, actor);
        native::cast_choices(&mut x, actor, "Exsanguinate", &[], 5, false, 3);
        native::finish(&mut x);
        let facts = public::capture(&x, actor).unwrap();
        validate_facts(&facts, actor.into()).unwrap();
        let trigger = facts.triggers.last().unwrap();
        assert_eq!(facts.casts[trigger.cast as usize].spell.x_value, 3);
        assert_eq!(trigger.created_copies[0].spell.x_value, 3);
        let mut malformed = facts.clone();
        malformed.triggers.last_mut().unwrap().created_copies[0]
            .spell
            .x_value = 4;
        assert!(validate_facts(&malformed, actor.into()).is_err());
        let mut malformed = x.clone();
        let at = malformed
            .engine
            .event_history
            .iter()
            .position(|e| {
                matches!(
                    e,
                    CommittedEvent::ThousandYearStormV1 {
                        journal: crate::effect::thousand_year_storm_v1::JournalV1::Copy { .. }
                    }
                )
            })
            .unwrap();
        let mut row = malformed.engine.event_history.remove(at);
        let CommittedEvent::ThousandYearStormV1 {
            journal: crate::effect::thousand_year_storm_v1::JournalV1::Copy { ordinal, .. },
        } = &mut row
        else {
            panic!()
        };
        *ordinal = 2;
        malformed.engine.event_history.insert(at, row);
        assert!(public::validate_native(&malformed).is_err());
    }
}

#[path = "stale_target_tests.rs"]
mod stale_targets;
