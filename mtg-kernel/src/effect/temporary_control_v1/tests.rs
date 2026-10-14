//! Actual registered recipients exercise the primitive, not printed admission.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state, ready_state_for};
use crate::state::StackTargetContractV4;

fn ready_turn(actor: PlayerId) -> GameState {
    let mut state = ready_state_for(actor);
    for owner in [actor, actor.opponent()] {
        put(&mut state, owner, "Forest", Zone::Library);
    }
    state
}

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
        assert!(
            !matches!(decision, crate::engine::Decision::Halted { .. }),
            "{decision:?}"
        );
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
            let mut state = ready_turn(controller);
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
        let mut state = ready_turn(controller);
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
        let ending_turn = restored.turn;
        cleanup(&mut restored);
        assert_eq!(
            restored.objects.get(target).controller,
            controller.opponent()
        );
        assert_eq!(restored.objects.get(target).owner, controller);
        // With no priority response available, the real engine advances through
        // Cleanup and into the baseline controller's next Untap. Control expiry
        // first restores that player; their new turn then clears sickness.
        assert_eq!(restored.active_player, controller.opponent());
        assert_eq!(
            restored.turn,
            ending_turn + u32::from(restored.active_player == restored.starting_player)
        );
        assert!(!restored.objects.get(target).summoning_sick);
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
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let grant = ctx(&state, source, controller.opponent(), target);
        execute(
            &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                object: ObjectRef::Target(0),
                keyword: Keywords::PROTECTION_FROM_MONOCOLORED,
            },
            &grant,
            &mut state,
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
        let mut state = ready_turn(controller);
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
        let mut state = ready_turn(controller.opponent());
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
#[test]
fn repeated_cleanup_clears_ordinary_and_restricted_mana_for_both_players() {
    for controller in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, target) =
            super::authority_tests::later_cleanup_priority(controller);
        gain_control(&ctx(&state, source, controller, target), 0, &mut state);
        for player in &mut state.players {
            player.mana_pool = [9; 6];
            player
                .restricted_mana_pool
                .0
                .push(crate::mana::RestrictedManaUnitV1 {
                    color: crate::mana::ManaColor::G,
                    restriction: crate::card_def::ManaSpendRestrictionDef::CreatureSpell,
                    source: crate::state::ObjectLinkV4 {
                        object: target,
                        zone_change_count: state.objects.get(target).zone_change_count,
                    },
                    source_card_def: state.objects.get(target).card_def,
                });
        }
        let mut restored = replay(&state);
        super::authority_tests::finish_later_cleanup(&mut state, controller);
        super::authority_tests::finish_later_cleanup(&mut restored, controller);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        for player in &state.players {
            assert_eq!(player.mana_pool, [0; 6]);
            assert!(player.restricted_mana_pool.is_empty());
        }
        assert_eq!(state.objects.get(target).controller, controller.opponent());
    }
}

