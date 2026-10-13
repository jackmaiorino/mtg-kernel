use super::*;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::put;
use crate::state::Zone;

fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(e) = response else {
        panic!("{response:?}")
    };
    e
}
pub(crate) fn choose(session: &mut FastActorSessionV1, want: impl Fn(&ActionSemanticV1) -> bool) {
    let e = expected(session);
    let (_, actions, binding) = session
        .hunter_current_decision_input_v45(e, e.acting_player)
        .unwrap();
    let index = actions
        .iter()
        .position(want)
        .unwrap_or_else(|| panic!("{actions:?}"));
    session
        .hunter_validate_bound_decision_v45(e, e.acting_player, &binding)
        .unwrap();
    session.step(e.episode_id, e.step, index as u32).unwrap();
}
#[test]
fn registered_complete_binding_reorders_forgeries_and_monotonic_restore_refuse_readonly_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        put(&mut state, actor, "Island", Zone::Hand);
        let mut session = FastActorSessionV1::from_hunter_v45_fixture_state(state);
        let e = expected(&session);
        let (_, actions, binding) = session
            .hunter_current_decision_input_v45(e, actor.into())
            .unwrap();
        assert!(actions.len() >= 3);
        let before = session.privileged_core_environment_hash();
        for case in 0..3 {
            let mut bad = binding.clone();
            match case {
                0 => bad.candidates.reverse(),
                1 => bad.observation.hunter.card_db_hash ^= 1,
                _ => bad.expected.legal_action_count += 1,
            };
            assert!(session
                .hunter_validate_bound_decision_v45(e, actor.into(), &bad)
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), before);
        }
        let snapshot = session.snapshot_v1();
        for _ in 0..3 {
            session.hunter_restore_v45(&snapshot).unwrap();
            assert!(session
                .hunter_validate_bound_decision_v45(e, actor.into(), &binding)
                .is_err());
            let current = expected(&session);
            let (_, _, binding) = session
                .hunter_current_decision_input_v45(current, actor.into())
                .unwrap();
            session
                .hunter_validate_bound_decision_v45(current, actor.into(), &binding)
                .unwrap();
        }
    }
}
#[test]
#[ignore = "unregistered Hunter; parent owns actual registration/native"]
fn actual_paid_cast_attack_decline_sacrifice_death_draw_and_restored_complete_public_owner_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let hunter = put(&mut state, actor, "High-Society Hunter", Zone::Hand);
            let other = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            put(&mut state, actor, "Forest", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 2;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
            let mut session = FastActorSessionV1::from_hunter_v45_fixture_state(state);
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==hunter.0),
            );
            for _ in 0..20 {
                if session.state.objects.get(hunter).zone == Zone::Battlefield {
                    break;
                }
                choose(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
            }
            assert_eq!(session.state.objects.get(hunter).zone, Zone::Battlefield);
            assert!(session.state.engine.pending_cast.is_none());
            assert_eq!(
                session.state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
                0
            );
            // Labeled creature haste/phase primitive after the genuine paid printed cast.
            let generation = session.state.objects.get(hunter).zone_change_count;
            crate::effect::install_temporary_boost(
                &mut session.state,
                crate::effect::EffectObjectBinding {
                    object: hunter,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: generation,
                },
                0,
                0,
                crate::card_def::Keywords::HASTE,
            );
            session.state.step = crate::state::Step::DeclareAttackers;
            crate::engine::step(
                &mut session.state,
                crate::engine::Action::DeclareAttackers(vec![hunter]),
            )
            .unwrap();
            session = FastActorSessionV1::from_hunter_v45_fixture_state(session.state.clone());
            for _ in 0..20 {
                if session
                    .state
                    .engine
                    .pending_effect
                    .as_ref()
                    .is_some_and(|p| p.choice.is_some())
                {
                    break;
                }
                choose(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
            }
            let e = expected(&session);
            let (o, _, binding) = session
                .hunter_current_decision_input_v45(e, actor.into())
                .unwrap();
            assert!(o.hunter.menu.is_some());
            assert_eq!(o.hunter.occurrences.len(), 1);
            let snapshot = session.snapshot_v1();
            session.hunter_restore_v45(&snapshot).unwrap();
            assert!(session
                .hunter_validate_bound_decision_v45(e, actor.into(), &binding)
                .is_err());
            let mut replay = session.clone();
            let hand = session.state.players[actor.index()].hand.len();
            for current in [&mut session, &mut replay] {
                if pay {
                    choose(
                        current,
                        |a| matches!(a,ActionSemanticV1::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==other.0),
                    );
                } else {
                    choose(current, |a| {
                        matches!(a, ActionSemanticV1::FinishEffectSelection { .. })
                    });
                }
                for _ in 0..30 {
                    if current.state.stack.is_empty()
                        && current.state.engine.pending_triggers.is_empty()
                    {
                        break;
                    }
                    choose(current, |a| matches!(a, ActionSemanticV1::Pass { .. }));
                }
                assert_eq!(
                    current.state.objects.get(hunter).counters.plus1_plus1,
                    i32::from(pay)
                );
                assert_eq!(
                    current.state.players[actor.index()].hand.len(),
                    hand + usize::from(pay)
                );
                let e = expected(current);
                let (o, _, _) = current
                    .hunter_current_decision_input_v45(e, e.acting_player)
                    .unwrap();
                assert!(o
                    .hunter
                    .occurrences
                    .iter()
                    .all(|row| row.phase
                        == crate::high_society_hunter_public_v45::PhaseV45::Completed));
                assert_eq!(
                    o.hunter
                        .occurrences
                        .iter()
                        .filter(|row| row.draw_attempted)
                        .count(),
                    usize::from(pay)
                );
            }
            assert_eq!(
                serde_json::to_vec(&session.state).unwrap(),
                serde_json::to_vec(&replay.state).unwrap()
            );
        }
    }
}

