//! Actual paid native bindings and numeric model source parity. Rust unrun.
use super::*;
use crate::{
    chandra_public_v74::ActivationStageV74,
    effect::chandra_flameshaper_v1::tests::owner_v74_tests::owning_ready, ids::PlayerId,
    rl::ActionSemanticV1 as A,
};
fn act(own: &mut ChandraSession, native: &mut FastActorSessionV1, choose: impl Fn(&A) -> bool) {
    let FastActorResponseV1::Decision(h) = own.current_response() else {
        panic!("numeric V74 ended")
    };
    let d = own.encode(h).unwrap();
    let i = d
        .backend
        .actions()
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("missing numeric V74 action: {:?}", d.backend.actions()));
    let original = d.backend.fixture_original_index_v74(i as u32).unwrap();
    let scores = NativeModel::deterministic_fixture()
        .unwrap()
        .forward(d.packet())
        .unwrap();
    assert_eq!(scores.len(), d.backend.actions().len());
    assert!(scores.iter().all(|v| v.is_finite()));
    assert_eq!(d.packet().source(), Source::ChandraV74);
    assert_eq!(
        own.apply(&d, i as u32).unwrap(),
        native.step(h.episode_id, h.step, original).unwrap()
    );
    assert_eq!(own.fixture_state(), native.chandra_fixture_state());
    assert_eq!(
        own.fixture_counts(),
        (native.policy_step_count(), native.physical_decision_count())
    );
}
fn complete(own: &mut ChandraSession, native: &mut FastActorSessionV1) {
    for _ in 0..512 {
        let FastActorResponseV1::Decision(h) = own.current_response() else {
            panic!()
        };
        let d = own.encode(h).unwrap();
        if d.backend
            .observation()
            .chandra
            .activations
            .last()
            .is_some_and(|a| a.stage == ActivationStageV74::NativeCompleted)
        {
            return;
        }
        let finish = d
            .backend
            .actions()
            .iter()
            .any(|a| matches!(a, A::FinishEffectSelection { .. }));
        act(own, native, |a| {
            if finish {
                matches!(a, A::FinishEffectSelection { .. })
            } else {
                matches!(
                    a,
                    A::Pass { .. }
                        | A::OrderTriggers { .. }
                        | A::ChooseTriggerOrderNext { .. }
                        | A::ChooseEffectTarget { .. }
                        | A::ChooseTarget { .. }
                        | A::ChooseAttackerInclusion { include: false, .. }
                        | A::ChooseBlockerInclusion { include: false, .. }
                )
            }
        });
    }
    panic!("numeric native root incomplete")
}

