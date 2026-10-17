//! Genuine paid Call Human actor/opponent routes. All setup is native fixture setup.
use super::*;
use crate::{
    call_observation_v62::tests::{hand, ready},
    ids::PlayerId,
    rl::ActionSemanticV1 as A,
    state::{GameState, Zone},
};
fn expected(s: &HumanCallSessionV62) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(e) = s.current_response() else {
        panic!("Call Human terminal")
    };
    e
}
fn own(s: GameState, p: PlayerId) -> HumanCallSessionV62 {
    HumanCallSessionV62::from_session(FastActorSessionV1::from_call_v62_fixture_state(s), p.into())
        .unwrap()
}
fn act(
    s: &mut HumanCallSessionV62,
    h: &mut HumanCallProjectorV62,
    native: &mut FastActorSessionV1,
    choose: impl Fn(&A) -> bool,
) {
    let d = s.inner.decision_v62().unwrap();
    let e = d.expected();
    let i = d.actions().iter().position(choose).unwrap();
    let raw = d.fixture_original_index_v62(i as u32).unwrap();
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
    assert_eq!(s.inner.fixture_state_v62(), native.call_fixture_state());
    assert_eq!(
        s.inner.fixture_counts_v62(),
        (native.policy_step_count(), native.physical_decision_count())
    );
    assert_eq!(s.current_response(), native.current_response());
}
#[test]
fn genuine_paid_call_human_full_roster_opponent_callback_snapshot_and_foreign_prompt_refusal_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut world = ready(p, "Forest");
        let card = hand(&mut world, p, "Valkyrie's Call");
        hand(&mut world, p.opponent(), "Lightning Bolt");
        let mut native = FastActorSessionV1::from_call_v62_fixture_state(world.clone());
        let mut s = own(world.clone(), p);
        let foreign = own(world, p);
        let mut h = HumanCallProjectorV62::new(p.into());
        act(
            &mut s,
            &mut h,
            &mut native,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0),
        );
        assert!(s.inner.fixture_state_v62().engine.pending_cast.is_none());
        assert_eq!(
            s.inner.fixture_state_v62().objects.get(card).zone,
            Zone::Stack
        );
        assert_eq!(
            s.inner.fixture_state_v62().players[p.index()].mana_pool,
            [0; 6]
        );
        let e = expected(&s);
        let prompt = h.project_current(&s, e).unwrap();
        assert_eq!(prompt.state.call.meaning.card_id, 459);
        assert_eq!(
            prompt.state.call.battlefield[0].len(),
            s.inner.fixture_state_v62().players[0].battlefield.len()
        );
        let snapshot = s.snapshot_v62();
        let ns = native.snapshot_v1();
        let before = s.inner.fixture_state_v62().clone();
        assert!(s.restore_v62(&foreign.snapshot_v62()).is_err());
        assert_eq!(s.inner.fixture_state_v62(), &before);
        act(&mut s, &mut h, &mut native, |a| matches!(a, A::Pass { .. }));
        assert_ne!(expected(&s).acting_player, s.human_seat);
        act(&mut s, &mut h, &mut native, |a| matches!(a, A::Pass { .. }));
        assert_eq!(
            s.inner.fixture_state_v62().objects.get(card).zone,
            Zone::Battlefield
        );
        s.restore_v62(&snapshot).unwrap();
        native.call_restore_v62(&ns).unwrap();
        assert_eq!(s.inner.fixture_state_v62(), native.call_fixture_state());
        assert_eq!(s.inner.fixture_state_v62(), &before);
        assert!(h
            .submit(
                &mut s,
                HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index: 0
                }
            )
            .is_err());
        let current = h.project_current(&s, expected(&s)).unwrap();
        assert!(current.prompt_seq > prompt.prompt_seq);
    }
}