#[test]
fn real_registered_generic_ward_payment_or_decline_keeps_original_spell_and_full_public_binding_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            let terror = put(
                &mut state,
                actor.opponent(),
                "Tolarian Terror",
                Zone::Battlefield,
            );
            put(&mut state, actor, "Forest", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
            let mut session = FastActorSessionV1::from_hunter_v45_fixture_state(state);
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==bolt.0),
            );
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==terror.0),
            );
            for _ in 0..16 {
                let e = expected(&session);
                let (o, actions, _) = session
                    .hunter_current_decision_input_v45(e, e.acting_player)
                    .unwrap();
                assert!(o.hunter.menu.is_none());
                if actions
                    .iter()
                    .any(|a| matches!(a, ActionSemanticV1::ChooseEffectBoolean { .. }))
                {
                    break;
                }
                choose(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
            }
            let saved = session.snapshot_v1();
            let mut replay = session.clone();
            replay.hunter_restore_v45(&saved).unwrap();
            for current in [&mut session, &mut replay] {
                choose(
                    current,
                    |a| matches!(a,ActionSemanticV1::ChooseEffectBoolean{value,..}if *value==pay),
                );
                for _ in 0..16 {
                    if current.state.stack.is_empty() {
                        break;
                    }
                    choose(current, |a| matches!(a, ActionSemanticV1::Pass { .. }));
                }
                assert_eq!(current.state.objects.get(bolt).zone, Zone::Graveyard);
                assert_eq!(
                    current.state.objects.get(terror).damage,
                    if pay { 3 } else { 0 }
                );
                assert_eq!(
                    current.state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                    if pay { 0 } else { 2 }
                );
                assert!(current.state.stack.is_empty());
            }
            assert_eq!(
                serde_json::to_vec(&session.state).unwrap(),
                serde_json::to_vec(&replay.state).unwrap()
            );
            assert_eq!(session.state.objects.get(terror).zone, Zone::Battlefield);
            assert!(session.state.engine.event_history.iter().any(|event|matches!(event,crate::event::CommittedEvent::Targeted{target,..}if *target==terror)));
        }
    }
}

