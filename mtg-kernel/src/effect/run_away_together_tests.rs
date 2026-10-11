//! Direct opcode tests; no catalog admission or definition substitution.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};

fn fixture(
    caster: PlayerId,
    reverse: bool,
    swap_owners: bool,
) -> (GameState, [ObjectId; 2], ExecCtx) {
    let mut state = ready_state();
    let source = put(&mut state, caster, "Lightning Bolt", Zone::Hand);
    let objects = std::array::from_fn(|index| {
        let controller = if index == 0 {
            caster
        } else {
            caster.opponent()
        };
        let owner = if swap_owners {
            controller.opponent()
        } else {
            controller
        };
        let object = put(&mut state, owner, "Llanowar Elves", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            event::ProposedEvent::zone_change_to_battlefield_under_controller(object, controller),
        );
        object
    });
    let mut ctx = ExecCtx::no_targets(source, caster);
    ctx.targets = if reverse {
        [objects[1], objects[0]]
    } else {
        objects
    }
    .into_iter()
    .map(Target::Object)
    .collect();
    ctx.target_contracts = ctx
        .targets
        .iter()
        .map(|&target| StackTargetContractV4::capture(&state, target))
        .collect();
    (state, objects, ctx)
}

fn transfer(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let former = state.objects.get(object).controller;
    state.players[former.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}

fn grant(state: &mut GameState, object: ObjectId, keyword: Keywords) {
    let binding = EffectObjectBinding {
        object,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: state.objects.get(object).zone_change_count,
    };
    install_temporary_boost(state, binding, 0, 0, keyword);
}

#[test]
fn run_away_primitive_batches_both_owner_hand_moves_in_either_target_order_and_restores() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            for swap_owners in [false, true] {
                let (mut state, objects, ctx) = fixture(caster, reverse, swap_owners);
                let owners = objects.map(|object| state.objects.get(object).owner);
                let controllers = objects.map(|object| state.objects.get(object).controller);
                let before = state.engine.event_history.len();
                let mut restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                for current in [&mut state, &mut restored] {
                    execute(
                        &EffectOp::ReturnTwoDifferentControllerCreaturesToOwnersHands,
                        &ctx,
                        current,
                    );
                    assert!(current.engine.halted.is_none());
                    for index in 0..2 {
                        let object = objects[index];
                        assert_eq!(current.objects.get(object).zone, Zone::Hand);
                        assert_eq!(current.objects.get(object).controller, owners[index]);
                        assert!(current.players[owners[index].index()]
                            .hand
                            .contains(&object));
                    }
                    let moves = &current.engine.event_history[before..];
                    assert_eq!(moves.len(), 2);
                    for (index, target) in ctx.targets.iter().enumerate() {
                        let Target::Object(object) = target else {
                            unreachable!()
                        };
                        let object_index = objects.iter().position(|id| id == object).unwrap();
                        assert!(
                            matches!(moves[index], event::CommittedEvent::ZoneChange { object: moved, from: Zone::Battlefield, to: Zone::Hand, controller_before } if moved == *object && controller_before == controllers[object_index])
                        );
                    }
                }
                assert_eq!(
                    state.diagnostic_state_hash(),
                    restored.diagnostic_state_hash()
                );
            }
        }
    }
}

#[test]
fn run_away_primitive_same_current_controller_invalidates_both_without_order_bias() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            let (mut state, objects, ctx) = fixture(caster, reverse, false);
            transfer(&mut state, objects[1], caster);
            let before = serde_json::to_vec(&state).unwrap();
            assert_eq!(
                crate::engine::different_controller_creature_targets_are_legal(
                    &state,
                    ctx.source,
                    caster,
                    &ctx.targets,
                    &ctx.target_contracts
                ),
                [false, false]
            );
            execute(
                &EffectOp::ReturnTwoDifferentControllerCreaturesToOwnersHands,
                &ctx,
                &mut state,
            );
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            // Swapping both live controllers keeps the pair legal; destinations
            // still use owners rather than either captured or current controller.
            let (mut state, objects, ctx) = fixture(caster, reverse, false);
            transfer(&mut state, objects[0], caster.opponent());
            transfer(&mut state, objects[1], caster);
            execute(
                &EffectOp::ReturnTwoDifferentControllerCreaturesToOwnersHands,
                &ctx,
                &mut state,
            );
            for object in objects {
                let owner = state.objects.get(object).owner;
                assert_eq!(state.objects.get(object).zone, Zone::Hand);
                assert!(state.players[owner.index()].hand.contains(&object));
            }
        }
    }
}

