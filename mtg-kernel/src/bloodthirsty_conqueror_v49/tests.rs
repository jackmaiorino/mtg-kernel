use super::*;
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::policy_observation_v6::tests::put;
use crate::state::Target;
fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    state
}
fn latest(state: &GameState) -> (&BatchV49, usize) {
    let index = state.engine.event_history.len() - 1;
    let CommittedEvent::ConquerorLossBatchV49 { batch } = &state.engine.event_history[index] else {
        panic!("actual physical loss operation missing");
    };
    (batch, index)
}
/// Physical genuine amount-only history; it owns no printed Conqueror trigger.
pub(crate) fn amount_only_sampler_state(actor: PlayerId) -> Option<GameState> {
    if enabled() {
        return None;
    }
    let mut state = ready(actor);
    let recorder = RecorderV49::original(&state).unwrap();
    event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 3));
    recorder.finish(&mut state);
    validate(&state).unwrap();
    assert!(!matches!(
        crate::engine::advance_until_decision(&mut state),
        crate::engine::Decision::Halted { .. }
    ));
    assert!(state.engine.event_log.is_empty());
    Some(state)
}
#[test]
fn absent_printing_keeps_actual_registered_operations_bytes_and_no_new_history() {
    if enabled() {
        return;
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let before = serde_json::to_vec(&state).unwrap();
        let hash = state.diagnostic_state_hash();
        assert!(RecorderV49::capture(&state).is_none());
        validate(&state).unwrap();
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert_eq!(state.diagnostic_state_hash(), hash);
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 2));
        assert!(state
            .engine
            .event_history
            .iter()
            .all(|event| !matches!(event, CommittedEvent::ConquerorLossBatchV49 { .. })));
    }
}
#[test]
fn physical_simultaneous_losses_sum_once_per_player_and_exclude_gain_zero_and_object_damage() {
    if enabled() {
        return;
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let creature = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        // This is a physical committed-amount primitive, not a registered Conqueror trigger.
        let recorder = RecorderV49::original(&state).unwrap();
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::life_loss(actor.opponent(), 2),
                ProposedEvent::life_payment(actor.opponent(), 3),
                ProposedEvent::damage(source, Target::Player(actor.opponent()), 4),
                ProposedEvent::life_loss(actor, 1),
                ProposedEvent::life_gain(actor.opponent(), 2),
                ProposedEvent::life_loss(actor.opponent(), 0),
                ProposedEvent::damage(source, Target::Object(creature), 1),
            ],
        );
        recorder.finish(&mut state);
        let (batch, index) = latest(&state);
        assert_eq!(batch.amount_for(actor.opponent()).unwrap(), 9);
        assert_eq!(batch.amount_for(actor).unwrap(), 1);
        assert_eq!(batch.losses.len(), 4);
        batch.validate(&state, index).unwrap();
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored, state);
        validate(&restored).unwrap();
        assert!(state
            .engine
            .event_log
            .iter()
            .all(|event| !matches!(event, CommittedEvent::ConquerorLossBatchV49 { .. })));
    }
}
#[test]
fn separate_real_loss_operations_keep_distinct_amounts_and_history_preserving_forgeries_refuse() {
    if enabled() {
        return;
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        for amount in [2, 3] {
            let recorder = RecorderV49::original(&state).unwrap();
            event::propose_and_commit(
                &mut state,
                ProposedEvent::life_loss(actor.opponent(), amount),
            );
            recorder.finish(&mut state);
            assert_eq!(
                latest(&state).0.amount_for(actor.opponent()).unwrap(),
                amount
            );
        }
        validate(&state).unwrap();
        for case in 0..3 {
            let mut forged: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            let CommittedEvent::ConquerorLossBatchV49 { batch } =
                forged.engine.event_history.last_mut().unwrap()
            else {
                unreachable!()
            };
            match case {
                0 => batch.losses[0].amount += 1,
                1 => batch.losses[0].player = actor,
                _ => batch.history_start = 0,
            }
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(crate::engine::step(&mut forged, crate::engine::Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
            let decision = crate::engine::advance_until_decision(&mut forged);
            assert!(matches!(
                decision,
                crate::engine::Decision::Halted {
                    source: ObjectId(0),
                    ..
                }
            ));
            let halted = forged.engine.halted.take();
            assert!(halted.is_some());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        }
    }
}

#[test]
fn physical_prevention_records_only_actual_surviving_player_loss() {
    if enabled() {
        return;
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state
            .engine
            .active_replacements
            .push(event::ActiveReplacement {
                id: 701,
                source,
                kind: event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Player(actor.opponent()),
                    remaining: 4,
                },
            });
        let recorder = RecorderV49::original(&state).unwrap();
        event::propose_and_commit(
            &mut state,
            ProposedEvent::damage(source, Target::Player(actor.opponent()), 6),
        );
        recorder.finish(&mut state);
        assert_eq!(latest(&state).0.amount_for(actor.opponent()).unwrap(), 2);
        validate(&state).unwrap();
        state
            .engine
            .active_replacements
            .push(event::ActiveReplacement {
                id: 702,
                source,
                kind: event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Player(actor.opponent()),
                    remaining: 4,
                },
            });
        let recorder = RecorderV49::original(&state).unwrap();
        let before = state.engine.event_history.len();
        event::propose_and_commit(
            &mut state,
            ProposedEvent::damage(source, Target::Player(actor.opponent()), 3),
        );
        recorder.finish(&mut state);
        assert_eq!(state.engine.event_history.len(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod future_printed {
    use super::*;
    use crate::effect::EffectOp;
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    fn next(state: &mut GameState) -> Decision {
        let decision = engine::advance_until_decision(state);
        assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
        decision
    }
    pub(crate) fn finish(state: &mut GameState) {
        for _ in 0..50 {
            let decision = next(state);
            if state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && matches!(decision, Decision::CastSpellOrPass { .. })
            {
                return;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                Decision::OrderTriggers { pending, .. } => {
                    engine::step(state, Action::OrderTriggers((0..pending.len()).collect()))
                        .unwrap()
                }
                other => panic!("actual Conqueror completion {other:?}"),
            }
        }
        panic!("actual completion missing");
    }
    pub(crate) fn paid(actor: PlayerId) -> (GameState, ObjectId) {
        let mut state = ready(actor);
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let source = put(&mut state, actor, "Bloodthirsty Conqueror", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        finish(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        (state, source)
    }
    fn readonly(mut state: GameState) {
        state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::Halted { .. }
        ));
        assert!(state.engine.halted.take().is_some());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
    #[test]
    #[ignore = "genuine Conqueror registration and native qualification belong to parent"]
    fn genuine_paid_capture_held_publication_removal_gain_and_closed_forgeries_refuse_original_journals(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source) = paid(actor);
            event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 4));
            let mut lost = state.clone();
            lost.engine
                .event_log
                .retain(|event| !matches!(event, CommittedEvent::ConquerorCapturedV49 { .. }));
            readonly(lost);
            next(&mut state);
            let item = state
                .stack
                .iter()
                .find(|item| {
                    item.source == source
                        && matches!(
                            item.inline_effect,
                            Some(EffectOp::ConquerorGainFromCaptureV49 { .. })
                        )
                })
                .unwrap()
                .clone();
            let publication = state.engine.event_history.iter().position(|event| matches!(event, CommittedEvent::ConquerorJournalV49 { journal } if matches!(journal.as_ref(), custody::JournalV49::Published { item: published, .. } if published.as_ref() == &item))).unwrap() as u64;
            for case in 0..4 {
                let mut forged = state.clone();
                match case {
                    0 => forged
                        .stack
                        .retain(|row| row.v4.stack_item_id != item.v4.stack_item_id),
                    1 => {
                        forged.engine.event_history.remove(publication as usize);
                    }
                    2 => {
                        forged
                            .stack
                            .retain(|row| row.v4.stack_item_id != item.v4.stack_item_id);
                        let removal = forged.engine.event_history.len() as u64;
                        forged
                            .engine
                            .event_history
                            .push(CommittedEvent::ConquerorJournalV49 {
                                journal: Box::new(custody::JournalV49::Removed {
                                    publication,
                                    input: u64::MAX,
                                    countered: true,
                                }),
                            });
                        forged
                            .engine
                            .event_history
                            .push(CommittedEvent::ConquerorJournalV49 {
                                journal: Box::new(custody::JournalV49::Closed {
                                    removal,
                                    gain: None,
                                }),
                            });
                    }
                    _ => {
                        let mut changed = item.clone();
                        changed.controller = actor.opponent();
                        *forged
                            .stack
                            .iter_mut()
                            .find(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                            .unwrap() = changed.clone();
                        let CommittedEvent::ConquerorJournalV49 { journal } =
                            &mut forged.engine.event_history[publication as usize]
                        else {
                            unreachable!()
                        };
                        let custody::JournalV49::Published { item, .. } = journal.as_mut() else {
                            unreachable!()
                        };
                        *item = Box::new(changed);
                    }
                }
                readonly(forged);
            }
            let mut countered =
                serde_json::from_slice::<GameState>(&serde_json::to_vec(&state).unwrap()).unwrap();
            // Genuine accepted typed counter-removal primitive, not a fabricated printed spell.
            engine::counter_stack_item_by_id(&mut countered, item.v4.stack_item_id).unwrap();
            validate(&countered).unwrap();
            assert_eq!(countered.players[actor.index()].life, 20);
            finish(&mut state);
            assert_eq!(state.players[actor.index()].life, 24);
            let mut erased = state.clone();
            erased.engine.event_history.retain(|event| !matches!(event, CommittedEvent::ConquerorJournalV49 { journal } if matches!(journal.as_ref(), custody::JournalV49::GainInput { .. } | custody::JournalV49::Gain { .. } | custody::JournalV49::Closed { .. })));
            readonly(erased);
        }
    }
    #[test]
    #[ignore = "genuine Conqueror registration and native qualification belong to parent"]
    fn original_post_event_universe_prevents_coherent_derived_observer_erasure_and_supports_simultaneous_departure(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source) = paid(actor);
            event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 2));
            let mut forged = state.clone();
            for event in &mut forged.engine.event_history {
                if let CommittedEvent::ConquerorLossBatchV49 { batch } = event {
                    batch.observers.clear();
                }
            }
            forged
                .engine
                .event_history
                .retain(|event| !matches!(event, CommittedEvent::ConquerorCapturedV49 { .. }));
            forged
                .engine
                .event_log
                .retain(|event| !matches!(event, CommittedEvent::ConquerorCapturedV49 { .. }));
            readonly(forged);
            for reverse in [false, true] {
                let (mut simultaneous, original) = paid(actor);
                let mut packets = vec![
                    ProposedEvent::zone_change(original, Zone::Graveyard),
                    ProposedEvent::life_loss(actor.opponent(), 3),
                ];
                if reverse {
                    packets.reverse();
                }
                event::propose_and_commit_batch(&mut simultaneous, packets);
                assert_eq!(simultaneous.objects.get(original).zone, Zone::Graveyard);
                assert!(!simultaneous
                    .engine
                    .event_history
                    .iter()
                    .any(|event| matches!(event, CommittedEvent::ConquerorCapturedV49 { .. })));
                validate(&simultaneous).unwrap();
            }
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        }
    }
}
