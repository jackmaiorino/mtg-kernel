use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    effect::lathril_v1_tests as native_fixture,
    engine,
    event::{self, CommittedEvent, ProposedEvent},
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    lathril_public_v1::{CombatStageV1, DrainStageV1},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::{advance_to_next_main_for, put},
    policy_surface_v5::LimitedAppendixV1,
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::GameState,
};

fn card(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let mut mainboards = [vec![card("Forest"); 40], vec![card("Forest"); 40]];
    mainboards[actor.index()] = [
        vec![PRINTING; 8], vec![card("Forest"); 16], vec![card("Swamp"); 16],
    ].concat();
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_468,
        starting_player: actor.into(),
        deck_ids: ["lathril-left".into(), "lathril-right".into()],
        mainboards,
        max_physical_decisions: 2_000,
        max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Lathril decision absent: {:?}", session.current_response());
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
        panic!("Lathril wire request rejected: {reply:?}");
    };
    payload.body
}

#[test]
fn declared_forty_card_v3_reset_pays_lathril_and_real_combat_creates_two_elves_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut bad = config.clone();
        bad.mainboards[actor.index()][0] = 482;
        assert!(reset_numeric_v1(&bad).is_err());
        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(&mut wire, 1, CommandV1::Reset { config: config.clone() })
        else { panic!() };
        let State::Decision { decision: wire_opening, .. } = *state else { panic!() };
        assert_eq!(wire_opening.schema, "mtg-kernel-limited-lathril-decision/v1");
        let expected = decision(&numeric).fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player).project_current(&human, expected).unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-lathril-decision/v1");
        assert_eq!(prompt.actions, wire_opening.actions);

        let mut source = None;
        let mut saw_publication = false;
        let mut completed = false;
        for _ in 0..650 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation().lathril.as_ref().unwrap();
            if let Some(combat) = facts.combat.first() {
                assert_eq!(combat.final_damage, 2);
                assert_eq!(combat.damaged_player, actor.opponent().into());
                assert_eq!(Some(ObjectId(combat.source.original_source.object)), source);
                saw_publication |= matches!(combat.stage, CombatStageV1::Published { .. });
                if let CombatStageV1::Resolved { tokens } = &combat.stage {
                    assert!(saw_publication);
                    assert_eq!(tokens.len(), 2);
                    for token in tokens {
                        let live = numeric.fixture_state().objects.get(ObjectId(token.object));
                        assert_eq!(live.card_def, facts.meaning.token_id);
                        assert_eq!(live.controller, actor);
                        assert_eq!(live.zone_change_count, 0);
                    }
                    let restored: GameState = serde_json::from_slice(
                        &serde_json::to_vec(numeric.fixture_state()).unwrap(),
                    ).unwrap();
                    assert_eq!(public::capture(&restored, actor).unwrap().combat, facts.combat);
                    let input = numeric.training_input_v1(&d).unwrap();
                    assert!(ModelV1::deterministic_fixture().forward(&input).unwrap().iter().all(|value| value.is_finite()));
                    completed = true;
                    break;
                }
            }
            let rows = &backend.candidates_v75().candidates;
            let choose = rows.iter().position(|row| matches!(&row.action,
                Action::CastSpell { source: card, .. } if source.is_none() && card.card_db_id == PRINTING
            ) || matches!(&row.action,
                Action::ChooseAttackerInclusion { attacker, include: true, .. }
                    if source == Some(ObjectId(attacker.arena_id))
            )).or_else(|| rows.iter().position(|row| matches!(row.action, Action::PlayLand { .. })))
                .or_else(|| rows.iter().position(|row| matches!(row.action,
                    Action::ChooseLondonMulligan { mulligan: false, .. }
                    | Action::ChooseAttackerInclusion { include: false, .. }
                    | Action::ChooseBlockerInclusion { include: false, .. }
                    | Action::Pass { .. }
                    | Action::Discard { .. }
                ))).expect("missing native Lathril fixture action");
            let row = &rows[choose];
            let tapped_lands = |state: &GameState| state.players[actor.index()].battlefield.iter()
                .filter(|&&id| state.objects.get(id).tapped).count();
            let before = tapped_lands(numeric.fixture_state());
            numeric.apply(&d, choose as u32).unwrap();
            if let Action::CastSpell { source: card, .. } = &row.action {
                source = Some(ObjectId(card.arena_id));
                assert_eq!(tapped_lands(numeric.fixture_state()) - before, 4);
            }
        }
        assert!(completed, "declared reset did not resolve Lathril's genuine combat trigger");
    }
}

