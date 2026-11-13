use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    effect::consuming_aberration_v1_tests as native,
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
        LimitedAppendixV1::Aberration,
    )
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!(
            "Aberration fixture stopped: {:?}",
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
        panic!("Aberration wire refused: {reply:?}");
    };
    payload.body
}

#[test]
fn declared_reset_keeps_exact_aberration_scope_and_shared_menus() {
    for seat in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut deck = vec![card_id_by_name("Island").unwrap(); 38];
        deck.extend([PRINTING; 2]);
        let config = ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: 550,
            env_seed: 550_467,
            starting_player: seat,
            deck_ids: ["aberration-left".into(), "aberration-right".into()],
            mainboards: [deck.clone(), deck],
            max_physical_decisions: 500,
            max_policy_steps: 500,
        };
        assert!(crate::limited_mixed_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::tutelage_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut invalid = config.clone();
        invalid.mainboards[0][0] = 466;
        assert!(reset_numeric_v1(&invalid).is_err());

        let numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let d = decision(&numeric);
        let expected = d.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected)
            .unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-aberration-decision/v1");
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
        assert_eq!(visible.schema, "mtg-kernel-limited-aberration-decision/v1");
        assert_eq!(prompt.state, visible.state);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
    }
}

#[test]
fn visible_cda_uses_owner_outside_battlefield_and_keeps_unrevealed_cards_private() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let owner = actor.opponent();
        let mut state = native::ready(actor, &["Forest"; 3]);
        let hand = put(&mut state, actor, public::NAME, Zone::Hand);
        let grave = put(&mut state, actor, public::NAME, Zone::Graveyard);
        let library = put(&mut state, actor, public::NAME, Zone::Library);
        let hidden = put(&mut state, owner, public::NAME, Zone::Hand);
        let position = state.players[actor.index()]
            .library
            .iter()
            .position(|&id| id == library)
            .unwrap();
        assert!(public::capture(&state, actor)
            .unwrap()
            .characteristics
            .iter()
            .all(|row| row.card.object != library.0 && row.card.object != hidden.0));
        state.reveal_library_position(actor, actor, position);
        let facts = public::capture(&state, actor).unwrap();
        for id in [hand, grave, library] {
            let row = facts
                .characteristics
                .iter()
                .find(|row| row.card.object == id.0)
                .unwrap();
            assert_eq!(row.relative_player, actor.into());
            assert_eq!(row.base_from_cda, Some(1));
            assert_eq!((row.form.power, row.form.toughness), (Some(1), Some(1)));
        }
        assert!(facts
            .characteristics
            .iter()
            .all(|row| row.card.object != hidden.0));
        let source = native::cast(&mut state, actor, public::NAME, None, 5);
        let on_stack = public::capture(&state, owner).unwrap();
        let row = on_stack
            .characteristics
            .iter()
            .find(|row| row.card.object == source.0)
            .unwrap();
        assert_eq!(row.card.zone, Zone::Stack);
        assert_eq!(row.base_from_cda, Some(1));
        assert!(on_stack.triggers.is_empty());
        native::finish(&mut state, false);
        let live = public::capture(&state, owner).unwrap();
        let row = live
            .characteristics
            .iter()
            .find(|row| row.card.object == source.0)
            .unwrap();
        assert_eq!(row.card.zone, Zone::Battlefield);
        assert_eq!(row.base_from_cda, Some(1));
    }
}

fn paid_prefix(actor: PlayerId, hidden_suffix: &str) -> GameState {
    let mut state = native::ready(actor, &["Lightning Bolt", "Forest", hidden_suffix]);
    native::paid_aberration(&mut state, actor);
    native::cast(
        &mut state,
        actor,
        "Lightning Bolt",
        Some(Target::Player(actor.opponent())),
        1,
    );
    state
}

