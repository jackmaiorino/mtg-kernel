//! Actual registered producers and typed phase-entry primitives; no admission hook.
use super::*;
use crate::policy_observation_v6::tests::{put, ready_state_for};

fn next(state: &mut GameState) -> Decision {
    let decision = advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn resolve_cast(state: &mut GameState, source: ObjectId) {
    for _ in 0..32 {
        let decision = next(state);
        if state.engine.pending_cast.is_none()
            && state.objects.get(source).zone == Zone::Battlefield
            && !state.stack.iter().any(|row| row.source == source)
        {
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        step(state, Action::Pass).unwrap();
    }
    panic!("actual cast did not finish");
}
fn paid(actor: PlayerId, name: &str) -> GameState {
    let mut state = ready_state_for(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    let source = put(&mut state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] =
        u8::from(name == "Heartfire Immolator");
    step(&mut state, Action::CastSpell(source)).unwrap();
    resolve_cast(&mut state, source);
    let target = put(
        &mut state,
        actor.opponent(),
        "Shivan Dragon",
        Zone::Battlefield,
    );
    state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
    step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_activation.is_none());
    assert!(state.stack.iter().any(|row| row.source == source));
    crate::source_power_activation_v1::validate_state(&state).unwrap();
    state
}
#[test]
fn real_paid_historical_sources_refuse_before_outer_and_inner_advance_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in ["Fanatical Firebrand", "Heartfire Immolator"] {
            let state = paid(actor, name);
            for mode in 0..3 {
                let mut bad: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                match mode {
                    0 => bad.engine.event_history.retain(|event| {
                        !matches!(
                            event,
                            CommittedEvent::FixedDamagePaidV3 { .. }
                                | CommittedEvent::SourcePowerActivationCommittedV1 { .. }
                        )
                    }),
                    1 => bad.stack.clear(),
                    _ => {
                        let item = bad.stack.last_mut().unwrap();
                        item.inline_effect = Some(EffectOp::Sequence(Vec::new()));
                    }
                }
                assert!(crate::source_power_activation_v1::validate_state(&bad).is_err());
                for advance in [
                    advance_until_decision as fn(&mut GameState) -> Decision,
                    advance_without_linked_aura_v1,
                ] {
                    let mut branch = bad.clone();
                    let bytes = serde_json::to_vec(&branch).unwrap();
                    assert!(matches!(
                        advance(&mut branch),
                        Decision::Halted {
                            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
                            ..
                        }
                    ));
                    branch.engine.halted = None;
                    assert_eq!(serde_json::to_vec(&branch).unwrap(), bytes);
                }
            }
        }
    }
}
#[test]
fn coherent_phase_entry_false_raid_completes_crystal_without_preparation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        put(&mut state, actor, "Forest", Zone::Battlefield);
        put(&mut state, actor.opponent(), "Forest", Zone::Battlefield);
        // Registered static recipient directly belongs to the opponent; no old
        // until-end-of-turn control owner is carried into the next epoch.
        put(
            &mut state,
            actor.opponent(),
            "Crystal Barricade",
            Zone::Battlefield,
        );
        let target = put(
            &mut state,
            actor.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        let attacker = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        state.objects.get_mut(attacker).summoning_sick = false;
        state.step = Step::DeclareAttackers;
        step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
        assert!(state.player_attacked_this_turn_v1(actor));
        state.step = Step::Main2;
        let source = put(&mut state, actor, "Gorehorn Raider", Zone::Hand);
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 4;
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
        step(&mut state, Action::CastSpell(source)).unwrap();
        for _ in 0..32 {
            match next(&mut state) {
                Decision::ChooseTargets { legal_targets, .. } => {
                    assert!(legal_targets.contains(&Target::Object(target)));
                    step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
                    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
                    break;
                }
                Decision::CastSpellOrPass { .. } => step(&mut state, Action::Pass).unwrap(),
                other => panic!("actual Raid placement: {other:?}"),
            }
        }
        assert!(state
            .stack
            .iter()
            .any(|row| row.source == source && row.kind == StackItemKind::TriggeredAbility));
        // This is an internal stale-turn phase primitive, not a claim that
        // ordinary priority carries an unresolved ability across a turn.
        // Execute genuine expiry, player flip and Untap/turn-anchor routines.
        state.step = Step::Cleanup;
        run_step_entry_action(&mut state, Step::Cleanup);
        advance_step(&mut state);
        assert_eq!(state.active_player, actor.opponent());
        assert_eq!(state.step, Step::Untap);
        assert!(!state.player_attacked_this_turn_v1(actor));
        crate::life_loss_turn_v1::validate(&state).unwrap();
        crate::raid_entry_v1::validate_state(&state).unwrap();
        state.step = Step::Main1;
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for current in [&mut state, &mut replay] {
            for _ in 0..32 {
                if current.stack.is_empty() {
                    break;
                }
                assert!(matches!(next(current), Decision::CastSpellOrPass { .. }));
                step(current, Action::Pass).unwrap();
            }
            assert!(current.stack.is_empty());
            assert_eq!(current.objects.get(target).damage, 0);
            assert!(!current
                .engine
                .event_history
                .iter()
                .any(|event| matches!(event, CommittedEvent::GorehornPreparedV1 { .. })));
            assert!(current
                .engine
                .event_history
                .iter()
                .any(|event| matches!(event,
                CommittedEvent::GorehornCrystalCompletionV1 { completion }
                    if completion.item.source == source)));
            assert!(current
                .engine
                .event_history
                .iter()
                .any(|event| matches!(event,
                CommittedEvent::CrystalExecutionV1 { execution }
                    if execution.item.source == source
                        && execution.steps.first().is_some_and(|step|
                            step.conditional_result == Some(false)))));
            crate::source_power_activation_v1::validate_state(current).unwrap();
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
