use super::*;
use crate::card_def::Keywords;
use crate::ids::PlayerId;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::put;
use crate::state::{GameState, Zone};

pub(crate) fn expected(session: &FastActorSessionV1) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(e) = response else {
        panic!("{response:?}")
    };
    e
}
pub(crate) fn choose(session: &mut FastActorSessionV1, want: impl Fn(&ActionSemanticV1) -> bool) {
    let e = expected(session);
    let (_, actions, binding) = session
        .panda_current_decision_input_v47(e, e.acting_player)
        .unwrap();
    let index = actions
        .iter()
        .position(want)
        .unwrap_or_else(|| panic!("{actions:?}"));
    session
        .panda_validate_bound_decision_v47(e, e.acting_player, &binding)
        .unwrap();
    session.step(e.episode_id, e.step, index as u32).unwrap();
}
pub(crate) fn drain(session: &mut FastActorSessionV1) {
    for _ in 0..80 {
        if session.state.stack.is_empty()
            && session.state.engine.pending_cast.is_none()
            && session.state.engine.pending_triggers.is_empty()
            && session.state.engine.pending_effect.is_none()
        {
            return;
        }
        choose(session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
    }
    panic!("genuine V47 resolution did not finish");
}
pub(crate) fn ordinary(actor: PlayerId) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
    state
}
#[test]
fn genuine_registered_ward_payment_decline_and_restore_preserve_captured_payer_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut state = ordinary(actor);
            let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            let target = put(
                &mut state,
                actor.opponent(),
                "Tolarian Terror",
                Zone::Battlefield,
            );
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
            let mut session = FastActorSessionV1::from_panda_v47_fixture_state(state);
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::CastSpell{source:s,..}if s.arena_id==source.0),
            );
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==target.0),
            );
            for _ in 0..20 {
                let e = expected(&session);
                let (_, actions, _) = session
                    .panda_current_decision_input_v47(e, e.acting_player)
                    .unwrap();
                if actions
                    .iter()
                    .any(|a| matches!(a, ActionSemanticV1::ChooseEffectBoolean { .. }))
                {
                    break;
                }
                choose(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
            }
            let saved = session.snapshot_v1();
            let mut restored = session.clone();
            restored.panda_restore_v47(&saved).unwrap();
            for branch in [&mut session, &mut restored] {
                choose(
                    branch,
                    |a| matches!(a,ActionSemanticV1::ChooseEffectBoolean{value,..}if *value==pay),
                );
                drain(branch);
                assert_eq!(
                    branch.state.objects.get(target).damage,
                    if pay { 3 } else { 0 }
                );
                assert_eq!(branch.state.objects.get(source).zone, Zone::Graveyard);
                assert_eq!(
                    branch.state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                    if pay { 0 } else { 2 }
                );
                assert!(branch.state.engine.event_history.iter().any(|event|matches!(event,crate::event::CommittedEvent::Targeted{target:t,targeting_controller,..}if *t==target && *targeting_controller==actor)));
            }
            assert_eq!(session.state, restored.state);
        }
    }
}
#[test]
fn actual_predecessor_fields_ordered_binding_and_owning_restore_refuse_forgeries_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = ordinary(actor);
        let mut session = FastActorSessionV1::from_panda_v47_fixture_state(state.clone());
        let e = expected(&session);
        let (public, actions, binding) = session
            .panda_current_decision_input_v47(e, actor.into())
            .unwrap();
        assert!(public.panda.occurrences.is_empty());
        let mut old = FastActorSessionV1::from_eaten_v46_fixture_state(state);
        let predecessor = expected(&old);
        let (prior, prior_actions, _) = old
            .eaten_current_decision_input_v46(predecessor, actor.into())
            .unwrap();
        assert_eq!(actions, prior_actions);
        assert_eq!(public.projection, prior.projection);
        assert_eq!(public.extensions, prior.extensions);
        assert_eq!(public.own_hand, prior.own_hand);
        assert_eq!(public.known_hand_cards, prior.known_hand_cards);
        assert_eq!(public.known_library_cards, prior.known_library_cards);
        assert_eq!(public.known_face_down_cards, prior.known_face_down_cards);
        assert_eq!(public.families, prior.families);
        assert_eq!(public.eaten, prior.eaten);
        assert_eq!(public.hunter, prior.hunter);
        for case in 0..4 {
            let mut bad = binding.clone();
            match case {
                0 => bad.candidates.reverse(),
                1 => bad.expected.acting_player = actor.opponent().into(),
                2 => bad.observation.panda.card_db_hash ^= 1,
                _ => bad.observation.panda.producer_sha256.push('0'),
            }
            bad.observation.projection_sha256 =
                crate::panda_observation_v47::commitment(&bad.observation).unwrap();
            let before = session.state.clone();
            assert!(session
                .panda_validate_bound_decision_v47(e, actor.into(), &bad)
                .is_err());
            assert_eq!(session.state, before);
        }
        let saved = session.snapshot_v1();
        for _ in 0..3 {
            session.panda_restore_v47(&saved).unwrap();
            assert!(session
                .panda_validate_bound_decision_v47(e, actor.into(), &binding)
                .is_err());
            let now = expected(&session);
            session
                .panda_current_decision_input_v47(now, actor.into())
                .unwrap();
        }
        let mut bad = saved.clone();
        bad.0.current.as_mut().unwrap().environment_revision += 1;
        let before = session.state.clone();
        assert!(session.panda_restore_v47(&bad).is_err());
        assert_eq!(session.state, before);
        let hand = session.state.players[actor.index()].hand[0];
        session
            .panda_fixture_state_mut()
            .objects
            .get_mut(hand)
            .owner = actor.opponent();
        let before = session.state.clone();
        assert!(session
            .panda_current_decision_input_v47(expected(&session), actor.into())
            .is_err());
        assert_eq!(session.state, before);
        // Predecessor owning restore still exercises its separate mode.
        let old_saved = old.snapshot_v1();
        old.eaten_restore_v46(&old_saved).unwrap();
    }
}
#[test]
fn real_divine_priority_kicker_finite_targets_and_exact_probe_remain_usable_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (kicked, count) in [(false, 1usize), (true, 0), (true, 3), (true, 260)] {
            let mut state = ordinary(actor);
            let source = put(&mut state, actor, "Divine Resilience", Zone::Hand);
            let targets = (0..count)
                .map(|_| put(&mut state, actor, "Llanowar Elves", Zone::Battlefield))
                .collect::<Vec<_>>();
            state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 2;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
            let mut session = FastActorSessionV1::from_panda_v47_fixture_state(state);
            let mut probe = session.state.clone();
            let mut surface = session.surface.clone();
            assert_eq!(
                surface.next_decision(&mut probe).unwrap(),
                session.current.as_ref().unwrap().origin_decision
            );
            assert_eq!(probe, session.state);
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::CastSpell{source:s,..}if s.arena_id==source.0),
            );
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::ChooseDivineKickerV44{pay,..}if *pay==kicked),
            );
            for (index, target) in targets.iter().enumerate() {
                let e = expected(&session);
                let (public, actions, _) = session
                    .panda_current_decision_input_v47(e, actor.into())
                    .unwrap();
                assert_eq!(
                    public.divine.pending.as_ref().unwrap().selected.len(),
                    index
                );
                assert_eq!(
                    actions
                        .iter()
                        .filter(|a| matches!(a, ActionSemanticV1::ChooseDivineTargetV44 { .. }))
                        .count(),
                    count - index
                );
                if index == 0 || index == 255 {
                    let saved = session.snapshot_v1();
                    session.panda_restore_v47(&saved).unwrap();
                }
                choose(
                    &mut session,
                    |a| matches!(a,ActionSemanticV1::ChooseDivineTargetV44{target:t,..}if t.arena_id==target.0),
                );
            }
            if kicked {
                choose(&mut session, |a| {
                    matches!(a, ActionSemanticV1::FinishDivineTargetsV44 { .. })
                });
            }
            drain(&mut session);
            assert_eq!(session.state.objects.get(source).zone, Zone::Graveyard);
            for target in targets {
                assert!(crate::engine::has_effective_keyword(
                    &session.state,
                    target,
                    Keywords::INDESTRUCTIBLE
                ));
            }
        }
    }
}
pub(crate) fn nine_group(actor: PlayerId) -> GameState {
    let mut state = ordinary(actor);
    for _ in 0..9 {
        put(&mut state, actor, "Homunculus Horde", Zone::Battlefield);
    }
    state.players[actor.index()].draws_this_turn = 0;
    for _ in 0..2 {
        crate::event::propose_and_commit(&mut state, crate::event::ProposedEvent::draw(actor));
    }
    let waiting = crate::trigger::collect_and_process(&mut state);
    assert_eq!(waiting.len(), 9);
    state.engine.pending_triggers.extend(waiting);
    state
}
#[test]
fn actual_large_order_still_refuses_exact_probe_and_commits_one_full_permutation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = FastActorSessionV1::from_panda_v47_fixture_state(nine_group(actor));
        let original = session.state.clone();
        let original_sources = original
            .engine
            .pending_triggers
            .iter()
            .rev()
            .map(|row| row.source)
            .collect::<Vec<_>>();
        let mut exact_surface = session.surface.clone();
        let mut probe = original.clone();
        assert!(exact_surface.next_decision(&mut probe).is_err());
        assert_eq!(probe, original);
        for (position, index) in (0..9).rev().enumerate() {
            let e = expected(&session);
            assert_eq!((e.substep_index, e.substep_count), (position as u32, 9));
            let saved = session.snapshot_v1();
            session.panda_restore_v47(&saved).unwrap();
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::ChooseTriggerOrderNext{trigger_index,..}if *trigger_index==index),
            );
            if position < 8 {
                assert_eq!(session.state, original);
            }
        }
        assert_eq!(session.physical_decision_count, 1);
        assert_eq!(
            session
                .state
                .stack
                .iter()
                .map(|row| row.source)
                .collect::<Vec<_>>(),
            original_sources
        );
    }
}

