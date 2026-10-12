//! Registered recipients directly exercise typed effect authority; no printed-definition bypass.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};

fn fixture(player: PlayerId, count: usize) -> (GameState, Vec<ObjectId>, ExecCtx) {
    let mut state = ready_state();
    let source = put(&mut state, player, "Lightning Bolt", Zone::Hand);
    let objects: Vec<_> = (0..count)
        .map(|_| put(&mut state, player, "Llanowar Elves", Zone::Battlefield))
        .collect();
    let mut ctx = ExecCtx::no_targets(source, player);
    ctx.kicked = true;
    ctx.targets = objects.iter().copied().map(Target::Object).collect();
    ctx.target_contracts = ctx
        .targets
        .iter()
        .map(|&target| StackTargetContractV4::capture(&state, target))
        .collect();
    (state, objects, ctx)
}

fn op() -> EffectOp {
    EffectOp::GrantKeywordToDeclaredTargetsUntilEndOfTurn {
        target_spec: crate::card_def::TargetSpec::KickerControlledCreatures,
        keyword: Keywords::INDESTRUCTIBLE,
    }
}

#[test]
fn variable_declared_grant_primitive_300_incarnations_and_empty_selection_restore_exactly() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for count in [0, 1, 300] {
            let (mut state, objects, ctx) = fixture(player, count);
            let untouched = put(&mut state, player, "Llanowar Elves", Zone::Battlefield);
            let other = put(
                &mut state,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let before = state.state_hash();
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for current in [&mut state, &mut replay] {
                execute(&op(), &ctx, current);
                assert!(objects
                    .iter()
                    .all(|&object| crate::engine::has_effective_keyword(
                        current,
                        object,
                        Keywords::INDESTRUCTIBLE
                    )));
                assert!(!crate::engine::has_effective_keyword(
                    current,
                    untouched,
                    Keywords::INDESTRUCTIBLE
                ));
                assert!(!crate::engine::has_effective_keyword(
                    current,
                    other,
                    Keywords::INDESTRUCTIBLE
                ));
                if count == 0 {
                    assert_eq!(before, current.state_hash());
                }
            }
            assert_eq!(state.state_hash(), replay.state_hash());
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn variable_declared_grant_primitive_skips_stale_reentered_control_changed_and_protected_targets() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let (mut state, objects, ctx) = fixture(player, 5);
        event::propose_and_commit(
            &mut state,
            event::ProposedEvent::zone_change(objects[0], Zone::Exile),
        );
        event::propose_and_commit(
            &mut state,
            event::ProposedEvent::zone_change(objects[1], Zone::Exile),
        );
        event::propose_and_commit(
            &mut state,
            event::ProposedEvent::zone_change(objects[1], Zone::Battlefield),
        );
        state.players[player.index()]
            .battlefield
            .retain(|&object| object != objects[2]);
        state.players[player.opponent().index()]
            .battlefield
            .push(objects[2]);
        state.objects.get_mut(objects[2]).controller = player.opponent();
        let protected = objects[3];
        let binding = EffectObjectBinding {
            object: protected,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: state.objects.get(protected).zone_change_count,
        };
        install_temporary_boost(
            &mut state,
            binding,
            0,
            0,
            Keywords::PROTECTION_FROM_MONOCOLORED,
        );
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for current in [&mut state, &mut replay] {
            execute(&op(), &ctx, current);
            for (index, &object) in objects.iter().enumerate() {
                assert_eq!(
                    crate::engine::has_effective_keyword(current, object, Keywords::INDESTRUCTIBLE),
                    index == 4
                );
            }
        }
        assert_eq!(state.state_hash(), replay.state_hash());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn variable_target_decisions_explicitly_refuse_frozen_action_and_rules_meaning_consumers() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let (state, objects, ctx) = fixture(player, 1);
        for decision in [
            crate::engine::Decision::ChooseVariableCastTargets {
                player,
                spell: ctx.source,
                selected_count: 300,
                legal_targets: vec![Target::Object(objects[0])],
                can_finish: true,
            },
            crate::engine::Decision::ChooseVariableCastKicker {
                player,
                spell: ctx.source,
                can_decline: false,
            },
        ] {
            let error = crate::rl::legal_action_candidates_v1(
                &crate::surface_v2::SurfaceDecision::Decision(decision),
                &state,
            )
            .unwrap_err();
            assert!(error.to_string().contains("successor"));
        }
        let refusal = std::panic::catch_unwind(|| {
            let mut collector = crate::rules_vector_v1::meaning::Collector::default();
            crate::rules_vector_v1::meaning::targets::target_spec(
                crate::card_def::TargetSpec::KickerControlledCreatures,
                &mut collector,
            );
        });
        assert!(refusal.is_err());
    }
}
