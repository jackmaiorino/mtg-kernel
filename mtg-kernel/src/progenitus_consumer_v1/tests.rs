use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    engine::{self, progenitus_v1_tests as native},
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::PlayerId,
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{GameState, Target},
};

fn fixture(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state.clone(),
        LimitedAppendixV1::Progenitus,
    )
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!(
            "Progenitus fixture stopped: {:?}",
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
        panic!("Progenitus wire refused: {reply:?}");
    };
    payload.body
}

#[test]
fn declared_decks_keep_progenitus_exclusive_and_share_the_opening_menu() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut deck = vec![card_id_by_name("Forest").unwrap(); 38];
        deck.extend([PRINTING; 2]);
        let config = ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: 550,
            env_seed: 550_480,
            starting_player: actor,
            deck_ids: ["progenitus-left".into(), "progenitus-right".into()],
            mainboards: [deck.clone(), deck],
            max_physical_decisions: 500,
            max_policy_steps: 500,
        };
        assert!(crate::limited_mixed_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::moon_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut invalid = config.clone();
        invalid.mainboards[0][0] = 484;
        assert!(reset_numeric_v1(&invalid).is_err());

        let numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let d = decision(&numeric);
        let backend = d.fixture_backend_v75();
        assert_eq!(backend.observation().schema, OBSERVATION_SCHEMA);
        let expected = backend.expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-progenitus-decision/v1");
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(&mut wire, 1, CommandV1::Reset { config }) else {
            panic!()
        };
        let State::Decision {
            decision: visible, ..
        } = *state
        else {
            panic!()
        };
        assert_eq!(visible.schema, "mtg-kernel-limited-progenitus-decision/v1");
        assert_eq!(prompt.state, visible.state);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
    }
}

#[test]
fn native_paid_cast_protection_counter_and_hidden_discard_are_distinct_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor, "Forest");
        let source = put(&mut state, actor, public::NAME, Zone::Hand);
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        let d = decision(&numeric);
        let index = d.fixture_backend_v75().candidates_v75().candidates.iter().position(|row| {
            matches!(&row.action, Action::CastSpell { source: card, .. } if card.arena_id == source.0)
        }).unwrap() as u32;
        numeric.apply(&d, index).unwrap();
        let paid = numeric.fixture_state().clone();
        assert_eq!(native::tapped(&paid, actor), 10);
        let stack = public::capture(&paid, actor.opponent()).unwrap();
        assert_eq!(stack.spells[0].card.object, source.0);
        assert_eq!(
            stack.spells[0].finalized_method,
            Some(crate::state::CastMethodV4::Normal)
        );
        assert!(stack.battlefield.is_empty());

        let mut resolved = paid.clone();
        native::finish(&mut resolved);
        let live = public::capture(&resolved, actor.opponent()).unwrap();
        assert!(live.battlefield[0].protection_from_everything_active);
        assert_eq!(live.battlefield[0].form.power, Some(10));
        crate::event::propose_and_commit(
            &mut resolved,
            crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        assert_eq!(resolved.objects.get(source).zone, Zone::Library);
        assert_eq!(
            public::capture(&resolved, actor.opponent())
                .unwrap()
                .completed_reveals_and_shuffles,
            [public::ShuffleV1 {
                owner: actor.into(),
                controller_before: actor.into(),
                from_zone: Zone::Battlefield,
            }],
        );

        let mut countered = paid;
        native::cast(
            &mut countered,
            actor.opponent(),
            "Counterspell",
            (false, 0, false),
            &[Target::Object(source)],
            None,
            2,
        );
        native::finish(&mut countered);
        assert_eq!(countered.objects.get(source).zone, Zone::Library);
        let receipts = public::capture(&countered, actor.opponent())
            .unwrap()
            .completed_reveals_and_shuffles;
        assert_eq!(
            receipts,
            [public::ShuffleV1 {
                owner: actor.into(),
                controller_before: actor.into(),
                from_zone: Zone::Stack,
            }]
        );

        let mut discarded = native::ready(actor, "Forest");
        let hidden = put(&mut discarded, actor, public::NAME, Zone::Hand);
        let before = public::capture(&discarded, actor.opponent()).unwrap();
        assert!(before.completed_reveals_and_shuffles.is_empty());
        native::cast(
            &mut discarded,
            actor,
            "Thrill of Possibility",
            (false, 0, false),
            &[],
            Some(hidden),
            2,
        );
        assert_eq!(discarded.objects.get(hidden).zone, Zone::Library);
        assert!(discarded
            .known_library_cards(actor.opponent(), actor)
            .is_empty());
        let receipts = public::capture(&discarded, actor.opponent())
            .unwrap()
            .completed_reveals_and_shuffles;
        assert_eq!(
            receipts,
            [public::ShuffleV1 {
                owner: actor.into(),
                controller_before: actor.into(),
                from_zone: Zone::Hand,
            }]
        );
    }
}

