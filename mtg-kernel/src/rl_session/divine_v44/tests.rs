//! Actual registered Divine cast path. No definition injection or root bypass.
use super::*;
use crate::card_def::Keywords;
use crate::event::{self, ProposedEvent};
use crate::ids::ObjectId;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::put;
use crate::state::{Target, Zone};

fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(current) = response else {
        panic!("actual decision: {response:?}")
    };
    current
}
fn setup(actor: PlayerId, count: usize) -> (FastActorSessionV1, ObjectId, Vec<ObjectId>) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut state, actor, "Divine Resilience", Zone::Hand);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 2;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    let targets = (0..count)
        .map(|_| put(&mut state, actor, "Llanowar Elves", Zone::Battlefield))
        .collect();
    (
        FastActorSessionV1::from_divine_v44_fixture_state(state),
        source,
        targets,
    )
}
fn choose(session: &mut FastActorSessionV1, want: impl Fn(&ActionSemanticV1) -> bool) {
    let e = expected(session);
    let (_, actions, binding) = session
        .divine_current_decision_input_v44(e, e.acting_player)
        .unwrap();
    let index = actions
        .iter()
        .position(want)
        .unwrap_or_else(|| panic!("actual offered actions: {actions:?}"));
    session
        .divine_validate_bound_decision_v44(e, e.acting_player, &binding)
        .unwrap();
    session.step(e.episode_id, e.step, index as u32).unwrap();
}
fn cast(session: &mut FastActorSessionV1, source: ObjectId, kicked: bool) {
    choose(
        session,
        |a| matches!(a,ActionSemanticV1::CastSpell{source:s,..} if s.arena_id==source.0),
    );
    let e = expected(session);
    let (observation, _, _) = session
        .divine_current_decision_input_v44(e, e.acting_player)
        .unwrap();
    let p = observation.divine.pending.unwrap();
    assert_eq!(
        p.unkicked_quote.unwrap().floating_mana_used[ManaColor::W.pool_index()],
        1
    );
    assert_eq!(
        p.kicked_quote.unwrap().floating_mana_used[ManaColor::W.pool_index()],
        2
    );
    choose(
        session,
        |a| matches!(a,ActionSemanticV1::ChooseDivineKickerV44{pay,..} if *pay==kicked),
    );
}
fn target(session: &mut FastActorSessionV1, id: ObjectId) {
    choose(
        session,
        |a| matches!(a,ActionSemanticV1::ChooseDivineTargetV44{target,..} if target.arena_id==id.0),
    );
}
fn finish(session: &mut FastActorSessionV1) {
    choose(session, |a| {
        matches!(a, ActionSemanticV1::FinishDivineTargetsV44 { .. })
    });
}
fn drain(session: &mut FastActorSessionV1) {
    for _ in 0..80 {
        if session.state.stack.is_empty()
            && session.state.engine.pending_cast.is_none()
            && session.state.engine.pending_effect.is_none()
        {
            return;
        }
        choose(session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
    }
    panic!("real Divine resolution did not finish");
}
fn rebuild(session: &mut FastActorSessionV1) {
    let bytes = serde_json::to_vec(&session.state).unwrap();
    let state = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
    *session = FastActorSessionV1::from_divine_v44_fixture_state(state);
}
fn indestructible(session: &FastActorSessionV1, id: ObjectId) -> bool {
    crate::engine::has_effective_keyword(&session.state, id, Keywords::INDESTRUCTIBLE)
}

#[test]
fn actual_normal_one_and_kicked_zero_two_three_have_exact_costs_targets_and_restored_grants_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (kicked, count) in [(false, 1), (true, 0), (true, 2), (true, 3)] {
            let (mut session, source, targets) = setup(actor, count);
            cast(&mut session, source, kicked);
            let e = expected(&session);
            let (o, actions, _) = session
                .divine_current_decision_input_v44(e, actor.into())
                .unwrap();
            let pending = o.divine.pending.unwrap();
            assert_eq!(
                (pending.minimum, pending.maximum),
                (
                    Some(if kicked { 0 } else { 1 }),
                    if kicked { None } else { Some(1) }
                )
            );
            assert_eq!(
                actions
                    .iter()
                    .any(|a| matches!(a, ActionSemanticV1::FinishDivineTargetsV44 { .. })),
                kicked
            );
            for id in &targets {
                target(&mut session, *id)
            }
            if kicked {
                finish(&mut session)
            }
            assert!(session.state.engine.pending_cast.is_none());
            let item = session
                .state
                .stack
                .iter()
                .find(|s| s.source == source)
                .unwrap();
            assert_eq!(
                item.targets,
                targets
                    .iter()
                    .copied()
                    .map(Target::Object)
                    .collect::<Vec<_>>()
            );
            assert_eq!(item.kicked, kicked);
            assert_eq!(item.v4.target_contracts.len(), count);
            assert_eq!(
                session.state.players[actor.index()].mana_pool[ManaColor::W.pool_index()],
                if kicked { 0 } else { 1 }
            );
            assert_eq!(
                session.state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                if kicked { 0 } else { 2 }
            );
            let mut replay = session.clone();
            rebuild(&mut replay);
            drain(&mut session);
            drain(&mut replay);
            assert_eq!(
                serde_json::to_vec(&session.state).unwrap(),
                serde_json::to_vec(&replay.state).unwrap()
            );
            assert_eq!(session.state.objects.get(source).zone, Zone::Graveyard);
            for id in targets {
                assert!(indestructible(&session, id))
            }
        }
    }
}

