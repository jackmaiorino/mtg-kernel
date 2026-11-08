use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    effect::{standard_exile::etali_primal_storm_tests as native, PendingEffectChoice},
    engine::{Action as NativeAction, Decision},
    etali_primal_storm_receipts_v1 as receipts,
    event::{self, CommittedEvent as E, ProposedEvent},
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::{LimitedAppendixV1, PolicyDecisionV5},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{GameState, Target},
    surface_v2::SurfaceDecision,
};

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}

fn config(actor: PlayerId) -> ResetV1 {
    let mut mainboards = [vec![id("Mountain"); 40], vec![id("Mountain"); 40]];
    mainboards[actor.index()][..8].fill(PRINTING);
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 550,
        env_seed: 550_481,
        starting_player: actor.into(),
        deck_ids: ["etali-left".into(), "etali-right".into()],
        mainboards,
        max_physical_decisions: 2_000,
        max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Etali decision absent: {:?}", session.current_response());
    };
    session.encode(expected).unwrap()
}

fn owner(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), LimitedAppendixV1::Etali)
}

fn body(wire: &mut JsonlServerV1, serial: u32, command: CommandV1) -> Body {
    let reply = wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(), request_id: serial.to_string(), command,
    });
    let ReplyBodyV1::State { payload } = reply.body else { panic!("{reply:?}") };
    payload.body
}

fn facts(state: &GameState, viewer: PlayerId, origin: Decision) -> FactsV1 {
    public::capture(state, viewer, &PolicyDecisionV5::Surface(SurfaceDecision::Decision(origin)))
        .unwrap()
}

#[test]
fn declared_forty_card_v3_reset_pays_six_for_etali_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut bad = config.clone();
        bad.mainboards[actor.index()][0] = 483;
        assert!(reset_numeric_v1(&bad).is_err());

        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().etali_commit_receipts_v1);
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(&mut wire, 1, CommandV1::Reset { config: config.clone() })
        else { panic!() };
        let State::Decision { decision: opening, .. } = *state else { panic!() };
        assert_eq!(opening.schema, "mtg-kernel-limited-etali-primal-storm-decision/v1");
        let expected = decision(&numeric).fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected).unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-etali-primal-storm-decision/v1");
        assert_eq!(prompt.actions, opening.actions);

        let mut cast = None;
        let mut entered = false;
        for _ in 0..560 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation().etali.as_ref().unwrap();
            assert!(facts.attacks.is_empty());
            if let Some(id) = cast {
                if numeric.fixture_state().objects.get(id).zone == Zone::Battlefield {
                    assert!(numeric.fixture_state().objects.get(id).summoning_sick);
                    assert!(numeric.training_input_v1(&d).is_ok());
                    entered = true;
                    break;
                }
            }
            let candidates = &backend.candidates_v75().candidates;
            let chosen = candidates.iter().position(|row|
                row.etali_action == Some(ActionV1::CastEtali) && cast.is_none()
            ).or_else(|| candidates.iter().position(|row| matches!(row.action, Action::PlayLand { .. })))
                .or_else(|| candidates.iter().position(|row| matches!(row.action,
                    Action::ChooseLondonMulligan { mulligan: false, .. }
                        | Action::Pass { .. } | Action::Discard { .. }
                        | Action::ChooseAttackerInclusion { include: false, .. }
                        | Action::ChooseBlockerInclusion { include: false, .. }
                ))).expect("missing native reset action");
            let tapped = |state: &GameState| state.players[actor.index()].battlefield.iter()
                .filter(|&&id| state.objects.get(id).tapped).count();
            let before = tapped(numeric.fixture_state());
            numeric.apply(&d, chosen as u32).unwrap();
            if let Action::CastSpell { source, .. } = &candidates[chosen].action {
                cast = Some(ObjectId(source.arena_id));
                assert_eq!(tapped(numeric.fixture_state()) - before, 6);
            }
        }
        assert!(entered, "declared reset did not genuinely cast Etali");
    }
}

fn two_hits(actor: PlayerId, hidden: &str) -> (GameState, ObjectId, [ObjectId; 2], Decision) {
    let (mut state, source) = native::paid(
        actor, &["Forest", "Llanowar Elves", "Forest"],
        &["Forest", "Juggernaut", hidden],
    );
    state.etali_commit_receipts_v1 = true;
    native::attack(&mut state, actor, source);
    // Explicit source-departure support operand after genuine declaration.
    // The existing native paid-response suite also removes it with a spell.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Graveyard));
    let batch = native::batch(&mut state, actor);
    let hits: Vec<_> = batch.scans.iter().filter_map(|scan| scan.hit.map(|card| card.object)).collect();
    let actual = native::next(&mut state);
    (state, source, hits.try_into().unwrap(), actual)
}

