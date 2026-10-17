use super::*;
use crate::{
    ids::PlayerId,
    rl::ActionSemanticV1 as A,
    rl_session::genesis_wave_v69::tests::{ready, Pair},
};
#[test]
fn genuine_paid_wave_numeric_full_packet_index_parity_restore_and_foreign_refusal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, wave, _) = ready(p, "Island");
        let mut pair = Pair::new(s);
        pair.cast(wave, p, 4);
        pair.to_subset();
        let before = pair.own.fixture_state_v69().clone();
        let mut model = GenesisWaveSession::from_genesis_wave(pair.own);
        let FastActorResponseV1::Decision(e) = model.current_response() else {
            panic!("Wave model terminal")
        };
        let d = model.encode(e).unwrap();
        assert_eq!(d.packet().source(), Source::GenesisWaveV69);
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
        let native = d.backend.fixture_original_index_v69(i).unwrap();
        assert_eq!(
            model.apply(&d, i).unwrap(),
            pair.native.step(e.episode_id, e.step, native).unwrap()
        );
        assert_eq!(
            model.fixture_state(),
            pair.native.genesis_wave_fixture_state()
        );
        model.restore(&save).unwrap();
        pair.native.genesis_wave_restore_v69(&raw).unwrap();
        assert_eq!(model.fixture_state(), &before);
        assert_eq!(
            model.fixture_state(),
            pair.native.genesis_wave_fixture_state()
        );
        assert!(model.apply(&d, i).is_err());
        let fresh = model
            .encode(match model.current_response() {
                FastActorResponseV1::Decision(e) => e,
                _ => panic!(),
            })
            .unwrap();
        assert_eq!(fresh.packet(), d.packet());
        let mut foreign = GenesisWaveSession::from_session(
            FastActorSessionV1::from_genesis_wave_v69_fixture_state(before.clone()),
        )
        .unwrap();
        assert!(foreign.apply(&fresh, 0).is_err());
        assert!(model.restore(&foreign.snapshot()).is_err());
        assert_eq!(model.fixture_state(), &before);
    }
}
#[test]
fn genuine_paid_wave_numeric_x256_and_independent_hidden_worlds_keep_exact_operands_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let mut s = crate::effect::genesis_wave_v1::tests::ready(
                p,
                &["Forest", "Giant Growth"],
                259,
                hidden,
            );
            let wave = crate::policy_observation_v6::tests::put(
                &mut s,
                p,
                "Genesis Wave",
                crate::state::Zone::Hand,
            );
            let mut pair = Pair::new(s);
            pair.cast(wave, p, 256);
            pair.to_subset();
            let model = GenesisWaveSession::from_genesis_wave(pair.own);
            let FastActorResponseV1::Decision(e) = model.current_response() else {
                panic!()
            };
            let d = model.encode(e).unwrap();
            assert_eq!(d.backend.observation().genesis_wave.waves[0].x_value, 256);
            assert_eq!(
                d.backend.observation().genesis_wave.waves[0].revealed.len(),
                2
            );
            (model, d)
        };
        let (a, ad) = make("Island");
        let (b, bd) = make("Mountain");
        assert_ne!(a.fixture_state(), b.fixture_state());
        assert_eq!(ad.packet(), bd.packet());
        let mut changed = ad.backend.candidates_v69().clone();
        changed.observation.genesis_wave.waves[0].x_value = 255;
        assert!(packet_from_current(&changed, ad.backend.actions()).is_err());
    }
}
