use super::*;
use crate::{ids::PlayerId, rl::ActionSemanticV1 as A, rl_session::genesis_wave_v69::tests::ready};
fn act(
    s: &mut HumanGenesisWaveSessionV69,
    h: &mut HumanGenesisWaveProjectorV69,
    native: &mut FastActorSessionV1,
    choose: impl Fn(&A) -> bool,
) {
    let d = s.inner.decision_v69().unwrap();
    let e = d.expected();
    let i = d.actions().iter().position(choose).unwrap();
    let raw = d.fixture_original_index_v69(i as u32).unwrap();
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
    assert_eq!(
        s.inner.fixture_state_v69(),
        native.genesis_wave_fixture_state()
    );
    assert_eq!(
        s.inner.fixture_counts_v69(),
        (native.policy_step_count(), native.physical_decision_count())
    );
    assert_eq!(s.current_response(), native.current_response());
}
#[test]
fn genuine_paid_wave_human_actor_opponent_full_reveal_restore_and_stale_prompt_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, wave, _) = ready(p, "Island");
        let mut native = FastActorSessionV1::from_genesis_wave_v69_fixture_state(world.clone());
        let mut s = HumanGenesisWaveSessionV69::from_session(
            FastActorSessionV1::from_genesis_wave_v69_fixture_state(world.clone()),
            p.into(),
        )
        .unwrap();
        let foreign = HumanGenesisWaveSessionV69::from_session(
            FastActorSessionV1::from_genesis_wave_v69_fixture_state(world),
            p.into(),
        )
        .unwrap();
        let mut h = HumanGenesisWaveProjectorV69::new(p.into());
        act(
            &mut s,
            &mut h,
            &mut native,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==wave.0),
        );
        act(&mut s, &mut h, &mut native, |a| {
            matches!(
                a,
                A::ChooseEffectOption {
                    option_index: 4,
                    ..
                }
            )
        });
        assert_eq!(
            crate::effect::genesis_wave_v1::tests::tapped(s.inner.fixture_state_v69(), p),
            7
        );
        for _ in 0..20 {
            if s.inner
                .decision_v69()
                .unwrap()
                .observation()
                .genesis_wave
                .waves[0]
                .stage
                == crate::genesis_wave_public_v69::StageV69::Subset
            {
                break;
            }
            act(&mut s, &mut h, &mut native, |a| matches!(a, A::Pass { .. }));
        }
        let e = s.inner.decision_v69().unwrap().expected();
        let prompt = h.project_current(&s, e).unwrap();
        assert_eq!(prompt.state.genesis_wave.waves[0].revealed.len(), 4);
        assert_eq!(prompt.state.genesis_wave.waves[0].x_value, 4);
        assert_eq!(prompt.actions.len(), prompt.action_meanings.len());
        let text = serde_json::to_string(&prompt).unwrap();
        for forbidden in [
            "original_library",
            "remaining_library",
            "arena_id",
            "zone_change_count",
            "history_index",
        ] {
            assert!(!text.contains(forbidden));
        }
        let saved = s.snapshot_v69();
        let raw = native.snapshot_v1();
        let before = s.inner.fixture_state_v69().clone();
        assert!(s.restore_v69(&foreign.snapshot_v69()).is_err());
        act(&mut s, &mut h, &mut native, |a| {
            matches!(a, A::FinishEffectSelection { .. })
        });
        s.restore_v69(&saved).unwrap();
        native.genesis_wave_restore_v69(&raw).unwrap();
        assert_eq!(s.inner.fixture_state_v69(), &before);
        assert_eq!(
            s.inner.fixture_state_v69(),
            native.genesis_wave_fixture_state()
        );
        assert!(h
            .submit(
                &mut s,
                HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index: 0
                }
            )
            .is_err());
        assert_eq!(s.inner.fixture_state_v69(), &before);
    }
}
#[test]
fn independent_prepaid_wave_human_hidden_worlds_have_same_prompt_and_facts_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let (world, wave, _) = ready(p, hidden);
            let mut pair = crate::rl_session::genesis_wave_v69::tests::Pair::new(world);
            pair.cast(wave, p, 4);
            pair.to_subset();
            let s = HumanGenesisWaveSessionV69 {
                inner: pair.own,
                human_seat: p.into(),
                authority: AuthorityV69 {
                    owner: Arc::new(()),
                    generation: 0,
                },
            };
            let mut h = HumanGenesisWaveProjectorV69::new(p.into());
            h.project_current(&s, s.inner.decision_v69().unwrap().expected())
                .unwrap()
        };
        let a = make("Island");
        let b = make("Mountain");
        assert_eq!(a, b);
        assert_eq!(a.state.genesis_wave.meaning.card_id, 487);
        assert_eq!(a.state.end_turn.meaning.card_id, 486);
    }
}