#[test]
#[ignore = "unregistered Hunter; parent owns actual registration/native"]
fn genuine_held_and_resolving_typed_copy_keep_original_controller_source_identity_counter_and_restore(
) {
    use crate::effect::{EffectObjectBinding, ExecCtx};
    use crate::state::{AbilitySourceContractV4, ObjectLinkV4};
    for actor in [PlayerId::P0, PlayerId::P1] {
        for held in [false, true] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let hunter = put(&mut state, actor, "High-Society Hunter", Zone::Battlefield);
            let other = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            put(&mut state, actor, "Forest", Zone::Hand);
            state.step = crate::state::Step::DeclareAttackers;
            crate::engine::step(
                &mut state,
                crate::engine::Action::DeclareAttackers(vec![hunter]),
            )
            .unwrap();
            if held {
                let waiting = crate::trigger::collect_and_process(&mut state);
                assert_eq!(waiting.len(), 1);
                state.engine.pending_triggers.extend(waiting);
            } else {
                for _ in 0..16 {
                    let decision = crate::engine::advance_until_decision(&mut state);
                    if matches!(
                        decision,
                        crate::engine::Decision::ChooseEffectTargets { .. }
                    ) {
                        break;
                    }
                    assert!(
                        matches!(decision, crate::engine::Decision::CastSpellOrPass { .. }),
                        "{decision:?}"
                    );
                    crate::engine::step(&mut state, crate::engine::Action::Pass).unwrap();
                }
                assert!(state
                    .engine
                    .pending_effect
                    .as_ref()
                    .is_some_and(|p| p.choice.is_some()));
            }
            // Genuine accepted typed Aegis copy primitive, not a printed mixed-pool spell.
            let attachment = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
            let donor = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(donor, Zone::Exile),
            );
            let generation = state.objects.get(hunter).zone_change_count;
            state
                .attach_object_exact(attachment, 0, hunter, generation)
                .unwrap();
            let source = AbilitySourceContractV4::capture(&state, attachment);
            state.objects.get_mut(donor).v4.exiled_by = Some(ObjectLinkV4 {
                object: attachment,
                zone_change_count: source.zone_change_count,
            });
            let mut ctx = ExecCtx::no_targets(attachment, actor);
            ctx.ability_source_contract = Some(source);
            let timestamp = state.objects.get(attachment).v4.layer_timestamp.unwrap();
            let binding = EffectObjectBinding {
                object: donor,
                expected_zone: Zone::Exile,
                expected_zone_change_count: state.objects.get(donor).zone_change_count,
            };
            let host = ObjectLinkV4 {
                object: hunter,
                zone_change_count: generation,
            };
            assert!(
                crate::standard_cards_v1::aegis_copy_candidates(&state, &ctx, host, timestamp)
                    .contains(&binding)
            );
            crate::standard_cards_v1::apply_aegis_copy(&mut state, &ctx, host, timestamp, binding);
            // Labeled physical controller transfer after real trigger capture.
            state.players[actor.index()]
                .battlefield
                .retain(|id| *id != hunter);
            state.players[actor.opponent().index()]
                .battlefield
                .push(hunter);
            state.objects.get_mut(hunter).controller = actor.opponent();
            let mut session = FastActorSessionV1::from_hunter_v45_fixture_state(state);
            for _ in 0..16 {
                if session
                    .state
                    .engine
                    .pending_effect
                    .as_ref()
                    .is_some_and(|p| p.choice.is_some())
                {
                    break;
                }
                choose(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
            }
            let e = expected(&session);
            let (o, actions, _) = session
                .hunter_current_decision_input_v45(e, actor.into())
                .unwrap();
            let original = &o.hunter.occurrences[0].source;
            assert_eq!(
                original.card_db_id,
                crate::card_def::card_id_by_name("High-Society Hunter").unwrap()
            );
            assert_eq!(original.controller, actor.into());
            assert_eq!(original.zone_change_count, generation);
            assert!(actions.iter().all(
                |a| !matches!(a,ActionSemanticV1::ChooseEffectTarget{source,..}if source!=original)
            ));
            assert_eq!(
                session.state.objects.get(hunter).card_def,
                crate::card_def::card_id_by_name("Llanowar Elves").unwrap()
            );
            let snapshot = session.snapshot_v1();
            let mut replay = session.clone();
            replay.hunter_restore_v45(&snapshot).unwrap();
            for current in [&mut session, &mut replay] {
                choose(
                    current,
                    |a| matches!(a,ActionSemanticV1::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==other.0),
                );
                for _ in 0..16 {
                    if current.state.stack.is_empty()
                        && current.state.engine.pending_triggers.is_empty()
                    {
                        break;
                    }
                    choose(current, |a| matches!(a, ActionSemanticV1::Pass { .. }));
                }
                assert_eq!(current.state.objects.get(hunter).counters.plus1_plus1, 1);
                assert_eq!(
                    current.state.objects.get(hunter).controller,
                    actor.opponent()
                );
                assert_eq!(
                    current.state.objects.get(hunter).zone_change_count,
                    generation
                );
            }
            assert_eq!(
                serde_json::to_vec(&session.state).unwrap(),
                serde_json::to_vec(&replay.state).unwrap()
            );
        }
    }
}
