//! Direct new primitive fixtures. Real printed-card acceptance is separate.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};

fn fixture(
    owner: PlayerId,
    controller: PlayerId,
    caster: PlayerId,
) -> (GameState, ObjectId, ExecCtx) {
    let mut state = ready_state();
    state.active_player = controller;
    state.priority_player = caster;
    state.step = crate::state::Step::DeclareAttackers;
    put(&mut state, owner, "Forest", Zone::Library);
    put(&mut state, owner, "Forest", Zone::Library);
    let source = put(&mut state, caster, "Lightning Bolt", Zone::Hand);
    let target = put(&mut state, owner, "Guttersnipe", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        event::ProposedEvent::zone_change_to_battlefield_under_controller(target, controller),
    );
    state.engine.combat.attackers = vec![target];
    state.engine.combat.attackers_declared = true;
    let mut ctx = ExecCtx::no_targets(source, caster);
    ctx.targets = vec![Target::Object(target)];
    ctx.target_contracts = vec![StackTargetContractV4::capture(
        &state,
        Target::Object(target),
    )];
    (state, target, ctx)
}

#[test]
fn condemn_primitive_freezes_current_controller_and_modified_toughness_before_owner_library_move() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for owner in [PlayerId::P0, PlayerId::P1] {
            for controller in [PlayerId::P0, PlayerId::P1] {
                let (mut state, target, ctx) = fixture(owner, controller, caster);
                state.objects.get_mut(target).counters.plus1_plus1 = 2;
                let binding = EffectObjectBinding {
                    object: target,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: state.objects.get(target).zone_change_count,
                };
                install_temporary_boost(&mut state, binding, 0, 3, Keywords::NONE);
                assert_eq!(crate::engine::effective_toughness(&state, target), 7);
                let original_library = state.players[owner.index()].library.clone();
                let mut restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                for current in [&mut state, &mut restored] {
                    execute(&EffectOp::PutTargetCreatureOnOwnersLibraryBottomThenControllerGainsToughness { target_index: 0 }, &ctx, current);
                    assert!(current.engine.halted.is_none());
                    assert_eq!(current.objects.get(target).zone, Zone::Library);
                    assert_eq!(current.objects.get(target).controller, owner);
                    assert_eq!(
                        current.players[owner.index()].library,
                        [original_library.clone(), vec![target]].concat()
                    );
                    assert_eq!(current.players[controller.index()].life, 27);
                    assert_eq!(current.players[controller.opponent().index()].life, 20);
                    assert_eq!(crate::engine::effective_toughness(current, target), 2);
                    let move_index = current.engine.event_history.iter().rposition(|event| matches!(event, event::CommittedEvent::ZoneChange { object, from: Zone::Battlefield, to: Zone::Library, .. } if *object == target)).unwrap();
                    assert!(
                        matches!(current.engine.event_history.get(move_index + 1), Some(event::CommittedEvent::LifeGain { player, amount: 7 }) if *player == controller)
                    );
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
fn condemn_primitive_nonpositive_toughness_moves_without_a_life_gain_event_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for delta in [-2, -3] {
            let (mut state, target, ctx) = fixture(player, player, player.opponent());
            let binding = EffectObjectBinding {
                object: target,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: state.objects.get(target).zone_change_count,
            };
            // This direct primitive fixture samples before any SBA, covering zero
            // and negative characteristic arithmetic without asserting a reachable
            // zero-toughness creature survives a real priority window.
            install_temporary_boost(&mut state, binding, 0, delta, Keywords::NONE);
            let before = state.engine.event_history.len();
            execute(
                &EffectOp::PutTargetCreatureOnOwnersLibraryBottomThenControllerGainsToughness {
                    target_index: 0,
                },
                &ctx,
                &mut state,
            );
            assert_eq!(state.objects.get(target).zone, Zone::Library);
            assert_eq!(state.players[player.index()].life, 20);
            assert!(!state.engine.event_history[before..]
                .iter()
                .any(|event| matches!(event, event::CommittedEvent::LifeGain { .. })));
        }
    }
}

#[test]
fn condemn_primitive_refuses_nonattacking_departed_reentered_protected_and_malformed_targets_readonly(
) {
    for player in [PlayerId::P0, PlayerId::P1] {
        for change in 0..6 {
            let (mut state, target, mut ctx) = fixture(player, player, player.opponent());
            match change {
                0 => state.engine.combat.attackers.clear(),
                1 => event::propose_and_commit(
                    &mut state,
                    event::ProposedEvent::zone_change(target, Zone::Hand),
                ),
                2 => {
                    event::propose_and_commit(
                        &mut state,
                        event::ProposedEvent::zone_change(target, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        event::ProposedEvent::zone_change(target, Zone::Battlefield),
                    );
                    state.engine.combat.attackers = vec![target];
                }
                3 => {
                    let binding = EffectObjectBinding {
                        object: target,
                        expected_zone: Zone::Battlefield,
                        expected_zone_change_count: state.objects.get(target).zone_change_count,
                    };
                    install_temporary_boost(
                        &mut state,
                        binding,
                        0,
                        0,
                        Keywords::PROTECTION_FROM_MONOCOLORED,
                    );
                }
                4 => ctx.target_contracts.clear(),
                _ => ctx.targets = vec![Target::Player(player)],
            }
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for current in [&mut state, &mut restored] {
                let before = serde_json::to_vec(&*current).unwrap();
                execute(
                    &EffectOp::PutTargetCreatureOnOwnersLibraryBottomThenControllerGainsToughness {
                        target_index: 0,
                    },
                    &ctx,
                    current,
                );
                assert_eq!(serde_json::to_vec(&*current).unwrap(), before);
            }
        }
    }
}
