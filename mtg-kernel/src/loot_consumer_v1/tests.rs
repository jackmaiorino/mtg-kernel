use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    effect::loot_v1::tests as native,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::GameState,
};

fn fixture(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), LimitedAppendixV1::Loot)
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Loot fixture stopped: {:?}", session.current_response());
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
        panic!("Loot wire refused: {reply:?}");
    };
    payload.body
}

#[test]
fn declared_decks_keep_loot_exclusive_and_share_the_opening_menu() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut deck = vec![card_id_by_name("Forest").unwrap(); 38];
        deck.extend([PRINTING; 2]);
        let config = ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: 550,
            env_seed: 550_480,
            starting_player: actor,
            deck_ids: ["loot-left".into(), "loot-right".into()],
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
        assert_eq!(prompt.schema, "mtg-kernel-human-loot-decision/v1");
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
        assert_eq!(visible.schema, "mtg-kernel-limited-loot-decision/v1");
        assert_eq!(prompt.state, visible.state);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
    }
}

fn paid_look(actor: PlayerId, hidden: &str) -> (GameState, ObjectId) {
    let mut state = native::ready(
        actor,
        &[
            "Forest",
            "Llanowar Elves",
            "Island",
            "Healer's Hawk",
            "Mountain",
            "Koma, World-Eater",
        ],
    );
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    let source = native::paid(&mut state, actor);
    native::next_own_main(&mut state, actor, source);
    native::activate(&mut state, actor, source);
    assert_eq!(native::prompt(&mut state, actor).len(), 2);
    (state, source)
}

#[test]
fn paid_private_look_preserves_numeric_human_wire_indices_and_random_bottom_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source) = paid_look(actor, "Forest");
        let (hidden, _) = paid_look(actor, public::NAME);
        assert_ne!(state.state_hash(), hidden.state_hash());
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        let mut other = KaitoSession::from_session(fixture(&hidden)).unwrap();
        let mut raw = fixture(&state);
        let mut human = HumanKaitoSessionV75::from_session(fixture(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(fixture(&state)).unwrap());
        let mut hidden_wire = JsonlServerV1::new();
        hidden_wire
            .0
            .install_fixture_core(KaitoSessionV75::from_session(fixture(&hidden)).unwrap());
        let d = decision(&numeric);
        assert_eq!(d.packet(), decision(&other).packet());
        assert!(other.training_input_v1(&d).is_err());
        let save = numeric.snapshot();
        assert!(other.restore(&save).is_err());
        let backend = d.fixture_backend_v75();
        let facts = backend.observation().loot.as_ref().unwrap();
        assert_eq!(facts.lands[actor.index()].total_plays_allowed, 2);
        let choice = facts
            .abilities
            .iter()
            .find_map(|row| row.choice.as_ref())
            .unwrap();
        assert_eq!(choice.looked_in_original_order.len(), 6);
        assert_eq!(choice.maximum_mana_value, 6);
        assert_eq!(choice.legal.len(), 2);
        assert!(public::capture(&state, actor.opponent())
            .unwrap()
            .abilities
            .iter()
            .all(|row| row.choice.is_none()));
        let index = backend
            .candidates_v75()
            .candidates
            .iter()
            .position(|row| row.loot_action == Some(ActionV1::ChooseCreature))
            .unwrap() as u32;
        let mut forged = backend.candidates_v75().candidates[index as usize]
            .action
            .clone();
        if let Action::ChooseEffectTarget { source, .. } = &mut forged {
            source.zone_change_count += 1;
        }
        assert!(action(Some(facts), &forged).is_err());
        // Native departure setup isolates the retained original source binding.
        // The look itself was paid through the real cast and activation above.
        let mut source_departed = state.clone();
        crate::event::propose_and_commit(
            &mut source_departed,
            crate::event::ProposedEvent::zone_change(source, Zone::Exile),
        );
        let departed_owner = KaitoSession::from_session(fixture(&source_departed)).unwrap();
        let departed_decision = decision(&departed_owner);
        assert!(departed_decision
            .fixture_backend_v75()
            .candidates_v75()
            .candidates
            .iter()
            .any(|row| row.loot_action == Some(ActionV1::ChooseCreature)));
        let selected = choice.legal[0].object;
        let expected = backend.expected();
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_save = human.snapshot_v75();
        let current = body(&mut wire, 1, CommandV1::Current);
        assert_eq!(current, body(&mut hidden_wire, 1, CommandV1::Current));
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
        let action_index = indices
            .iter()
            .position(|&original| original == index)
            .unwrap() as u32;
        let human_action = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index,
        };
        let wire_action = CommandV1::Step {
            episode_id,
            authority_generation,
            actor: actor.into(),
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
            body(&mut wire, 3, wire_action.clone()),
            Body::State {
                receipt: Some(_),
                ..
            }
        ));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());
        let after_pick = decision(&numeric);
        let input = numeric.training_input_v1(&after_pick).unwrap();
        assert!(ModelV1::deterministic_fixture()
            .forward(&input)
            .unwrap()
            .iter()
            .all(|score| score.is_finite()));
        if let Some(confirm) = after_pick
            .fixture_backend_v75()
            .candidates_v75()
            .candidates
            .iter()
            .position(|row| row.loot_action == Some(ActionV1::ConfirmCreature))
        {
            numeric.apply(&after_pick, confirm as u32).unwrap();
        }
        assert_eq!(
            numeric.fixture_state().objects.get(ObjectId(selected)).zone,
            Zone::Battlefield
        );
        let completed = public::capture(numeric.fixture_state(), actor).unwrap();
        assert!(completed
            .abilities
            .iter()
            .any(|row| row.stage == public::StageV1::Completed));
        assert!(completed.abilities.iter().all(|row| row.choice.is_none()));
        assert_eq!(
            numeric.fixture_state().players[actor.index()].library.len(),
            5
        );
        assert!(numeric
            .fixture_state()
            .known_library_cards(actor, actor)
            .is_empty());
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(numeric.fixture_state()).unwrap()).unwrap();
        assert_eq!(public::capture(&restored, actor).unwrap(), completed);
        assert_eq!(restored.state_hash(), numeric.fixture_state().state_hash());

        let mut decline = KaitoSession::from_session(fixture(&state)).unwrap();
        let decline_decision = decision(&decline);
        let decline_index = decline_decision
            .fixture_backend_v75()
            .candidates_v75()
            .candidates
            .iter()
            .position(|row| row.loot_action == Some(ActionV1::DeclineCreature))
            .unwrap() as u32;
        decline.apply(&decline_decision, decline_index).unwrap();
        assert_eq!(
            decline.fixture_state().players[actor.index()].library.len(),
            6
        );
        assert!(decline
            .fixture_state()
            .known_library_cards(actor, actor)
            .is_empty());
        assert!(decline.fixture_state().objects.get(source).tapped);

        numeric.restore(&save).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, index).is_err());
        assert!(projector.submit(&mut human, human_action).is_err());
        body(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(
            body(&mut wire, 5, wire_action),
            Body::Error { .. }
        ));
    }
}

