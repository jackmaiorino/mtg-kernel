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
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state
            .engine
            .active_replacements
            .push(event::ActiveReplacement {
                id: crate::ids::ReplacementId(701),
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
                id: crate::ids::ReplacementId(702),
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
