use super::*;
use crate::engine::{self, Action, Decision};
use crate::event::{self, CommittedEvent};
use crate::policy_observation_v6::tests::{put, ready_state_for};

#[test]
fn direct_panda_leaf_never_acquires_an_original_publication_owner() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let context = ExecCtx::no_targets(source, actor);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(execute(&mut state, &context, &life_recipe()).is_err());
        assert!(execute(&mut state, &context, &death_recipe()).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        validate_state(&state).unwrap();
    }
}

#[test]
fn absent_printing_emits_no_capture_and_genuine_predecessor_gain_keeps_its_old_path_both_seats() {
    if crate::card_def::card_id_by_name("Fiendish Panda").is_some() {
        return;
    }
    assert!(!enabled());
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(
            capture_death(&state, &ProposedEvent::zone_change(source, Zone::Graveyard)).is_none()
        );
        assert!(validate_state(&state).is_ok());
        assert!(require_legacy_supported(&state, Some(actor)).is_ok());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let life = state.players[actor.index()].life;
        event::propose_and_commit(&mut state, ProposedEvent::life_gain(actor, 4));
        assert_eq!(state.players[actor.index()].life, life + 4);
        assert!(state.engine.event_history.iter().any(
            |e| matches!(e, CommittedEvent::LifeGain { player, amount: 4 } if *player == actor)
        ));
        assert!(!state.engine.event_history.iter().any(|e| matches!(
            e,
            CommittedEvent::FiendishPandaLifeGainCaptureV1 { .. }
                | CommittedEvent::FiendishPandaDeathCaptureV1 { .. }
                | CommittedEvent::FiendishPandaCaptureCollectedV1 { .. }
        )));
        let mut expected = state.clone();
        record_gain(&mut expected, 0);
        assert_eq!(
            serde_json::to_vec(&expected).unwrap(),
            serde_json::to_vec(&state).unwrap()
        );
    }
}

#[test]
fn foreign_actual_gain_cannot_authorize_a_panda_root_or_an_orphan_collection_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut original = ready_state_for(actor);
        let source = put(&mut original, actor, "Lightning Bolt", Zone::Hand);
        let gain = original.engine.event_history.len();
        event::propose_and_commit(&mut original, ProposedEvent::life_gain(actor, 1));
        let contract = AbilitySourceContractV4::capture(&original, source);
        for orphan in [false, true] {
            let mut state = original.clone();
            if orphan {
                state
                    .engine
                    .event_history
                    .push(CommittedEvent::FiendishPandaCaptureCollectedV1 {
                        capture_history_index: gain as u64,
                    });
            } else {
                state
                    .engine
                    .event_history
                    .push(CommittedEvent::FiendishPandaLifeGainCaptureV1 {
                        source: contract,
                        gain_history_index: gain as u64,
                        amount: 1,
                    });
            }
            let before = serde_json::to_vec(&state).unwrap();
            assert!(validate_state(&state).is_err());
            assert!(require_legacy_supported(&state, Some(actor)).is_err());
            assert!(engine::step(&mut state, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            let history = state.engine.event_history.clone();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::Halted { .. }
            ));
            assert_eq!(state.engine.event_history, history);
            state.engine.halted = None;
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}