#[test]
fn genuine_paid_reveal_stops_at_first_land_and_orders_through_each_owning_interface() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let chooser = actor.opponent();
        let mut state = paid_prefix(actor, "Swamp");
        let mut paired = paid_prefix(actor, "Mountain");
        assert_ne!(state.state_hash(), paired.state_hash());
        let prefix = state.players[chooser.index()].library[..2].to_vec();
        let hidden = state.players[chooser.index()].library[2];
        let before = KaitoSession::from_session(fixture(&state)).unwrap();
        let before_pair = KaitoSession::from_session(fixture(&paired)).unwrap();
        assert_eq!(decision(&before).packet(), decision(&before_pair).packet());
        let published = public::capture(&state, actor).unwrap();
        assert_eq!(published.triggers[0].stage, public::StageV1::Published);
        assert!(published.triggers[0].revealed_prefix.is_empty());
        native::to_order(&mut state);
        native::to_order(&mut paired);

        let chooser_facts = public::capture(&state, chooser).unwrap();
        let other_facts = public::capture(&state, actor).unwrap();
        let revealed = &chooser_facts.triggers[0];
        assert_eq!(revealed.stage, public::StageV1::Revealed);
        assert_eq!(
            revealed
                .revealed_prefix
                .iter()
                .map(|card| card.object)
                .collect::<Vec<_>>(),
            prefix.iter().map(|id| id.0).collect::<Vec<_>>()
        );
        assert_eq!(
            other_facts.triggers[0].revealed_prefix,
            revealed.revealed_prefix
        );
        assert!(other_facts.triggers[0].order.is_none());
        assert!(revealed.order.is_some());
        assert!(state
            .known_library_cards(actor, chooser)
            .iter()
            .all(|entry| entry.object != hidden));

        let mut raw = fixture(&state);
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        let paired = KaitoSession::from_session(fixture(&paired)).unwrap();
        let mut human =
            HumanKaitoSessionV75::from_session(fixture(&state), chooser.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(chooser.into());
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(fixture(&state)).unwrap());
        let d = decision(&numeric);
        assert_eq!(d.packet(), decision(&paired).packet());
        let backend = d.fixture_backend_v75();
        let expected = backend.expected();
        let index = backend
            .candidates_v75()
            .candidates
            .iter()
            .position(|row| {
                row.aberration_action == Some(ActionV1::ChooseGraveyardOrder { selected_count: 0 })
                    && matches!(&row.action, Action::ChooseEffectTarget {
                    target: TargetRefV1::Object { object }, ..
                } if object.arena_id == prefix[1].0)
            })
            .unwrap() as u32;
        let original = backend.fixture_original_index_v75(index).unwrap();
        let input = numeric.training_input_v1(&d).unwrap();
        assert!(ModelV1::deterministic_fixture()
            .forward(&input)
            .unwrap()
            .iter()
            .all(|value| value.is_finite()));
        let prompt = projector.project_current(&human, expected).unwrap();
        let Body::State { state: visible, .. } = body(&mut wire, 1, CommandV1::Current) else {
            panic!()
        };
        let State::Decision {
            episode_id,
            authority_generation,
            decision: visible,
        } = *visible
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
            actor: chooser.into(),
            prompt_seq: visible.prompt_seq,
            action_index,
        };
        let save = numeric.snapshot();
        let human_save = human.snapshot_v75();
        let Body::State {
            snapshot_id: Some(snapshot_id),
            ..
        } = body(&mut wire, 2, CommandV1::Snapshot)
        else {
            panic!()
        };
        let result = raw
            .step(expected.episode_id, expected.step, original)
            .unwrap();
        assert_eq!(numeric.apply(&d, index).unwrap(), result);
        projector.submit(&mut human, action.clone()).unwrap();
        assert_eq!(human.current_response(), result);
        assert!(matches!(
            body(&mut wire, 3, step.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());
        let done = numeric.fixture_state();
        assert!(done.players[chooser.index()]
            .graveyard
            .ends_with(&[prefix[1], prefix[0]]));
        assert!(done.players[chooser.index()].library.contains(&hidden));
        let facts = public::capture(done, actor).unwrap();
        assert!(facts.triggers.is_empty());
        assert!(facts
            .characteristics
            .iter()
            .any(|row| row.card.zone == Zone::Battlefield && row.base_from_cda == Some(3)));

        numeric.restore(&save).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, index).is_err());
        assert!(projector.submit(&mut human, action).is_err());
        body(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    }
}
