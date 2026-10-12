use super::*;
use std::hash::{Hash, Hasher};
#[test]
fn absent_fixed_marker_has_no_hash_input_or_serialized_stack_field() {
    let mut old = std::collections::hash_map::DefaultHasher::new();
    let baseline = old.finish();
    crate::state::FixedDamagePaidV3::default().hash(&mut old);
    assert_eq!(old.finish(), baseline);
    let value = serde_json::to_value(crate::state::StackStateV4::default()).unwrap();
    assert!(value.get("fixed_damage_paid_v3").is_none());
}
#[test]
fn absent_crystal_catalog_does_not_select_firebrand_history() {
    if CARD_DEFS
        .iter()
        .any(|d| d.name == "Crystal Barricade" && d.is_executable())
    {
        return;
    }
    assert!(!enabled());
}

#[test]
#[ignore = "Crystal is unregistered; actual future registered Firebrand counter-primitive qualification"]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn future_real_firebrand_counter_primitive_owns_removal_and_refuses_erasure_both_seats() {
    use crate::engine::{self, Action, Decision};
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};
    for caster in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        assert!(enabled(), "actual future Crystal registration required");
        state.active_player = caster;
        state.priority_player = caster;
        put(&mut state, caster, "Lightning Bolt", Zone::Hand);
        put(&mut state, caster.opponent(), "Lightning Bolt", Zone::Hand);
        for player in [PlayerId::P0, PlayerId::P1] {
            state.players[player.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
        }
        let fire = put(&mut state, caster, "Fanatical Firebrand", Zone::Battlefield);
        let target = put(
            &mut state,
            caster.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        assert!(
            matches!(engine::advance_until_decision(&mut state),Decision::CastSpellOrPass{player,..} if player==caster)
        );
        engine::step(&mut state, Action::ActivateAbility(fire, 0)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        let _ = engine::advance_until_decision(&mut state);
        let item = state
            .stack
            .iter()
            .find(|i| i.source == fire)
            .unwrap()
            .clone();
        assert!(item.v4.fixed_damage_paid_v3.0.is_some());
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
        for current in [&mut state, &mut replay] {
            // Real accepted typed counter primitive, not an invented in-pool spell.
            assert_eq!(
                engine::counter_stack_item_by_id(current, item.v4.stack_item_id).unwrap(),
                Some(item.clone())
            );
            validate_state(current).unwrap();
            assert_eq!(current.objects.get(target).damage, 0);
            assert_eq!(current.objects.get(fire).zone, Zone::Graveyard);
            assert_eq!(current.engine.event_history.iter().filter(|e|matches!(e,
                CommittedEvent::FixedDamageRemovedV3{removal} if removal.outcome==OutcomeV3::Countered)).count(),1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        let mut erased = state.clone();
        erased
            .engine
            .event_history
            .retain(|e| !matches!(e, CommittedEvent::FixedDamageRemovedV3 { .. }));
        let before = serde_json::to_vec(&erased).unwrap();
        assert!(engine::step(&mut erased, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&erased).unwrap(), before);
        let mut reintroduced = state.clone();
        reintroduced.stack.push(item);
        let before = serde_json::to_vec(&reintroduced).unwrap();
        assert!(engine::step(&mut reintroduced, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&reintroduced).unwrap(), before);
    }
}
