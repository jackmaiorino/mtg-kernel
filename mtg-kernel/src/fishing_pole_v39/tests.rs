use super::*;
use crate::engine::{self, Action, Decision};
use crate::policy_observation_v6::tests::{put, ready_state_for};
use crate::state::{Counters, Target};
use std::hash::{Hash, Hasher};

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn resolve(state: &mut GameState) {
    for _ in 0..16 {
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        if state.stack.is_empty() {
            return;
        }
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("registered equipment resolution did not drain");
}

#[test]
fn absent_bait_retains_original_counter_hash_and_serialization() {
    let counters = Counters::default();
    let mut current = std::collections::hash_map::DefaultHasher::new();
    counters.hash(&mut current);
    let mut predecessor = std::collections::hash_map::DefaultHasher::new();
    for _ in 0..5 {
        0i16.hash(&mut predecessor);
    }
    assert_eq!(current.finish(), predecessor.finish());
    assert!(serde_json::to_value(counters)
        .unwrap()
        .get("bait")
        .is_none());
    let decoded: Counters = serde_json::from_value(serde_json::json!({
        "plus1_plus1": 0, "minus1_minus1": 0, "minus0_minus1": 0, "stun": 0, "lore": 0,
    }))
    .unwrap();
    assert_eq!(decoded, counters);
    let bait = Counters {
        bait: 1,
        ..counters
    };
    assert!(bait.any());
    let mut extended = std::collections::hash_map::DefaultHasher::new();
    bait.hash(&mut extended);
    assert_ne!(extended.finish(), predecessor.finish());
}

#[test]
fn actual_registered_longbow_grant_never_becomes_a_named_pole_cost_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let equipment = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 3;
        engine::step(&mut state, Action::ActivateAbility(equipment, 0)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(host))).unwrap();
        resolve(&mut state);
        assert_eq!(
            state.objects.get(equipment).v4.attached_to.unwrap().object,
            host
        );
        assert!(grants(&state, host).is_empty());
        assert!(cost_candidates(&state, actor, host).is_empty());
        assert!(granted_ability(&state, host).is_none());
        let index = u8::try_from(
            CARD_DEFS[state.objects.get(host).card_def as usize]
                .activated_abilities
                .len(),
        )
        .unwrap();
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { ref activatable_abilities, .. }
            if activatable_abilities.contains(&(host,index)))
        );
        engine::step(&mut state, Action::ActivateAbility(host, index)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Player(actor.opponent())),
        )
        .unwrap();
        next(&mut state);
        let item = state.stack.last().unwrap();
        assert_eq!(item.source, host);
        assert_eq!(item.v4.granted_by.unwrap().source, equipment);
        assert!(state.objects.get(host).tapped);
        assert!(!state.objects.get(equipment).tapped);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut restored] {
            resolve(branch);
            assert_eq!(branch.players[actor.opponent().index()].life, 19);
        }
        assert_eq!(state, restored);
    }
}
