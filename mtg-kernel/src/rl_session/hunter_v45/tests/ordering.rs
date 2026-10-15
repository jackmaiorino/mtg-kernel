use super::*;
use crate::event::{propose_and_commit, ProposedEvent};

pub(crate) fn registered_group(actor: PlayerId) -> crate::state::GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    for _ in 0..9 {
        put(&mut state, actor, "Homunculus Horde", Zone::Battlefield);
    }
    state.players[actor.index()].draws_this_turn = 0;
    for _ in 0..2 {
        propose_and_commit(&mut state, ProposedEvent::draw(actor));
    }
    let triggers = crate::trigger::collect_and_process(&mut state);
    assert_eq!(triggers.len(), 9);
    state.engine.pending_triggers.extend(triggers);
    state
}
#[test]
fn registered_nine_order_answers_commit_once_and_restore_exact_prefix_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = registered_group(actor);
        let sources: Vec<_> = state
            .engine
            .pending_triggers
            .iter()
            .rev()
            .map(|t| t.source)
            .collect();
        let mut session = FastActorSessionV1::from_hunter_v45_fixture_state(state);
        let original = session.state.clone();
        for (position, index) in (0..9).rev().enumerate() {
            let e = expected(&session);
            assert_eq!((e.substep_index, e.substep_count), (position as u32, 9));
            assert_eq!(e.physical_decision_id, 0);
            let (public, actions, binding) = session
                .hunter_current_decision_input_v45(e, actor.into())
                .unwrap();
            assert_eq!(actions.len(), 9 - position);
            assert_eq!(
                public.trigger_order.as_ref().unwrap().ordered_prefix.len(),
                position
            );
            let snapshot = session.snapshot_v1();
            session.hunter_restore_v45(&snapshot).unwrap();
            assert!(session
                .hunter_validate_bound_decision_v45(e, actor.into(), &binding)
                .is_err());
            assert_eq!(session.state, original);
            choose(
                &mut session,
                |a| matches!(a,ActionSemanticV1::ChooseTriggerOrderNext {trigger_index,..}if *trigger_index==index),
            );
            if position < 8 {
                assert_eq!(session.state, original);
                assert_eq!(session.physical_decision_count, 0);
            }
        }
        assert_eq!(session.physical_decision_count, 1);
        assert_eq!(session.policy_step_count, 9);
        assert_eq!(
            session
                .state
                .stack
                .iter()
                .map(|item| item.source)
                .collect::<Vec<_>>(),
            sources
        );
        assert!(session
            .surface
            .hunter_order_context_v45(&session.state)
            .unwrap()
            .is_none());
    }
}
#[test]
fn registered_staged_roster_header_prefix_and_complete_binding_forgeries_refuse_readonly_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session =
            FastActorSessionV1::from_hunter_v45_fixture_state(registered_group(actor));
        choose(&mut session, |a| {
            matches!(
                a,
                ActionSemanticV1::ChooseTriggerOrderNext {
                    trigger_index: 8,
                    ..
                }
            )
        });
        let e = expected(&session);
        let (_, _, binding) = session
            .hunter_current_decision_input_v45(e, actor.into())
            .unwrap();
        for case in 0..4 {
            let mut bad = session.clone();
            match case {
                0 => bad.state.engine.pending_triggers.swap(0, 1),
                1 => bad.current.as_mut().unwrap().substep_index = 0,
                2 => bad.current.as_mut().unwrap().actor = actor.opponent(),
                _ => bad.current.as_mut().unwrap().candidates.reverse(),
            }
            let before = bad.state.clone();
            assert!(bad
                .hunter_current_decision_input_v45(e, actor.into())
                .is_err());
            assert!(bad.step(e.episode_id, e.step, 0).is_err());
            assert_eq!(bad.state, before);
        }
        let mut forged = binding.clone();
        forged
            .observation
            .trigger_order
            .as_mut()
            .unwrap()
            .ordered_prefix
            .clear();
        assert!(session
            .hunter_validate_bound_decision_v45(e, actor.into(), &forged)
            .is_err());
        let before = session.state.clone();
        let revision = session.environment_revision;
        let mut surface = session.surface.clone();
        assert!(surface
            .apply_owned(
                &mut session.state,
                PolicyActionV5::Surface(crate::surface_v2::SurfaceAction::Action(
                    DecisionAction::OrderTriggers(vec![8, 8])
                )),
                revision,
                revision + 1
            )
            .is_err());
        assert_eq!(session.state, before);
    }
}
use crate::engine::Action as DecisionAction;
#[test]
fn registered_large_order_predecessor_refusal_and_whole_group_budget_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = registered_group(actor);
        let origin =
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers {
                player: actor,
                pending: state.engine.pending_triggers.clone(),
            }));
        assert!(crate::rl::core_policy_action_candidates_v5(&origin, &state).is_err());
        let mut predecessor = PolicySurfaceV5::new_with_engine_priority_v1();
        let mut copy = state.clone();
        assert_eq!(predecessor.next_decision(&mut copy).unwrap(), origin);
        assert_eq!(copy, state);
        assert!(predecessor
            .hunter_order_context_v45(&state)
            .unwrap()
            .is_none());
        let mut session = FastActorSessionV1::from_hunter_v45_fixture_state(state);
        session.current = None;
        session.max_policy_steps = 8;
        let before = session.state.clone();
        session.advance_to_decision_or_terminal();
        assert!(matches!(
            session.current_response(),
            FastActorResponseV1::Terminal(_)
        ));
        assert_eq!(session.state, before);
        assert!(session.state.stack.is_empty());
    }
}