#[test]
fn precise_panda_meaning_never_relabels_a_registered_predecessor_as_the_missing_printing() {
    let bolt = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
    use crate::rules_meaning_fiendish_panda_v1::{printed_meaning, RootV1};
    for root in [
        RootV1::CreatureSpell,
        RootV1::ControllerLifeGain,
        RootV1::SourceDeath,
    ] {
        assert!(printed_meaning(bolt, root).is_err());
        assert!(printed_meaning(u16::MAX, root).is_err());
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod printed {
    use super::*;
    fn readonly_refusal(state: &GameState) {
        assert!(validate_state(state).is_err());
        let mut rejected = state.clone();
        let bytes = serde_json::to_vec(&rejected).unwrap();
        assert!(engine::step(&mut rejected, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&rejected).unwrap(), bytes);
        assert!(matches!(
            engine::advance_until_decision(&mut rejected),
            Decision::Halted { .. }
        ));
        rejected.engine.halted = state.engine.halted;
        assert_eq!(serde_json::to_vec(&rejected).unwrap(), bytes);
    }
    #[test]
    #[ignore = "Requires genuine Fiendish Panda registration and current native qualification"]
    fn actual_fresh_held_and_full_publication_owners_survive_restore_and_refuse_erasure_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = panda(&mut state, actor);
            for amount in [2, 4] {
                event::propose_and_commit(&mut state, ProposedEvent::life_gain(actor, amount));
            }
            validate_state(&state).unwrap();
            let mut erased = state.clone();
            erased.engine.event_log.clear();
            readonly_refusal(&erased);
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::OrderTriggers { .. }
            ));
            validate_state(&state).unwrap();
            let mut erased = state.clone();
            erased.engine.pending_triggers.clear();
            readonly_refusal(&erased);
            engine::step(&mut state, Action::OrderTriggers(vec![0, 1])).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            assert_eq!(state.stack.iter().filter(|i| i.source == source).count(), 2);
            validate_state(&state).unwrap();
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            validate_state(&restored).unwrap();
            let mut erased = restored.clone();
            erased.stack.pop().unwrap();
            readonly_refusal(&erased);
            let mut duplicated = restored.clone();
            let mut duplicate = duplicated.stack.last().unwrap().clone();
            duplicated.engine.next_stack_item_id =
                duplicated.engine.next_stack_item_id.checked_add(1).unwrap();
            duplicate.v4.stack_item_id =
                crate::ids::StackItemId(duplicated.engine.next_stack_item_id);
            duplicated.stack.push(duplicate);
            readonly_refusal(&duplicated);
            let mut changed = restored;
            changed.stack.last_mut().unwrap().mode_chosen = 1;
            readonly_refusal(&changed);
            settle_stack(&mut state);
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 2);
            validate_state(&state).unwrap();
        }
    }

    #[test]
    #[ignore = "Requires genuine Fiendish Panda registration and current native qualification"]
    fn actual_counter_output_and_context_cannot_be_erased_changed_or_replayed_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = panda(&mut state, actor);
            event::propose_and_commit(&mut state, ProposedEvent::life_gain(actor, 5));
            settle_stack(&mut state);
            validate_state(&state).unwrap();
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
            let mut changed = state.clone();
            let output = changed.engine.event_history.iter_mut().find(|e| matches!(e, CommittedEvent::PlusOneCountersAdded { object, .. } if *object == source)).unwrap();
            let CommittedEvent::PlusOneCountersAdded { count, .. } = output else {
                unreachable!()
            };
            *count = 0;
            readonly_refusal(&changed);
            let mut changed = state.clone();
            let output = changed
                .engine
                .event_history
                .iter_mut()
                .find_map(|e| match e {
                    CommittedEvent::FiendishPandaJournalV1 { journal } => match journal.as_mut() {
                        custody::JournalV1::Operation {
                            after: Some(after), ..
                        } => Some(after),
                        _ => None,
                    },
                    _ => None,
                })
                .unwrap();
            output.counters.plus1_plus1 = 0;
            readonly_refusal(&changed);
            let context = state
                .engine
                .event_history
                .iter()
                .find_map(|e| match e {
                    CommittedEvent::FiendishPandaJournalV1 { journal } => match journal.as_ref() {
                        custody::JournalV1::OperationInput { context, .. } => {
                            Some(context.as_ref().clone())
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .unwrap();
            let effect = state
                .engine
                .event_history
                .iter()
                .find_map(|e| match e {
                    CommittedEvent::FiendishPandaJournalV1 { journal } => match journal.as_ref() {
                        custody::JournalV1::Published { item, .. } => item.inline_effect.clone(),
                        _ => None,
                    },
                    _ => None,
                })
                .unwrap();
            let bytes = serde_json::to_vec(&state).unwrap();
            assert!(execute(&mut state, &context, &effect).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
            let input = state.engine.event_history.iter().find(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), custody::JournalV1::OperationInput { .. }))).unwrap().clone();
            state.engine.event_history.push(input);
            readonly_refusal(&state);
        }
    }

    fn death_publication(state: &mut GameState, source: ObjectId, target: ObjectId) {
        event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Graveyard));
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::CastSpellOrPass { .. }
        ));
        validate_state(state).unwrap();
    }
    #[test]
    #[ignore = "Requires genuine Fiendish Panda registration and current native qualification"]
    fn actual_target_return_counter_fizzle_and_unavailable_have_distinct_closures_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for outcome in 0..3 {
                let mut state = ready_state_for(actor);
                let source = panda(&mut state, actor);
                let target = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
                death_publication(&mut state, source, target);
                let item = state.stack.last().unwrap().clone();
                match outcome {
                    0 => {
                        settle_stack(&mut state);
                        assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
                        assert!(state.objects.get(target).summoning_sick);
                        for field in 0..5 {
                            let mut corrupted = state.clone();
                            let after = corrupted
                                .engine
                                .event_history
                                .iter_mut()
                                .find_map(|e| match e {
                                    CommittedEvent::FiendishPandaJournalV1 { journal } => {
                                        match journal.as_mut() {
                                            custody::JournalV1::Operation {
                                                after: Some(after),
                                                ..
                                            } => Some(after),
                                            _ => None,
                                        }
                                    }
                                    _ => None,
                                })
                                .unwrap();
                            match field {
                                0 => after.controller = actor.opponent(),
                                1 => after.tapped = !after.tapped,
                                2 => after.summoning_sick = false,
                                3 => after.counters.plus1_plus1 = 99,
                                _ => after.v4.is_token = true,
                            }
                            readonly_refusal(&corrupted);
                        }
                        let mut corrupted = state.clone();
                        let before = corrupted
                            .engine
                            .event_history
                            .iter_mut()
                            .find_map(|e| match e {
                                CommittedEvent::FiendishPandaJournalV1 { journal } => {
                                    match journal.as_mut() {
                                        custody::JournalV1::OperationInput {
                                            before: Some(before),
                                            ..
                                        } => Some(before),
                                        _ => None,
                                    }
                                }
                                _ => None,
                            })
                            .unwrap();
                        before.zone_change_count += 1;
                        readonly_refusal(&corrupted);
                    }
                    1 => {
                        engine::counter_stack_item_by_id(&mut state, item.v4.stack_item_id)
                            .unwrap();
                        assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
                        let mut relabeled = state.clone();
                        let kind = relabeled
                            .engine
                            .event_history
                            .iter_mut()
                            .find_map(|e| match e {
                                CommittedEvent::FiendishPandaJournalV1 { journal } => {
                                    match journal.as_mut() {
                                        custody::JournalV1::Removed { kind, .. } => Some(kind),
                                        _ => None,
                                    }
                                }
                                _ => None,
                            })
                            .unwrap();
                        *kind = custody::RemovalV1::NoLegalTarget;
                        readonly_refusal(&relabeled);
                    }
                    _ => {
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(target, Zone::Exile),
                        );
                        settle_stack(&mut state);
                        assert_eq!(state.objects.get(target).zone, Zone::Exile);
                    }
                }
                validate_state(&state).unwrap();
                let restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                validate_state(&restored).unwrap();
                let mut replay = restored.clone();
                replay.stack.push(item);
                readonly_refusal(&replay);
                let mut erased = restored;
                erased.engine.event_history.retain(|e| !matches!(e, CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), custody::JournalV1::Removed { .. })));
                readonly_refusal(&erased);
            }
            let mut state = ready_state_for(actor);
            let source = panda(&mut state, actor);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            assert!(state.stack.is_empty());
            validate_state(&state).unwrap();
            assert!(state.engine.event_history.iter().any(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), custody::JournalV1::Unavailable { .. }))));
        }
    }
    #[test]
    #[ignore = "Requires genuine Fiendish Panda registration and current native qualification"]
    fn legal_death_target_cannot_be_replaced_by_fabricated_unavailable_or_fizzle_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = panda(&mut state, actor);
            let target = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            let pending = state.engine.pending_triggers[0].clone();
            let EffectOp::FiendishPandaReturnFromCaptureV1 {
                capture_history_index: capture,
                ..
            } = pending.effect
            else {
                panic!("genuine Panda death root expected")
            };
            let bytes = serde_json::to_vec(&state).unwrap();
            assert!(custody::unavailable(&mut state, &pending).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
            let mut fabricated = state.clone();
            fabricated.engine.pending_triggers.remove(0);
            fabricated
                .engine
                .event_history
                .push(CommittedEvent::FiendishPandaJournalV1 {
                    journal: Box::new(custody::JournalV1::Unavailable {
                        capture,
                        pending: Box::new(pending),
                        input: capture,
                    }),
                });
            readonly_refusal(&fabricated);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            let publication = state.engine.event_history.iter().position(|e| matches!(e, CommittedEvent::FiendishPandaJournalV1 { journal } if matches!(journal.as_ref(), custody::JournalV1::Published { .. }))).unwrap() as u64;
            let item = state.stack.pop().unwrap();
            let removal = state.engine.event_history.len() as u64;
            state
                .engine
                .event_history
                .push(CommittedEvent::FiendishPandaJournalV1 {
                    journal: Box::new(custody::JournalV1::Removed {
                        publication,
                        item: Box::new(item),
                        kind: custody::RemovalV1::NoLegalTarget,
                        input: publication,
                    }),
                });
            state
                .engine
                .event_history
                .push(CommittedEvent::FiendishPandaJournalV1 {
                    journal: Box::new(custody::JournalV1::Closed {
                        removal,
                        operation: None,
                    }),
                });
            readonly_refusal(&state);
        }
    }
    fn settle_stack(state: &mut GameState) {
        for _ in 0..32 {
            match engine::advance_until_decision(state) {
                Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                other => panic!("unexpected genuine root resolution decision {other:?}"),
            }
        }
        panic!("genuine stack failed to finish");
    }

    #[test]
    #[ignore = "Requires parent-owned genuine Fiendish Panda registration; no source-validation bypass"]
    fn actual_gain_preserves_seraph_cat_first_gain_and_each_panda_gain_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            // Select the genuine first-gain consumer at construction so its
            // production pool initializer creates the actual turn ledger.
            let seraph_def = crate::card_def::card_id_by_name("Vanguard Seraph").unwrap();
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &[seraph_def],
                &[seraph_def],
                crate::rl::card_name,
                110_120,
                actor,
            );
            state.step = crate::state::Step::Main1;
            assert!(state.life_gain_turn_v1.is_some());
            let source = panda(&mut state, actor);
            // Actual supported presence primitives exercise independent event producers.
            let seraph = put(&mut state, actor, "Vanguard Seraph", Zone::Battlefield);
            let cat = put(&mut state, actor, "Cat Collector", Zone::Battlefield);
            for amount in [4, 1] {
                event::propose_and_commit(&mut state, ProposedEvent::life_gain(actor, amount));
            }
            let pending = crate::trigger::capture_triggers_without_sba(&mut state);
            assert_eq!(pending.iter().filter(|p| p.source == source).count(), 2);
            assert_eq!(pending.iter().filter(|p| p.source == seraph).count(), 1);
            assert_eq!(pending.iter().filter(|p| p.source == cat).count(), 1);
            assert!(state.engine.halted.is_none());
            assert!(crate::trigger::capture_triggers_without_sba(&mut state).is_empty());
        }
    }

    #[test]
    #[ignore = "Requires parent-owned genuine Fiendish Panda registration; no source-validation bypass"]
    fn actual_counter_and_return_resolve_in_tyrant_and_crystal_prevention_environments_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for tyrant in [false, true] {
                let mut state = ready_state_for(actor);
                let source = panda(&mut state, actor);
                let environment = put(
                    &mut state,
                    actor,
                    if tyrant {
                        "Twinflame Tyrant"
                    } else {
                        "Crystal Barricade"
                    },
                    Zone::Battlefield,
                );
                // Real supported presence plus the existing synthetic numeric
                // shield primitive. No claim that a printing grants this shield.
                if !tyrant {
                    let id = state.engine.next_replacement_id.checked_add(1).unwrap();
                    state.engine.next_replacement_id = id;
                    state
                        .engine
                        .active_replacements
                        .push(event::ActiveReplacement {
                            id,
                            source: environment,
                            kind: event::ReplacementEffectKind::PreventNextDamage {
                                target: Target::Player(actor),
                                remaining: 1,
                            },
                        });
                }
                event::propose_and_commit(&mut state, ProposedEvent::life_gain(actor, 3));
                settle_stack(&mut state);
                assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
                assert!(state.engine.halted.is_none());
                let recipient = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                match engine::advance_until_decision(&mut state) {
                    Decision::ChooseTargets { .. } => {
                        engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient)))
                            .unwrap()
                    }
                    Decision::CastSpellOrPass { .. } => {}
                    other => panic!("unexpected actual Panda death placement {other:?}"),
                }
                settle_stack(&mut state);
                assert_eq!(state.objects.get(recipient).zone, Zone::Battlefield);
                assert!(state.objects.get(recipient).summoning_sick);
                assert!(state.engine.halted.is_none());
                if !tyrant {
                    assert!(state.engine.active_replacements.iter().any(|r| matches!(
                        r.kind,
                        event::ReplacementEffectKind::PreventNextDamage { remaining: 1, .. }
                    )));
                }
            }
        }
    }
    #[test]
    #[ignore = "Requires genuine Fiendish Panda registration and current native qualification"]
    fn actual_return_and_restoration_keep_an_unrelated_paid_banishing_link_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = panda(&mut state, actor);
            let exiled = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let aura = put(&mut state, actor, "Banishing Light", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
            engine::step(&mut state, Action::CastSpell(aura)).unwrap();
            for _ in 0..24 {
                if state.objects.get(exiled).zone == Zone::Exile {
                    break;
                }
                match engine::advance_until_decision(&mut state) {
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    Decision::ChooseTargets { .. } => {
                        engine::step(&mut state, Action::ChooseTarget(Target::Object(exiled)))
                            .unwrap()
                    }
                    other => panic!("unexpected actual Banishing exile decision {other:?}"),
                }
            }
            assert_eq!(state.objects.get(exiled).zone, Zone::Exile);
            assert_eq!(state.objects.get(aura).zone, Zone::Battlefield);
            assert!(!state.engine.linked_exile_records.is_empty());
            let target = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
            death_publication(&mut state, source, target);
            settle_stack(&mut state);
            assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
            assert!(state.objects.get(target).summoning_sick);
            assert_eq!(state.objects.get(exiled).zone, Zone::Exile);
            validate_state(&state).unwrap();
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            validate_state(&restored).unwrap();
            assert_eq!(
                serde_json::to_vec(&restored).unwrap(),
                serde_json::to_vec(&state).unwrap()
            );
        }
    }
    #[test]
    #[ignore = "Requires genuine Fiendish Panda and Doubling Season registration and current native qualification"]
    fn actual_paid_doubling_season_changes_placement_amount_without_repeating_panda_trigger_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = panda(&mut state, actor);
            let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            engine::step(&mut state, Action::CastSpell(season)).unwrap();
            settle_stack(&mut state);
            assert_eq!(state.objects.get(season).zone, Zone::Battlefield);
            event::propose_and_commit(&mut state, ProposedEvent::life_gain(actor, 6));
            settle_stack(&mut state);
            assert_eq!(state.objects.get(source).counters.plus1_plus1, 2);
            assert_eq!(state.engine.event_history.iter().filter(|e| matches!(e,
                CommittedEvent::FiendishPandaLifeGainCaptureV1 { source: captured, .. } if captured.source == source)).count(), 1);
            assert_eq!(
                state
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| matches!(e,
                CommittedEvent::PlusOneCountersAdded { object, count: 2, .. } if *object == source))
                    .count(),
                1
            );
            validate_state(&state).unwrap();
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            validate_state(&restored).unwrap();
        }
    }
    fn panda(state: &mut GameState, actor: PlayerId) -> ObjectId {
        let source = put(state, actor, "Fiendish Panda", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        engine::step(state, Action::CastSpell(source)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        for _ in 0..12 {
            if state.objects.get(source).zone == Zone::Battlefield {
                return source;
            }
            assert!(matches!(
                engine::advance_until_decision(state),
                Decision::CastSpellOrPass { .. }
            ));
            engine::step(state, Action::Pass).unwrap();
        }
        panic!("genuine paid Panda creature spell did not resolve");
    }
    #[test]
    #[ignore = "Requires parent-owned genuine Fiendish Panda registration; no source-validation bypass"]
    fn actual_paid_panda_captures_each_gain_event_once_regardless_of_amount_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = panda(&mut state, actor);
            for (player, amount) in [(actor, 4), (actor, 1), (actor.opponent(), 3), (actor, 0)] {
                event::propose_and_commit(&mut state, ProposedEvent::life_gain(player, amount));
            }
            let pending = crate::trigger::capture_triggers_without_sba(&mut state);
            let own = pending
                .iter()
                .filter(|p| p.source == source)
                .collect::<Vec<_>>();
            assert_eq!(own.len(), 2);
            assert!(own.iter().all(|p| p.controller == actor
                && p.target_spec == TargetSpec::None
                && matches!(p.effect, EffectOp::FiendishPandaCounterFromCaptureV1 { .. })));
            assert!(crate::trigger::capture_triggers_without_sba(&mut state)
                .iter()
                .all(|p| p.source != source));
            assert_eq!(
                state.objects.get(source).counters.plus1_plus1,
                0,
                "waiting counter roots cannot rescue lethal damage before resolution"
            );
        }
    }
    #[test]
    #[ignore = "Requires parent-owned genuine Fiendish Panda registration; no source-validation bypass"]
    fn actual_paid_panda_death_freezes_power_before_simultaneous_batch_removes_anthem_both_orders_and_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for reverse in [false, true] {
                let mut state = ready_state_for(actor);
                let source = panda(&mut state, actor);
                let anthem = put(&mut state, actor, "Anthem of Champions", Zone::Battlefield);
                assert_eq!(crate::engine::effective_power(&state, source), 4);
                let mut deaths = vec![anthem, source];
                if reverse {
                    deaths.reverse();
                }
                event::propose_and_commit_batch(
                    &mut state,
                    deaths
                        .into_iter()
                        .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
                        .collect(),
                );
                let pending = crate::trigger::capture_triggers_without_sba(&mut state);
                let own = pending
                    .iter()
                    .filter(|p| p.source == source)
                    .collect::<Vec<_>>();
                assert_eq!(own.len(), 1);
                assert_eq!(own[0].controller, actor);
                assert_eq!(
                    own[0].target_spec,
                    TargetSpec::AnotherNonBearCreatureCardInOwnGraveyardManaValueAtMost(4)
                );
                assert!(matches!(
                    own[0].effect,
                    EffectOp::FiendishPandaReturnFromCaptureV1 { maximum: 4, .. }
                ));
                // The capture API returns a private batch. Install its genuine
                // pending owners before checking the externally held state.
                state.engine.pending_triggers.extend(pending);
                assert!(validate_state(&state).is_ok());
            }
        }
    }
}