#[test]
fn run_away_primitive_original_departure_lki_survives_reentry_and_restore() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            for departed_index in 0..2 {
                for reenter in [false, true] {
                    let (mut state, objects, ctx) = fixture(caster, reverse, true);
                    let departed = objects[departed_index];
                    let survivor = objects[1 - departed_index];
                    // This direct opcode fixture has no printed spell on the
                    // stack. Supply its departure authority explicitly; printed
                    // integration tests exercise actual engine emission.
                    state.engine.event_history.push(
                        event::CommittedEvent::DifferentControllerTargetDepartureV1 {
                            target: StackTargetContractV4::capture(
                                &state,
                                Target::Object(departed),
                            ),
                            controller_before: state.objects.get(departed).controller,
                        },
                    );
                    event::propose_and_commit(
                        &mut state,
                        event::ProposedEvent::zone_change(departed, Zone::Graveyard),
                    );
                    if reenter {
                        let survivor_controller = state.objects.get(survivor).controller;
                        event::propose_and_commit(
                            &mut state,
                            event::ProposedEvent::zone_change_to_battlefield_under_controller(
                                departed,
                                survivor_controller,
                            ),
                        );
                    }
                    let expected_zone = state.objects.get(departed).zone;
                    let generation = state.objects.get(departed).zone_change_count;
                    let mut restored: GameState =
                        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                    for current in [&mut state, &mut restored] {
                        execute(
                            &EffectOp::ReturnTwoDifferentControllerCreaturesToOwnersHands,
                            &ctx,
                            current,
                        );
                        assert_eq!(current.objects.get(departed).zone, expected_zone);
                        assert_eq!(current.objects.get(departed).zone_change_count, generation);
                        assert_eq!(current.objects.get(survivor).zone, Zone::Hand);
                        let owner = current.objects.get(survivor).owner;
                        assert!(current.players[owner.index()].hand.contains(&survivor));
                    }
                    assert_eq!(
                        state.diagnostic_state_hash(),
                        restored.diagnostic_state_hash()
                    );
                }
            }
        }
    }
}

#[test]
fn run_away_primitive_protection_hexproof_and_malformed_pair_refusal() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            for keyword in [Keywords::PROTECTION_FROM_MONOCOLORED, Keywords::HEXPROOF] {
                let (mut state, objects, ctx) = fixture(caster, reverse, true);
                grant(&mut state, objects[1], keyword);
                execute(
                    &EffectOp::ReturnTwoDifferentControllerCreaturesToOwnersHands,
                    &ctx,
                    &mut state,
                );
                assert_eq!(state.objects.get(objects[1]).zone, Zone::Battlefield);
                assert_eq!(state.objects.get(objects[0]).zone, Zone::Hand);
            }
            // A caster's own hexproof creature remains targetable.
            let (mut state, objects, ctx) = fixture(caster, reverse, false);
            grant(&mut state, objects[0], Keywords::HEXPROOF);
            execute(
                &EffectOp::ReturnTwoDifferentControllerCreaturesToOwnersHands,
                &ctx,
                &mut state,
            );
            assert!(objects
                .iter()
                .all(|&object| state.objects.get(object).zone == Zone::Hand));
            for corrupt in 0..4 {
                let (mut state, _, mut ctx) = fixture(caster, reverse, false);
                match corrupt {
                    0 => {
                        ctx.targets.pop();
                        ctx.target_contracts.pop();
                    }
                    1 => ctx.target_contracts.clear(),
                    2 => {
                        ctx.targets[1] = ctx.targets[0];
                        ctx.target_contracts[1] = ctx.target_contracts[0];
                    }
                    _ => {
                        ctx.targets[0] = Target::Player(caster);
                        ctx.target_contracts[0] = StackTargetContractV4::Player(caster);
                    }
                }
                let before = serde_json::to_vec(&state).unwrap();
                execute(
                    &EffectOp::ReturnTwoDifferentControllerCreaturesToOwnersHands,
                    &ctx,
                    &mut state,
                );
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            }
        }
    }
}

#[test]
fn run_away_primitive_missing_duplicate_or_mutable_log_lki_cannot_authorize_return() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            for corrupt in 0..3 {
                let (mut state, objects, ctx) = fixture(caster, reverse, true);
                let marker = event::CommittedEvent::DifferentControllerTargetDepartureV1 {
                    target: StackTargetContractV4::capture(&state, Target::Object(objects[0])),
                    controller_before: state.objects.get(objects[0]).controller,
                };
                event::propose_and_commit(
                    &mut state,
                    event::ProposedEvent::zone_change(objects[0], Zone::Hand),
                );
                match corrupt {
                    0 => {} // Missing immutable receipt.
                    1 => state.engine.event_history.extend([marker.clone(), marker]),
                    _ => state.engine.event_log.push(marker),
                }
                let before = serde_json::to_vec(&state).unwrap();
                execute(
                    &EffectOp::ReturnTwoDifferentControllerCreaturesToOwnersHands,
                    &ctx,
                    &mut state,
                );
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            }
        }
    }
}
