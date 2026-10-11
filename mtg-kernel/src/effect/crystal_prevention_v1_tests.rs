//! Registered absence operands, not Crystal admission.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};
#[test]
fn absent_crystal_keeps_registered_shield_damage_and_restore_unchanged() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.engine.active_replacements.push(ActiveReplacement {
            id: 1,
            source: target,
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Object(target),
                remaining: 2,
            },
        });
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(!requires_successor(&state));
        validate(&state).unwrap();
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
        for current in [&mut state, &mut replay] {
            crate::event::propose_and_commit(
                current,
                crate::event::ProposedEvent::damage(source, Target::Object(target), 3),
            );
            assert_eq!(current.objects.get(target).damage, 1);
            assert!(current.engine.active_replacements.is_empty());
            assert!(!requires_successor(current));
            validate(current).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn absent_stack_wrapper_writes_no_hash_input_or_serialized_field() {
    #[derive(Default)]
    struct Writes(usize);
    impl std::hash::Hasher for Writes {
        fn finish(&self) -> u64 {
            self.0 as u64
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0 += bytes.len();
        }
    }
    let mut hash = Writes::default();
    std::hash::Hash::hash(&CrystalPreventionStartedV1::default(), &mut hash);
    assert_eq!(hash.0, 0);
    let value = serde_json::to_value(crate::state::StackStateV4::default()).unwrap();
    assert!(value.get("crystal_prevention_started_v1").is_none());
    std::hash::Hash::hash(&CrystalPreventionStartedV1(Some(0)), &mut hash);
    assert!(hash.0 > 0);
}
