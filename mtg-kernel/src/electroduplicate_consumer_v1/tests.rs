use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    engine,
    effect::electroduplicate_v1_tests as native,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{GameState, Target},
};

fn fixture(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state.clone(),
        LimitedAppendixV1::Electroduplicate,
    )
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!(
            "Electroduplicate fixture stopped: {:?}",
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
        panic!("Electroduplicate wire refused: {reply:?}");
    };
    payload.body
}

#[test]
fn declared_decks_keep_electroduplicate_exclusive_and_share_the_opening_menu() {
    for actor in [PlayerSeatV1::P0, PlayerSeatV1::P1] {
        let mut deck = vec![card_id_by_name("Forest").unwrap(); 38];
        deck.extend([PRINTING; 2]);
        let config = ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: 550,
            env_seed: 550_480,
            starting_player: actor,
            deck_ids: ["electroduplicate-left".into(), "electroduplicate-right".into()],
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
        assert_eq!(prompt.schema, "mtg-kernel-human-electroduplicate-decision/v1");
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
        assert_eq!(visible.schema, "mtg-kernel-limited-electroduplicate-decision/v1");
        assert_eq!(prompt.state, visible.state);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
    }
}

fn paid_target_choice(actor: PlayerId, hidden: &str) -> (GameState, ObjectId, ObjectId) {
    let mut state = native::ready_with_hidden(actor, hidden);
    let host = native::paid(&mut state, actor, "Llanowar Elves", 1);
    if matches!(engine::advance_until_decision(&mut state), engine::Decision::CastSpellOrPass {
        player, ..
    } if player != actor)
    {
        engine::step(&mut state, engine::Action::Pass).unwrap();
    }
    assert!(
        matches!(engine::advance_until_decision(&mut state), engine::Decision::CastSpellOrPass {
        player, ..
    } if player == actor)
    );
    let source = put(&mut state, actor, public::NAME, Zone::Hand);
    engine::step(&mut state, engine::Action::CastSpell(source)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(&mut state), engine::Decision::ChooseTargets {
        spell, player, ..
    } if spell == source && player == actor)
    );
    (state, host, source)
}

#[test]
fn paid_cast_target_hidden_pair_indices_restore_and_fizzle_on_new_incarnation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, host, source) = paid_target_choice(actor, "Island");
        let (other_state, _, _) = paid_target_choice(actor, public::NAME);
        assert_ne!(state.state_hash(), other_state.state_hash());
        let mut raw = fixture(&state);
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        let mut other = KaitoSession::from_session(fixture(&other_state)).unwrap();
        let mut human = HumanKaitoSessionV75::from_session(fixture(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let mut wire = JsonlServerV1::new();
        wire.0
            .install_fixture_core(KaitoSessionV75::from_session(fixture(&state)).unwrap());
        let mut other_wire = JsonlServerV1::new();
        other_wire
            .0
            .install_fixture_core(KaitoSessionV75::from_session(fixture(&other_state)).unwrap());

        let d = decision(&numeric);
        assert_eq!(d.packet(), decision(&other).packet());
        assert!(other.training_input_v1(&d).is_err());
        let save = numeric.snapshot();
        assert!(other.restore(&save).is_err());
        let backend = d.fixture_backend_v75();
        let rows = &backend.candidates_v75().candidates;
        let index = rows
            .iter()
            .position(|row| {
                row.electroduplicate_action == Some(ActionV1::ChooseCopyTarget)
                    && matches!(&row.action, Action::ChooseTarget {
                target: TargetRefV1::Object { object }, ..
            } if object.arena_id == host.0)
            })
            .unwrap() as u32;
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
        let action_index = indices.iter().position(|&i| i == index).unwrap() as u32;
        let human_action = HumanActionRequestV1 {
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
        assert_eq!(
            native::taps(numeric.fixture_state(), actor) - native::taps(&state, actor),
            3
        );

        let input = numeric.training_input_v1(&decision(&numeric)).unwrap();
        assert!(ModelV1::deterministic_fixture().forward(&input).unwrap().iter().all(|v| v.is_finite()));
        let mut copied = numeric.fixture_state().clone();
        native::finish(&mut copied);
        assert_eq!(copied.objects.get(source).zone, Zone::Graveyard);
        let initial = public::capture(&copied, actor).unwrap();
        assert_eq!(initial.copied_tokens.len(), 1);
        assert_eq!(initial.copied_tokens[0].copiable_haste_and_each_end_step_sacrifice_instances, 1);
        assert_ne!(initial.copied_tokens[0].form.keyword_bits & crate::card_def::Keywords::HASTE.0, 0);
        let token = ObjectId(initial.copied_tokens[0].card.object);
        // The graveyard spell pays actual 2RR, targeting the first real copy.
        native::cast_card(&mut copied, actor, source, &[Target::Object(token)], 4);
        let flashback = public::capture(&copied, actor.opponent()).unwrap();
        assert_eq!(flashback.casts[0].origin_zone, Some(Zone::Graveyard));
        assert_eq!(flashback.casts[0].finalized_method, Some(crate::state::CastMethodV4::Flashback));
        native::finish(&mut copied);
        assert_eq!(copied.objects.get(source).zone, Zone::Exile);
        let copies = public::capture(&copied, actor).unwrap();
        assert_eq!(copies.copied_tokens.iter().map(|row| row.copiable_haste_and_each_end_step_sacrifice_instances).collect::<Vec<_>>(), [1, 2]);
        let copy_owner = KaitoSession::from_session(fixture(&copied)).unwrap();
        let copy_decision = decision(&copy_owner);
        assert_eq!(copy_decision.fixture_backend_v75().observation().chandra.copied_tokens.len(), 2);
        assert!(ModelV1::deterministic_fixture().forward(&copy_owner.training_input_v1(&copy_decision).unwrap()).unwrap().iter().all(|v| v.is_finite()));
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&copied).unwrap()).unwrap();
        assert_eq!(copied.state_hash(), restored.state_hash());
        assert_eq!(public::capture(&restored, actor).unwrap(), copies);
        let end = native::end_priority(&mut copied);
        let published = native::publish_end(&mut copied, end);
        assert_eq!(published.len(), 3);
        native::finish(&mut copied);
        assert!(public::capture(&copied, actor).unwrap().copied_tokens.is_empty());

        let mut departed = numeric.fixture_state().clone();
        let generation = departed.objects.get(host).zone_change_count;
        // Native zone-change and reentry operands isolate the original target
        // contract. The Electroduplicate itself was paid through its actual menu above.
        for destination in [Zone::Exile, Zone::Battlefield] {
            crate::event::propose_and_commit(
                &mut departed,
                crate::event::ProposedEvent::zone_change(host, destination),
            );
        }
        let view = public::capture(&departed, actor).unwrap();
        let target = view.casts[0].target.as_ref().unwrap();
        assert_eq!(target.original.generation, generation);
        assert!(target.current_original.is_none());
        assert!(!target.currently_legal);
        let saved: GameState =
            serde_json::from_slice(&serde_json::to_vec(&departed).unwrap()).unwrap();
        assert_eq!(public::capture(&saved, actor).unwrap(), view);
        native::finish(&mut departed);
        assert_eq!(departed.objects.get(source).zone, Zone::Graveyard);
        assert!(native::copies(&departed, card_id_by_name("Llanowar Elves").unwrap()).is_empty());

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
