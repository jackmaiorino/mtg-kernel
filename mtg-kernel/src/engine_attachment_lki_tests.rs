//! Attachment changes while an independently existing ability waits.

use super::*;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};

fn equip(state: &mut GameState, equipment: ObjectId, host: ObjectId) {
    step(state, Action::ActivateAbility(equipment, 0)).unwrap();
    assert!(
        matches!(advance_until_decision(state), Decision::ChooseTargets { ref legal_targets, .. }
        if legal_targets.contains(&Target::Object(host)))
    );
    step(state, Action::ChooseTarget(Target::Object(host))).unwrap();
    assert!(matches!(
        advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
}

fn resolve_until(state: &mut GameState, done: impl Fn(&GameState) -> bool) {
    for _ in 0..32 {
        let decision = advance_until_decision(state);
        assert!(
            state.engine.halted.is_none(),
            "unexpected halt: {decision:?}"
        );
        if done(state) {
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        step(state, Action::Pass).unwrap();
    }
    panic!("focused stack resolution did not finish");
}

fn waiting_equip() -> (GameState, ObjectId, ObjectId, ObjectId, ObjectId) {
    let mut state = ready_state();
    let equipment = put(
        &mut state,
        PlayerId::P0,
        "Hunter's Blowgun",
        Zone::Battlefield,
    );
    let first = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let second = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let removal = put(&mut state, PlayerId::P0, "Snuff Out", Zone::Hand);
    // A playable instant holds a real priority decision after removal, so
    // the V3 fixture cannot silently resolve Equip before its observation.
    put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 20;
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    equip(&mut state, equipment, first);
    resolve_until(&mut state, |s| s.stack.is_empty());
    assert_eq!(
        state.objects.get(equipment).v4.attached_to.unwrap().object,
        first
    );
    equip(&mut state, equipment, second);
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].source, equipment);
    (state, equipment, first, second, removal)
}

fn assert_v3_observes_waiting_equip(state: &GameState, equipment: ObjectId) {
    let session = FastActorSessionV1::from_v3_fixture_state(state.clone());
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("waiting Equip must retain a real V3 decision");
    };
    let observation = session.flat_policy_observation_v3(decision).unwrap();
    assert!(observation
        .projection
        .surface
        .stack
        .iter()
        .any(|item| item.source.arena_id == equipment.0));
}

#[test]
fn equip_resolves_and_v3_observes_after_old_host_is_destroyed_in_response() {
    let (mut state, equipment, first, second, removal) = waiting_equip();
    let frozen = state.stack[0].v4.ability_source_contract.unwrap();
    step(&mut state, Action::CastSpell(removal)).unwrap();
    assert!(
        matches!(advance_until_decision(&mut state), Decision::ChooseTargets { ref legal_targets, .. }
        if legal_targets.contains(&Target::Object(first)))
    );
    step(&mut state, Action::ChooseTarget(Target::Object(first))).unwrap();
    resolve_until(&mut state, |s| s.objects.get(first).zone == Zone::Graveyard);
    assert_eq!(
        state.stack.len(),
        1,
        "Equip is still waiting after Snuff Out"
    );
    assert_eq!(
        state.objects.get(equipment).zone_change_count,
        frozen.zone_change_count
    );
    assert_eq!(state.objects.get(equipment).v4.attached_to, None);
    assert_eq!(state.stack[0].v4.ability_source_contract, Some(frozen));
    assert_eq!(frozen.attached_to.unwrap().object, first);
    validated_stack_item_target_spec(&state.stack[0], &state).unwrap();
    assert_v3_observes_waiting_equip(&state, equipment);
    resolve_until(&mut state, |s| s.stack.is_empty());
    assert_eq!(
        state.objects.get(equipment).v4.attached_to.unwrap().object,
        second
    );
    state.validate_attachment_relations().unwrap();
}

#[test]
fn equip_keeps_frozen_host_when_same_incarnation_is_reattached_while_waiting() {
    let (mut state, equipment, first, second, _) = waiting_equip();
    let third = put(&mut state, PlayerId::P0, "Myr Enforcer", Zone::Battlefield);
    let frozen = state.stack[0].v4.ability_source_contract.unwrap();
    let third_generation = state.objects.get(third).zone_change_count;
    // Exercise the real attachment transition primitive. Equip itself is
    // sorcery-speed; this does not pretend a second Equip was legal here.
    state
        .attach_object_exact(equipment, frozen.zone_change_count, third, third_generation)
        .unwrap();
    assert_eq!(
        state.objects.get(equipment).v4.attached_to.unwrap().object,
        third
    );
    assert_eq!(
        state.objects.get(equipment).zone_change_count,
        frozen.zone_change_count
    );
    assert_eq!(state.stack[0].v4.ability_source_contract, Some(frozen));
    assert_eq!(frozen.attached_to.unwrap().object, first);
    validated_stack_item_target_spec(&state.stack[0], &state).unwrap();
    assert_v3_observes_waiting_equip(&state, equipment);
    resolve_until(&mut state, |s| s.stack.is_empty());
    assert_eq!(
        state.objects.get(equipment).v4.attached_to.unwrap().object,
        second
    );
    assert!(!state.objects.get(third).attachments.contains(&equipment));
    state.validate_attachment_relations().unwrap();
}

#[test]
fn equip_source_identity_and_generation_tampering_still_rejects() {
    let (state, equipment, first, _, _) = waiting_equip();
    let original = state.stack[0].v4.ability_source_contract.unwrap();
    for mutation in 0..7 {
        let mut tampered = state.clone();
        let contract = tampered.stack[0]
            .v4
            .ability_source_contract
            .as_mut()
            .unwrap();
        match mutation {
            0 => contract.source = first,
            1 => contract.card_def = tampered.objects.get(first).card_def,
            2 => contract.owner = PlayerId::P1,
            3 => contract.controller = PlayerId::P1,
            4 => contract.zone = Zone::Stack,
            5 => contract.zone = Zone::Graveyard,
            6 => contract.zone_change_count += 1,
            _ => unreachable!(),
        }
        assert!(
            validated_stack_item_target_spec(&tampered.stack[0], &tampered).is_err(),
            "mutation {mutation}"
        );
        assert!(
            crate::rl::policy_observation_extensions_v6(&tampered, PlayerId::P0).is_err(),
            "mutation {mutation}"
        );
        let before = tampered.objects.get(equipment).v4.attached_to;
        for _ in 0..8 {
            let decision = advance_until_decision(&mut tampered);
            if matches!(decision, Decision::Halted { .. }) {
                break;
            }
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            step(&mut tampered, Action::Pass).unwrap();
        }
        assert!(
            tampered.engine.halted.is_some(),
            "mutation {mutation} must halt before resolution"
        );
        assert_eq!(tampered.objects.get(equipment).v4.attached_to, before);
    }
    assert_eq!(state.stack[0].v4.ability_source_contract, Some(original));
}
