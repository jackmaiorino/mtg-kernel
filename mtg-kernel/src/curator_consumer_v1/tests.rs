use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    effect::curator_v1::tests as native,
    engine,
    event::{self, CommittedEvent, ProposedEvent},
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_surface_v5::LimitedAppendixV1,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::GameState,
};

fn id(name: &str) -> u16 { card_id_by_name(name).unwrap() }

fn config(actor: PlayerId) -> ResetV1 {
    let mut mainboards = [vec![id("Island"); 40], vec![id("Island"); 40]];
    mainboards[actor.index()][..8].fill(PRINTING);
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550, env_seed: 550_472, starting_player: actor.into(),
        deck_ids: ["curator-left".into(), "curator-right".into()], mainboards,
        max_physical_decisions: 2_000, max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Curator decision absent: {:?}", session.current_response());
    };
    session.encode(expected).unwrap()
}

fn raw_owner(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), LimitedAppendixV1::Curator)
}

fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(), request_id: serial.to_string(), command,
    });
    let ReplyBodyV1::State { payload } = reply.body else { panic!("Curator wire refused: {reply:?}") };
    payload.body
}

#[test]
fn declared_forty_card_v3_reset_pays_six_and_looks_at_the_original_five_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut bad = config.clone();
        bad.mainboards[actor.index()][0] = 468;
        assert!(reset_numeric_v1(&bad).is_err());
        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().curator_unordered_reveal_v1);
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(&mut wire, 1, CommandV1::Reset { config: config.clone() })
        else { panic!() };
        let State::Decision { decision: wire_opening, .. } = *state else { panic!() };
        assert_eq!(wire_opening.schema, "mtg-kernel-limited-curator-decision/v1");
        let expected = decision(&numeric).fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected).unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-curator-decision/v1");
        assert_eq!(prompt.actions, wire_opening.actions);

        let mut cast = None;
        let mut saw_publication = false;
        let mut looked = false;
        for _ in 0..550 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation().curator.as_ref().unwrap();
            saw_publication |= facts.entries.iter().any(|entry| matches!(entry.stage, StageV1::Published { .. }));
            if let Some(ChoiceV1::Cards { source, looked: top, stage: CardsStageV1::SetFaceDownPile, .. }) = &facts.chooser {
                assert!(saw_publication);
                assert_eq!(cast, Some(source.object));
                assert_eq!(facts.viewer, actor.into());
                assert_eq!(top.iter().map(|row| ObjectId(row.object)).collect::<Vec<_>>(),
                    numeric.fixture_state().players[actor.index()].library[..5]);
                let input = numeric.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture().forward(&input).unwrap()
                    .iter().all(|value| value.is_finite()));
                looked = true;
                break;
            }
            let candidates = &backend.candidates_v75().candidates;
            let chosen = candidates.iter().position(|row| matches!(&row.action,
                Action::CastSpell { source, .. } if source.card_db_id == PRINTING && cast.is_none()
            )).or_else(|| candidates.iter().position(|row| matches!(row.action, Action::PlayLand { .. })))
                .or_else(|| candidates.iter().position(|row| matches!(row.action,
                    Action::ChooseLondonMulligan { mulligan: false, .. }
                    | Action::Pass { .. } | Action::Discard { .. }
                    | Action::ChooseAttackerInclusion { include: false, .. }
                    | Action::ChooseBlockerInclusion { include: false, .. }
                ))).expect("missing native Curator reset action");
            let tapped = |state: &GameState| state.players[actor.index()].battlefield.iter()
                .filter(|&&id| state.objects.get(id).tapped).count();
            let before = tapped(numeric.fixture_state());
            numeric.apply(&d, chosen as u32).unwrap();
            if let Action::CastSpell { source, .. } = &candidates[chosen].action {
                cast = Some(source.arena_id);
                assert_eq!(tapped(numeric.fixture_state()) - before, 6);
            }
        }
        assert!(looked, "declared reset did not reach Curator's paid private look");
    }
}