#[test]
fn paid_attack_repeated_free_casts_source_departure_hidden_pair_and_index_parity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source, [first, second], actual) = two_hits(actor, "Forest");
        let (pair, _, _, paired_origin) = two_hits(actor, "Island");
        assert_ne!(state.state_hash(), pair.state_hash());
        let other = facts(&state, actor.opponent(), actual.clone());
        assert!(other.chooser.is_none());
        assert!(crate::effect::standard_exile::refuse_top_batch_legacy(&state).is_err());
        assert_eq!(other, facts(&pair, actor.opponent(), paired_origin));
        assert!(other.attacks[0].source.current_original_source.is_none());
        assert_eq!(other.attacks[0].source.original_source.object, source.0);
        assert_eq!(other.attacks[0].original_exiles[1].owner, actor.opponent().into());

        let mut raw = owner(&state);
        let mut numeric = KaitoSession::from_session(owner(&state)).unwrap();
        let mut paired = KaitoSession::from_session(owner(&pair)).unwrap();
        let d = decision(&numeric);
        let snapshot = numeric.snapshot();
        assert_eq!(d.packet(), decision(&paired).packet());
        assert!(paired.restore(&snapshot).is_err());
        assert!(paired.training_input_v1(&d).is_err());
        assert!(numeric.training_input_v1(&d).is_ok());
        let backend = d.fixture_backend_v75();
        let expected = backend.expected();
        let mut human = HumanKaitoSessionV75::from_session(owner(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_snapshot = human.snapshot_v75();
        let mut wire = JsonlServerV1::new();
        wire.0.install_fixture_core(KaitoSessionV75::from_session(owner(&state)).unwrap());
        let Body::State { state: current, .. } = body(&mut wire, 1, CommandV1::Current) else { panic!() };
        let State::Decision { episode_id, authority_generation, decision: wire_prompt } = *current
        else { panic!() };
        let (projected, indices) = project_limited_kaito_v16(backend, actor.into(), wire_prompt.prompt_seq)
            .unwrap();
        assert_eq!(*wire_prompt, projected);
        assert_eq!(prompt.state, wire_prompt.state);
        assert_eq!(prompt.actions, wire_prompt.actions);
        assert_eq!(prompt.action_meanings, wire_prompt.action_meanings);
        let Decision::ChooseEffectTargets { legal_targets, .. } = actual else { panic!() };
        let original = legal_targets.iter().position(|target| *target == Target::Object(second)).unwrap();
        let ui = indices.iter().position(|&index|
            backend.fixture_original_index_v75(index) == Some(original as u32)).unwrap();
        let request = HumanActionRequestV1 { prompt_seq: prompt.prompt_seq, action_index: ui as u32 };
        let step = CommandV1::Step { episode_id, authority_generation, actor: actor.into(),
            prompt_seq: wire_prompt.prompt_seq, action_index: ui as u32 };
        let Body::State { snapshot_id: Some(snapshot_id), .. } = body(&mut wire, 2, CommandV1::Snapshot)
        else { panic!() };
        let response = raw.step(expected.episode_id, expected.step, original as u32).unwrap();
        assert_eq!(numeric.apply(&d, indices[ui]).unwrap(), response);
        projector.submit(&mut human, request.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(body(&mut wire, 3, step.clone()), Body::State { receipt: Some(_), .. }));

        let mut actual = numeric.fixture_state().clone();
        let next = native::batch(&mut actual, actor);
        assert_eq!(next.cast[0].object, second);
        native::answer(&mut actual, NativeAction::ChooseEffectTarget(Target::Object(first)));
        native::settle(&mut actual);
        let result = receipts::occurrences(&actual).unwrap().pop().unwrap();
        assert_eq!(result.casts.iter().map(|row| row.source.source).collect::<Vec<_>>(), [second, first]);
        assert_eq!(result.casts[0].source.owner, actor.opponent());
        assert_eq!(result.casts[0].source.controller, actor);
        assert_eq!(result.finished, Some(false));

        let mut declined = state.clone();
        native::answer(&mut declined, NativeAction::FinishEffectSelection);
        native::settle(&mut declined);
        let result = receipts::occurrences(&declined).unwrap().pop().unwrap();
        assert_eq!(result.finished, Some(true));
        assert!(result.casts.is_empty());
        assert_eq!(declined.objects.get(first).zone, Zone::Exile);
        assert_eq!(declined.objects.get(second).zone, Zone::Exile);
        event::propose_and_commit(&mut declined, ProposedEvent::zone_change(first, Zone::Hand));
        let mut relabeled: GameState = serde_json::from_slice(&serde_json::to_vec(&declined).unwrap()).unwrap();
        assert!(receipts::occurrences(&relabeled).is_ok());
        let mut changed = false;
        for event in relabeled.engine.event_history.iter_mut() {
            if let E::EtaliPrimalStormReceiptV1 { journal } = event {
                if let receipts::JournalV1::TopExiled { row, .. } = journal.as_mut() {
                    if row.original.source == first {
                        row.original.card_def = id("Forest");
                        row.arrival.card_def = id("Forest");
                        changed = true;
                    }
                }
            }
        }
        assert!(changed);
        assert!(receipts::occurrences(&relabeled).is_err());

        let mut altered: GameState = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        let Some(PendingEffectChoice::SelectTargets { legal, .. }) =
            &mut altered.engine.pending_effect.as_mut().unwrap().choice else { panic!() };
        legal.reverse();
        let receipts = receipts::occurrences(&altered).unwrap();
        assert!(crate::etali_primal_storm_receipts_v1::validate_current(&altered, &receipts).is_err());

        numeric.restore(&snapshot).unwrap();
        human.restore_v75(&human_snapshot).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, indices[ui]).is_err());
        assert!(projector.submit(&mut human, request).is_err());
        body(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    }
}

