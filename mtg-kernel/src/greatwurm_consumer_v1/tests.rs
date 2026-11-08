use super::*;
use crate::{
    card_def::{card_id_by_name, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoDecision,
    current_tree_learner_v1::ModelV1,
    effect::{greatwurm_v1::tests as native, EffectOp, ExecCtx},
    engine,
    event::CommittedEvent,
    greatwurm_public_v1::CombatStageV1,
    human_bo3_v1::{project_limited_kaito_v16, HumanActionRequestV1, HumanKaitoProjectorV75},
    ids::{ObjectId, PlayerId},
    limited_kaito_session_v16::{LimitedKaitoBodyV16 as Body, LimitedKaitoStateV16 as State},
    policy_observation_v6::tests::put,
    policy_surface_v5::{LimitedAppendixV1, PolicyDecisionV5},
    rl_session::{FastActorResponseV1, KaitoSessionV75},
    state::GameState,
    surface_v2::SurfaceDecision,
};

fn id(name: &str) -> u16 { card_id_by_name(name).unwrap() }

fn config(actor: PlayerId) -> ResetV1 {
    let mut mainboards = [vec![id("Forest"); 40], vec![id("Forest"); 40]];
    mainboards[actor.index()][..8].fill(PRINTING);
    ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH, episode_id: 550, env_seed: 550_476,
        starting_player: actor.into(), deck_ids: ["greatwurm-left".into(), "greatwurm-right".into()],
        mainboards, max_physical_decisions: 2_000, max_policy_steps: 2_000,
    }
}

fn decision(session: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("Greatwurm decision absent: {:?}", session.current_response());
    };
    session.encode(expected).unwrap()
}

fn raw_owner(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(state.clone(), LimitedAppendixV1::Greatwurm)
}

fn body(wire: &mut JsonlServerV1, serial: u64, command: CommandV1) -> Body {
    let reply = wire.handle_request(RequestV1 {
        profile: PROFILE_V1.into(), request_id: serial.to_string(), command,
    });
    let ReplyBodyV1::State { payload } = reply.body else { panic!("Greatwurm wire refused: {reply:?}") };
    payload.body
}

fn direct_facts(state: &GameState, actor: PlayerId) -> FactsV1 {
    let mut peek = state.clone();
    let origin = engine::advance_until_decision(&mut peek);
    assert_eq!(&peek, state);
    public::capture(state, actor, &PolicyDecisionV5::Surface(SurfaceDecision::Decision(origin))).unwrap()
}

