use super::*;
use crate::{
    card_def::{card_id_by_name, Subtype, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    effect::electroduplicate_v1_tests as native,
    engine,
    event::{self, CommittedEvent, ProposedEvent},
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::{LimitedAppendixV1, PolicyDecisionV5},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::{GameState, Target},
    surface_v2::SurfaceDecision,
};

fn id(name: &str) -> u16 { card_id_by_name(name).unwrap() }

fn config(actor: PlayerId) -> ResetV1 {
    let mut mainboards = [vec![id("Swamp"); 40], vec![id("Swamp"); 40]];
    mainboards[actor.index()][..8].fill(PRINTING);
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH, episode_id: 550, env_seed: 550_479,
        starting_player: actor.into(), deck_ids: ["harvester-left".into(), "harvester-right".into()],
        mainboards, max_physical_decisions: 2_000, max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Harvester decision absent: {:?}", session.current_response());
    };
    session.encode(expected).unwrap()
}

fn raw_owner(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), LimitedAppendixV1::Harvester)
}

fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(), request_id: serial.to_string(), command,
    });
    let ReplyBodyV1::State { payload } = reply.body else { panic!("Harvester wire refused: {reply:?}") };
    payload.body
}

fn direct_facts(state: &GameState, actor: PlayerId) -> FactsV1 {
    let mut peek = state.clone();
    let origin = engine::advance_until_decision(&mut peek);
    assert_eq!(&peek, state);
    public::capture(state, actor, &PolicyDecisionV5::Surface(SurfaceDecision::Decision(origin))).unwrap()
}

#[test]
fn declared_forty_card_v3_reset_pays_three_for_exact_harvester_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut bad = config.clone();
        bad.mainboards[actor.index()][0] = 472;
        assert!(reset_numeric_v1(&bad).is_err());
        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().harvester_commit_receipts_v1);
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(&mut wire, 1, CommandV1::Reset { config: config.clone() })
        else { panic!() };
        let State::Decision { decision: opening, .. } = *state else { panic!() };
        assert_eq!(opening.schema, "mtg-kernel-limited-harvester-decision/v1");
        let expected = decision(&numeric).fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected).unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-harvester-decision/v1");
        assert_eq!(prompt.actions, opening.actions);

        let mut cast = None;
        let mut entered = false;
        for _ in 0..360 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation().harvester.as_ref().unwrap();
            if let Some(row) = facts.sources.first() {
                assert_eq!(Some(row.source.object), cast);
                assert!(row.summoning_sick);
                assert!(facts.eligible_graveyard_cards.is_empty());
                let input = numeric.training_input_v1(&d).unwrap();
                assert!(ModelV1::deterministic_fixture().forward(&input).unwrap()
                    .iter().all(|value| value.is_finite()));
                entered = true;
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
                ))).expect("missing native Harvester reset action");
            let tapped = |state: &GameState| state.players[actor.index()].battlefield.iter()
                .filter(|&&id| state.objects.get(id).tapped).count();
            let before = tapped(numeric.fixture_state());
            numeric.apply(&d, chosen as u32).unwrap();
            if let Action::CastSpell { source, .. } = &candidates[chosen].action {
                cast = Some(source.arena_id);
                assert_eq!(tapped(numeric.fixture_state()) - before, 3);
            }
        }
        assert!(entered, "declared reset did not genuinely cast Harvester");
    }
}