#[test]
fn native_paid_mill_hides_pre_reveal_identity_and_binds_order_across_owners() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor, "Forest");
        let mut paired = state.clone();
        let top = native::primitive_top(&mut state, actor, &[public::NAME, "Forest", "Island"]);
        native::primitive_top(&mut paired, actor, &["Swamp", "Forest", "Island"]);
        let before = KaitoSession::from_session(fixture(&state)).unwrap();
        let hidden_pair = KaitoSession::from_session(fixture(&paired)).unwrap();
        assert_ne!(state.state_hash(), paired.state_hash());
        assert_eq!(decision(&before).packet(), decision(&hidden_pair).packet());
        assert!(state
            .known_library_cards(actor.opponent(), actor)
            .is_empty());

        native::cast(
            &mut state,
            actor,
            "Billowing Shriekmass",
            (false, 0, false),
            &[],
            None,
            4,
        );
        for _ in 0..20 {
            let choice = engine::advance_until_decision(&mut state);
            if matches!(choice, engine::Decision::ChooseEffectTargets { .. }) {
                break;
            }
            native::pass(&mut state, choice);
        }
        let chooser = public::capture(&state, actor).unwrap();
        let private_order = chooser.graveyard_order.unwrap();
        assert_eq!(
            private_order
                .legal
                .iter()
                .map(|card| card.object)
                .collect::<Vec<_>>(),
            [top[1].0, top[2].0]
        );
        let opponent = public::capture(&state, actor.opponent()).unwrap();
        assert!(opponent.graveyard_order.is_none());
        assert!(opponent.completed_reveals_and_shuffles.is_empty());
        assert_eq!(
            state
                .known_library_cards(actor.opponent(), actor)
                .iter()
                .map(|card| card.object)
                .collect::<Vec<_>>(),
            [top[0]]
        );

        let mut raw = fixture(&state);
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        let mut human = HumanKaitoSessionV75::from_session(fixture(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(fixture(&state)).unwrap());
        let d = decision(&numeric);
        let backend = d.fixture_backend_v75();
        let expected = backend.expected();
        let index = backend
            .candidates_v75()
            .candidates
            .iter()
            .position(|row| {
                row.progenitus_action == Some(ActionV1::ChooseGraveyardOrder { selected_count: 0 })
                    && matches!(&row.action, Action::ChooseEffectTarget {
                    target: TargetRefV1::Object { object }, ..
                } if object.arena_id == top[2].0)
            })
            .unwrap() as u32;
        let native_index = backend.fixture_original_index_v75(index).unwrap();
        let input = numeric.training_input_v1(&d).unwrap();
        assert!(ModelV1::deterministic_fixture()
            .forward(&input)
            .unwrap()
            .iter()
            .all(|logit| logit.is_finite()));

        let prompt = projector.project_current(&human, expected).unwrap();
        let Body::State { state: current, .. } = body(&mut wire, 1, CommandV1::Current) else {
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
        let action_index = indices.iter().position(|&i| i == index).unwrap() as u32;
        let action = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index,
        };
        let step = CommandV1::Step {
            episode_id,
            authority_generation,
            actor: actor.into(),
            prompt_seq: visible.prompt_seq,
            action_index,
        };
        let snapshot = numeric.snapshot();
        let human_snapshot = human.snapshot_v75();
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = body(&mut wire, 2, CommandV1::Snapshot)
        else {
            panic!()
        };
        let response = raw
            .step(expected.episode_id, expected.step, native_index)
            .unwrap();
        assert_eq!(numeric.apply(&d, index).unwrap(), response);
        projector.submit(&mut human, action.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(
            body(&mut wire, 3, step.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());

        let mut completed = numeric.fixture_state().clone();
        native::finish(&mut completed);
        assert_eq!(completed.objects.get(top[0]).zone, Zone::Library);
        assert!(completed.players[actor.index()]
            .graveyard
            .ends_with(&[top[2], top[1]]));
        assert!(completed
            .known_library_cards(actor.opponent(), actor)
            .is_empty());
        assert_eq!(
            public::capture(&completed, actor.opponent())
                .unwrap()
                .completed_reveals_and_shuffles,
            [public::ShuffleV1 {
                owner: actor.into(),
                controller_before: actor.into(),
                from_zone: Zone::Library
            }]
        );

        numeric.restore(&snapshot).unwrap();
        human.restore_v75(&human_snapshot).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, index).is_err());
        assert!(projector.submit(&mut human, action).is_err());
        body(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    }
}
