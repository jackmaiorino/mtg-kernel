//! Registered absence operands exercise the unchanged common target/damage paths.
use super::*;
use crate::card_def::TargetSpec;
use crate::engine;
use crate::event::{self, ProposedEvent};
use crate::policy_observation_v6::tests::{put, ready_state};

#[test]
fn absent_registered_source_preserves_both_player_targets_and_real_damage_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.priority_player = actor;
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let before = serde_json::to_vec(&state).unwrap();
        assert!(!active(&state));
        assert!(!controller_has_hexproof(&state, actor));
        assert!(!prevents(
            &state,
            match &ProposedEvent::damage(source, Target::Object(target), 1) {
                ProposedEvent::Damage(d) => d,
                _ => unreachable!(),
            }
        ));
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let targets = engine::legal_targets_for(TargetSpec::AnyTarget, &[], &state);
        assert!(targets.contains(&Target::Player(actor)));
        assert!(targets.contains(&Target::Player(actor.opponent())));
        let mut replay: GameState = serde_json::from_slice(&before).unwrap();
        for current in [&mut state, &mut replay] {
            event::propose_and_commit(
                current,
                ProposedEvent::damage(source, Target::Object(target), 1),
            );
            assert_eq!(current.objects.get(target).damage, 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn absent_registered_source_never_prevents_combat_player_or_zero_proposals() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        for target in [Target::Player(actor), Target::Object(source)] {
            for amount in [0, 1, 10] {
                let ProposedEvent::Damage(mut damage) =
                    ProposedEvent::damage(source, target, amount)
                else {
                    unreachable!()
                };
                for combat in [false, true] {
                    damage.is_combat = combat;
                    assert!(!prevents(&state, &damage));
                }
            }
        }
    }
}