#[test]
fn actual_kicked_260_target_roster_retains_wide_count_order_finish_and_every_legal_grant_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut session, source, targets) = setup(actor, 260);
        cast(&mut session, source, true);
        for (count, id) in targets.iter().enumerate() {
            let e = expected(&session);
            let (o, actions, _) = session
                .divine_current_decision_input_v44(e, actor.into())
                .unwrap();
            let p = o.divine.pending.unwrap();
            assert_eq!(
                p.selected.iter().map(|r| r.arena_id).collect::<Vec<_>>(),
                targets[..count].iter().map(|id| id.0).collect::<Vec<_>>()
            );
            assert!(actions.iter().any(|a|matches!(a,ActionSemanticV1::FinishDivineTargetsV44{selected_count,..} if *selected_count==count as u64)));
            assert!(actions.iter().all(|a|!matches!(a,ActionSemanticV1::ChooseDivineTargetV44{selected_count,..} if *selected_count!=count as u64)));
            assert_eq!(p.maximum, None);
            target(&mut session, *id);
        }
        let e = expected(&session);
        let (o, actions, _) = session
            .divine_current_decision_input_v44(e, actor.into())
            .unwrap();
        assert_eq!(o.divine.pending.unwrap().selected.len(), 260);
        assert!(matches!(
            actions.as_slice(),
            [ActionSemanticV1::FinishDivineTargetsV44 {
                selected_count: 260,
                ..
            }]
        ));
        let snapshot = session.snapshot_v1();
        session.divine_restore_v44(&snapshot).unwrap();
        finish(&mut session);
        assert_eq!(
            session
                .state
                .stack
                .iter()
                .find(|s| s.source == source)
                .unwrap()
                .targets
                .len(),
            260
        );
        drain(&mut session);
        assert!(targets.into_iter().all(|id| indestructible(&session, id)));
    }
}

