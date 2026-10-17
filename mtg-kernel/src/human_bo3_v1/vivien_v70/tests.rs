use super::*;
use crate::{ids::PlayerId, rl::ActionSemanticV1 as A, rl_session::vivien_v70::tests::ready};
fn act(
    s: &mut HumanVivienSessionV70,
    h: &mut HumanVivienProjectorV70,
    native: &mut FastActorSessionV1,
    choose: impl Fn(&A) -> bool,
) {
    let d = s.inner.decision_v70().unwrap();
    let e = d.expected();
    let i = d.actions().iter().position(choose).unwrap();
    let raw = d.fixture_original_index_v70(i as u32).unwrap();
    if e.acting_player == s.human_seat {
        let v = h.project_current(s, e).unwrap();
        let index = h
            .pending
            .as_ref()
            .unwrap()
            .indexes
            .iter()
            .position(|r| *r == i as u32)
            .unwrap();
        h.submit(
            s,
            HumanActionRequestV1 {
                prompt_seq: v.prompt_seq,
                action_index: index as u32,
            },
        )
        .unwrap();
    } else {
        s.step_opponent_with(e, |_, _| Ok(i as u32)).unwrap();
    }
    native.step(e.episode_id, e.step, raw).unwrap();
    assert_eq!(s.inner.fixture_state_v70(), native.vivien_fixture_state());
    assert_eq!(
        s.inner.fixture_counts_v70(),
        (native.policy_step_count(), native.physical_decision_count())
    );
    assert_eq!(s.current_response(), native.current_response());
}
#[test]
fn genuine_paid_vivien_human_loyalty_private_look_prompt_restore_and_opponent_native_parity_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s0, v, _) = ready(p, "Island");
        let mut pair = crate::rl_session::vivien_v70::tests::Pair::new(s0);
        pair.cast(v, p);
        pair.act(
            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==v.0),
        );
        pair.advance_to_look();
        let world = pair.own.fixture_state_v70().clone();
        let mut native = FastActorSessionV1::from_vivien_v70_fixture_state(world.clone());
        let mut s = HumanVivienSessionV70::from_session(
            FastActorSessionV1::from_vivien_v70_fixture_state(world),
            p.into(),
        )
        .unwrap();
        let mut h = HumanVivienProjectorV70::new(p.into());
        let e = s.inner.decision_v70().unwrap().expected();
        let prompt = h.project_current(&s, e).unwrap();
        assert_eq!(prompt.state.vivien.activations[0].loyalty_after, 6);
        assert_eq!(prompt.actions.len(), prompt.action_meanings.len());
        let text = serde_json::to_string(&prompt).unwrap();
        for private in [
            "arena_id",
            "history_index",
            "remaining_library",
            "private_world",
        ] {
            assert!(!text.contains(private));
        }
        let save = s.snapshot_v70();
        let raw = native.snapshot_v1();
        act(&mut s, &mut h, &mut native, |a| {
            matches!(a, A::FinishEffectSelection { .. })
        });
        s.restore_v70(&save).unwrap();
        native.vivien_restore_v70(&raw).unwrap();
        let before = s.inner.fixture_state_v70().clone();
        assert!(h
            .submit(
                &mut s,
                HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index: 0
                }
            )
            .is_err());
        assert_eq!(s.inner.fixture_state_v70(), &before);
        act(&mut s, &mut h, &mut native, |a| {
            matches!(a, A::FinishEffectSelection { .. })
        });
        for _ in 0..4 {
            act(&mut s, &mut h, &mut native, |a| matches!(a, A::Pass { .. }));
        }
    }
}
