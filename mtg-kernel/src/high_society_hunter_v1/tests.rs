use super::*;
use crate::event::{self, ProposedEvent};
use crate::policy_observation_v6::tests::put;

#[test]
fn registered_counter_recipient_primitive_binds_incarnation_and_keeps_original_controller_after_restore(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for leave in [false, true] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            // Actual registered permanent, exercising only the shared exact-source counter primitive.
            let source = put(&mut state, actor, "Forest", Zone::Battlefield);
            let captured = AbilitySourceContractV4::capture(&state, source);
            if leave {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                );
            } else {
                state.players[actor.index()]
                    .battlefield
                    .retain(|&id| id != source);
                state.players[actor.opponent().index()]
                    .battlefield
                    .push(source);
                state.objects.get_mut(source).controller = actor.opponent();
            }
            let bytes = serde_json::to_vec(&state).unwrap();
            let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
            for current in [&mut state, &mut replay] {
                counter_exact_source(current, captured).unwrap();
                assert_eq!(
                    current.objects.get(source).counters.plus1_plus1,
                    i32::from(!leave)
                );
                assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
                // Stand-in cannot authenticate or execute a printed Hunter sacrifice root.
                let before = serde_json::to_vec(current).unwrap();
                assert!(validate_source(current, captured).is_err());
                assert_eq!(serde_json::to_vec(current).unwrap(), before);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn registered_absence_has_no_new_capture_or_consumer_mutation_and_retains_state_hash() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let source = put(&mut state, actor, "Forest", Zone::Battlefield);
        let declared = CommittedEvent::DeclaredAttacker {
            source,
            source_zone_change_count: state.objects.get(source).zone_change_count,
            controller: actor,
        };
        state.engine.event_history.push(declared.clone());
        state.engine.event_log.push(declared);
        let before = serde_json::to_vec(&state).unwrap();
        let hash = state.state_hash();
        let index = state.engine.event_history.len() - 1;
        capture_attack(&mut state, index);
        assert!(collect_attacks(&mut state, &[]).unwrap().is_empty());
        require_legacy_supported(&state, Some(actor)).unwrap();
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert_eq!(state.state_hash(), hash);
    }
}
