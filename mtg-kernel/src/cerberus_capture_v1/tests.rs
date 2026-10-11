use super::*;
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::engine::{self, Action, Decision};
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::state::Step;

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
#[test]
fn registered_absence_preserves_bytes_hash_and_has_no_new_occurrence() {
    use std::hash::{Hash, Hasher};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let before = serde_json::to_vec(&state).unwrap();
        let mut old = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut old);
        record_begin(&mut state, 0);
        validate_state(&state).unwrap();
        let mut current = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut current);
        assert_eq!(old.finish(), current.finish());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert!(!requires_successor(&state));
    }
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn actual_trigger(actor: PlayerId) -> (GameState, ObjectId) {
    actual_trigger_group(actor, false)
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_trigger_group(actor: PlayerId, duplicate: bool) -> (GameState, ObjectId) {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    state.step = Step::Main1;
    for seat in [PlayerId::P0, PlayerId::P1] {
        put(&mut state, seat, "Lightning Bolt", Zone::Hand);
        put(&mut state, seat, "Mountain", Zone::Battlefield);
    }
    let source = put(&mut state, actor, "Slumbering Cerberus", Zone::Hand);
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 1;
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    for _ in 0..24 {
        let decision = engine::advance_until_decision(&mut state);
        if state.objects.get(source).zone == Zone::Battlefield {
            break;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(&mut state, Action::Pass).unwrap();
    }
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    state.objects.get_mut(source).tapped = true;
    if duplicate {
        // A second physical printed observer; the first source above was really cast.
        let second = put(&mut state, actor, "Slumbering Cerberus", Zone::Battlefield);
        state.objects.get_mut(second).tapped = true;
    }
    let dead = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    crate::event::propose_and_commit(
        &mut state,
        crate::event::ProposedEvent::zone_change(dead, Zone::Graveyard),
    );
    state.step = Step::Main2;
    for seat in [PlayerId::P0, PlayerId::P1] {
        state.players[seat.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
    }
    for _ in 0..24 {
        let decision = engine::advance_until_decision(&mut state);
        if duplicate && matches!(decision, Decision::OrderTriggers { .. }) {
            validate_state(&state).unwrap();
            return (state, source);
        }
        if state
            .stack
            .iter()
            .any(|item| matches!(item.inline_effect, Some(EffectOp::CerberusUntapV1 { .. })))
        {
            assert_eq!(state.step, Step::End);
            validate_state(&state).unwrap();
            return (state, source);
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(&mut state, Action::Pass).unwrap();
    }
    panic!("real end-step trigger not reached");
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn resolve_actual(state: &mut GameState) {
    for _ in 0..24 {
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty() {
            validate_state(state).unwrap();
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("actual trigger did not resolve");
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn printed_capture_control_stun_departure_morbid_recheck_and_counter_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..6 {
            let (mut state, source) = actual_trigger(actor);
            match mode {
                1 => state.objects.get_mut(source).controller = actor.opponent(),
                2 => state.objects.get_mut(source).counters.stun = 1,
                3 => {
                    crate::event::propose_and_commit(
                        &mut state,
                        crate::event::ProposedEvent::zone_change(source, Zone::Hand),
                    );
                    crate::event::propose_and_commit(
                        &mut state,
                        crate::event::ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                    state.objects.get_mut(source).tapped = true;
                }
                // A typed recheck probe, not priority crossing a normal turn.
                4 => state.creature_death_turn_v1 = None,
                _ => {}
            }
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                if mode == 5 {
                    let id = branch.stack.last().unwrap().v4.stack_item_id;
                    assert!(engine::counter_stack_item_by_id(branch, id)
                        .unwrap()
                        .is_some());
                    validate_state(branch).unwrap();
                } else {
                    resolve_actual(branch);
                }
                assert_eq!(branch.objects.get(source).tapped, mode >= 2);
                assert!(branch
                    .engine
                    .event_history
                    .iter()
                    .any(|event| matches!(event, CommittedEvent::CerberusCompletedV1 { .. })));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            let mut corrupt = restored(&state);
            corrupt
                .engine
                .event_history
                .retain(|event| !matches!(event, CommittedEvent::CerberusCompletedV1 { .. }));
            let bytes = serde_json::to_vec(&corrupt).unwrap();
            assert!(engine::step(&mut corrupt, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&corrupt).unwrap(), bytes);
        }
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn committed_capture_publication_and_closed_owner_forgeries_are_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, _) = actual_trigger(actor);
        for mode in 0..8 {
            let mut corrupt = restored(&state);
            match mode {
                0 => corrupt.stack.clear(),
                1 => corrupt
                    .engine
                    .event_history
                    .retain(|event| !matches!(event, CommittedEvent::CerberusPublishedV1 { .. })),
                2 => corrupt
                    .engine
                    .event_history
                    .retain(|event| !matches!(event, CommittedEvent::CerberusCaptureV1 { .. })),
                3 => {
                    corrupt.engine.event_history.retain(|event| {
                        !matches!(event, CommittedEvent::CerberusCollectedV1 { .. })
                    });
                    corrupt.engine.pending_triggers.clear();
                    corrupt.stack.clear();
                }
                4 => corrupt.stack.last_mut().unwrap().controller = actor.opponent(),
                5 => {
                    let event = corrupt
                        .engine
                        .event_history
                        .iter()
                        .find(|event| matches!(event, CommittedEvent::CerberusCaptureV1 { .. }))
                        .cloned()
                        .unwrap();
                    corrupt.engine.event_log.push(event);
                }
                6 => {
                    let event = corrupt
                        .engine
                        .event_history
                        .iter()
                        .find(|event| matches!(event, CommittedEvent::CerberusPublishedV1 { .. }))
                        .cloned()
                        .unwrap();
                    corrupt.engine.event_history.push(event);
                }
                _ => {
                    corrupt.stack.last_mut().unwrap().inline_effect =
                        Some(crate::slumbering_cerberus_v1::program_v1())
                }
            }
            let mut corrupt = restored(&corrupt);
            let bytes = serde_json::to_vec(&corrupt).unwrap();
            assert!(validate_state(&corrupt).is_err(), "mode {mode}");
            assert!(
                engine::step(&mut corrupt, Action::Pass).is_err(),
                "mode {mode}"
            );
            assert_eq!(serde_json::to_vec(&corrupt).unwrap(), bytes, "mode {mode}");
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn genuine_waiting_ordered_trigger_owners_survive_restore_and_cannot_be_dropped() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = actual_trigger_group(actor, true);
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::OrderTriggers { ref pending, .. } if pending.len() == 2));
        let mut replay = restored(&state);
        let mut corrupt = restored(&state);
        corrupt.engine.pending_triggers.pop();
        let before = serde_json::to_vec(&corrupt).unwrap();
        assert!(engine::step(&mut corrupt, Action::OrderTriggers(vec![0])).is_err());
        assert_eq!(serde_json::to_vec(&corrupt).unwrap(), before);
        for branch in [&mut state, &mut replay] {
            engine::step(branch, Action::OrderTriggers(vec![1, 0])).unwrap();
            engine::advance_until_decision(branch);
            validate_state(branch).unwrap();
            assert_eq!(branch.stack.len(), 2);
            resolve_actual(branch);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn completed_live_source_cannot_be_rewritten_to_absent_before_or_after_later_departure() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for stun in [0, 1] {
            let (mut state, source) = actual_trigger(actor);
            state.objects.get_mut(source).counters.stun = stun;
            resolve_actual(&mut state);
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
            assert_eq!(state.objects.get(source).tapped, stun > 0);
            assert_eq!(state.objects.get(source).counters.stun, 0);
            for depart_after_operation in [false, true] {
                let mut corrupt = restored(&state);
                if depart_after_operation {
                    crate::event::propose_and_commit(
                        &mut corrupt,
                        crate::event::ProposedEvent::zone_change(source, Zone::Hand),
                    );
                    validate_state(&corrupt).unwrap();
                }
                let kind = corrupt
                    .engine
                    .event_history
                    .iter_mut()
                    .find_map(|event| match event {
                        CommittedEvent::CerberusOperationV1 { kind, .. } => Some(kind),
                        _ => None,
                    })
                    .unwrap();
                let CerberusOperationKindV1::Resolved {
                    before_tapped,
                    before_stun,
                    after_tapped,
                    after_stun,
                    ..
                } = kind
                else {
                    panic!("actual resolved operation");
                };
                *before_tapped = None;
                *before_stun = None;
                *after_tapped = None;
                *after_stun = None;
                let mut corrupt = restored(&corrupt);
                let before = serde_json::to_vec(&corrupt).unwrap();
                assert!(validate_state(&corrupt).is_err());
                assert!(engine::step(&mut corrupt, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&corrupt).unwrap(), before);
                assert!(matches!(
                    engine::advance_until_decision(&mut corrupt),
                    Decision::Halted { .. }
                ));
                assert_eq!(
                    corrupt.engine.halted,
                    Some((
                        engine::UnsupportedMechanic::InvalidEffectContinuation,
                        source,
                    ))
                );
                // The fail-closed halt is the only permitted boundary mutation.
                corrupt.engine.halted = None;
                assert_eq!(serde_json::to_vec(&corrupt).unwrap(), before);
            }
        }
    }
}
