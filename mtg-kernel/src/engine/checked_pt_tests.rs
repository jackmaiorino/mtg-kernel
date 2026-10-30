use super::*;
use crate::policy_observation_v6::tests::{put, ready_state_for};

fn bytes(state: &GameState) -> Vec<u8> {
    serde_json::to_vec(state).unwrap()
}

fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&bytes(state)).unwrap()
}

fn finish(state: &mut GameState) -> Decision {
    for _ in 0..48 {
        let decision = advance_until_decision(state);
        match &decision {
            Decision::Halted { .. } => return decision,
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty()
                    && state.engine.pending_cast.is_none()
                    && state.engine.pending_activation.is_none()
                    && state.engine.pending_effect.is_none()
                    && state.engine.pending_triggers.is_empty() =>
            {
                return decision;
            }
            Decision::CastSpellOrPass { .. } => step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("actual paid PT fixture: {other:?}"),
        }
    }
    panic!("actual paid PT fixture did not finish")
}

fn paid_hatcher(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = ready_state_for(actor);
    for seat in [actor, actor.opponent()] {
        put(&mut state, seat, "Forest", Zone::Hand);
        for _ in 0..12 {
            put(&mut state, seat, "Island", Zone::Library);
        }
    }
    let hatcher = put(&mut state, actor, "Drake Hatcher", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    step(&mut state, Action::CastSpell(hatcher)).unwrap();
    assert!(
        matches!(finish(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    assert_eq!(state.objects.get(hatcher).zone, Zone::Battlefield);
    assert!(state.players[actor.index()]
        .mana_pool
        .iter()
        .all(|n| *n == 0));
    (state, hatcher)
}

fn pay_think_twice(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let spell = put(state, actor, "Think Twice", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    step(state, Action::CastSpell(spell)).unwrap();
    // The real cast is staged. Advance before asserting actual publication,
    // payment or the materialized printed Prowess trigger.
    assert!(matches!(
        advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.engine.pending_cast.is_none());
    assert!(state.players[actor.index()]
        .mana_pool
        .iter()
        .all(|n| *n == 0));
    assert!(state.stack.iter().any(|item| {
        matches!(
            item.inline_effect,
            Some(EffectOp::BoostBoundObjectUntilEndOfTurn {
                power: 1,
                toughness: 1,
                ..
            })
        )
    }));
    spell
}

#[test]
fn real_paid_prowess_exact_native_boundary_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for count in [0, i32::MAX - 4] {
            let (mut state, hatcher) = paid_hatcher(actor);
            event::add_plus_one_counters(&mut state, hatcher, actor, count).unwrap();
            assert_eq!(
                checked_effective_pt(&state, hatcher).unwrap(),
                (count + 1, count + 3)
            );
            let spell = pay_think_twice(&mut state, actor);
            let mut restored = restore(&state);
            for branch in [&mut state, &mut restored] {
                assert!(matches!(finish(branch), Decision::CastSpellOrPass { .. }));
                let expected = (count + 2, count + 4);
                assert_eq!(checked_effective_pt(branch, hatcher).unwrap(), expected);
                assert_eq!(effective_power(branch, hatcher), expected.0);
                assert_eq!(effective_toughness(branch, hatcher), expected.1);
                assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(branch.objects.get(hatcher).zone, Zone::Battlefield);
                crate::drake_public_v53::capture(branch).unwrap();
                crate::trigger::sba_fixed_point(branch);
                assert!(branch.engine.halted.is_none());
            }
            assert_eq!(bytes(&state), bytes(&restored));
        }
    }
}

#[test]
fn real_paid_prowess_refuses_before_modifier_or_timestamp_and_restores_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, hatcher) = paid_hatcher(actor);
        event::add_plus_one_counters(&mut state, hatcher, actor, i32::MAX - 3).unwrap();
        assert_eq!(
            checked_effective_pt(&state, hatcher).unwrap(),
            (i32::MAX - 2, i32::MAX)
        );
        let spell = pay_think_twice(&mut state, actor);
        let modifiers = state.engine.until_end_of_turn.clone();
        // The paid cast and genuine Prowess publication already allocated all
        // their timestamps. Only the resolving boost must remain uninstalled.
        let timestamp = state.engine.next_effect_timestamp;
        let hand = state.players[actor.index()].hand.clone();
        let mut restored = restore(&state);
        for branch in [&mut state, &mut restored] {
            assert!(matches!(finish(branch), Decision::Halted {
                mechanic: UnsupportedMechanic::InvalidEffectContinuation, source
            } if source == hatcher));
            assert_eq!(branch.engine.until_end_of_turn, modifiers);
            assert_eq!(branch.engine.next_effect_timestamp, timestamp);
            assert_eq!(branch.players[actor.index()].hand, hand);
            assert_eq!(branch.objects.get(spell).zone, Zone::Stack);
            assert_eq!(branch.objects.get(hatcher).zone, Zone::Battlefield);
            assert_eq!(
                checked_effective_pt(branch, hatcher).unwrap(),
                (i32::MAX - 2, i32::MAX)
            );
        }
        assert_eq!(bytes(&state), bytes(&restored));
    }
}

#[test]
fn retained_out_of_envelope_placement_refuses_step_producers_and_sba_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, hatcher) = paid_hatcher(actor);
        event::add_plus_one_counters(&mut state, hatcher, actor, i32::MAX - 4).unwrap();
        pay_think_twice(&mut state, actor);
        assert!(matches!(
            finish(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        // Counter placement is intentionally not one of this patch's writer
        // preflights. Preserve its genuine event and test the next ingress/SBA.
        event::add_plus_one_counters(&mut state, hatcher, actor, 1).unwrap();
        assert!(checked_effective_pt(&state, hatcher).is_err());
        for _ in 0..3 {
            let mut branch = restore(&state);
            let before = bytes(&branch);
            assert!(step(&mut branch, Action::Pass).is_err());
            assert_eq!(bytes(&branch), before);
            assert!(crate::drake_public_v53::capture(&branch).is_err());
            assert!(crate::elenda_public_v52::capture(&branch).is_err());
            assert!(crate::prepared_observation_v57::validate_state(&branch).is_err());
            assert_eq!(bytes(&branch), before);
            let mut halted = branch.clone();
            halted.engine.halted = Some((UnsupportedMechanic::InvalidEffectContinuation, hatcher));
            crate::trigger::sba_fixed_point(&mut branch);
            assert_eq!(bytes(&branch), bytes(&halted));
            assert_eq!(branch.objects.get(hatcher).zone, Zone::Battlefield);
            let mut advancing = restore(&state);
            assert!(
                matches!(advance_until_decision(&mut advancing), Decision::Halted { source, .. } if source == hatcher)
            );
            assert_eq!(bytes(&advancing), bytes(&halted));
        }
    }
}

#[test]
fn shared_query_preserves_cancellation_and_sequence_stops_on_lower_boundary_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, hatcher) = paid_hatcher(actor);
        let binding = EffectObjectBinding {
            object: hatcher,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: state.objects.get(hatcher).zone_change_count,
        };
        // Explicit typed arithmetic operands, not a fictional printed spell.
        effect::install_temporary_boost(&mut state, binding, -3, -3, Keywords::NONE);
        event::add_plus_one_counters(&mut state, hatcher, actor, i32::MAX).unwrap();
        assert_eq!(
            checked_effective_pt(&state, hatcher).unwrap(),
            (i32::MAX - 2, i32::MAX)
        );
        let mut lower = paid_hatcher(actor).0;
        let object = lower.players[actor.index()]
            .battlefield
            .iter()
            .copied()
            .find(|id| lower.objects.get(*id).name == "Drake Hatcher")
            .unwrap();
        let binding = EffectObjectBinding {
            object,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: lower.objects.get(object).zone_change_count,
        };
        effect::install_temporary_boost(&mut lower, binding, i32::MIN, i32::MIN, Keywords::NONE);
        assert_eq!(
            checked_effective_pt(&lower, object).unwrap(),
            (i32::MIN + 1, i32::MIN + 3)
        );
        let original = lower.clone();
        let program = EffectOp::Sequence(vec![
            EffectOp::BoostBoundObjectUntilEndOfTurn {
                object: binding,
                power: -2,
                toughness: -4,
            },
            EffectOp::GainLife {
                player: effect::PlayerRef::Controller,
                amount: 1,
            },
        ]);
        let mut restored = restore(&lower);
        for branch in [&mut lower, &mut restored] {
            let mut halted = original.clone();
            halted.engine.halted = Some((UnsupportedMechanic::InvalidEffectContinuation, object));
            // The primitive itself must refuse before any modifier, timestamp,
            // event or custody write. A sequence's public child dispatcher also
            // authenticates that refused instruction as a whole native input.
            let mut primitive = branch.clone();
            effect::install_temporary_boost(&mut primitive, binding, -2, -4, Keywords::NONE);
            assert_eq!(bytes(&primitive), bytes(&halted));
            let ctx = ExecCtx::no_targets(object, actor);
            effect::execute_without_linked_aura_v1(&program, &ctx, branch);
            // This is only an expected comparison value, never an engine input.
            // Bind the full actual original/output scenes and empty emitted
            // segment rather than copying or removing the observed history.
            let index = u64::try_from(original.engine.event_history.len()).unwrap();
            let mut input_scene = original.clone();
            input_scene.engine.event_history.clear();
            let mut output_scene = halted.clone();
            output_scene.engine.event_history.clear();
            let mut expected = halted;
            expected
                .engine
                .event_history
                .push(CommittedEvent::DrakeJournalV1 {
                    record: Box::new(crate::drake_hatcher_v1::custody::RecordV1 {
                        index,
                        value: crate::drake_hatcher_v1::custody::JournalV1::Enclosing {
                            start: index,
                            original: std::sync::Arc::new(input_scene),
                            operation: crate::linked_aura_operation_v1::OperationV1::Effect {
                                op: EffectOp::BoostBoundObjectUntilEndOfTurn {
                                    object: binding,
                                    power: -2,
                                    toughness: -4,
                                },
                                ctx: Box::new(ctx),
                            },
                            answers: vec![],
                            used_banner: false,
                            output: std::sync::Arc::new(output_scene),
                        },
                    }),
                });
            expected.engine.drake_ledger_v1.issued = original
                .engine
                .drake_ledger_v1
                .issued
                .checked_add(1)
                .unwrap();
            assert_eq!(bytes(branch), bytes(&expected));
            crate::drake_hatcher_v1::custody::validate(branch).unwrap();
        }
        assert_eq!(bytes(&lower), bytes(&restored));
    }
}
