//! Genuine private declaration stage, before its mandatory collection checkpoint.
use super::*;
use crate::ashroot_dispatch_v2::JournalV2;
use crate::policy_observation_v6::tests::{put, ready_state};

#[test]
fn actual_pre_collection_attack_marker_erasure_refuses_and_public_checkpoint_matches_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        state.step = Step::DeclareAttackers;
        let source = put(&mut state, actor, "Ashroot Animist", Zone::Battlefield);
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        state.objects.get_mut(source).summoning_sick = false;
        validate_declare_attackers(&state, &[source]).unwrap();
        let mut public_path = state.clone();
        assert!(!crate::attack_target_v1::begin(&mut state, &[source]));
        // Both paths use the same actual production declaration commit. No
        // DeclaredAttacker or capture journal is manufactured by this fixture.
        commit_attack_declaration(&mut state, vec![source]);
        crate::ashroot_dispatch_v2::validate_state(&state).unwrap();
        assert_eq!(
            state
                .engine
                .event_log
                .iter()
                .filter(|e| matches!(
                    e,
                    CommittedEvent::AshrootDispatchV2 {
                        journal: JournalV2::Capture { .. }
                    }
                ))
                .count(),
            1
        );
        assert!(!state.engine.event_history.iter().any(|e| matches!(
            e,
            CommittedEvent::AshrootDispatchV2 {
                journal: JournalV2::Collected { .. }
            }
        )));
        for erase_history in [false, true] {
            let mut bad = state.clone();
            if erase_history {
                // Preserve the independent original attack/declaration journal.
                bad.engine.event_history.retain(|e| {
                    !matches!(
                        e,
                        CommittedEvent::AshrootDispatchV2 {
                            journal: JournalV2::Capture { .. }
                        }
                    )
                });
            } else {
                bad.engine.event_log.retain(|e| {
                    !matches!(
                        e,
                        CommittedEvent::AshrootDispatchV2 {
                            journal: JournalV2::Capture { .. }
                        }
                    )
                });
            }
            assert!(crate::ashroot_dispatch_v2::validate_state(&bad).is_err());
            let bytes = serde_json::to_vec(&bad).unwrap();
            for _ in 0..2 {
                let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
                assert!(step(&mut restored, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
            }
        }
        collect_and_queue_triggers(&mut state);
        reset_priority(&mut state);
        step(&mut public_path, Action::DeclareAttackers(vec![source])).unwrap();
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&public_path).unwrap()
        );
        assert_eq!(state.engine.pending_triggers.len(), 1);
        crate::ashroot_dispatch_v2::validate_state(&state).unwrap();
    }
}
