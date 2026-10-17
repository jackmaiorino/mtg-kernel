use super::*;
use crate::{
    ids::PlayerId,
    rl::ActionSemanticV1 as A,
    rl_session::vivien_v70::tests::{ready, Pair},
};
#[test]
fn genuine_paid_vivien_numeric_full_packet_index_parity_restore_and_foreign_refusal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, wave, _) = ready(p, "Island");
        let mut pair = Pair::new(s);
        pair.cast(wave, p);
        pair.act(
            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==wave.0),
        );
        pair.advance_to_look();
        let before = pair.own.fixture_state_v70().clone();
        let mut model = VivienSession::from_vivien(pair.own);
        let FastActorResponseV1::Decision(e) = model.current_response() else {
            panic!("Wave model terminal")
        };
        let d = model.encode(e).unwrap();
        assert_eq!(d.packet().source(), Source::VivienV70);
        let logits = NativeModel::deterministic_fixture()
            .unwrap()
            .forward(d.packet())
            .unwrap();
        assert_eq!(logits.len(), d.backend.actions().len());
        assert!(logits.iter().all(|v| v.is_finite()));
        assert!(!d.packet().state().rows().is_empty());
        assert!(d
            .packet()
            .state()
            .tensors()
            .iter()
            .flatten()
            .all(|v| v.is_finite()));
        let save = model.snapshot();
        let raw = pair.native.snapshot_v1();
        let i = d
            .backend
            .actions()
            .iter()
            .position(|a| matches!(a, A::FinishEffectSelection { .. }))
            .unwrap() as u32;
        let native = d.backend.fixture_original_index_v70(i).unwrap();
        assert_eq!(
            model.apply(&d, i).unwrap(),
            pair.native.step(e.episode_id, e.step, native).unwrap()
        );
        assert_eq!(model.fixture_state(), pair.native.vivien_fixture_state());
        assert_eq!(
            model.fixture_counts(),
            (
                pair.native.policy_step_count(),
                pair.native.physical_decision_count()
            )
        );
        model.restore(&save).unwrap();
        pair.native.vivien_restore_v70(&raw).unwrap();
        assert_eq!(model.fixture_state(), &before);
        assert_eq!(model.fixture_state(), pair.native.vivien_fixture_state());
        assert!(model.apply(&d, i).is_err());
        let fresh = model
            .encode(match model.current_response() {
                FastActorResponseV1::Decision(e) => e,
                _ => panic!(),
            })
            .unwrap();
        assert_eq!(fresh.packet(), d.packet());
        let mut foreign = VivienSession::from_session(
            FastActorSessionV1::from_vivien_v70_fixture_state(before.clone()),
        )
        .unwrap();
        assert!(foreign.apply(&fresh, 0).is_err());
        assert!(model.restore(&foreign.snapshot()).is_err());
        assert_eq!(model.fixture_state(), &before);
    }
}