#[test]
fn paid_loot_adds_a_real_land_play_without_refunding_used_plays_on_departure_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor, &["Forest"; 8]);
        let source = native::paid(&mut state, actor);
        for _ in 0..3 {
            put(&mut state, actor, "Forest", Zone::Hand);
        }
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        for used in 0..2 {
            let d = decision(&numeric);
            let facts = d.fixture_backend_v75().observation().loot.as_ref().unwrap();
            assert_eq!(facts.lands[actor.index()].remaining_plays, 2 - used);
            let play = d.fixture_backend_v75().candidates_v75().candidates.iter()
                .position(|row| matches!(row.action, Action::PlayLand { actor: player, .. } if player == actor.into())).unwrap();
            numeric.apply(&d, play as u32).unwrap();
        }
        assert_eq!(
            numeric.fixture_state().players[actor.index()].lands_played_this_turn,
            2
        );
        let d = decision(&numeric);
        assert!(!d
            .fixture_backend_v75()
            .candidates_v75()
            .candidates
            .iter()
            .any(|row| matches!(row.action, Action::PlayLand { .. })));
        let mut departed = numeric.fixture_state().clone();
        crate::event::propose_and_commit(
            &mut departed,
            crate::event::ProposedEvent::zone_change(source, Zone::Exile),
        );
        let facts = public::capture(&departed, actor).unwrap();
        assert_eq!(facts.lands[actor.index()].total_plays_allowed, 1);
        assert_eq!(facts.lands[actor.index()].remaining_plays, 0);
        assert_eq!(facts.lands[actor.index()].plays_used, 2);
        assert!(native::land_menu(&mut departed, actor).is_empty());
    }
}