fn fixture(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), LimitedAppendixV1::Lathril)
}

fn payment_fixture(actor: PlayerId, hidden: &str) -> (GameState, ObjectId, Vec<ObjectId>) {
    let (mut state, source) = native_fixture::paid(actor);
    advance_to_next_main_for(&mut state, actor);
    let elves = native_fixture::create_elves(&mut state, actor);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    engine::step(&mut state, engine::Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(engine::advance_until_decision(&mut state), engine::Decision::ChooseCostTargets {
        player, source: actual, remaining: 10, ..
    } if player == actor && actual == source));
    (state, source, elves)
}

#[test]
fn paid_ten_elf_hidden_pairs_keep_original_indices_and_observe_actual_life_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source, elves) = payment_fixture(actor, "Island");
        let (hidden, _, _) = payment_fixture(actor, "Swamp");
        assert_ne!(state.state_hash(), hidden.state_hash());
        assert!(public::capture(&state, actor.opponent()).unwrap().payment.is_none());
        let mut raw = fixture(&state);
        let mut numeric = KaitoSession::from_session(fixture(&state)).unwrap();
        let mut other = KaitoSession::from_session(fixture(&hidden)).unwrap();
        let mut human = HumanKaitoSessionV75::from_session(fixture(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let mut wire = JsonlServerV1::new();
        wire.0.install_fixture_core(KaitoSessionV75::from_session(fixture(&state)).unwrap());
        let mut hidden_wire = JsonlServerV1::new();
        hidden_wire.0.install_fixture_core(KaitoSessionV75::from_session(fixture(&hidden)).unwrap());

        let d = decision(&numeric);
        assert_eq!(d.packet(), decision(&other).packet());
        assert!(other.training_input_v1(&d).is_err());
        let save = numeric.snapshot();
        assert!(other.restore(&save).is_err());
        let backend = d.fixture_backend_v75();
        let payment = backend.observation().lathril.as_ref().unwrap().payment.as_ref().unwrap();
        assert_eq!(payment.remaining, 10);
        assert_eq!(payment.candidates.iter().map(|card| ObjectId(card.object)).collect::<Vec<_>>(), elves);
        let candidates = &backend.candidates_v75().candidates;
        assert_eq!(candidates.len(), elves.len());
        for (index, elf) in elves.iter().enumerate() {
            assert_eq!(backend.fixture_original_index_v75(index as u32), Some(index as u32));
            assert_eq!(candidates[index].lathril_action, Some(ActionV1::ChooseElfPayment { remaining: 10 }));
            assert!(matches!(&candidates[index].action, Action::ChooseCostTarget { candidate, .. } if candidate.arena_id == elf.0));
        }
        let expected = backend.expected();
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_save = human.snapshot_v75();
        let current = body(&mut wire, 1, CommandV1::Current);
        assert_eq!(current, body(&mut hidden_wire, 1, CommandV1::Current));
        let Body::State { state: current, .. } = current else { panic!() };
        let State::Decision { episode_id, authority_generation, decision: visible } = *current else { panic!() };
        let (projected, indices) = project_limited_kaito_v16(backend, expected.acting_player, visible.prompt_seq).unwrap();
        assert_eq!(*visible, projected);
        assert_eq!(prompt.state, visible.state);
        assert_eq!(prompt.actions, visible.actions);
        assert_eq!(prompt.action_meanings, visible.action_meanings);
        let original = backend.fixture_original_index_v75(indices[0]).unwrap();
        let request = HumanActionRequestV1 { prompt_seq: prompt.prompt_seq, action_index: 0 };
        let wire_action = CommandV1::Step { episode_id, authority_generation, actor: actor.into(),
            prompt_seq: visible.prompt_seq, action_index: 0 };
        let Body::State { snapshot_id: Some(snapshot_id), .. } = body(&mut wire, 2, CommandV1::Snapshot)
        else { panic!() };
        let response = raw.step(expected.episode_id, expected.step, original).unwrap();
        assert_eq!(numeric.apply(&d, indices[0]).unwrap(), response);
        projector.submit(&mut human, request.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(body(&mut wire, 3, wire_action.clone()), Body::State { receipt: Some(_), .. }));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());

        for (offset, elf) in elves[1..10].iter().enumerate() {
            let pay = decision(&numeric);
            let current = pay.fixture_backend_v75();
            let facts = current.observation().lathril.as_ref().unwrap().payment.as_ref().unwrap();
            assert_eq!(facts.selected.len(), offset + 1);
            assert_eq!(facts.selected[0].object, elves[0].0);
            let index = current.candidates_v75().candidates.iter().position(|row|
                matches!(&row.action, Action::ChooseCostTarget { candidate, .. } if candidate.arena_id == elf.0)
            ).unwrap() as u32;
            numeric.apply(&pay, index).unwrap();
        }
        let paid = numeric.fixture_state();
        let live = public::capture(paid, actor.opponent()).unwrap();
        assert!(live.payment.is_none());
        assert_eq!(live.drains[0].original_paid_elves.iter().map(|elf| ObjectId(elf.object)).collect::<Vec<_>>(), elves[..10]);
        assert!(matches!(live.drains[0].stage, DrainStageV1::Published { .. }));
        assert!(paid.objects.get(source).tapped);
        assert!(elves[..10].iter().all(|&elf| paid.objects.get(elf).tapped));
        assert!(!paid.objects.get(elves[10]).tapped);
        let life = paid.players.each_ref().map(|p| p.life);
        let id = paid.stack.last().unwrap().v4.stack_item_id;
        let mut countered = paid.clone();
        engine::counter_stack_item_by_id(&mut countered, id).unwrap().unwrap();
        assert_eq!(public::capture(&countered, actor).unwrap().drains[0].stage, DrainStageV1::Countered);
        assert_eq!(countered.players.each_ref().map(|p| p.life), life);

        let mut resolved = paid.clone();
        // Exact native zone change as a source-lifetime operand after the genuine paid ability.
        event::propose_and_commit(&mut resolved, ProposedEvent::zone_change(source, Zone::Hand));
        native_fixture::finish(&mut resolved);
        let facts = public::capture(&resolved, actor.opponent()).unwrap();
        assert!(facts.drains[0].source.current_original_source.is_none());
        assert_eq!(facts.drains[0].stage, DrainStageV1::Resolved { opponent_lost: 10, controller_gained: 10 });
        assert_eq!(resolved.players[actor.index()].life, life[actor.index()] + 10);
        assert_eq!(resolved.players[actor.opponent().index()].life, life[actor.opponent().index()] - 10);
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&resolved).unwrap()).unwrap();
        assert_eq!(public::capture(&restored, actor.opponent()).unwrap(), facts);
        let mut legacy = paid.clone();
        legacy.engine.event_history.retain(|row| !matches!(row, CommittedEvent::LathrilReceiptV1 { .. }));
        assert!(public::capture(&legacy, actor).is_err());

        numeric.restore(&save).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, indices[0]).is_err());
        assert!(projector.submit(&mut human, request).is_err());
        body(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(body(&mut wire, 5, wire_action), Body::Error { .. }));
    }
}