#[test]
fn declared_forty_card_v3_reset_pays_six_and_resolves_actual_combat_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let config = config(actor);
        assert!(crate::limited_v68_v75_consumer_v1::reset_numeric_v1(&config).is_err());
        let mut bad = config.clone();
        bad.mainboards[actor.index()][0] = 472;
        assert!(reset_numeric_v1(&bad).is_err());
        let mut numeric = reset_numeric_reveal_facts_v3(&config).unwrap();
        assert!(numeric.fixture_state().greatwurm_commit_receipts_v1);
        assert!(numeric.fixture_state().reveal_knowledge_v4.is_some());
        let mut wire = JsonlServerV1::new();
        let Body::State { state, .. } = body(&mut wire, 1, CommandV1::Reset { config: config.clone() })
        else { panic!() };
        let State::Decision { decision: opening, .. } = *state else { panic!() };
        assert_eq!(opening.schema, "mtg-kernel-limited-greatwurm-decision/v1");
        let expected = decision(&numeric).fixture_backend_v75().expected();
        let human = reset_human_v1(&config, expected.acting_player).unwrap();
        let prompt = HumanKaitoProjectorV75::new(expected.acting_player)
            .project_current(&human, expected).unwrap();
        assert_eq!(prompt.schema, "mtg-kernel-human-greatwurm-decision/v1");
        assert_eq!(prompt.actions, opening.actions);

        let mut source = None;
        let mut saw_published = false;
        let mut resolved = false;
        for _ in 0..550 {
            let d = decision(&numeric);
            let backend = d.fixture_backend_v75();
            let facts = backend.observation().greatwurm.as_ref().unwrap();
            if let Some(row) = facts.combat.first() {
                assert_eq!(row.final_combat_damage, 7);
                assert_eq!(Some(row.original_creature.object), source);
                if !saw_published && matches!(row.stage, CombatStageV1::Published { .. }) {
                    let mut countered = numeric.fixture_state().clone();
                    let trigger = countered.stack.iter().find(|item| matches!(
                        item.inline_effect, Some(EffectOp::GreatwurmCountersV1 { .. })
                    )).unwrap().v4.stack_item_id;
                    engine::counter_stack_item_by_id(&mut countered, trigger).unwrap();
                    native::finish(&mut countered);
                    assert_eq!(direct_facts(&countered, actor).combat[0].stage, CombatStageV1::Countered);

                    let mut gone = numeric.fixture_state().clone();
                    native::graveyard(&mut gone, ObjectId(source.unwrap()));
                    native::finish(&mut gone);
                    let after = direct_facts(&gone, actor);
                    assert_eq!(after.combat[0].stage, CombatStageV1::OriginalCreatureGone);
                    assert!(after.combat[0].current_original_creature.is_none());
                }
                saw_published |= matches!(row.stage, CombatStageV1::Published { .. });
                if let CombatStageV1::Resolved { recipient_controller, placed } = row.stage {
                    assert!(saw_published);
                    assert_eq!((recipient_controller, placed), (actor.into(), 7));
                    assert_eq!(numeric.fixture_state().objects.get(ObjectId(source.unwrap())).counters.plus1_plus1, 7);
                    let input = numeric.training_input_v1(&d).unwrap();
                    assert!(ModelV1::deterministic_fixture().forward(&input).unwrap()
                        .iter().all(|value| value.is_finite()));
                    let restored: GameState = serde_json::from_slice(
                        &serde_json::to_vec(numeric.fixture_state()).unwrap(),
                    ).unwrap();
                    public::validate_native(&restored).unwrap();
                    for mode in 0..3 {
                        let mut changed_terminal = restored.clone();
                        for event in changed_terminal.engine.event_history.iter_mut() {
                            let CommittedEvent::GreatwurmCommitReceiptV1 { journal } = event else {
                                continue;
                            };
                            let crate::greatwurm_receipts_v1::JournalV1::Ended { result, .. } =
                                journal.as_mut() else { continue };
                            let crate::greatwurm_receipts_v1::EndV1::Resolved { controller, before, .. } =
                                *result else { continue };
                            use crate::greatwurm_receipts_v1::EndV1;
                            *result = match mode {
                                0 => EndV1::Countered,
                                1 => EndV1::OriginalCreatureGone,
                                _ => EndV1::Resolved {
                                    controller, before, after: before, placement_history_index: None,
                                },
                            };
                        }
                        assert!(public::validate_native(&changed_terminal).is_err());
                    }
                    let mut wrong_placement = restored;
                    for event in wrong_placement.engine.event_history.iter_mut() {
                        if let CommittedEvent::GreatwurmCommitReceiptV1 { journal } = event {
                            if let crate::greatwurm_receipts_v1::JournalV1::Ended {
                                result: crate::greatwurm_receipts_v1::EndV1::Resolved { after, .. }, ..
                            } = journal.as_mut()
                            {
                                *after += 1;
                            }
                        }
                    }
                    assert!(public::validate_native(&wrong_placement).is_err());
                    resolved = true;
                    break;
                }
            }
            let candidates = &backend.candidates_v75().candidates;
            let chosen = candidates.iter().position(|row| matches!(&row.action,
                Action::CastSpell { source: card, .. } if card.card_db_id == PRINTING && source.is_none()
            ) || matches!(&row.action, Action::ChooseAttackerInclusion { attacker, include: true, .. }
                if source == Some(attacker.arena_id)))
                .or_else(|| candidates.iter().position(|row| matches!(row.action, Action::PlayLand { .. })))
                .or_else(|| candidates.iter().position(|row| matches!(row.action,
                    Action::ChooseLondonMulligan { mulligan: false, .. }
                        | Action::Pass { .. } | Action::Discard { .. }
                        | Action::ChooseAttackerInclusion { include: false, .. }
                        | Action::ChooseBlockerInclusion { include: false, .. }
                ))).expect("missing native Greatwurm reset action");
            let before = native::tapped(numeric.fixture_state(), actor);
            numeric.apply(&d, chosen as u32).unwrap();
            if let Action::CastSpell { source: card, .. } = &candidates[chosen].action {
                source = Some(card.arena_id);
                assert_eq!(native::tapped(numeric.fixture_state(), actor) - before, 6);
            }
        }
        assert!(resolved, "declared reset did not resolve the genuine damage trigger");
    }
}

