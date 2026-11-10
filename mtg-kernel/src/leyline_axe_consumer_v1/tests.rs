use super::*;
use crate::{
    card_def::{card_id_by_name, Keywords, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    engine,
    event::CommittedEvent,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl_session::FastActorResponseV1,
    state::{GameState, Step},
};

fn card(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId, opponent_axes: bool) -> ResetV1 {
    let mut mainboards = [vec![card("Forest"); 40], vec![card("Forest"); 40]];
    mainboards[actor.index()].fill(PRINTING);
    if opponent_axes {
        mainboards[actor.opponent().index()].fill(PRINTING);
    }
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_482,
        starting_player: actor.into(),
        deck_ids: ["axe-left".into(), "axe-right".into()],
        mainboards,
        max_physical_decisions: 500,
        max_policy_steps: 500,
    }
}

fn wire_step(current: &Body, action_index: u32) -> CommandV1 {
    let Body::State { state, .. } = current else {
        panic!()
    };
    let State::Decision {
        episode_id,
        authority_generation,
        decision,
    } = state.as_ref()
    else {
        panic!("missing Leyline Axe wire decision")
    };
    CommandV1::Step {
        episode_id: *episode_id,
        authority_generation: *authority_generation,
        actor: decision.actor,
        prompt_seq: decision.prompt_seq,
        action_index,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!(
            "Leyline Axe decision absent: {:?}",
            session.current_response()
        );
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
        panic!("Leyline Axe wire request rejected: {reply:?}");
    };
    payload.body
}

