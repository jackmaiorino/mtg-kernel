use super::*;
use crate::{card_def::{card_id_by_name, KERNEL_CARDDB_HASH}, current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1, effect::kykar_v1::tests as native,
    engine::{self, Action as NativeAction}, event::{CommittedEvent, ProposedEvent},
    human_bo3_v1::{HumanActionRequestV1, HumanKaitoProjectorV75, project_limited_kaito_v16},
    ids::PlayerId, limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_surface_v5::LimitedAppendixV1, rl_session::{FastActorResponseV1, KaitoSessionV75}, state::GameState};
fn card(name: &str) -> u16 { card_id_by_name(name).unwrap() }
fn config(actor: PlayerId) -> ResetV1 {
    let deck: Vec<_> = [vec![card("Island"); 18], vec![card("Plains"); 18], vec![PRINTING; 4]].concat();
    ResetV1 { expected_card_db_hash: KERNEL_CARDDB_HASH, episode_id: 550, env_seed: 550_474,
        starting_player: actor.into(), deck_ids: ["kykar-left".into(), "kykar-right".into()],
        mainboards: [deck.clone(), deck], max_physical_decisions: 2000, max_policy_steps: 2000 }
}
fn fixture(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), LimitedAppendixV1::Kykar)
}
fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else { panic!("Kykar decision absent"); };
    session.encode(expected).unwrap()
}
fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire.handle_request(RequestV1 { profile: PROFILE_V1.into(), request_id: serial.to_string(), command });
    let ReplyBodyV1::State { payload } = reply.body else { panic!("Kykar wire rejected {reply:?}"); };
    payload.body
}
#[test]
fn declared_forty_card_v3_numeric_human_wire_reset_is_exclusive_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        assert!(crate::limited_mixed_consumer_v1::reset_numeric_v1(&config).is_err());
        for foreign in [473, 484, public::token_id().unwrap()] {
            let mut bad = config.clone(); bad.mainboards[actor.index()][0] = foreign;
            assert!(reset_numeric_v1(&bad).is_err());
        }
        let numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let d = decision(&numeric); let expected = d.fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player).project_current(&human, expected).unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-kykar-decision/v1");
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(&mut wire, 1, CommandV1::Reset { config }) else { panic!() };
        let State::Decision { decision: visible, .. } = *state else { panic!() };
        assert_eq!(visible.schema, "mtg-kernel-limited-kykar-decision/v1");
        assert_eq!(prompt.state, visible.state); assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
    }
}