fn open_target_choice(actor: PlayerId, hidden: &str) -> (GameState, ObjectId, ObjectId, ObjectId, Vec<ObjectId>) {
    let mut state = native::hv_ready(actor);
    state.harvester_commit_receipts_v1 = true;
    let source = native::paid(&mut state, actor, public::NAME, 3);
    native::hv_main(&mut state, actor);
    let early = native::hv_death(&mut state, actor);
    native::hv_activate(&mut state, actor, source, early);
    native::hv_finish_pair(&mut state);
    let old_nightmare = native::hv_nightmares(&state, actor)[0];
    native::hv_main(&mut state, actor.opponent());
    native::hv_main(&mut state, actor);
    native::hv_main(&mut state, actor.opponent());
    let first = native::hv_death(&mut state, actor.opponent());
    let second = native::hv_death(&mut state, actor.opponent());
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    for _ in 0..4 {
        let d = engine::advance_until_decision(&mut state);
        match d {
            engine::Decision::CastSpellOrPass { player, activatable_abilities, .. } if player == actor => {
                assert!(activatable_abilities.contains(&(source, 0)));
                break;
            }
            engine::Decision::CastSpellOrPass { .. } => engine::step(&mut state, engine::Action::Pass).unwrap(),
            other => panic!("expected Harvester priority: {other:?}"),
        }
    }
    engine::step(&mut state, engine::Action::ActivateAbility(source, 0)).unwrap();
    let engine::Decision::ChooseTargets { spell, player, legal_targets, .. } =
        engine::advance_until_decision(&mut state) else { panic!() };
    assert_eq!((spell, player), (source, actor));
    let legal = legal_targets.into_iter().map(|row| {
        let Target::Object(id) = row else { panic!() };
        id
    }).collect::<Vec<_>>();
    assert!(legal.contains(&first) && legal.contains(&second));
    (state, source, old_nightmare, first, legal)
}