fn private_look(actor: PlayerId, library: &[&str]) -> (GameState, ObjectId, Vec<ObjectId>) {
    let mut state = native::ready(actor, library);
    state.curator_unordered_reveal_v1 = true;
    state.reveal_knowledge_v4 = Some(Default::default());
    state.reveal_library_position(actor.opponent(), actor, 6);
    let (source, _) = native::paid_to_trigger(&mut state, actor);
    // Native zone primitive after the genuine paid spell and ETB publication.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
    let prefix = native::partition_prompt(&mut state, actor);
    (state, source, prefix)
}

fn answer_partition(state: &GameState, desired: [ObjectId; 2]) -> GameState {
    let mut numeric = KaitoSession::from_session(raw_owner(state)).unwrap();
    for (offset, id) in desired.into_iter().enumerate() {
        let d = decision(&numeric);
        let backend = d.fixture_backend_v75();
        let Some(ChoiceV1::Cards { stage: CardsStageV1::SetFaceDownPile, looked, selected, legal, .. }) =
            &backend.observation().curator.as_ref().unwrap().chooser else { panic!() };
        assert_eq!((looked.len(), selected.len()), (5, offset));
        let index = legal.iter().position(|card| card.object == id.0).unwrap() as u32;
        assert_eq!(backend.fixture_original_index_v75(index), Some(index));
        assert_eq!(backend.candidates_v75().candidates[index as usize].curator_action,
            Some(ActionV1::ChooseCard { stage: CardsStageV1::SetFaceDownPile, selected_count: offset as u16 }));
        numeric.apply(&d, index).unwrap();
    }
    let d = decision(&numeric);
    let backend = d.fixture_backend_v75();
    let last = backend.candidates_v75().candidates.len() as u32 - 1;
    assert_eq!(backend.fixture_original_index_v75(last), Some(last));
    assert_eq!(backend.candidates_v75().candidates[last as usize].curator_action,
        Some(ActionV1::FinishFaceDownPile { selected_count: 2 }));
    numeric.apply(&d, last).unwrap();
    numeric.fixture_state().clone()
}