fn parity(state: &GameState, typed: ActionV1) -> GameState {
    let mut numeric = KaitoSession::from_session(fixture(state)).unwrap();
    let mut raw = fixture(state);
    let d = decision(&numeric); let backend = d.fixture_backend_v75(); let expected = backend.expected();
    let actor: PlayerId = expected.acting_player.into();
    let mut hidden = state.clone();
    let concealed = hidden.players[actor.opponent().index()].library[0];
    hidden.objects.get_mut(concealed).card_def = card("Mountain");
    assert_ne!(state.state_hash(), hidden.state_hash());
    let mut foreign = KaitoSession::from_session(fixture(&hidden)).unwrap();
    assert_eq!(d.packet(), decision(&foreign).packet());
    let save = numeric.snapshot(); assert!(foreign.restore(&save).is_err()); assert!(foreign.training_input_v1(&d).is_err());
    let mut human = HumanKaitoSessionV75::from_session(fixture(state), expected.acting_player).unwrap();
    let human_save = human.snapshot_v75(); let mut projector = HumanKaitoProjectorV75::new(expected.acting_player);
    let prompt = projector.project_current(&human, expected).unwrap();
    let mut wire = JsonlServerV1::new(); wire.0.install_fixture_core(KaitoSessionV75::from_session(fixture(state)).unwrap());
    let mut other_wire = JsonlServerV1::new(); other_wire.0.install_fixture_core(KaitoSessionV75::from_session(fixture(&hidden)).unwrap());
    let current = body(&mut wire, 1, CommandV1::Current); assert_eq!(current, body(&mut other_wire, 1, CommandV1::Current));
    let Body::State { state: current, .. } = current else { panic!() };
    let State::Decision { episode_id, authority_generation, decision: visible } = *current else { panic!() };
    let (projected, indices) = project_limited_kaito_v16(backend, expected.acting_player, visible.prompt_seq).unwrap();
    assert_eq!(*visible, projected); assert_eq!(prompt.state, visible.state); assert_eq!(prompt.actions, visible.actions);
    assert_eq!(prompt.action_meanings, visible.action_meanings);
    let index = backend.candidates_v75().candidates.iter().position(|row| row.kykar_action == Some(typed)).unwrap() as u32;
    let action_index = indices.iter().position(|i| *i == index).unwrap() as u32;
    let original = backend.fixture_original_index_v75(index).unwrap();
    let request = HumanActionRequestV1 { prompt_seq: prompt.prompt_seq, action_index };
    let step = CommandV1::Step { episode_id, authority_generation, actor: expected.acting_player,
        prompt_seq: visible.prompt_seq, action_index };
    let Body::State { snapshot_id: Some(snapshot_id), .. } = body(&mut wire, 2, CommandV1::Snapshot) else { panic!() };
    let response = raw.step(expected.episode_id, expected.step, original).unwrap();
    assert_eq!(numeric.apply(&d, index).unwrap(), response); projector.submit(&mut human, request.clone()).unwrap();
    assert_eq!(human.current_response(), response); assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());
    assert!(matches!(body(&mut wire, 3, step.clone()), Body::State { receipt: Some(_), .. }));
    let result = numeric.fixture_state().clone();
    numeric.restore(&save).unwrap(); human.restore_v75(&human_save).unwrap();
    assert_eq!(decision(&numeric).packet(), d.packet()); assert!(numeric.apply(&d, index).is_err());
    assert!(projector.submit(&mut human, request).is_err()); body(&mut wire, 4, CommandV1::Restore { snapshot_id });
    assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    result
}

#[test]
fn paid_modes_and_stolen_original_return_preserve_all_consumer_indices_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor); let source = native::paid(&mut state, actor);
        native::start(&mut state, actor, source);
        let mut spirit = parity(&state, ActionV1::ChooseSpiritMode); native::finish(&mut spirit);
        let observed = public::capture(&spirit, actor).unwrap();
        assert_eq!(observed.battlefield.iter().filter(|card| card.card.identity == (IdentityV1::Known { card_id: observed.meaning.spirit_card_id })).count(), 1);
        let numeric = KaitoSession::from_session(fixture(&spirit)).unwrap(); let d = decision(&numeric);
        assert!(ModelV1::deterministic_fixture().forward(&numeric.training_input_v1(&d).unwrap()).unwrap().iter().all(|x| x.is_finite()));

        let mut state = native::ready(actor); let source = native::paid(&mut state, actor);
        // Native foreign-creature and control setup; Kykar and its triggering
        // noncreature spell are genuinely cast and paid in this fixture.
        let elf = crate::policy_observation_v6::tests::put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Battlefield);
        crate::standard_cards_v1::gain_control(&mut state, elf, actor);
        native::start(&mut state, actor, source);
        let selected = parity(&state, ActionV1::ChooseExileMode);
        let mut targeted = parity(&selected, ActionV1::ChooseExileTarget);
        let facts = public::capture(&targeted, actor).unwrap();
        assert!(facts.abilities.iter().any(|row| row.mode == public::ModeV1::Exile && row.target.as_ref().is_some_and(|t| t.original.object == elf.0 && t.currently_legal)));
        // Actual native move primitive after publication; the original source remains authoritative.
        crate::event::propose_and_commit(&mut targeted, ProposedEvent::zone_change(source, Zone::Exile));
        native::finish(&mut targeted);
        assert_eq!(targeted.objects.get(elf).zone, Zone::Exile);
        let (index, original_exile) = native::schedule(&targeted);
        let mut restored: GameState = serde_json::from_slice(&serde_json::to_vec(&targeted).unwrap()).unwrap();
        assert_eq!(targeted.state_hash(), restored.state_hash());
        let facts = public::capture(&restored, actor).unwrap();
        assert_eq!(facts.schedules[0].physical_owner, actor.opponent().into());
        assert!(!facts.schedules[0].collected_at_first_end_step);
        assert!(facts.schedules[0].source.current_original_source.is_none());
        native::return_row(&mut restored, index);
        let numeric = KaitoSession::from_session(fixture(&restored)).unwrap();
        let d = decision(&numeric); let facts = d.fixture_backend_v75().observation().kykar.as_ref().unwrap();
        assert!(facts.schedules[0].collected_at_first_end_step);
        assert!(facts.abilities.iter().any(|row| row.mode == (public::ModeV1::Return { schedule: 0 })));
        native::finish(&mut restored);
        assert_eq!(restored.objects.get(elf).zone, Zone::Battlefield);
        assert_eq!(restored.objects.get(elf).controller, actor.opponent());
        assert_eq!(restored.objects.get(elf).zone_change_count, original_exile.zone_change_count + 1);
        let facts = public::capture(&restored, actor).unwrap();
        assert!(facts.schedules[0].current_original_exile.is_none());
        let mut forged = restored.clone();
        for event in forged.engine.event_history.iter_mut() {
            if let CommittedEvent::KykarExiledV1 { owner, .. } = event { *owner = owner.opponent(); break; }
        }
        assert!(public::capture(&forged, actor).is_err());
    }
}

