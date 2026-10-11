//! Actual registered recipients exercise the primitive, not printed admission.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::StackTargetContractV4;

fn ctx(state: &GameState, source: ObjectId, controller: PlayerId, target: ObjectId) -> ExecCtx {
    let mut ctx = ExecCtx::no_targets(source, controller);
    ctx.targets.push(Target::Object(target));
    ctx.target_contracts.push(StackTargetContractV4::capture(
        state,
        Target::Object(target),
    ));
    ctx
}
fn replay(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}
fn cleanup(state: &mut GameState) {
    state.step = crate::state::Step::End;
    for _ in 0..8 {
        let decision = crate::engine::advance_until_decision(state);
        if !state.engine.until_end_of_turn.iter().any(|effect| {
            matches!(
                effect,
                UntilEndOfTurnEffect::ResolvedObjectControlEffect { .. }
            )
        }) {
            return;
        }
        match decision {
            crate::engine::Decision::CastSpellOrPass { .. } => {
                crate::engine::step(state, crate::engine::Action::Pass).unwrap();
            }
            other => panic!("unexpected cleanup decision {other:?}"),
        }
    }
    panic!("did not execute cleanup");
}
#[test]
fn prepared_recipe_registered_recipients_control_untap_haste_treasure_and_real_cleanup_both_seats()
{
    for controller in [PlayerId::P0, PlayerId::P1] {
        for owner in [controller, controller.opponent()] {
            let mut state = ready_state();
            state.active_player = controller;
            state.priority_player = controller;
            let source = put(&mut state, controller, "Lightning Bolt", Zone::Hand);
            let target = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
            state.objects.get_mut(target).tapped = true;
            let incarnation = state.objects.get(target).zone_change_count;
            let context = ctx(&state, source, controller, target);
            execute(&involuntary_employment_program(), &context, &mut state);
            let live = state.objects.get(target);
            assert_eq!(live.owner, owner);
            assert_eq!(live.controller, controller);
            assert_eq!(live.zone_change_count, incarnation);
            assert!(!live.tapped);
            assert_eq!(live.summoning_sick, owner != controller);
            assert!(crate::engine::has_effective_keyword(
                &state,
                target,
                Keywords::HASTE
            ));
            assert_eq!(
                state.players[controller.index()]
                    .battlefield
                    .iter()
                    .filter(|id| **id == target)
                    .count(),
                1
            );
            assert_eq!(
                state.players[controller.index()]
                    .battlefield
                    .iter()
                    .filter(|id| state.objects.get(**id).name == "Treasure Token")
                    .count(),
                1
            );
            if owner != controller {
                assert!(!state.players[owner.index()].battlefield.contains(&target));
            }
            let mut restored = replay(&state);
            cleanup(&mut state);
            cleanup(&mut restored);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert_eq!(state.objects.get(target).controller, owner);
            assert!(state.players[owner.index()].battlefield.contains(&target));
            assert!(!crate::engine::has_effective_keyword(
                &state,
                target,
                Keywords::HASTE
            ));
        }
    }
}
#[test]
fn overlapping_control_effects_unwind_to_nonowner_baseline_and_survive_source_departure() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = controller;
        let source = put(&mut state, controller, "Lightning Bolt", Zone::Hand);
        let target = put(&mut state, controller, "Llanowar Elves", Zone::Battlefield);
        // A real borrowed-creature baseline: changing control does not change ownership.
        transfer(&mut state, target, controller.opponent());
        for actor in [controller, controller, controller.opponent(), controller] {
            let context = ctx(&state, source, actor, target);
            gain_control(&context, 0, &mut state);
            assert_eq!(state.objects.get(target).controller, actor);
        }
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        let mut restored = replay(&state);
        cleanup(&mut restored);
        assert_eq!(
            restored.objects.get(target).controller,
            controller.opponent()
        );
        assert_eq!(restored.objects.get(target).owner, controller);
        assert!(restored.objects.get(target).summoning_sick);
    }
}
#[test]
fn stale_departed_reentered_and_protected_target_primitives_cannot_gain_control() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let mut state = ready_state();
            let source = put(&mut state, controller, "Lightning Bolt", Zone::Hand);
            let target = put(
                &mut state,
                controller.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let context = ctx(&state, source, controller, target);
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(target, Zone::Hand),
            );
            if reenter {
                crate::event::propose_and_commit(
                    &mut state,
                    crate::event::ProposedEvent::zone_change(target, Zone::Battlefield),
                );
            }
            let before = serde_json::to_vec(&state).unwrap();
            gain_control(&context, 0, &mut state);
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
        }
        let mut state = ready_state();
        let source = put(&mut state, controller, "Lightning Bolt", Zone::Hand);
        let target = put(
            &mut state,
            controller.opponent(),
            "Guardian of the Guildpact",
            Zone::Battlefield,
        );
        let context = ctx(&state, source, controller, target);
        let before = serde_json::to_vec(&state).unwrap();
        gain_control(&context, 0, &mut state);
        assert_eq!(before, serde_json::to_vec(&state).unwrap());
    }
}
#[test]
fn control_effect_expiry_never_controls_a_reentered_incarnation() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = controller;
        let source = put(&mut state, controller, "Lightning Bolt", Zone::Hand);
        let target = put(
            &mut state,
            controller.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let context = ctx(&state, source, controller, target);
        gain_control(&context, 0, &mut state);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(target, Zone::Hand),
        );
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change_to_battlefield_under_controller(
                target, controller,
            ),
        );
        let mut restored = replay(&state);
        cleanup(&mut restored);
        assert_eq!(restored.objects.get(target).controller, controller);
        assert!(restored.players[controller.index()]
            .battlefield
            .contains(&target));
    }
}
#[test]
fn control_change_removes_real_attackers_from_combat() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = controller.opponent();
        state.priority_player = controller.opponent();
        state.step = crate::state::Step::DeclareAttackers;
        let target = put(
            &mut state,
            controller.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let source = put(&mut state, controller, "Lightning Bolt", Zone::Hand);
        crate::engine::step(
            &mut state,
            crate::engine::Action::DeclareAttackers(vec![target]),
        )
        .unwrap();
        assert!(state.engine.combat.attackers.contains(&target));
        let context = ctx(&state, source, controller, target);
        gain_control(&context, 0, &mut state);
        assert!(!state.engine.combat.attackers.contains(&target));
        assert_eq!(state.objects.get(target).controller, controller);
    }
}
#[test]
fn frozen_observations_refuse_expiring_control_for_both_viewers_without_hidden_details() {
    let mut state = ready_state();
    let source = put(&mut state, PlayerId::P0, "Lightning Bolt", Zone::Hand);
    let target = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let context = ctx(&state, source, PlayerId::P0, target);
    gain_control(&context, 0, &mut state);
    let surface = crate::surface_v2::HarnessSurfaceV2::default();
    let policy = crate::policy_surface_v5::PolicySurfaceV5::new_with_engine_priority_v1();
    for viewer in [PlayerId::P0, PlayerId::P1] {
        assert!(crate::rl::observe_v1(&state, viewer, 0)
            .unwrap_err()
            .to_string()
            .contains("successor"));
        assert!(crate::rl::observe_v2(&state, &surface, viewer, 0).is_err());
        let mut hidden_changed = replay(&state);
        put(
            &mut hidden_changed,
            viewer.opponent(),
            "Counterspell",
            Zone::Library,
        );
        assert_eq!(
            crate::rl::observe_v2(&state, &surface, viewer, 0)
                .unwrap_err()
                .to_string(),
            crate::rl::observe_v2(&hidden_changed, &surface, viewer, 0)
                .unwrap_err()
                .to_string()
        );
        assert!(crate::rl::observe_policy_v5(&state, &policy, viewer, 0, 0, 0, 1).is_err());
        assert!(crate::rl::observe_policy_v6(&state, &policy, viewer, 0, 0, 0, 1).is_err());
        assert!(crate::rl::observe_policy_v8(&state, &policy, viewer, 0, 0, 0, 1).is_err());
    }
}
