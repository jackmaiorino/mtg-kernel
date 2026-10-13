//! Actual shared target-filter checks, independent of future Panda registration.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state_for};

fn source_for(state: &GameState, source: ObjectId) -> TargetingSource {
    let object = state.objects.get(source);
    TargetingSource {
        object: source,
        card_def: object.card_def,
        zone: Zone::Battlefield,
        zone_change_count: object.zone_change_count,
        other_than: None,
        original_source_incarnation_only: false,
    }
}

fn targets(
    state: &GameState,
    actor: PlayerId,
    source: Option<TargetingSource>,
    maximum: i32,
) -> Vec<Target> {
    legal_targets_for_controller_from_source(
        TargetSpec::AnotherNonBearCreatureCardInOwnGraveyardManaValueAtMost(maximum),
        &[],
        actor,
        source,
        state,
    )
}

#[test]
fn signed_bound_requires_another_non_bear_card_in_the_actual_graveyard_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
        let zero = put(&mut state, actor, "Ornithopter", Zone::Graveyard);
        let one = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
        let seven = put(&mut state, actor, "Myr Enforcer", Zone::Graveyard);
        let bear = put(&mut state, actor, "Masked Vandal", Zone::Graveyard);
        let _spell = put(&mut state, actor, "Lightning Bolt", Zone::Graveyard);
        let _opponent = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Graveyard,
        );
        let _hand = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        let token = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
        // A primitive token flag exercises card exclusion; this is not a
        // claim that the Llanowar Elves printing creates a token.
        state.objects.get_mut(token).v4.is_token = true;
        assert!(card_def::CARD_DEFS[usize::from(state.objects.get(bear).card_def)].changeling);
        let binding = Some(source_for(&state, source));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(targets(&state, actor, binding, -1).is_empty());
        assert_eq!(
            targets(&state, actor, binding, 0),
            vec![Target::Object(zero)]
        );
        assert_eq!(
            targets(&state, actor, binding, 1),
            vec![Target::Object(zero), Target::Object(one)]
        );
        assert_eq!(
            targets(&state, actor, binding, 6),
            vec![Target::Object(zero), Target::Object(one)]
        );
        assert_eq!(
            targets(&state, actor, binding, 7),
            vec![
                Target::Object(zero),
                Target::Object(one),
                Target::Object(seven)
            ]
        );
        assert_eq!(
            targets(&state, actor, binding, i32::MAX),
            targets(&state, actor, binding, 7)
        );
        assert!(targets(&state, actor, None, i32::MAX).is_empty());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[test]
fn malformed_roster_entries_and_new_source_incarnations_do_not_change_the_filter_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
        let binding = Some(source_for(&state, source));
        let one = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
        let wrong_zone = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        state.players[actor.index()].graveyard.push(wrong_zone);
        let wrong_owner = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Graveyard,
        );
        state.players[actor.index()].graveyard.push(wrong_owner);
        state.players[actor.index()]
            .graveyard
            .push(ObjectId(u32::MAX));
        state.objects.get_mut(source).zone_change_count += 2;
        let before = serde_json::to_vec(&state).unwrap();
        assert_eq!(
            targets(&state, actor, binding, 1),
            vec![Target::Object(one)]
        );
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[test]
fn new_spec_has_an_appended_id_and_only_accepts_a_graveyard_object_contract() {
    let spec = TargetSpec::AnotherNonBearCreatureCardInOwnGraveyardManaValueAtMost(-1);
    assert_eq!(spec.stable_id(), 103);
    assert_eq!(target_count(spec), 1);
    assert_eq!(
        serde_json::from_str::<TargetSpec>(&serde_json::to_string(&spec).unwrap()).unwrap(),
        spec
    );
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let card = put(&mut state, actor, "Llanowar Elves", Zone::Graveyard);
        let target = Target::Object(card);
        let contract = StackTargetContractV4::capture(&state, target);
        assert!(stack_target_contract_is_structurally_valid(
            &state, spec, 0, target, contract
        ));
        let hand = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        let hand_target = Target::Object(hand);
        let hand_contract = StackTargetContractV4::capture(&state, hand_target);
        assert!(!stack_target_contract_is_structurally_valid(
            &state,
            spec,
            0,
            hand_target,
            hand_contract
        ));
    }
}