#[test]
fn paid_activation_targets_other_graveyard_and_preserves_hidden_pairs_indices_and_actual_cleanup() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source, old, target, legal) = open_target_choice(actor, "Island");
        let (other, _, _, _, _) = open_target_choice(actor, "Mountain");
        assert_ne!(state.state_hash(), other.state_hash());
        assert!(direct_facts(&state, actor.opponent()).chooser.is_none());
        let mut raw = raw_owner(&state);
        let mut numeric = KaitoSession::from_session(raw_owner(&state)).unwrap();
        let mut paired = KaitoSession::from_session(raw_owner(&other)).unwrap();
        let d = decision(&numeric);
        let save = numeric.snapshot();
        assert_eq!(d.packet(), decision(&paired).packet());
        assert!(paired.restore(&save).is_err());
        assert!(paired.training_input_v1(&d).is_err());
        let backend = d.fixture_backend_v75();
        let facts = backend.observation().harvester.as_ref().unwrap();
        let choice = facts.chooser.as_ref().unwrap();
        assert_eq!(choice.source.object, source.0);
        assert_eq!(choice.legal.iter().map(|card| ObjectId(card.object)).collect::<Vec<_>>(), legal);
        assert!(choice.legal.iter().all(|card| card.owner == actor.opponent().into()));
        assert!(!state.objects.get(source).tapped);
        for index in 0..choice.legal.len() {
            assert_eq!(backend.fixture_original_index_v75(index as u32), Some(index as u32));
            assert_eq!(backend.candidates_v75().candidates[index].harvester_action,
                Some(ActionV1::ChooseOriginalGraveyardTarget));
        }

        let expected = backend.expected();
        let mut human = HumanKaitoSessionV75::from_session(raw_owner(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_save = human.snapshot_v75();
        let mut wire = JsonlServerV1::new();
        wire.0.install_fixture_core(KaitoSessionV75::from_session(raw_owner(&state)).unwrap());
        let mut other_wire = JsonlServerV1::new();
        other_wire.0.install_fixture_core(KaitoSessionV75::from_session(raw_owner(&other)).unwrap());
        let current = body(&mut wire, 1, CommandV1::Current);
        assert_eq!(current, body(&mut other_wire, 1, CommandV1::Current));
        let Body::State { state: current, .. } = current else { panic!() };
        let State::Decision { episode_id, authority_generation, decision: wire_prompt } = *current else { panic!() };
        let (projected, indices) = project_limited_kaito_v16(backend, actor.into(), wire_prompt.prompt_seq).unwrap();
        assert_eq!(*wire_prompt, projected);
        assert_eq!(prompt.state, wire_prompt.state);
        assert_eq!(prompt.actions, wire_prompt.actions);
        assert_eq!(prompt.action_meanings, wire_prompt.action_meanings);
        let ui = indices.iter().position(|&index|
            backend.fixture_original_index_v75(index).is_some_and(|original| legal[original as usize] == target)
        ).unwrap();
        let original = backend.fixture_original_index_v75(indices[ui]).unwrap();
        let request = HumanActionRequestV1 { prompt_seq: prompt.prompt_seq, action_index: ui as u32 };
        let step = CommandV1::Step { episode_id, authority_generation, actor: actor.into(),
            prompt_seq: wire_prompt.prompt_seq, action_index: ui as u32 };
        let Body::State { snapshot_id: Some(snapshot_id), .. } = body(&mut wire, 2, CommandV1::Snapshot)
        else { panic!() };
        let response = raw.step(expected.episode_id, expected.step, original).unwrap();
        assert_eq!(numeric.apply(&d, indices[ui]).unwrap(), response);
        projector.submit(&mut human, request.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(body(&mut wire, 3, step.clone()), Body::State { receipt: Some(_), .. }));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());
        let paid = numeric.fixture_state();
        assert!(paid.objects.get(source).tapped);
        let activation = direct_facts(paid, actor.opponent()).activations.pop().unwrap();
        assert_eq!(activation.original_target.object, target.0);
        assert_eq!(activation.target_currently_legal, Some(true));

        let mut fizzled = paid.clone();
        event::propose_and_commit(&mut fizzled, ProposedEvent::zone_change(target, Zone::Hand));
        native::hv_finish_pair(&mut fizzled);
        let fizzled_facts = direct_facts(&fizzled, actor.opponent());
        let outcome = fizzled_facts.activations.last().unwrap();
        assert_eq!(outcome.outcome, Some(OutcomeV1::NoLegalTarget));
        assert!(outcome.created.is_empty() && outcome.other_nightmare_movements.is_empty());
        assert_eq!(fizzled.objects.get(old).zone, Zone::Battlefield);

        let mut resolved = paid.clone();
        event::propose_and_commit(&mut resolved, ProposedEvent::zone_change(source, Zone::Hand));
        native::hv_finish_pair(&mut resolved);
        let facts = direct_facts(&resolved, actor.opponent());
        let outcome = facts.activations.last().unwrap();
        assert_eq!(outcome.outcome, Some(OutcomeV1::Resolved));
        assert!(outcome.source.current_original_source.is_none());
        assert_eq!(outcome.target_movement.as_ref().unwrap().actual_destination, Zone::Exile);
        assert_eq!(outcome.other_nightmare_movements[0].original.object, old.0);
        assert_eq!(outcome.other_nightmare_movements[0].actual_destination, Zone::Exile);
        assert_eq!(outcome.created.len(), 1);
        assert_eq!(outcome.created[0].original.owner, actor.into());
        assert!(outcome.created[0].added_copiable_nightmare);
        let token = &facts.nightmare_copies[0];
        assert!(token.form.subtype_ids.contains(&Subtype::Elf.stable_id()));
        assert!(token.form.subtype_ids.contains(&Subtype::Nightmare.stable_id()));
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&resolved).unwrap()).unwrap();
        assert_eq!(direct_facts(&restored, actor.opponent()), facts);
        let mut altered = restored;
        altered.engine.event_history.retain(|event| !matches!(event,
            CommittedEvent::HarvesterCommitReceiptV1 { journal }
                if matches!(journal.as_ref(), crate::harvester_receipts_v1::JournalV1::Moved { movement, .. }
                    if movement.original.source == old)));
        assert!(public::validate_native(&altered).is_err());

        numeric.restore(&save).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, indices[ui]).is_err());
        assert!(projector.submit(&mut human, request).is_err());
        body(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    }
}