#[test]
fn original_target_return_incarnation_and_unselected_mode_do_not_rebind() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor); let source = native::paid(&mut state, actor);
        let elf = native::cast(&mut state, actor, "Llanowar Elves", &[], 1); native::finish(&mut state);
        native::start(&mut state, actor, source); native::choose(&mut state, actor, 0, Some(elf));
        crate::event::propose_and_commit(&mut state, ProposedEvent::zone_change(elf, Zone::Hand));
        let facts = public::capture(&state, actor).unwrap();
        let target = facts.abilities.iter().find_map(|a| a.target.as_ref()).unwrap();
        assert!(!target.currently_legal); assert!(target.current_original.is_none());
        native::finish(&mut state); assert!(public::capture(&state, actor).unwrap().schedules.is_empty());
        let mut state = native::ready(actor); let source = native::paid(&mut state, actor);
        native::start(&mut state, actor, source);
        let before = state.clone(); assert!(engine::step(&mut state, NativeAction::ChooseTriggerMode(0)).is_err()); assert_eq!(state, before);
        let numeric = KaitoSession::from_session(fixture(&state)).unwrap(); let d = decision(&numeric);
        let rows = &d.fixture_backend_v75().candidates_v75().candidates;
        assert_eq!(rows.len(), 1); assert_eq!(rows[0].kykar_action, Some(ActionV1::ChooseSpiritMode));
        let mut changed = rows[0].action.clone();
        let Action::ChooseSpellMode { source: changed_source, .. } = &mut changed else { panic!() }; changed_source.zone_change_count += 1;
        assert!(action(d.fixture_backend_v75().observation().kykar.as_ref(), &changed).is_err());

        native::choose(&mut state, actor, 1, None); native::finish(&mut state);
        let token = state.objects.iter().find_map(|(id, live)|
            (live.zone == Zone::Battlefield && live.card_def == public::token_id().unwrap()).then_some(id)).unwrap();
        native::start(&mut state, actor, source); native::choose(&mut state, actor, 0, Some(token)); native::finish(&mut state);
        assert!(!state.exile.contains(&token));
        let facts = public::capture(&state, actor).unwrap();
        assert_eq!(facts.schedules.len(), 1); assert!(facts.schedules[0].current_original_exile.is_none());
        let (schedule, _) = native::schedule(&state); native::return_row(&mut state, schedule); native::finish(&mut state);
        assert!(!state.players[actor.index()].battlefield.contains(&token));
        assert!(public::capture(&state, actor).unwrap().schedules[0].current_original_exile.is_none());
    }
}
