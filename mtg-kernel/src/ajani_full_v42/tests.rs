//! Registered physical primitives; future paid full-Ajani games are separate.
use super::*;
use crate::effect::ExecCtx;
use crate::engine;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state_for};
use crate::state::{GameState, StackTargetContractV4, Target};

#[test]
fn registered_optional_creature_pool_and_controller_life_preserve_exact_bounds_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let other = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        put(&mut state, actor, "Forest", Zone::Battlefield);
        let expected =
            engine::legal_targets_for_controller(TargetSpec::Creature, &[], actor, &state);
        let optional = engine::legal_targets_for_controller(
            TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToOneCreature),
            &[],
            actor,
            &state,
        );
        assert_eq!(optional, expected);
        assert!(
            optional.contains(&Target::Object(own)) && optional.contains(&Target::Object(other))
        );
        for life in [-2, 0, 1, 19, i32::MAX] {
            state.players[actor.index()].life = life;
            state.players[actor.opponent().index()].life = 73;
            assert_eq!(
                engine::evaluate_dynamic_value(&state, LIFE_COUNT, actor),
                life
            );
        }
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            engine::evaluate_dynamic_value(&restored, LIFE_COUNT, actor),
            i32::MAX
        );
        assert_eq!(
            TargetSpec::StandardV1(crate::standard_cards_v1::StandardTargetV1::UpToOneCreature)
                .stable_id(),
            60
        );
    }
}

#[test]
fn actual_optional_counter_leaf_zero_and_exact_target_incarnation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let op = EffectOp::AddCountersToTarget {
            target_index: 0,
            optional: true,
            plus1_plus1: 1,
            lifelink: 0,
            stun: 0,
        };
        let mut ctx = ExecCtx::no_targets(source, actor);
        let before = serde_json::to_vec(&state).unwrap();
        crate::effect::execute(&op, &ctx, &mut state);
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        ctx.targets = vec![Target::Object(target)];
        ctx.target_contracts = vec![StackTargetContractV4::capture(
            &state,
            Target::Object(target),
        )];
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut restored] {
            crate::effect::execute(&op, &ctx, branch);
            assert_eq!(branch.objects.get(target).counters.plus1_plus1, 1);
            crate::event::propose_and_commit(
                branch,
                crate::event::ProposedEvent::zone_change(target, Zone::Hand),
            );
            crate::event::propose_and_commit(
                branch,
                crate::event::ProposedEvent::zone_change(target, Zone::Battlefield),
            );
            crate::effect::execute(&op, &ctx, branch);
            assert_eq!(branch.objects.get(target).counters.plus1_plus1, 0);
            assert!(branch.engine.halted.is_none());
        }
        assert_eq!(state, restored);
    }
}
