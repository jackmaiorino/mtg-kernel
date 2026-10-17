use super::*;
use crate::{ids::PlayerId, rl::ActionSemanticV1 as A, rl_session::muldrotha_v71::tests::ready};
fn act(s: &mut MuldrothaSession, n: &mut FastActorSessionV1, choose: impl Fn(&A) -> bool) {
    let FastActorResponseV1::Decision(expected) = s.current_response() else {
        panic!()
    };
    let d = s.encode(expected).unwrap();
    let i = d.backend.actions().iter().position(choose).unwrap();
    let raw = d.backend.fixture_original_index_v71(i as u32).unwrap();
    let scores = NativeModel::deterministic_fixture()
        .unwrap()
        .forward(d.packet())
        .unwrap();
    assert_eq!(scores.len(), d.backend.actions().len());
    assert!(scores.iter().all(|v| v.is_finite()));
    assert_eq!(d.packet().source(), Source::MuldrothaV71);
    assert_eq!(
        s.apply(&d, i as u32).unwrap(),
        n.step(expected.episode_id, expected.step, raw).unwrap()
    );
    assert_eq!(s.fixture_state(), n.muldrotha_fixture_state());
    assert_eq!(
        s.fixture_counts(),
        (n.policy_step_count(), n.physical_decision_count())
    );
}
#[test]
fn paid_numeric_permission_cost_native_parity_fresh_packet_and_snapshot_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, _, card) = ready(p, "Island");
        let mut native = FastActorSessionV1::from_muldrotha_v71_fixture_state(world.clone());
        let mut own = MuldrothaSession::from_session(
            FastActorSessionV1::from_muldrotha_v71_fixture_state(world),
        )
        .unwrap();
        act(
            &mut own,
            &mut native,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0),
        );
        let FastActorResponseV1::Decision(e) = own.current_response() else {
            panic!()
        };
        let d = own.encode(e).unwrap();
        let save = own.snapshot();
        let raw = native.snapshot_v1();
        let mut forged = d.clone();
        forged.packet.actions.clear();
        let before = own.fixture_state().clone();
        assert!(own.apply(&forged, 0).is_err());
        assert_eq!(own.fixture_state(), &before);
        act(&mut own, &mut native, |a| {
            matches!(
                a,
                A::ChooseEffectOption {
                    option_index: 0,
                    ..
                }
            )
        });
        own.restore(&save).unwrap();
        native.restore_v1(&raw);
        let before = own.fixture_state().clone();
        assert!(own.apply(&d, 0).is_err());
        assert_eq!(own.fixture_state(), &before);
        act(&mut own, &mut native, |a| {
            matches!(
                a,
                A::ChooseEffectOption {
                    option_index: 0,
                    ..
                }
            )
        });
        let mut other = MuldrothaSession::from_session(
            FastActorSessionV1::from_muldrotha_v71_fixture_state(before),
        )
        .unwrap();
        let FastActorResponseV1::Decision(e) = own.current_response() else {
            panic!()
        };
        let d = own.encode(e).unwrap();
        assert!(other.apply(&d, 0).is_err());
    }
}
