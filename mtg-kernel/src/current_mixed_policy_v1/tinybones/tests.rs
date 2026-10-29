//! Genuine paid native inputs and independent numeric/native continuations.
use super::*;
use crate::{
    engine::tinybones_v72::tests::complete_observation::owning_ready, ids::PlayerId,
    rl::ActionSemanticV1 as A, state::Zone,
};

fn act(own: &mut TinybonesSession, native: &mut FastActorSessionV1, choose: impl Fn(&A) -> bool) {
    let FastActorResponseV1::Decision(expected) = own.current_response() else {
        panic!("V72 numeric stream ended")
    };
    let d = own.encode(expected).unwrap();
    let index = d
        .backend
        .actions()
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("missing genuine numeric action: {:?}", d.backend.actions()));
    let original = d.backend.fixture_original_index_v72(index as u32).unwrap();
    let scores = NativeModel::deterministic_fixture()
        .unwrap()
        .forward(d.packet())
        .unwrap();
    assert_eq!(scores.len(), d.backend.actions().len());
    assert!(scores.iter().all(|v| v.is_finite()));
    assert_eq!(d.packet().source(), Source::TinybonesV72);
    assert_eq!(
        own.apply(&d, index as u32).unwrap(),
        native
            .step(expected.episode_id, expected.step, original)
            .unwrap()
    );
    assert_eq!(own.fixture_state(), native.tinybones_fixture_state());
    assert_eq!(
        own.fixture_counts(),
        (native.policy_step_count(), native.physical_decision_count())
    );
}
fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. }
            | A::OrderTriggers { .. }
            | A::ChooseTriggerOrderNext { .. }
            | A::ChooseAttackerInclusion { include: false, .. }
            | A::ChooseBlockerInclusion { include: false, .. }
    )
}

#[test]
fn genuine_paid_numeric_activation_discard_stash_payment_snapshot_and_native_parity_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (state, tiny, card) = owning_ready(p, "Forest");
        let mut native = FastActorSessionV1::from_tinybones_v72_fixture_state(state.clone());
        let mut own = TinybonesSession::from_session(
            FastActorSessionV1::from_tinybones_v72_fixture_state(state),
        )
        .unwrap();
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==tiny.0),
        );
        for _ in 0..32 {
            let FastActorResponseV1::Decision(e) = own.current_response() else {
                panic!()
            };
            if own
                .encode(e)
                .unwrap()
                .backend
                .actions()
                .iter()
                .any(|a| matches!(a, A::Discard { .. }))
            {
                break;
            }
            act(&mut own, &mut native, quiet);
        }
        let FastActorResponseV1::Decision(expected) = own.current_response() else {
            panic!()
        };
        let old = own.encode(expected).unwrap();
        assert_eq!(
            old.backend.observation().tinybones.viewer,
            p.opponent().into()
        );
        let saved = own.snapshot();
        let raw = native.snapshot_v1();
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::Discard{cards,..}if cards.iter().any(|r|r.arena_id==card.0)),
        );
        own.restore(&saved).unwrap();
        native.tinybones_restore_v72(&raw).unwrap();
        let before = own.fixture_state().clone();
        assert!(own.apply(&old, 0).is_err());
        assert_eq!(own.fixture_state(), &before);
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::Discard{cards,..}if cards.iter().any(|r|r.arena_id==card.0)),
        );
        for _ in 0..48 {
            if own.fixture_state().objects.get(card).zone == Zone::Exile {
                break;
            }
            act(&mut own, &mut native, quiet);
        }
        assert_eq!(own.fixture_state().objects.get(card).zone, Zone::Exile);
        assert_eq!(own.fixture_state().objects.get(card).owner, p.opponent());
        assert_eq!(own.fixture_state().objects.get(card).counters.stash, 1);
        assert_eq!(own.fixture_state(), native.tinybones_fixture_state());
    }
}

#[test]
fn genuine_paid_numeric_foreign_full_packet_and_unknown_operand_refusals_are_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (state, _, _) = owning_ready(p, "Forest");
        let mut a = TinybonesSession::from_session(
            FastActorSessionV1::from_tinybones_v72_fixture_state(state.clone()),
        )
        .unwrap();
        let mut b = TinybonesSession::from_session(
            FastActorSessionV1::from_tinybones_v72_fixture_state(state),
        )
        .unwrap();
        let FastActorResponseV1::Decision(e) = a.current_response() else {
            panic!()
        };
        let d = a.encode(e).unwrap();
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
        assert!(a.apply(&d, u32::MAX).is_err());
        assert_eq!(a.fixture_state(), &before);
        assert_eq!(a.fixture_counts(), counts);
    }
}

#[test]
fn paid_numeric_eighteen_counter_storage_is_exact_and_hidden_worlds_remain_equal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut packets = vec![];
        for hidden in ["Forest", "Imprisoned in the Moon"] {
            let (state, _, _) = owning_ready(p, hidden);
            let own = TinybonesSession::from_session(
                FastActorSessionV1::from_tinybones_v72_fixture_state(state),
            )
            .unwrap();
            let FastActorResponseV1::Decision(e) = own.current_response() else {
                panic!()
            };
            packets.push(own.encode(e).unwrap().packet().clone());
        }
        assert_eq!(packets[0], packets[1]);
        let (state, tiny, _) = owning_ready(p, "Forest");
        let mut wide = vec![];
        for count in [i32::MAX - 1, i32::MAX] {
            let mut world = state.clone();
            // Explicit full-width native storage operand on independently paid history.
            let c = &mut world.objects.get_mut(tiny).counters;
            c.stash = count;
            c.fellowship = i32::MAX;
            c.charge = i32::MAX;
            let own = TinybonesSession::from_session(
                FastActorSessionV1::from_tinybones_v72_fixture_state(world),
            )
            .unwrap();
            let FastActorResponseV1::Decision(e) = own.current_response() else {
                panic!()
            };
            let d = own.encode(e).unwrap();
            assert_eq!(
                d.backend.observation().tinybones.battlefield[p.index()]
                    .iter()
                    .find(|r| r.card.object == tiny.0)
                    .unwrap()
                    .counters
                    .stash,
                count
            );
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