#[test]
fn paid_numeric_chandra_all_roots_original_allocation_exile_copy_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1, 2] {
            let (s, id, _, _) = owning_ready(p, false, "Island");
            let mut native = FastActorSessionV1::from_chandra_v74_fixture_state(s.clone());
            let mut own =
                ChandraSession::from_session(FastActorSessionV1::from_chandra_v74_fixture_state(s))
                    .unwrap();
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index),
            );
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let old = own.encode(h).unwrap();
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            complete(&mut own, &mut native);
            let expected = own.fixture_state().clone();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let completed = own.encode(h).unwrap();
            let f = &completed.backend.observation().chandra;
            assert_eq!(f.activations[0].ability_index, index);
            assert_eq!(
                f.activations[0].loyalty_after,
                match index {
                    0 => 8,
                    1 => 7,
                    2 => 2,
                    _ => unreachable!(),
                }
            );
            if index == 0 {
                assert_eq!(f.exiles[0].cards.len(), 3);
                assert!(f.exiles[0].permission_present);
            }
            if index == 1 {
                assert_eq!(f.copied_tokens.len(), 1);
                assert_eq!(f.copied_tokens[0].current_haste, Some(true));
            }
            if index == 2 {
                assert!(f.activations[0].targets.is_empty());
            }
            own.restore(&saved).unwrap();
            native.chandra_restore_v74(&raw).unwrap();
            let before = own.fixture_state().clone();
            assert!(own.apply(&old, 0).is_err());
            assert_eq!(own.fixture_state(), &before);
            complete(&mut own, &mut native);
            assert_eq!(own.fixture_state(), &expected);
        }
    }
}
#[test]
fn paid_numeric_chandra_original_sourcegone_and_concealment_bind_exact_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for concealed in [false, true] {
            let (s,_,_)=crate::effect::chandra_flameshaper_v1::tests::owner_v74_tests::published_source_boundary(p,concealed);
            let mut native = FastActorSessionV1::from_chandra_v74_fixture_state(s.clone());
            let mut own =
                ChandraSession::from_session(FastActorSessionV1::from_chandra_v74_fixture_state(s))
                    .unwrap();
            complete(&mut own, &mut native);
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            assert_eq!(
                d.backend.observation().chandra.activations[0].stage,
                ActivationStageV74::NativeCompleted
            );
        }
    }
}
#[test]
fn paid_numeric_foreign_forged_packet_and_action_refusals_leave_native_unchanged_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _, _, _) = owning_ready(p, false, "Island");
        let mut a = ChandraSession::from_session(
            FastActorSessionV1::from_chandra_v74_fixture_state(s.clone()),
        )
        .unwrap();
        let mut b =
            ChandraSession::from_session(FastActorSessionV1::from_chandra_v74_fixture_state(s))
                .unwrap();
        let FastActorResponseV1::Decision(h) = a.current_response() else {
            panic!()
        };
        let d = a.encode(h).unwrap();
        let before = a.fixture_state().clone();
        let counts = a.fixture_counts();
        assert!(b.apply(&d, 0).is_err());
        assert!(a.restore(&b.snapshot()).is_err());
        let mut erased = d.clone();
        erased.packet.actions.clear();
        assert!(a.apply(&erased, 0).is_err());
        let mut changed = d.clone();
        changed.packet.state = crate::current_mixed_policy_v1::NumericTree::encode(
            &serde_json::json!({"replacement":"forged"}),
        )
        .unwrap();
        assert!(a.apply(&changed, 0).is_err());
        let mut forged = d.backend.candidates_v74().clone();
        forged.observation.chandra.activations.clear();
        forged.observation.chandra.authorized_printings.clear();
        assert!(packet_from_current(&forged, d.backend.actions()).is_err());
        assert!(a.apply(&d, u32::MAX).is_err());
        assert_eq!(a.fixture_state(), &before);
        assert_eq!(a.fixture_counts(), counts);
    }
}
#[test]
fn independently_paid_numeric_hidden_world_equality_and_full_native_integer_distinction_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut packets = vec![];
        for hidden in ["Island", "Imprisoned in the Moon"] {
            let (s, _, _, _) = owning_ready(p, false, hidden);
            let own =
                ChandraSession::from_session(FastActorSessionV1::from_chandra_v74_fixture_state(s))
                    .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            packets.push(own.encode(h).unwrap().packet().clone());
        }
        assert_eq!(packets[0], packets[1]);
        let (s, id, _, _) = owning_ready(p, false, "Island");
        let mut wide = vec![];
        for count in [i32::MAX - 1, i32::MAX] {
            let mut world = s.clone();
            let c = &mut world.objects.get_mut(id).counters;
            // Explicit native storage boundary on a genuinely six-mana paid printing.
            c.stash = count;
            c.fellowship = i32::MAX;
            c.charge = i32::MAX;
            let own = ChandraSession::from_session(
                FastActorSessionV1::from_chandra_v74_fixture_state(world),
            )
            .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            let current = d.backend.observation().chandra.battlefield[p.index()]
                .iter()
                .find(|c| c.card.object == id.0)
                .unwrap();
            assert_eq!(current.counters.stash, count);
            assert!(NativeModel::deterministic_fixture()
                .unwrap()
                .forward(d.packet())
                .unwrap()
                .iter()
                .all(|v| v.is_finite()));
            wide.push(d.packet().clone());
        }
        assert_ne!(wide[0], wide[1]);
    }
}