fn recast(actor: PlayerId, hidden: &str, grant: bool) -> (GameState, ObjectId, [ObjectId; 2]) {
    let mut state = native::ready_hidden(actor, hidden);
    state.greatwurm_commit_receipts_v1 = true;
    let wurm = native::paid(&mut state, actor);
    native::graveyard(&mut state, wurm);
    // Registered native counter/control setup. The Greatwurm casts are paid.
    let owned = put(&mut state, actor, "Axgard Cavalry", Zone::Battlefield);
    let stolen = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Battlefield);
    crate::standard_cards_v1::gain_control(&mut state, stolen, actor);
    state.objects.get_mut(owned).counters.charge = 80_000;
    state.objects.get_mut(stolen).counters.stun = 3;
    if grant {
        let grant = EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::GraveyardCreaturesCastableThisTurn);
        crate::effect::execute(&grant, &ExecCtx::no_targets(wurm, actor), &mut state);
    }
    native::begin_graveyard(&mut state, actor, wurm);
    (state, wurm, [owned, stolen])
}

#[test]
fn genuine_graveyard_cast_keeps_private_units_original_indices_and_full_width_debits_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, wurm, donors) = recast(actor, "Forest", false);
        let (other, _, _) = recast(actor, "Island", false);
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
        let Some(ChoiceV1::Unit { legal, remaining: 6, selected, .. }) =
            &backend.observation().greatwurm.as_ref().unwrap().chooser else { panic!() };
        assert!(selected.is_empty());
        assert_eq!(legal.len(), 2);
        assert_eq!((legal[0].permanent.object, legal[0].kind, legal[0].before_payment),
            (donors[0].0, CounterKindV1::Charge, 80_000));
        assert_eq!((legal[1].permanent.object, legal[1].kind, legal[1].permanent.owner),
            (donors[1].0, CounterKindV1::Stun, actor.opponent().into()));
        assert_eq!(backend.fixture_original_index_v75(0), Some(0));
        assert_eq!(backend.fixture_original_index_v75(1), Some(1));

        let expected = backend.expected();
        let mut human = HumanKaitoSessionV75::from_session(raw_owner(&state), actor.into()).unwrap();
        let mut projector = HumanKaitoProjectorV75::new(actor.into());
        let prompt = projector.project_current(&human, expected).unwrap();
        let human_save = human.snapshot_v75();
        let mut wire = JsonlServerV1::new();
        wire.0.install_fixture_core(KaitoSessionV75::from_session(raw_owner(&state)).unwrap());
        let mut alternate_wire = JsonlServerV1::new();
        alternate_wire.0.install_fixture_core(KaitoSessionV75::from_session(raw_owner(&other)).unwrap());
        let current = body(&mut wire, 1, CommandV1::Current);
        assert_eq!(current, body(&mut alternate_wire, 1, CommandV1::Current));
        let Body::State { state: menu, .. } = current else { panic!() };
        let State::Decision { episode_id, authority_generation, decision: wire_prompt } = *menu else { panic!() };
        let (projected, indices) = project_limited_kaito_v16(backend, actor.into(), wire_prompt.prompt_seq).unwrap();
        assert_eq!(*wire_prompt, projected);
        assert_eq!(prompt.state, wire_prompt.state);
        assert_eq!(prompt.actions, wire_prompt.actions);
        assert_eq!(prompt.action_meanings, wire_prompt.action_meanings);
        let request = HumanActionRequestV1 { prompt_seq: prompt.prompt_seq, action_index: 0 };
        let step = CommandV1::Step { episode_id, authority_generation, actor: actor.into(),
            prompt_seq: wire_prompt.prompt_seq, action_index: 0 };
        let Body::State { snapshot_id: Some(snapshot_id), .. } = body(&mut wire, 2, CommandV1::Snapshot)
        else { panic!() };
        let original = backend.fixture_original_index_v75(indices[0]).unwrap();
        let response = raw.step(expected.episode_id, expected.step, original).unwrap();
        assert_eq!(numeric.apply(&d, indices[0]).unwrap(), response);
        projector.submit(&mut human, request.clone()).unwrap();
        assert_eq!(human.current_response(), response);
        assert!(matches!(body(&mut wire, 3, step.clone()), Body::State { receipt: Some(_), .. }));
        assert_eq!(raw.kaito_fixture_state(), numeric.fixture_state());
        assert_eq!(numeric.fixture_state().objects.get(donors[0]).counters.charge, 80_000);

        let before = native::tapped(numeric.fixture_state(), actor);
        for (selected, index) in [0_u32, 0, 0, 1, 1].into_iter().enumerate() {
            let current = decision(&numeric);
            let backend = current.fixture_backend_v75();
            let Some(ChoiceV1::Unit { selected: prefix, remaining, .. }) =
                &backend.observation().greatwurm.as_ref().unwrap().chooser else { panic!() };
            assert_eq!(prefix.len(), selected + 1);
            assert_eq!(usize::from(*remaining), 5 - selected);
            assert!(direct_facts(numeric.fixture_state(), actor.opponent()).chooser.is_none());
            numeric.apply(&current, index).unwrap();
        }
        let paid = numeric.fixture_state();
        assert_eq!(native::tapped(paid, actor) - before, 6);
        assert_eq!(paid.objects.get(donors[0]).counters.charge, 79_996);
        assert_eq!(paid.objects.get(donors[1]).counters.stun, 1);
        let facts = direct_facts(paid, actor.opponent());
        assert_eq!(facts.casts[0].finalized_graveyard_route, Some(GraveyardRouteV1::PrintedSixCounterCost));
        assert_eq!(facts.paid[0].original_spell.object, wurm.0);
        let stolen = &facts.paid[0].removals[1];
        assert_eq!((stolen.original.owner, stolen.original.controller, stolen.original_count, stolen.remaining_count),
            (actor.opponent().into(), actor.into(), 3, 1));
        let mut finished = paid.clone();
        native::finish(&mut finished);
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&finished).unwrap()).unwrap();
        assert_eq!(direct_facts(&restored, actor.opponent()).paid, facts.paid);
        let mut changed = restored;
        for event in changed.engine.event_history.iter_mut() {
            match event {
                CommittedEvent::GreatwurmCounterInputV1 { input }
                | CommittedEvent::GreatwurmCounterPaymentV1 { paid: input, .. } => {
                    for (n, unit) in input.units.iter().enumerate() {
                        if unit.kind == CounterKindV1::Charge {
                            input.original_counts[n] = 80_001;
                        }
                    }
                }
                _ => {}
            }
        }
        assert!(public::validate_native(&changed).is_err());

        numeric.restore(&save).unwrap();
        human.restore_v75(&human_save).unwrap();
        assert_eq!(decision(&numeric).packet(), d.packet());
        assert!(numeric.apply(&d, indices[0]).is_err());
        assert!(projector.submit(&mut human, request).is_err());
        body(&mut wire, 4, CommandV1::Restore { snapshot_id });
        assert!(matches!(body(&mut wire, 5, step), Body::Error { .. }));
    }
}