#[test]
#[ignore = "genuine Panda printing required; parent owns registration/native"]
fn real_paid_angel_then_panda_life_counter_publication_restore_and_erased_owner_refusal_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ordinary(actor);
        let angel = put(&mut state, actor, "Dazzling Angel", Zone::Hand);
        let source = put(&mut state, actor, "Fiendish Panda", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
        let mut session = FastActorSessionV1::from_panda_v47_fixture_state(state);
        choose(
            &mut session,
            |a| matches!(a,ActionSemanticV1::CastSpell{source:s,..}if s.arena_id==angel.0),
        );
        drain(&mut session);
        choose(
            &mut session,
            |a| matches!(a,ActionSemanticV1::CastSpell{source:s,..}if s.arena_id==source.0),
        );
        for _ in 0..40 {
            if session
                .state
                .stack
                .iter()
                .any(crate::panda_public_v47::selected)
            {
                break;
            }
            choose(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
        }
        assert!(session
            .state
            .stack
            .iter()
            .any(crate::panda_public_v47::selected));
        let e = expected(&session);
        let (public, _, binding) = session
            .panda_current_decision_input_v47(e, actor.into())
            .unwrap();
        assert_eq!(public.panda.occurrences[0].life_gained, Some(1));
        assert_eq!(
            public.panda.occurrences[0].phase,
            crate::panda_public_v47::PhaseV47::Published
        );
        assert_eq!(public.panda.occurrences[0].source.arena_id, source.0);
        let saved = session.snapshot_v1();
        let mut bad = session.clone();
        bad.state
            .stack
            .retain(|item| !crate::panda_public_v47::selected(item));
        let before = bad.state.clone();
        assert!(bad
            .panda_validate_bound_decision_v47(e, actor.into(), &binding)
            .is_err());
        assert!(bad.step(e.episode_id, e.step, 0).is_err());
        assert_eq!(bad.state, before);
        session.panda_restore_v47(&saved).unwrap();
        drain(&mut session);
        assert_eq!(session.state.objects.get(source).counters.plus1_plus1, 1);
        let facts = crate::panda_public_v47::capture(&session.state).unwrap();
        assert!(facts.occurrences[0].operation_attempted);
        assert_eq!(facts.occurrences[0].actual_counters_added, 1);
    }
}
#[test]
#[ignore = "genuine Panda printing required; parent owns registration/native"]
fn genuine_published_death_target_retains_original_public_identity_after_hidden_departure_and_private_forgery_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ordinary(actor);
        let source = put(&mut state, actor, "Fiendish Panda", Zone::Hand);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(target, Zone::Graveyard),
        );
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        let mut session = FastActorSessionV1::from_panda_v47_fixture_state(state);
        choose(
            &mut session,
            |a| matches!(a,ActionSemanticV1::CastSpell{source:s,..}if s.arena_id==source.0),
        );
        drain(&mut session);
        let mut state = session.state.clone();
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        session = FastActorSessionV1::from_panda_v47_fixture_state(state);
        choose(
            &mut session,
            |a| matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==target.0),
        );
        let original = crate::panda_public_v47::capture(&session.state)
            .unwrap()
            .occurrences[0]
            .original_target
            .clone()
            .unwrap();
        let mut departed = session.state.clone();
        crate::event::propose_and_commit(
            &mut departed,
            crate::event::ProposedEvent::zone_change(target, Zone::Hand),
        );
        for name in ["Forest", "Island"] {
            // Labeled information-set operand probe, not a physical card-changing game action.
            let mut hidden = departed.clone();
            hidden.objects.get_mut(target).card_def =
                crate::card_def::card_id_by_name(name).unwrap();
            hidden.objects.get_mut(target).name = name.into();
            let mut probe = FastActorSessionV1::from_panda_v47_fixture_state(hidden);
            let e = expected(&probe);
            let (public, _, binding) = probe
                .panda_current_decision_input_v47(e, e.acting_player)
                .unwrap();
            assert_eq!(
                public.panda.occurrences[0].original_target,
                Some(original.clone())
            );
            assert!(public
                .projection
                .surface
                .stack
                .iter()
                .any(|row| row.targets.contains(&crate::rl::TargetRefV1::Object {
                    object: original.clone()
                })));
            let mut forged = binding.clone();
            forged.observation.panda.occurrences[0].captured_power = Some(4);
            forged.observation.projection_sha256 =
                crate::panda_observation_v47::commitment(&forged.observation).unwrap();
            let before = probe.state.clone();
            assert!(probe
                .panda_validate_bound_decision_v47(e, e.acting_player, &forged)
                .is_err());
            assert_eq!(probe.state, before);
            let saved = probe.snapshot_v1();
            probe.panda_restore_v47(&saved).unwrap();
        }
        let mut real = FastActorSessionV1::from_panda_v47_fixture_state(departed);
        drain(&mut real);
        let facts = crate::panda_public_v47::capture(&real.state).unwrap();
        assert_eq!(
            facts.occurrences[0].phase,
            crate::panda_public_v47::PhaseV47::Fizzled
        );
        assert_eq!(facts.occurrences[0].original_target, Some(original));
        assert_eq!(real.state.objects.get(target).zone, Zone::Hand);
    }
}
#[test]
#[ignore = "genuine Panda and Eaten printing required; parent owns registration/native"]
fn real_eaten_cost_sacrifices_paid_panda_then_mandatory_return_and_exile_coexist_after_restore_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ordinary(actor);
        let panda = put(&mut state, actor, "Fiendish Panda", Zone::Hand);
        let eaten = put(&mut state, actor, "Eaten Alive", Zone::Hand);
        let returned = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        let exiled = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(returned, Zone::Graveyard),
        );
        state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 6;
        let mut session = FastActorSessionV1::from_panda_v47_fixture_state(state);
        choose(
            &mut session,
            |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==panda.0),
        );
        drain(&mut session);
        choose(
            &mut session,
            |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==eaten.0),
        );
        choose(
            &mut session,
            |a| matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==exiled.0),
        );
        let e = expected(&session);
        let (public, _, _) = session
            .panda_current_decision_input_v47(e, actor.into())
            .unwrap();
        let branch = public
            .eaten
            .pending
            .as_ref()
            .unwrap()
            .payable_branches
            .iter()
            .position(|branch| *branch == crate::eaten_public_v46::BranchV46::SacrificeCreature)
            .unwrap();
        choose(
            &mut session,
            |a| matches!(a,ActionSemanticV1::ChooseEffectOption{option_index,..}if usize::from(*option_index)==branch),
        );
        let saved = session.snapshot_v1();
        let mut replay = session.clone();
        replay.panda_restore_v47(&saved).unwrap();
        for current in [&mut session, &mut replay] {
            choose(
                current,
                |a| matches!(a,ActionSemanticV1::ChooseCostTarget{candidate,..}if candidate.arena_id==panda.0),
            );
            assert_eq!(current.state.objects.get(panda).zone, Zone::Graveyard);
            let e = expected(current);
            let (public, _, _) = current
                .panda_current_decision_input_v47(e, actor.into())
                .unwrap();
            assert_eq!(public.panda.occurrences[0].captured_power, Some(3));
            assert_eq!(public.eaten.payments.len(), 1);
            choose(
                current,
                |a| matches!(a,ActionSemanticV1::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==returned.0),
            );
            drain(current);
            assert_eq!(current.state.objects.get(returned).zone, Zone::Battlefield);
            assert_eq!(current.state.objects.get(exiled).zone, Zone::Exile);
        }
        assert_eq!(session.state, replay.state);
    }
}