#[test]
fn declared_forty_card_opening_choice_keeps_opponent_roster_private_and_binds_all_owners() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let hidden_pair = config(actor, true);
        let config = config(actor, false);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut invalid = config.clone();
        invalid.mainboards[actor.index()][0] = 484;
        assert!(reset_numeric_v1(&invalid).is_err());
        let mut raw = native(&config).unwrap();
        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        let mut other = reset_numeric_reveal_facts_v3(&hidden_pair).unwrap();
        assert_ne!(
            numeric.fixture_state().state_hash(),
            other.fixture_state().state_hash()
        );
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let mut human = reset_human_v1(&config, actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let mut wire = JsonlServerV1::new();
        let mut other_wire = JsonlServerV1::new();
        let mut current = body(
            &mut wire,
            1,
            CommandV1::Reset {
                config: config.clone(),
            },
        );
        let mut other_current = body(
            &mut other_wire,
            1,
            CommandV1::Reset {
                config: hidden_pair,
            },
        );
        for serial in 2..=3 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let expected = backend.expected();
            let index = backend
                .candidates_v75()
                .candidates
                .iter()
                .position(|row| {
                    matches!(
                        row.action,
                        Action::ChooseLondonMulligan {
                            mulligan: false,
                            ..
                        }
                    )
                })
                .unwrap() as u32;
            assert_eq!(index, 0);
            let original = backend.fixture_original_index_v75(index).unwrap();
            let response = raw
                .step(expected.episode_id, expected.step, original)
                .unwrap();
            assert_eq!(numeric.apply(&d, index).unwrap(), response);
            let hidden = decision(&other);
            other.apply(&hidden, index).unwrap();
            if expected.acting_player == actor.into() {
                let prompt = projector.project_current(&human, expected).unwrap();
                projector
                    .submit(
                        &mut human,
                        HumanActionRequestV1 {
                            prompt_seq: prompt.prompt_seq,
                            action_index: index,
                        },
                    )
                    .unwrap();
            } else {
                human
                    .step_opponent_with(expected, |_, _| Ok(index))
                    .unwrap();
            }
            current = body(&mut wire, serial, wire_step(&current, index));
            other_current = body(&mut other_wire, serial, wire_step(&other_current, index));
        }
        assert_eq!(current, other_current);
        let current = body(&mut wire, 4, CommandV1::Current);
        assert_eq!(current, body(&mut other_wire, 4, CommandV1::Current));
        let wire_action = wire_step(&current, 1);
        let Body::State { state, .. } = current else {
            panic!()
        };
        let State::Decision {
            decision: visible, ..
        } = *state
        else {
            panic!()
        };
        assert_eq!(visible.schema, "mtg-kernel-limited-leyline-axe-decision/v1");

        let d = decision(&numeric);
        assert_eq!(d.packet(), decision(&other).packet());
        assert!(other.training_input_v1(&d).is_err());
        let saved = numeric.snapshot();
        assert!(other.restore(&saved).is_err());
        let backend = d.fixture_backend_v75();
        let expected = backend.expected();
        let facts = backend.observation().leyline_axe.as_ref().unwrap();
        let opening = facts.opening_choice.as_ref().unwrap();
        assert_eq!(opening.owner, actor.into());
        assert_eq!(numeric.fixture_state().step, Step::Untap);
        assert!(public::capture(numeric.fixture_state(), actor.opponent())
            .unwrap()
            .opening_choice
            .is_none());
        let source = ObjectId(opening.object);
        let rows = &backend.candidates_v75().candidates;
        let native_answer = |index| backend.fixture_original_index_v75(index).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].leyline_axe_action,
            Some(ActionV1::ChooseOpeningEntry { accept: false })
        );
        assert_eq!(
            rows[1].leyline_axe_action,
            Some(ActionV1::ChooseOpeningEntry { accept: true })
        );
        assert_eq!([native_answer(0), native_answer(1)], [0, 1]);

        let prompt = projector.project_current(&human, expected).unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-leyline-axe-decision/v1");
        let (projected, indices) =
            project_limited_kaito_v16(backend, expected.acting_player, visible.prompt_seq).unwrap();
        assert_eq!(*visible, projected);
        assert_eq!(indices, [0, 1]);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);

        let human_saved = human.snapshot_v75();
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = body(&mut wire, 5, CommandV1::Snapshot)
        else {
            panic!()
        };
        let action = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: 1,
        };
        let input = numeric.training_input_v1(&d).unwrap();
        assert!(ModelV1::deterministic_fixture()
            .forward(&input)
            .unwrap()
            .iter()
            .all(|value| value.is_finite()));
        let response = raw
            .step(expected.episode_id, expected.step, native_answer(1))
            .unwrap();
        assert_eq!(numeric.apply(&d, 1).unwrap(), response);
        projector.submit(&mut human, action.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(
            body(&mut wire, 6, wire_action.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        let accepted = numeric.fixture_state();
        assert_eq!(accepted.objects.get(source).zone, Zone::Battlefield);
        assert_eq!(
            accepted.objects.get(source).zone_change_count,
            opening.generation + 1
        );
        assert_eq!(
            public::capture(accepted, actor.opponent())
                .unwrap()
                .opening_entries
                .len(),
            1
        );
        assert!(!accepted
            .engine
            .event_history
            .iter()
            .any(|row| matches!(row, CommittedEvent::SpellCast { .. })));

        let declined = decision(&numeric);
        let second = declined
            .fixture_backend_v75()
            .observation()
            .leyline_axe
            .as_ref()
            .unwrap()
            .opening_choice
            .as_ref()
            .unwrap()
            .object;
        numeric.apply(&declined, 0).unwrap();
        assert_eq!(
            numeric.fixture_state().objects.get(ObjectId(second)).zone,
            Zone::Hand
        );
        assert_eq!(
            public::capture(numeric.fixture_state(), actor.opponent())
                .unwrap()
                .opening_entries
                .len(),
            1
        );

        let hidden_opening = decision(&other);
        other.apply(&hidden_opening, 1).unwrap();
        for session in [&mut numeric, &mut other] {
            while session.fixture_state().leyline_opening_v1.is_some() {
                let decline = decision(session);
                assert_eq!(
                    decline.fixture_backend_v75().candidates_v75().candidates[0].leyline_axe_action,
                    Some(ActionV1::ChooseOpeningEntry { accept: false })
                );
                session.apply(&decline, 0).unwrap();
            }
        }
        assert_eq!(
            public::capture(numeric.fixture_state(), actor).unwrap(),
            public::capture(other.fixture_state(), actor).unwrap(),
        );

        numeric.restore(&saved).unwrap();
        human.restore_v75(&human_saved).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, 1).is_err());
        assert!(projector.submit(&mut human, action).is_err());
        body(&mut wire, 7, CommandV1::Restore { snapshot_id });
        assert!(matches!(
            body(&mut wire, 8, wire_action),
            Body::Error { .. }
        ));
    }
}