#[test]
fn granted_and_printed_routes_keep_the_native_two_option_order_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, _, donors) = recast(actor, "Forest", true);
        let mut numeric = KaitoSession::from_session(raw_owner(&state)).unwrap();
        let d = decision(&numeric);
        let backend = d.fixture_backend_v75();
        let Some(ChoiceV1::Route { routes, .. }) =
            &backend.observation().greatwurm.as_ref().unwrap().chooser else { panic!() };
        assert_eq!(*routes, [GraveyardRouteV1::GrantedPermission, GraveyardRouteV1::PrintedSixCounterCost]);
        for (index, route) in routes.iter().enumerate() {
            assert_eq!(backend.fixture_original_index_v75(index as u32), Some(index as u32));
            assert_eq!(backend.candidates_v75().candidates[index].greatwurm_action,
                Some(ActionV1::ChooseGraveyardRoute { index: index as u16, route: *route }));
        }
        let before = native::tapped(&state, actor);
        numeric.apply(&d, 0).unwrap();
        let paid = numeric.fixture_state();
        assert_eq!(native::tapped(paid, actor) - before, 6);
        assert_eq!(paid.objects.get(donors[0]).counters.charge, 80_000);
        let facts = direct_facts(paid, actor.opponent());
        assert_eq!(facts.casts[0].finalized_graveyard_route, Some(GraveyardRouteV1::GrantedPermission));
        assert!(facts.paid.is_empty());
    }
}
