//! Actual paid native bindings and numeric model source parity. Rust unrun.
use super::*;
use crate::{
    effect::kaito_cunning_v75::tests::owner_v75_tests::owning_ready, ids::PlayerId,
    kaito_public_v75::StageV75, rl::ActionSemanticV1 as A,
};
fn act(own: &mut KaitoSession, native: &mut FastActorSessionV1, choose: impl Fn(&A) -> bool) {
    let FastActorResponseV1::Decision(h) = own.current_response() else {
        panic!("numeric V75 ended")
    };
    let d = own.encode(h).unwrap();
    let i = d
        .backend
        .actions()
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("missing numeric V75 action: {:?}", d.backend.actions()));
    let original = d.backend.fixture_original_index_v75(i as u32).unwrap();
    let scores = NativeModel::deterministic_fixture()
        .unwrap()
        .forward(d.packet())
        .unwrap();
    assert_eq!(scores.len(), d.backend.actions().len());
    assert!(scores.iter().all(|v| v.is_finite()));
    assert_eq!(d.packet().source(), Source::KaitoV75);
    assert_eq!(
        own.apply(&d, i as u32).unwrap(),
        native.step(h.episode_id, h.step, original).unwrap()
    );
    assert_eq!(own.fixture_state(), native.kaito_fixture_state());
    assert_eq!(
        own.fixture_counts(),
        (native.policy_step_count(), native.physical_decision_count())
    );
}
fn complete(own: &mut KaitoSession, native: &mut FastActorSessionV1) {
    for _ in 0..512 {
        let FastActorResponseV1::Decision(h) = own.current_response() else {
            panic!()
        };
        let d = own.encode(h).unwrap();
        if d.backend
            .observation()
            .kaito
            .activations
            .last()
            .is_some_and(|a| a.stage == StageV75::Resolved)
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
                        | A::Discard { .. }
                        | A::ChooseAttackerInclusion { include: false, .. }
                        | A::ChooseBlockerInclusion { include: false, .. }
                )
            }
        });
    }
    panic!("numeric native root incomplete")
}

#[test]
fn paid_numeric_kaito_all_three_roots_actual_discard_ninja_emblem_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1, 2] {
            let (mut s, k, _) = owning_ready(p, false, "Island");
            if index == 2 {
                crate::effect::kaito_cunning_v75::tests::owner_v75_tests::prepare_ultimate(
                    &mut s, k,
                );
            }
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            act(
                &mut own,
                &mut native,
                |a| matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==k.0&&*ability_index==index),
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
            let d = own.encode(h).unwrap();
            let f = &d.backend.observation().kaito;
            assert_eq!(f.activations[0].ability_index, index);
            assert_eq!(
                f.activations[0].loyalty_after,
                match index {
                    0 => 4,
                    1 => 1,
                    2 => 0,
                    _ => unreachable!(),
                }
            );
            if index == 0 {
                assert_eq!(
                    f.activations[0].draw,
                    crate::kaito_public_v75::DrawV75::Drawn
                );
                assert!(f.activations[0].discard.as_ref().unwrap().answered);
            }
            if index == 1 {
                assert_eq!(f.ninja_batches[0].created.len(), 1);
                assert!(printing(495).unwrap().is_some());
            }
            if index == 2 {
                assert_eq!(f.emblems.len(), 1);
                assert_eq!(f.emblems[0].controller, p.into());
            }
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            let before = own.fixture_state().clone();
            assert!(own.apply(&old, 0).is_err());
            assert_eq!(own.fixture_state(), &before);
            complete(&mut own, &mut native);
            assert_eq!(own.fixture_state(), &expected);
        }
    }
}
#[test]
fn paid_numeric_kaito_departed_or_concealed_original_controller_keeps_private_discard_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for concealed in [false, true] {
            let (s, _, _) =
                crate::effect::kaito_cunning_v75::tests::owner_v75_tests::published_source_boundary(
                    p, concealed,
                );
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            complete(&mut own, &mut native);
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            assert_eq!(
                d.backend.observation().kaito.activations[0]
                    .source
                    .captured_controller,
                p.into()
            );
            assert_eq!(
                d.backend.observation().kaito.activations[0].stage,
                StageV75::Resolved
            );
        }
    }
}
fn settle(own: &mut KaitoSession, native: &mut FastActorSessionV1) {
    for _ in 0..512 {
        let s = own.fixture_state();
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_discard.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            return;
        }
        act(own, native, |a| {
            matches!(
                a,
                A::Pass { .. }
                    | A::OrderTriggers { .. }
                    | A::ChooseTriggerOrderNext { .. }
                    | A::ChooseEffectTarget { .. }
                    | A::ChooseTarget { .. }
                    | A::Discard { .. }
                    | A::ChooseAttackerInclusion { include: false, .. }
                    | A::ChooseBlockerInclusion { include: false, .. }
            )
        });
    }
    panic!("Kaito passive work incomplete")
}
#[test]
fn paid_numeric_kaito_original_combat_and_persistent_emblem_cast_roles_match_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for emblem in [false, true] {
            let s = if emblem {
                crate::effect::kaito_cunning_v75::tests::owner_v75_tests::emblem_cast_boundary(p)
            } else {
                crate::effect::kaito_cunning_v75::tests::owner_v75_tests::combat_boundary(p, false)
            };
            let mut native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
            let mut own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            let f = &d.backend.observation().kaito;
            if emblem {
                assert_eq!(f.casts.len(), 2);
                assert_eq!(f.emblems.len(), 1);
            } else {
                assert_eq!(f.combats.len(), 1);
                assert_eq!(f.combats[0].final_damage, 2);
            }
            let saved = own.snapshot();
            let raw = native.snapshot_v1();
            settle(&mut own, &mut native);
            let expected = own.fixture_state().clone();
            own.restore(&saved).unwrap();
            native.kaito_restore_v75(&raw).unwrap();
            settle(&mut own, &mut native);
            assert_eq!(own.fixture_state(), &expected);
        }
    }
}
#[test]
fn paid_numeric_foreign_forged_packet_and_action_refusals_leave_native_unchanged_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _, _) = owning_ready(p, false, "Island");
        let mut a =
            KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s.clone()))
                .unwrap();
        let mut b = KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
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
        let mut forged = d.backend.candidates_v75().clone();
        forged.observation.kaito.activations.clear();
        forged.observation.kaito.authorized_printings.clear();
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
            let (s, _, _) = owning_ready(p, false, hidden);
            let own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                    .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            packets.push(own.encode(h).unwrap().packet().clone());
        }
        assert_eq!(packets[0], packets[1]);
        let (s, id, _) = owning_ready(p, false, "Island");
        let mut wide = vec![];
        for count in [i32::MAX - 1, i32::MAX] {
            let mut world = s.clone();
            let c = &mut world.objects.get_mut(id).counters;
            // Explicit native storage boundary on a genuinely six-mana paid printing.
            c.stash = count;
            c.fellowship = i32::MAX;
            c.charge = i32::MAX;
            let own =
                KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(world))
                    .unwrap();
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!()
            };
            let d = own.encode(h).unwrap();
            let current = d.backend.observation().kaito.battlefield[p.index()]
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