fn fixture(state: GameState) -> KaitoSession {
    KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state,
        LimitedAppendixV1::LeylineAxe,
    ))
    .unwrap()
}

fn play(
    session: &mut KaitoSession,
    pick: impl Fn(&crate::kaito_candidates_v75::CandidateMeaningV75) -> bool,
) {
    let d = decision(session);
    let index = d
        .fixture_backend_v75()
        .candidates_v75()
        .candidates
        .iter()
        .position(pick)
        .expect("missing native Leyline Axe fixture action");
    session.apply(&d, index as u32).unwrap();
}

fn settle(session: &mut KaitoSession, actor: PlayerId) {
    for _ in 0..36 {
        let d = decision(session);
        if session.fixture_state().stack.is_empty()
            && d.fixture_backend_v75().expected().acting_player == actor.into()
        {
            return;
        }
        play(session, |row| matches!(row.action, Action::Pass { .. }));
    }
    panic!("Leyline Axe fixture stack did not settle");
}

#[test]
fn actual_four_mana_cast_and_three_mana_equip_preserve_target_and_host_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let land = vec![card("Forest"); 40];
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &land,
            &land,
            crate::rl::card_name,
            550_482,
            actor,
        );
        state.step = Step::Main1;
        let source = put(&mut state, actor, public::NAME, Zone::Hand);
        let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let second_host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        for id in [host, second_host] {
            state.objects.get_mut(id).summoning_sick = true;
        }
        for _ in 0..7 {
            put(&mut state, actor, "Forest", Zone::Battlefield);
        }
        let mut session = fixture(state);
        let tapped_lands = |state: &GameState| {
            state.players[actor.index()]
                .battlefield
                .iter()
                .filter(|&&id| {
                    let live = state.objects.get(id);
                    live.card_def == card("Forest") && live.tapped
                })
                .count()
        };
        play(
            &mut session,
            |row| matches!(&row.action, Action::CastSpell { source: spell, .. } if spell.arena_id == source.0),
        );
        assert_eq!(tapped_lands(session.fixture_state()), 4);
        assert!(decision(&session)
            .fixture_backend_v75()
            .observation()
            .leyline_axe
            .as_ref()
            .unwrap()
            .casts
            .iter()
            .any(|spell| spell.finalized_method == Some(crate::state::CastMethodV4::Normal)));
        settle(&mut session, actor);
        play(&mut session, |row| {
            row.leyline_axe_action == Some(ActionV1::ActivateEquip { generic_cost: 3 })
        });
        let target_menu = decision(&session);
        let targets = &target_menu
            .fixture_backend_v75()
            .candidates_v75()
            .candidates;
        for legal in [host, second_host] {
            assert!(targets.iter().any(|row| row.leyline_axe_action == Some(ActionV1::ChooseEquipTarget)
                && matches!(&row.action, Action::ChooseTarget { target: TargetRefV1::Object { object }, .. } if object.arena_id == legal.0)));
        }
        play(&mut session, |row| {
            row.leyline_axe_action == Some(ActionV1::ChooseEquipTarget)
                && matches!(&row.action, Action::ChooseTarget { target: TargetRefV1::Object { object }, .. } if object.arena_id == host.0)
        });
        assert_eq!(tapped_lands(session.fixture_state()), 7);
        let on_stack = session.snapshot();
        settle(&mut session, actor);
        let current = decision(&session);
        let equipment = &current
            .fixture_backend_v75()
            .observation()
            .leyline_axe
            .as_ref()
            .unwrap()
            .battlefield[0];
        let host_view = equipment.host.as_ref().unwrap();
        assert_eq!(host_view.card.object, host.0);
        assert_eq!(host_view.form.power, Some(2));
        assert_eq!(host_view.form.toughness, Some(2));
        assert!(host_view.receives_printed_bonus);
        assert_eq!(
            host_view.form.keyword_bits & (Keywords::DOUBLE_STRIKE.0 | Keywords::TRAMPLE.0),
            Keywords::DOUBLE_STRIKE.0 | Keywords::TRAMPLE.0
        );
        assert!(engine::has_effective_keyword(
            session.fixture_state(),
            host,
            Keywords::DOUBLE_STRIKE
        ));
        session.restore(&on_stack).unwrap();
        settle(&mut session, actor);
        assert_eq!(decision(&session).packet(), current.packet());
    }
}