#[test]
fn actual_zero_normal_refusal_protection_and_incarnation_recheck_partial_and_all_fizzle_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (session, source, _) = setup(actor, 0);
        let e = expected(&session);
        let (_, actions, _) = session
            .divine_current_decision_input_v44(e, actor.into())
            .unwrap();
        // Zero controlled creatures permits only the kicked path. Declining kicker is absent.
        let mut session = session;
        assert!(actions
            .iter()
            .any(|a| matches!(a,ActionSemanticV1::CastSpell{source:s,..} if s.arena_id==source.0)));
        choose(
            &mut session,
            |a| matches!(a,ActionSemanticV1::CastSpell{source:s,..} if s.arena_id==source.0),
        );
        let e = expected(&session);
        let (_, actions, _) = session
            .divine_current_decision_input_v44(e, actor.into())
            .unwrap();
        assert!(matches!(
            actions.as_slice(),
            [ActionSemanticV1::ChooseDivineKickerV44 { pay: true, .. }]
        ));
        let before = serde_json::to_vec(&session.state).unwrap();
        assert!(crate::engine::step(
            &mut session.state,
            crate::engine::Action::ChooseKicker(false)
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&session.state).unwrap(), before);
        for count in [1, 3] {
            let (mut session, source, targets) = setup(actor, count);
            cast(&mut session, source, count > 1);
            for id in &targets {
                target(&mut session, *id)
            }
            if count > 1 {
                finish(&mut session)
            }
            event::propose_and_commit(
                &mut session.state,
                ProposedEvent::zone_change(targets[0], Zone::Exile),
            );
            event::propose_and_commit(
                &mut session.state,
                ProposedEvent::zone_change(targets[0], Zone::Battlefield),
            );
            if count > 1 {
                let id = targets[1];
                let binding = crate::effect::EffectObjectBinding {
                    object: id,
                    expected_zone: Zone::Battlefield,
                    expected_zone_change_count: session.state.objects.get(id).zone_change_count,
                };
                crate::effect::install_temporary_boost(
                    &mut session.state,
                    binding,
                    0,
                    0,
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                );
            }
            rebuild(&mut session);
            drain(&mut session);
            assert!(!indestructible(&session, targets[0]));
            if count > 1 {
                assert!(!indestructible(&session, targets[1]));
                assert!(indestructible(&session, targets[2]));
            }
            assert_eq!(session.state.objects.get(source).zone, Zone::Graveyard);
        }
    }
}

#[test]
fn exact_original_origin_full_ordered_private_binding_and_wide_count_forgeries_are_readonly_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut session, source, targets) = setup(actor, 3);
        cast(&mut session, source, true);
        target(&mut session, targets[0]);
        let e = expected(&session);
        let (_, _, binding) = session
            .divine_current_decision_input_v44(e, actor.into())
            .unwrap();
        for mode in 0..7 {
            let mut forged = binding.clone();
            match mode {
                0 => forged.candidates.reverse(),
                1 => {
                    if let ActionSemanticV1::ChooseDivineTargetV44 { selected_count, .. } =
                        &mut forged.candidates[0].semantic
                    {
                        *selected_count = 256
                    }
                }
                2 => forged
                    .observation
                    .divine
                    .pending
                    .as_mut()
                    .unwrap()
                    .selected
                    .clear(),
                3 => forged.observation.divine.pending.as_mut().unwrap().maximum = Some(2),
                4 => forged.observation.divine.pending.as_mut().unwrap().kicked = Some(false),
                5 => forged.families.reverse(),
                _ => forged.observation.divine.finalized.clear(),
            }
            if mode == 6 {
                forged.expected.environment_revision += 1
            }
            let before = serde_json::to_vec(&session.state).unwrap();
            assert!(
                session
                    .divine_validate_bound_decision_v44(e, actor.into(), &forged)
                    .is_err(),
                "mode {mode}"
            );
            assert_eq!(serde_json::to_vec(&session.state).unwrap(), before);
        }
        let mut forged = session.clone();
        if let PolicyDecisionV5::Surface(SurfaceDecision::Decision(
            Decision::ChooseVariableCastTargets { selected_count, .. },
        )) = &mut forged.current.as_mut().unwrap().origin_decision
        {
            *selected_count += 1
        } else {
            panic!("actual variable target origin")
        }
        let before = serde_json::to_vec(&forged.state).unwrap();
        assert!(forged
            .divine_current_decision_input_v44(e, actor.into())
            .is_err());
        assert!(forged.step(e.episode_id, e.step, 0).is_err());
        assert_eq!(serde_json::to_vec(&forged.state).unwrap(), before);
        let saved = session.snapshot_v1();
        for _ in 0..3 {
            session.divine_restore_v44(&saved).unwrap();
            let now = expected(&session);
            assert!(now.environment_revision > e.environment_revision);
            assert!(session
                .divine_validate_bound_decision_v44(e, actor.into(), &binding)
                .is_err());
            let (_, _, current) = session
                .divine_current_decision_input_v44(now, actor.into())
                .unwrap();
            session
                .divine_validate_bound_decision_v44(now, actor.into(), &current)
                .unwrap();
        }
    }
}
