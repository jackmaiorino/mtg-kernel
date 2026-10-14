use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};

#[test]
fn unrelated_registered_entries_and_real_token_copy_preserve_absent_authority_state() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        let elf = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(elf, actor),
        );
        assert!(!state.objects.get(elf).tapped);
        let token =
            crate::standard_cards_v1::create_token_copy(&mut state, elf, actor.opponent()).unwrap();
        assert!(!state.objects.get(token).tapped);
        assert!(state.objects.get(token).v4.is_token);
        assert!(!state.engine.event_history.iter().any(|event| matches!(
            event,
            CommittedEvent::AuthorityEntryCaptureV1 { .. }
                | CommittedEvent::AuthorityEntryCollectedV1 { .. }
                | CommittedEvent::AuthorityEntryObserversV1 { .. }
                | CommittedEvent::AuthorityEntryCompletedV1 { .. }
                | CommittedEvent::AuthorityEntryOperationV1 { .. }
                | CommittedEvent::AuthorityEntryPublishedV1 { .. }
        )));
        assert!(!requires_successor(&state));
        let before = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&before).unwrap();
        assert_eq!(
            restored.diagnostic_state_hash(),
            state.diagnostic_state_hash()
        );
        validate_state(&restored).unwrap();
        let events = restored.engine.event_log.clone();
        assert!(collect(&mut restored, &events).unwrap().is_empty());
        assert_eq!(serde_json::to_vec(&restored).unwrap(), before);
        assert!(validate_source(
            &restored,
            &EffectOp::AuthorityEntryGainLifeV1 {
                capture_history_index: 0
            },
            Some(AbilitySourceContractV4::capture(&restored, elf))
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&restored).unwrap(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod printed {
    use super::*;
    use crate::engine::{self, Action, Decision};
    use crate::policy_observation_v6::tests::ready_state_for;
    fn settle(state: &mut GameState) {
        for _ in 0..40 {
            match engine::advance_until_decision(state) {
                Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                Decision::OrderTriggers { pending, .. } => {
                    engine::step(state, Action::OrderTriggers((0..pending.len()).collect()))
                        .unwrap()
                }
                other => panic!("unexpected actual entry decision {other:?}"),
            }
        }
        panic!("entry did not settle");
    }
    #[test]
    fn actual_captures_authenticate_typed_deferred_and_legend_return_owners() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for transport in 0..2 {
                let mut state = ready_state_for(actor);
                let observer = put(
                    &mut state,
                    actor,
                    "Authority of the Consuls",
                    Zone::Battlefield,
                );
                let elf = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(elf, Zone::Battlefield),
                );
                let events = state.engine.event_log.clone();
                let pending = collect(&mut state, &events).unwrap();
                assert_eq!(pending.len(), 1);
                state.engine.event_log.clear();
                if transport == 0 {
                    state.legend_pending_v1 = Some(pending);
                } else {
                    // A typed owner-storage probe, not a fabricated printed
                    // resolution-play producer. Actual capture/source authority
                    // and the consumer's real deferred field are used.
                    state
                        .standard_v1
                        .get_or_insert_with(Default::default)
                        .resolution_play =
                        Some(crate::engine::resolution_cast_v1::ResolutionPlayV1 {
                            parent: crate::ids::StackItemId::default(),
                            parent_index: 0,
                            suspended: None,
                            permission: None,
                            deferred_triggers: pending,
                            kicked_source: None,
                            authorization: None,
                        });
                }
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(observer, Zone::Hand),
                );
                validate_state(&state).unwrap();
                assert!(requires_successor(&state));
                let bytes = serde_json::to_vec(&state).unwrap();
                let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
                validate_state(&replay).unwrap();
                assert_eq!(serde_json::to_vec(&replay).unwrap(), bytes);
                if transport == 0 {
                    replay.legend_pending_v1 = None;
                } else {
                    replay
                        .standard_v1
                        .as_mut()
                        .unwrap()
                        .resolution_play
                        .as_mut()
                        .unwrap()
                        .deferred_triggers
                        .clear();
                }
                let before = serde_json::to_vec(&replay).unwrap();
                assert!(validate_state(&replay).is_err());
                assert_eq!(serde_json::to_vec(&replay).unwrap(), before);
            }
        }
    }
    #[test]
    fn actual_simultaneous_token_entry_and_accepted_ability_counter_close_exact_dispatch() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for observer_first in [false, true] {
                let mut state = ready_state_for(actor);
                let observer = put(&mut state, actor, "Authority of the Consuls", Zone::Hand);
                let token_def = crate::card_def::card_id_by_name("Human Soldier Token").unwrap();
                assert!(crate::card_def::CARD_DEFS[token_def as usize].is_token);
                let observer_entry = ProposedEvent::zone_change(observer, Zone::Battlefield);
                let creature_entry = ProposedEvent::create_token(token_def, actor.opponent());
                event::propose_and_commit_batch(
                    &mut state,
                    if observer_first {
                        vec![observer_entry, creature_entry]
                    } else {
                        vec![creature_entry, observer_entry]
                    },
                );
                let entrant = state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::CreateToken {
                            object,
                            token_def: created,
                            controller,
                        } if *created == token_def && *controller == actor.opponent() => {
                            Some(*object)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert!(!state.objects.get(entrant).tapped);
                assert!(state.objects.get(entrant).v4.is_token);
                assert_eq!(state.objects.get(entrant).owner, actor.opponent());
                let captures = state
                    .engine
                    .event_log
                    .iter()
                    .filter_map(|event| match event {
                        CommittedEvent::AuthorityEntryCaptureV1 { capture } => Some(capture),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(captures.len(), 1);
                assert_eq!(captures[0].entrant.object, entrant);
                assert_eq!(captures[0].source.source, observer);
                assert_eq!(captures[0].source.controller, actor);
                assert!(matches!(
                    engine::advance_until_decision(&mut state),
                    Decision::CastSpellOrPass { .. }
                ));
                let item = state.stack.last().unwrap().clone();
                assert!(matches!(
                    item.inline_effect,
                    Some(EffectOp::AuthorityEntryGainLifeV1 { .. })
                ));
                // Genuine accepted core counter primitive. This does not claim
                // a printed FDN card that counters triggered abilities.
                engine::counter_stack_item_by_id(&mut state, item.v4.stack_item_id).unwrap();
                validate_state(&state).unwrap();
                assert_eq!(state.players[actor.index()].life, 20);
                assert!(state.stack.is_empty());
                let mut replay: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                validate_state(&replay).unwrap();
                replay.stack.push(item);
                let before = serde_json::to_vec(&replay).unwrap();
                assert!(engine::step(&mut replay, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&replay).unwrap(), before);
            }
        }
    }
    #[test]
    fn future_actual_token_entry_captured_observer_survives_control_departure_restore_and_log_forgery(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let observer = put(
                &mut state,
                actor,
                "Authority of the Consuls",
                Zone::Battlefield,
            );
            settle(&mut state);
            let original = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            settle(&mut state);
            let life = state.players[actor.index()].life;
            let token =
                crate::standard_cards_v1::create_token_copy(&mut state, original, actor.opponent())
                    .unwrap();
            assert!(state.objects.get(token).tapped);
            let capture = state
                .engine
                .event_log
                .iter()
                .find_map(|event| match event {
                    CommittedEvent::AuthorityEntryCaptureV1 { capture } => Some(capture.clone()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(capture.entrant.object, token);
            assert_eq!(capture.entrant_controller, actor.opponent());
            assert_eq!(capture.source.source, observer);
            assert_eq!(capture.source.controller, actor);
            let opposing_observer = put(
                &mut state,
                actor.opponent(),
                "Authority of the Consuls",
                Zone::Battlefield,
            );
            let events = state.engine.event_log.clone();
            let before = serde_json::to_vec(&state).unwrap();
            for corruption in 0..4 {
                let mut forged = events.clone();
                match corruption {
                    0 => forged.retain(|event| {
                        !matches!(event, CommittedEvent::AuthorityEntryCaptureV1 { .. })
                    }),
                    1 => forged.push(CommittedEvent::AuthorityEntryCaptureV1 {
                        capture: capture.clone(),
                    }),
                    2 => {
                        let altered = forged
                            .iter_mut()
                            .find_map(|event| match event {
                                CommittedEvent::AuthorityEntryCaptureV1 { capture } => {
                                    Some(capture)
                                }
                                _ => None,
                            })
                            .unwrap();
                        altered.source.controller = actor.opponent();
                    }
                    _ => {
                        let mut extra = capture.clone();
                        extra.source = AbilitySourceContractV4::capture(&state, opposing_observer);
                        // Coherent real opposing incarnation with a forged
                        // original beneficiary cannot acquire another trigger.
                        extra.source.controller = actor;
                        forged.push(CommittedEvent::AuthorityEntryCaptureV1 { capture: extra });
                    }
                }
                assert!(collect(&mut state, &forged).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            }
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            assert_eq!(state.stack.last().unwrap().controller, actor);
            let after = serde_json::to_vec(&state).unwrap();
            assert!(collect(&mut state, &events).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), after);
            crate::standard_cards_v1::gain_control(&mut state, observer, actor.opponent());
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(observer, Zone::Hand));
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(token, Zone::Hand));
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for current in [&mut state, &mut restored] {
                settle(current);
                assert_eq!(current.players[actor.index()].life, life + 1);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
    #[test]
    fn future_actual_suppressed_printed_observer_uses_accepted_hidden_permanent_primitive() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            let observer = put(
                &mut state,
                actor,
                "Authority of the Consuls",
                Zone::Battlefield,
            );
            // Physical face-down characteristic primitive, not a claim that
            // the FDN printed catalog supplies a face-down producer.
            state.objects.get_mut(observer).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: false,
                lookers: 1 << actor.index(),
                hidden_by: None,
                hidden_by_source: None,
            });
            assert!(!active(&state, observer));
            let elf = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            assert!(!state.objects.get(elf).tapped);
            assert!(!state
                .engine
                .event_log
                .iter()
                .any(|event| matches!(event, CommittedEvent::AuthorityEntryCaptureV1 { .. })));
        }
    }
}
