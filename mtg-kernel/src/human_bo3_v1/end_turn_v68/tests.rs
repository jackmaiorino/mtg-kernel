//! Genuine paid Time Stop Human actor/opponent routes. All setup is native fixture setup.
use super::*;
use crate::{
    call_observation_v62::tests::{hand, ready},
    ids::PlayerId,
    rl::ActionSemanticV1 as A,
    state::GameState,
};
fn expected(s: &HumanEndTurnSessionV68) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(e) = s.current_response() else {
        panic!("Call Human terminal")
    };
    e
}
fn own(s: GameState, p: PlayerId) -> HumanEndTurnSessionV68 {
    HumanEndTurnSessionV68::from_session(
        FastActorSessionV1::from_end_turn_v68_fixture_state(s),
        p.into(),
    )
    .unwrap()
}
fn act(
    s: &mut HumanEndTurnSessionV68,
    h: &mut HumanEndTurnProjectorV68,
    native: &mut FastActorSessionV1,
    choose: impl Fn(&A) -> bool,
) {
    let d = s.inner.decision_v68().unwrap();
    let e = d.expected();
    let i = d.actions().iter().position(choose).unwrap();
    let raw = d.fixture_original_index_v68(i as u32).unwrap();
    if e.acting_player == s.human_seat {
        let visible = h.project_current(s, e).unwrap();
        assert_eq!(visible.actions.len(), visible.action_meanings.len());
        let index = h
            .pending
            .as_ref()
            .unwrap()
            .indexes
            .iter()
            .position(|&v| v as usize == i)
            .unwrap();
        h.submit(
            s,
            HumanActionRequestV1 {
                prompt_seq: visible.prompt_seq,
                action_index: index as u32,
            },
        )
        .unwrap();
    } else {
        s.step_opponent_with(e, |_, _| Ok(i as u32)).unwrap();
    }
    native.step(e.episode_id, e.step, raw).unwrap();
    assert_eq!(s.inner.fixture_state_v68(), native.end_turn_fixture_state());
    assert_eq!(
        s.inner.fixture_counts_v68(),
        (native.policy_step_count(), native.physical_decision_count())
    );
    assert_eq!(s.current_response(), native.current_response());
}

#[test]
fn genuine_paid_time_stop_human_disposed_rows_opponent_restore_and_foreign_prompt_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut world = ready(p, "Forest");
        let card = hand(&mut world, p, "Time Stop");
        let mut native = FastActorSessionV1::from_end_turn_v68_fixture_state(world.clone());
        let mut s = own(world.clone(), p);
        let foreign = own(world, p);
        let mut h = HumanEndTurnProjectorV68::new(p.into());
        let before_taps =
            crate::engine::time_stop_v1::tests::tapped(s.inner.fixture_state_v68(), p);
        act(
            &mut s,
            &mut h,
            &mut native,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0),
        );
        assert_eq!(
            crate::engine::time_stop_v1::tests::tapped(s.inner.fixture_state_v68(), p)
                - before_taps,
            6
        );
        let e = expected(&s);
        let prompt = h.project_current(&s, e).unwrap();
        assert_eq!(prompt.state.end_turn.meaning.card_id, 486);
        assert_eq!(
            prompt.state.call.battlefield[0].len(),
            s.inner.fixture_state_v68().players[0].battlefield.len()
        );
        let snapshot = s.snapshot_v68();
        let ns = native.snapshot_v1();
        let before = s.inner.fixture_state_v68().clone();
        assert!(s.restore_v68(&foreign.snapshot_v68()).is_err());
        assert_eq!(s.inner.fixture_state_v68(), &before);
        for _ in 0..80 {
            if !s
                .inner
                .decision_v68()
                .unwrap()
                .observation()
                .end_turn
                .disposals
                .is_empty()
            {
                break;
            }
            act(&mut s, &mut h, &mut native, |a| matches!(a, A::Pass { .. }));
        }
        assert_eq!(
            s.inner
                .decision_v68()
                .unwrap()
                .observation()
                .end_turn
                .disposals
                .len(),
            1
        );
        s.restore_v68(&snapshot).unwrap();
        native.end_turn_restore_v68(&ns).unwrap();
        assert_eq!(s.inner.fixture_state_v68(), native.end_turn_fixture_state());
        assert!(h
            .submit(
                &mut s,
                HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index: 0
                }
            )
            .is_err());
        let fresh = h.project_current(&s, expected(&s)).unwrap();
        assert!(fresh.prompt_seq > prompt.prompt_seq);
    }
}