#[test]
fn hidden_partition_positions_and_names_leave_opponent_packet_human_wire_and_restore_equal() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let chooser = actor.opponent();
        let left = ["Forest", "Island", "Mountain", "Mountain", "Plains", "Swamp", "Island", "Forest"];
        let right = ["Mountain", "Swamp", "Plains", "Mountain", "Forest", "Swamp", "Island", "Forest"];
        let (a, source, top_a) = private_look(actor, &left);
        let (b, _, top_b) = private_look(actor, &right);
        assert!(public::capture(&a, chooser).unwrap().chooser.is_none());
        assert_eq!(a.known_library_cards(actor, actor).len(), 5);
        let independently_known = a.known_library_cards(chooser, actor).to_vec();
        assert_eq!(independently_known.len(), 1);
        let a = answer_partition(&a, [top_a[0], top_a[1]]);
        let b = answer_partition(&b, [top_b[1], top_b[4]]);
        assert_ne!(a.state_hash(), b.state_hash());
        assert_eq!(a.known_library_cards(chooser, actor), independently_known);
        assert_eq!(b.known_library_cards(chooser, actor), independently_known);
        assert!(crate::effect::curator_v1::refuse_legacy(&a).is_err());
        assert!(crate::limited_match_v1::sample_world_v1(
            &raw_owner(&a), crate::limited_match_v1::WorldModeV1::Peek, None, 1,
        ).is_err());
        let mut numeric = KaitoSession::from_session(raw_owner(&a)).unwrap();
        let mut alternative = KaitoSession::from_session(raw_owner(&b)).unwrap();
        let d = decision(&numeric);
        assert_eq!(d.packet(), decision(&alternative).packet());
        assert!(numeric.sampled_clone(&d, 1).is_err());
        let snapshot = numeric.snapshot();
        assert!(alternative.restore(&snapshot).is_err());
        assert!(alternative.training_input_v1(&d).is_err());
        let backend = d.fixture_backend_v75();
        let facts = backend.observation().curator.as_ref().unwrap();
        assert_eq!(facts.entries[0].source.original_source.object, source.0);
        assert!(facts.entries[0].source.current_original_source.is_none());
        let Some(ChoiceV1::Pile { piles, chooser: actual, .. }) = &facts.chooser else { panic!() };
        assert_eq!(*actual, chooser.into());
        assert_eq!(piles.face_down_count, 2);
        assert_eq!(piles.face_up, {
            let mut names = vec![
                public::NameCountV1 { card_id: id("Mountain"), copies: 2 },
                public::NameCountV1 { card_id: id("Plains"), copies: 1 },
            ];
            names.sort_by_key(|row| row.card_id);
            names
        });
        assert_eq!(backend.fixture_original_index_v75(0), Some(0));
        assert_eq!(backend.fixture_original_index_v75(1), Some(1));

        let mut human = HumanKaitoSessionV75::from_session(raw_owner(&a), chooser.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(chooser.into());
        let expected = backend.expected();
        let human_prompt = projector.project_current(&human, expected).unwrap();
        let human_save = human.snapshot_v75();
        let mut wire = JsonlServerV1::new();
        wire.0.install_fixture_core(KaitoSessionV75::from_session(raw_owner(&a)).unwrap());
        let mut other_wire = JsonlServerV1::new();
        other_wire.0.install_fixture_core(KaitoSessionV75::from_session(raw_owner(&b)).unwrap());
        let current = body(&mut wire, 1, CommandV1::Current);
        assert_eq!(current, body(&mut other_wire, 1, CommandV1::Current));
        let Body::State { state, .. } = current else { panic!() };
        let State::Decision { episode_id, authority_generation, decision: wire_prompt } = *state else { panic!() };
        let (projected, indices) = project_limited_kaito_v16(backend, chooser.into(), wire_prompt.prompt_seq).unwrap();
        assert_eq!(*wire_prompt, projected);
        assert_eq!(human_prompt.state, wire_prompt.state);
        assert_eq!(human_prompt.actions, wire_prompt.actions);
        assert_eq!(human_prompt.action_meanings, wire_prompt.action_meanings);
        let request = HumanActionRequestV1 { prompt_seq: human_prompt.prompt_seq, action_index: 0 };
        let step = CommandV1::Step { episode_id, authority_generation, actor: chooser.into(),
            prompt_seq: wire_prompt.prompt_seq, action_index: 0 };
        let Body::State { snapshot_id: Some(snapshot_id), .. } = body(&mut wire, 2, CommandV1::Snapshot)
        else { panic!() };
        let mut raw = raw_owner(&a);
        let index = backend.fixture_original_index_v75(indices[0]).unwrap();
        let response = raw.step(expected.episode_id, expected.step, index).unwrap();
        assert_eq!(numeric.apply(&d, indices[0]).unwrap(), response);
        projector.submit(&mut human, request.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(body(&mut wire, 3, step.clone()), Body::State { receipt: Some(_), .. }));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());
        let order = decision(&numeric);
        let backend = order.fixture_backend_v75();
        let Some(ChoiceV1::Cards { stage: CardsStageV1::OrderGraveyard, legal, .. }) =
            &backend.observation().curator.as_ref().unwrap().chooser else { panic!() };
        assert_eq!(legal.iter().map(|card| ObjectId(card.object)).collect::<Vec<_>>(), top_a[..2]);
        assert!(public::capture(numeric.fixture_state(), chooser).unwrap().chooser.is_none());
        assert_eq!(backend.fixture_original_index_v75(1), Some(1));
        numeric.apply(&order, 1).unwrap();
        let finished = numeric.fixture_state();
        let facts = public::capture(finished, chooser).unwrap();
        let StageV1::Resolved(outcome) = &facts.entries[0].stage else { panic!() };
        assert_eq!(outcome.chosen, PileV1::FaceUp);
        assert_eq!(outcome.actual_hand_arrivals, 3);
        assert_eq!(outcome.revealed_hand_arrivals, piles.face_up);
        assert_eq!(outcome.public_moves.iter().map(|row| ObjectId(row.original.object)).collect::<Vec<_>>(),
            [top_a[1], top_a[0]]);
        assert!(outcome.public_moves.iter().all(|row| row.destination == Zone::Graveyard));
        let knowledge = finished.reveal_knowledge_v4.as_ref().unwrap()[chooser.index()]
            .hand(actor).collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(knowledge.get(&id("Mountain")), Some(&2));
        assert_eq!(knowledge.get(&id("Plains")), Some(&1));
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(finished).unwrap()).unwrap();
        assert_eq!(public::capture(&restored, chooser).unwrap(), facts);
        for original in [finished, &restored] {
            let mut wrong_name = original.clone();
            let move_row = wrong_name.engine.event_history.iter_mut().find_map(|event| match event {
                CommittedEvent::CuratorReceiptV1 { journal } => match journal.as_mut() {
                    crate::curator_receipts_v1::JournalV1::Moved { move_row, .. }
                        if move_row.destination == Zone::Graveyard => Some(move_row),
                    _ => None,
                },
                _ => None,
            }).unwrap();
            move_row.public_printing = Some(id("Mountain"));
            assert!(public::validate_native(&wrong_name).is_err());
            let mut hidden_arrival = original.clone();
            let mask = hidden_arrival.engine.event_history.iter_mut().find_map(|event| match event {
                CommittedEvent::CuratorReceiptV1 { journal } => match journal.as_mut() {
                    crate::curator_receipts_v1::JournalV1::ArrivalVisibility {
                        mask, disclosed_printing: Some(_), ..
                    } => Some(mask),
                    _ => None,
                },
                _ => None,
            }).unwrap();
            *mask = 1;
            assert!(public::validate_native(&hidden_arrival).is_err());
        }
        let mut missing = finished.clone();
        missing.engine.event_history.retain(|event| !matches!(event,
            CommittedEvent::CuratorReceiptV1 { journal }
                if matches!(journal.as_ref(), crate::curator_receipts_v1::JournalV1::Moved { .. })));
        assert!(public::validate_native(&missing).is_err());

        numeric.restore(&snapshot).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, indices[0]).is_err());
        assert!(projector.submit(&mut human, request).is_err());
        body(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    }
}

