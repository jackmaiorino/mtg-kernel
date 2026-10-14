//! Genuine registered operands verify the evasion substrate, not Gourmand admission.
use super::*;
use crate::policy_observation_v6::tests::put;
use crate::state::AbilitySourceContractV4;
use crate::vampire_gourmand_v1;

#[test]
fn predecessor_unstarted_stack_guard_is_absent_from_serialization_and_hash_input() {
    use std::hash::{Hash, Hasher};
    struct NoWrites;
    impl Hasher for NoWrites {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, _: &[u8]) {
            panic!("empty successor wrote Hash input");
        }
    }
    let legacy = crate::state::StackStateV4::default();
    assert!(legacy.vampire_gourmand_started_v1.is_empty());
    assert!(serde_json::to_value(&legacy)
        .unwrap()
        .get("vampire_gourmand_started_v1")
        .is_none());
    legacy.vampire_gourmand_started_v1.hash(&mut NoWrites);
    let restored: crate::state::StackStateV4 =
        serde_json::from_value(serde_json::to_value(&legacy).unwrap()).unwrap();
    assert_eq!(restored, legacy);
}

#[test]
fn registered_incarnation_evasion_uses_real_blocker_validator_restore_and_reentry() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
        let attacker = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let blocker = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let source = AbilitySourceContractV4::capture(&state, attacker);
        state.step = Step::DeclareAttackers;
        step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
        state.step = Step::DeclareBlockers;
        assert!(legal_blockers_for(&state, attacker).contains(&blocker));
        vampire_gourmand_v1::install(&mut state, source);
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for current in [&mut state, &mut replay] {
            assert!(vampire_gourmand_v1::cant_be_blocked(current, attacker));
            assert!(legal_blockers_for(current, attacker).is_empty());
            assert!(!has_effective_keyword(
                current,
                attacker,
                Keywords::CANT_BE_BLOCKED
            ));
            let before = serde_json::to_vec(current).unwrap();
            assert!(step(current, Action::DeclareBlockers(vec![(blocker, attacker)])).is_err());
            assert_eq!(serde_json::to_vec(current).unwrap(), before);
            event::propose_and_commit(
                current,
                ProposedEvent::zone_change(attacker, Zone::Graveyard),
            );
            event::propose_and_commit(
                current,
                ProposedEvent::zone_change(attacker, Zone::Battlefield),
            );
            assert!(!vampire_gourmand_v1::cant_be_blocked(current, attacker));
            let before = serde_json::to_vec(current).unwrap();
            vampire_gourmand_v1::install(current, source);
            assert_eq!(serde_json::to_vec(current).unwrap(), before);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn registered_cleanup_priority_evasion_expires_before_player_flip_and_clears_pools() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let capture = AbilitySourceContractV4::capture(&state, source);
        state.step = Step::Cleanup;
        run_step_entry_action(&mut state, Step::Cleanup);
        vampire_gourmand_v1::install(&mut state, capture);
        for player in &mut state.players {
            player.mana_pool = [1; 6];
        }
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for current in [&mut state, &mut replay] {
            advance_step(current);
            assert_eq!(
                (current.step, current.active_player),
                (Step::Cleanup, actor)
            );
            assert!(!vampire_gourmand_v1::cant_be_blocked(current, source));
            assert!(current.engine.until_end_of_turn.is_empty());
            for player in &current.players {
                assert_eq!(player.mana_pool, [0; 6]);
            }
            advance_step(current);
            assert_eq!(current.active_player, actor.opponent());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
