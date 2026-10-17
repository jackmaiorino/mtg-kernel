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

#[test]
fn paid_numeric_derived_owner_preserves_packet_actual_payment_and_foreign_refusal_both_seats() {
    use crate::{
        engine::muldrotha_v1::tests as native, policy_observation_v6::tests::put, state::Zone,
    };
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut world, _, card) = ready(p, "Island");
        put(&mut world, p.opponent(), "Mountain", Zone::Hand);
        let mut pair = crate::rl_session::muldrotha_v71::tests::Pair::new(world);
        pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0));
        let own = MuldrothaSession::from_muldrotha(pair.own);
        let FastActorResponseV1::Decision(expected) = own.current_response() else {
            panic!()
        };
        let d = own.encode(expected).unwrap();
        let original = own.fixture_state().clone();
        let mut clone = own.sampled_clone(&d, 8177).unwrap();
        let repeat = own.sampled_clone(&d, 8177).unwrap();
        assert_eq!(clone.fixture_state(), repeat.fixture_state());
        assert_eq!(own.fixture_state(), &original);
        let fresh = clone.encode(expected).unwrap();
        assert_eq!(fresh.packet(), d.packet());
        assert!(clone.apply(&d, 0).is_err());
        assert!(own.sampled_clone(&fresh, 8177).is_err());
        let mut forged = d.clone();
        forged.packet.actions.clear();
        assert!(own.sampled_clone(&forged, 8177).is_err());
        let before = native::tapped(clone.fixture_state(), p);
        let mut direct = clone.inner.fixture_native_clone_v71();
        let original_index = fresh.backend.fixture_original_index_v71(0).unwrap();
        let model = NativeModel::deterministic_fixture().unwrap();
        let (scores, selected, result) = clone.score_and_apply(&fresh, &model).unwrap();
        assert_eq!(selected, 0);
        assert_eq!(scores.len(), 1);
        assert!(scores[0].is_finite());
        assert_eq!(
            result,
            direct
                .step(expected.episode_id, expected.step, original_index)
                .unwrap()
        );
        assert_eq!(clone.fixture_state(), direct.muldrotha_fixture_state());
        assert_eq!(native::tapped(clone.fixture_state(), p) - before, 1);
    }
}

#[test]
fn production_reset_keeps_full_only_decks_and_partial_refusal_both_seats() {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let muldrotha = crate::card_def::card_id_by_name("Muldrotha, the Gravetide").unwrap();
    for p in [PlayerId::P0, PlayerId::P1] {
        let decks = [vec![forest; 40], vec![forest; 40]];
        let owner = MuldrothaSession::reset_explicit_mainboards(
            71,
            8178,
            (1000, 1000),
            ["full0".into(), "full1".into()],
            decks.clone(),
            p,
        )
        .unwrap();
        assert!(matches!(
            owner.current_response(),
            FastActorResponseV1::Decision(_)
        ));
        let mut partial = decks;
        partial[p.index()][0] = muldrotha;
        assert!(MuldrothaSession::reset_explicit_mainboards(
            71,
            8178,
            (1000, 1000),
            ["partial0".into(), "partial1".into()],
            partial,
            p
        )
        .is_err());
    }
}