#[test]
fn chosen_concealed_hand_cards_stay_unidentified_and_countered_entry_never_looks() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let library = ["Forest", "Island", "Mountain", "Mountain", "Plains", "Swamp", "Island"];
        let mut countered = native::ready(actor, &library);
        assert!(serde_json::to_value(&countered).unwrap().get("curator_unordered_reveal_v1").is_none());
        countered.curator_unordered_reveal_v1 = true;
        let (_, item) = native::paid_to_trigger(&mut countered, actor);
        engine::counter_stack_item_by_id(&mut countered, item.v4.stack_item_id).unwrap();
        native::finish(&mut countered);
        assert!(matches!(public::capture(&countered, actor).unwrap().entries[0].stage, StageV1::Countered));
        assert!(countered.known_library_cards(actor, actor).is_empty());

        let (mut state, _, top) = private_look(actor, &library);
        native::partition(&mut state, &top[..2]);
        native::pile_prompt(&mut state, actor, 2, 3);
        engine::step(&mut state, engine::Action::ChooseEffectBoolean(true)).unwrap();
        native::finish(&mut state);
        let facts = public::capture(&state, actor.opponent()).unwrap();
        let StageV1::Resolved(outcome) = &facts.entries[0].stage else { panic!() };
        assert_eq!(outcome.chosen, PileV1::FaceDown);
        assert_eq!(outcome.actual_hand_arrivals, 2);
        assert!(outcome.revealed_hand_arrivals.is_empty());
        let minima = state.reveal_knowledge_v4.as_ref().unwrap()[actor.opponent().index()]
            .hand(actor).collect::<std::collections::BTreeMap<_, _>>();
        assert!(!minima.contains_key(&id("Forest")) && !minima.contains_key(&id("Island")));
    }
}