// Real attachment timestamps exist only in the Limited rules feature;
// Standard enables that feature. Default preserves its timestamp-free state.
#[cfg(feature = "limited-fdn-fixtures")]
#[test]
fn accepted_typed_aegis_copy_preserves_employment_target_and_refuses_foreign_or_stale_history() {
    use crate::state::{AbilitySourceContractV4, ObjectLinkV4};
    for controller in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_turn(controller);
        let source = put(&mut state, controller, "Lightning Bolt", Zone::Hand);
        let target = put(
            &mut state,
            controller.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let original_ctx = ctx(&state, source, controller, target);
        let attachment = put(
            &mut state,
            controller.opponent(),
            "Viridian Longbow",
            Zone::Battlefield,
        );
        let donor = put(&mut state, controller, "Tolarian Terror", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            event::ProposedEvent::zone_change(donor, Zone::Exile),
        );
        let host = ObjectLinkV4 {
            object: target,
            zone_change_count: state.objects.get(target).zone_change_count,
        };
        state
            .attach_object_exact(attachment, 0, target, host.zone_change_count)
            .unwrap();
        let attachment_source = AbilitySourceContractV4::capture(&state, attachment);
        let attachment_link = ObjectLinkV4 {
            object: attachment,
            zone_change_count: attachment_source.zone_change_count,
        };
        state.objects.get_mut(donor).v4.exiled_by = Some(attachment_link);
        let mut copy_ctx = ExecCtx::no_targets(attachment, controller.opponent());
        copy_ctx.ability_source_contract = Some(attachment_source);
        let timestamp = state.objects.get(attachment).v4.layer_timestamp.unwrap();
        let binding = EffectObjectBinding {
            object: donor,
            expected_zone: Zone::Exile,
            expected_zone_change_count: state.objects.get(donor).zone_change_count,
        };
        // Explicitly typed accepted Aegis copy primitive with registered physical
        // stand-ins. No printed Longbow or Employment definition is changed.
        assert!(crate::standard_cards_v1::aegis_copy_candidates(
            &state, &copy_ctx, host, timestamp
        )
        .contains(&binding));
        crate::standard_cards_v1::apply_aegis_copy(&mut state, &copy_ctx, host, timestamp, binding);
        assert_eq!(
            state.objects.get(target).card_def,
            state.objects.get(donor).card_def
        );
        assert!(!original_ctx.target_incarnation_matches(0, &state));
        let original = original_ctx.target_contracts[0];
        let normalized = normalize_historical_target(&state, original);
        assert!(crate::engine::target_contract_matches_live(
            &state,
            Target::Object(target),
            normalized
        ));
        assert!(crate::state::stack_target_contract_is_structurally_valid(
            &state,
            crate::card_def::TargetSpec::Creature,
            0,
            Target::Object(target),
            normalized
        ));
        let mut protected = replay(&state);
        let mut normalized_ctx = original_ctx.clone();
        normalized_ctx.target_contracts[0] = normalized;
        execute(
            &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                object: ObjectRef::Target(0),
                keyword: Keywords::PROTECTION_FROM_MONOCOLORED,
            },
            &normalized_ctx,
            &mut protected,
        );
        let protected_before = serde_json::to_vec(&protected).unwrap();
        gain_control(&normalized_ctx, 0, &mut protected);
        assert_eq!(protected_before, serde_json::to_vec(&protected).unwrap());
        let mut restored = replay(&state);
        for current in [&mut state, &mut restored] {
            let mut context = original_ctx.clone();
            context.target_contracts[0] = normalize_historical_target(current, original);
            assert!(context.target_incarnation_matches(0, current));
            execute(&involuntary_employment_program(), &context, current);
            assert_eq!(current.objects.get(target).controller, controller);
            assert!(crate::engine::has_effective_keyword(
                current,
                target,
                Keywords::HASTE
            ));
            assert_eq!(
                current.players[controller.index()]
                    .battlefield
                    .iter()
                    .filter(|id| current.objects.get(**id).name == "Treasure Token")
                    .count(),
                1
            );
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        for mutate in [0, 1, 2] {
            let mut forged = original;
            if let StackTargetContractV4::Object {
                card_def,
                owner,
                zone_change_count,
                ..
            } = &mut forged
            {
                match mutate {
                    0 => *card_def = crate::card_def::card_id_by_name("Forest").unwrap(),
                    1 => *owner = controller,
                    _ => *zone_change_count += 1,
                }
            }
            assert_eq!(normalize_historical_target(&state, forged), forged);
            assert!(!crate::engine::target_contract_matches_live(
                &state,
                Target::Object(target),
                forged
            ));
        }
        event::propose_and_commit(
            &mut state,
            event::ProposedEvent::zone_change(target, Zone::Hand),
        );
        event::propose_and_commit(
            &mut state,
            event::ProposedEvent::zone_change(target, Zone::Battlefield),
        );
        assert_eq!(normalize_historical_target(&state, original), original);
        assert!(!crate::engine::target_contract_matches_live(
            &state,
            Target::Object(target),
            original
        ));
    }
}
