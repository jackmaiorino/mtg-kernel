//! Source-aware target-filter primitives, not printed-card admission.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};

#[test]
fn variable_cast_prefix_primitive_distinct_more_than_255_and_both_controller_seats() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        let source = put(&mut state, controller, "Lightning Bolt", Zone::Hand);
        let objects: Vec<_> = (0..300)
            .map(|_| put(&mut state, controller, "Llanowar Elves", Zone::Battlefield))
            .collect();
        let other = put(
            &mut state,
            controller.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let targets: Vec<_> = objects.iter().copied().map(Target::Object).collect();
        let spec = TargetSpec::KickerControlledCreatures;
        let source = targeting_source_for_object(&state, source);
        assert!(target_prefix_can_complete_for_controller_and_source(
            spec, &targets, controller, source, &state
        ));
        let mut duplicate = targets.clone();
        duplicate.push(targets[0]);
        assert!(!target_prefix_can_complete_for_controller_and_source(
            spec, &duplicate, controller, source, &state
        ));
        let mut foreign = targets.clone();
        foreign.push(Target::Object(other));
        assert!(!target_prefix_can_complete_for_controller_and_source(
            spec, &foreign, controller, source, &state
        ));
        let replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(state.state_hash(), replay.state_hash());
        assert!(legal_targets_for_controller_from_source(
            spec, &targets, controller, source, &replay
        )
        .is_empty());
    }
}

#[test]
fn absent_variable_source_binding_preserves_legacy_finalized_record_json_and_hash() {
    #[derive(std::hash::Hash, serde::Serialize)]
    struct HistoricalBinding {
        x_value: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        chosen_creature_cost: Option<PaidCostRefV4>,
    }
    fn hash(value: &impl std::hash::Hash) -> u64 {
        use std::hash::Hasher;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut h);
        h.finish()
    }
    let old = HistoricalBinding {
        x_value: 7,
        chosen_creature_cost: None,
    };
    let current = FinalizedCastBindingV1 {
        x_value: 7,
        chosen_creature_cost: None,
        variable_targeting_kicked: None,
        mandatory_additional_cost: None,
    };
    assert_eq!(
        serde_json::to_vec(&old).unwrap(),
        serde_json::to_vec(&current).unwrap()
    );
    assert_eq!(hash(&old), hash(&current));
    let mut successor = current;
    successor.variable_targeting_kicked = Some(true);
    assert_ne!(hash(&current), hash(&successor));
    let restored: FinalizedCastBindingV1 =
        serde_json::from_slice(&serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(current, restored);
}

#[test]
fn restored_ordered_trigger_refuses_spell_only_cardinality_without_panicking() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = controller;
        state.priority_player = controller;
        let source = put(&mut state, controller, "Journey to Nowhere", Zone::Hand);
        let target = put(&mut state, controller, "Llanowar Elves", Zone::Battlefield);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        collect_and_queue_triggers(&mut state);
        assert_eq!(state.engine.pending_triggers.len(), 1);
        assert!(matches!(
            drain_pending_triggers_or_decide(&mut state),
            Some(Decision::ChooseTargets { .. })
        ));
        validate_pending_trigger(&state, &state.engine.pending_triggers[0]).unwrap();
        state.engine.pending_triggers[0].target_spec = TargetSpec::KickerControlledCreatures;
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        let before = serde_json::to_vec(&replay).unwrap();
        assert!(exact_targeting_producer(&replay).is_err());
        assert!(step(&mut replay, Action::ChooseTarget(Target::Object(target))).is_err());
        assert!(step(&mut replay, Action::Pass).is_err());
        assert_eq!(before, serde_json::to_vec(&replay).unwrap());
    }
}