#[test]
fn paid_etali_child_keeps_mandatory_discard_and_land_exiled_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = native::paid(
            actor, &["Forest", "Thrill of Possibility", "Forest", "Forest"],
            &["Forest", "Mountain", "Forest", "Forest"],
        );
        state.etali_commit_receipts_v1 = true;
        native::attack(&mut state, actor, source);
        let batch = native::batch(&mut state, actor);
        let card = batch.scans[0].hit.unwrap().object;
        let land = batch.scans[1].exiled[0].object;
        assert!(batch.scans[1].hit.is_none());
        let discard = state.players[actor.index()].hand[0];
        let kept = put(&mut state, actor, "Mountain", Zone::Hand);
        native::answer(&mut state, NativeAction::ChooseEffectTarget(Target::Object(card)));
        let actual = native::next(&mut state);
        assert!(matches!(actual, Decision::Discard { player, count: 1, .. } if player == actor));
        assert!(facts(&state, actor.opponent(), actual.clone()).chooser.is_none());

        let mut numeric = KaitoSession::from_session(owner(&state)).unwrap();
        let d = decision(&numeric);
        let backend = d.fixture_backend_v75();
        let Some(ChoiceV1::ChildCast { original_exile, .. }) =
            &backend.observation().etali.as_ref().unwrap().chooser else { panic!() };
        assert_eq!(original_exile.object, card.0);
        let index = backend.candidates_v75().candidates.iter().position(|row|
            matches!(&row.action, Action::Discard { cards, .. } if cards.iter().any(|r| r.arena_id == discard.0))
        ).unwrap();
        assert_eq!(backend.candidates_v75().candidates[index].etali_action, Some(ActionV1::ChildCastStep));
        assert!(numeric.training_input_v1(&d).is_ok());
        numeric.apply(&d, index as u32).unwrap();
        assert_eq!(numeric.fixture_state().objects.get(discard).zone, Zone::Graveyard);
        assert_eq!(numeric.fixture_state().objects.get(kept).zone, Zone::Hand);
        assert_eq!(numeric.fixture_state().objects.get(land).zone, Zone::Exile);
        let result = receipts::occurrences(numeric.fixture_state()).unwrap().pop().unwrap();
        assert_eq!(result.casts.len(), 1);
        assert_eq!(result.casts[0].source.finalized_cast_binding.unwrap().x_value, 0);
    }
}


#[test]
fn paid_etali_keeps_actual_zero_x_and_native_target_choice_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = native::paid(
            actor, &["Forest", "Wildwood Scourge", "Forest"],
            &["Forest", "Lightning Bolt", "Forest"],
        );
        state.etali_commit_receipts_v1 = true;
        native::attack(&mut state, actor, source);
        let batch = native::batch(&mut state, actor);
        let hydra = batch.scans[0].hit.unwrap().object;
        let bolt = batch.scans[1].hit.unwrap().object;
        native::answer(&mut state, NativeAction::ChooseEffectTarget(Target::Object(hydra)));
        let result = receipts::occurrences(&state).unwrap().pop().unwrap();
        assert_eq!(result.casts[0].source.source, hydra);
        assert_eq!(result.casts[0].source.finalized_cast_binding.unwrap().x_value, 0);

        native::answer(&mut state, NativeAction::ChooseEffectTarget(Target::Object(bolt)));
        assert!(matches!(native::next(&mut state), Decision::ChooseTargets { player, .. } if player == actor));
        let mut numeric = KaitoSession::from_session(owner(&state)).unwrap();
        let d = decision(&numeric);
        let backend = d.fixture_backend_v75();
        let index = backend.candidates_v75().candidates.iter().position(|row| matches!(
            &row.action, Action::ChooseTarget { source, target: TargetRefV1::Player { player }, .. }
                if source.arena_id == bolt.0 && *player == actor.opponent().into()
        )).unwrap();
        assert_eq!(backend.candidates_v75().candidates[index].etali_action, Some(ActionV1::ChildCastStep));
        assert_eq!(backend.fixture_original_index_v75(index as u32), Some(index as u32));
        assert!(numeric.training_input_v1(&d).is_ok());
        numeric.apply(&d, index as u32).unwrap();
        let spell = numeric.fixture_state().stack.iter().find(|row| row.source == bolt).unwrap();
        assert_eq!(spell.targets, [Target::Player(actor.opponent())]);
        assert_eq!(spell.v4.source_contract.unwrap().owner, actor.opponent());
        assert_eq!(spell.controller, actor);
    }
}
