//! Current-slot sampling source cases. Initial scenes are native setup;
//! every tested Wave is genuinely paid. Rust/runtime qualification is UNRUN.
use super::tests::Pair;
use super::*;
use crate::{
    ids::ObjectId, policy_observation_v6::tests::put, rl::ActionSemanticV1 as A, state::Zone,
};
fn scene(p: PlayerId) -> (Pair, ObjectId, ObjectId) {
    let mut state = crate::effect::genesis_wave_v1::tests::ready(
        p,
        &[
            "Forest",
            "Pacifism",
            "Mountain",
            "Island",
            "Llanowar Elves",
            "Plains",
            "Giant Growth",
            "Forest",
        ],
        12,
        "Island",
    );
    let wave = put(&mut state, p, "Genesis Wave", Zone::Hand);
    let host = put(&mut state, p, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, p.opponent(), "Mountain", Zone::Hand);
    put(&mut state, p.opponent(), "Forest", Zone::Hand);
    (Pair::new(state), wave, host)
}
fn sampled(owner: &GenesisWaveSessionV69, seed: u64) -> GenesisWaveSessionV69 {
    let decision = owner.decision_v69().unwrap();
    let original = owner.fixture_state_v69().clone();
    let clone = owner.sampled_clone_v69(&decision, seed).unwrap();
    let repeat = owner.sampled_clone_v69(&decision, seed).unwrap();
    assert_eq!(clone.fixture_state_v69(), repeat.fixture_state_v69());
    assert_eq!(clone.fresh_binding().unwrap(), decision.binding);
    assert!(!std::sync::Arc::ptr_eq(&clone.owner, &owner.owner));
    assert_eq!(
        clone.fixture_state_v69().engine.event_history,
        original.engine.event_history
    );
    assert_eq!(
        clone.fixture_state_v69().engine.event_log,
        original.engine.event_log
    );
    assert_eq!(owner.fixture_state_v69(), &original);
    assert!(clone.validate_decision_v69(&decision).is_err());
    let fresh = clone.decision_v69().unwrap();
    assert!(owner.validate_decision_v69(&fresh).is_err());
    let decoded: GameState =
        serde_json::from_slice(&serde_json::to_vec(clone.fixture_state_v69()).unwrap()).unwrap();
    assert_eq!(&decoded, clone.fixture_state_v69());
    observation::validate_state(&decoded).unwrap();
    clone
}
fn act(owner: &mut GenesisWaveSessionV69, choose: impl Fn(&A) -> bool) {
    let decision = owner.decision_v69().unwrap();
    let i = decision.actions().iter().position(choose).unwrap();
    owner.step_v69(&decision, i as u32).unwrap();
}
fn finish(owner: &mut GenesisWaveSessionV69) {
    for _ in 0..32 {
        let d = owner.decision_v69().unwrap();
        if d.observation().genesis_wave.waves[0].stage
            == crate::genesis_wave_public_v69::StageV69::Finished
        {
            return;
        }
        let a = d
            .actions()
            .iter()
            .find(|a| {
                matches!(
                    a,
                    A::ChooseEffectTarget { .. } | A::FinishEffectSelection { .. } | A::Pass { .. }
                )
            })
            .unwrap()
            .clone();
        act(owner, |r| r == &a);
    }
    panic!("genuine sampled Wave did not finish")
}
#[test]
fn genuine_paid_wave_sampler_pre_reveal_and_active_subset_keep_public_slots_and_fresh_owner_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut pair, wave, _) = scene(p);
        pair.cast(wave, p, 2);
        let mut changed = false;
        for seed in 1..=8 {
            let clone = sampled(&pair.own, seed);
            changed |= clone.fixture_state_v69().players[p.index()].library
                != pair.own.fixture_state_v69().players[p.index()].library;
        }
        assert!(changed, "pre-reveal unknown library must remain eligible");
        pair.to_subset();
        let before = pair.own.fixture_state_v69().players[p.index()]
            .library
            .clone();
        let mut changed = false;
        for seed in 1..=8 {
            let mut clone = sampled(&pair.own, seed);
            assert_eq!(
                &clone.fixture_state_v69().players[p.index()].library[..2],
                &before[..2]
            );
            changed |= clone.fixture_state_v69().players[p.index()].library[2..] != before[2..];
            let d = clone.decision_v69().unwrap();
            let save = clone.snapshot_v69();
            act(&mut clone, |a| matches!(a, A::FinishEffectSelection { .. }));
            clone.restore_v69(&save).unwrap();
            assert!(clone.validate_decision_v69(&d).is_err());
            assert_eq!(
                clone.decision_v69().unwrap().candidates_v69(),
                d.candidates_v69()
            );
        }
        assert!(
            changed,
            "untouched future suffix must not inherit private historical order"
        );
    }
}
#[test]
fn genuine_paid_wave_sampler_aura_remainder_terminal_and_history_order_independence_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut pair, wave, host) = scene(p);
        pair.cast(wave, p, 2);
        pair.to_subset();
        pair.act(|a|matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.card_db_id==crate::card_def::card_id_by_name("Pacifism").unwrap()));
        pair.act(|a| matches!(a, A::FinishEffectSelection { .. }));
        let original = pair.own.decision_v69().unwrap();
        assert!(matches!(
            original.observation().genesis_wave.waves[0].stage,
            crate::genesis_wave_public_v69::StageV69::AuraHost { .. }
        ));
        let mut clone = sampled(&pair.own, 8172);
        act(
            &mut clone,
            |a| matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==host.0),
        );
        assert_eq!(
            clone
                .decision_v69()
                .unwrap()
                .observation()
                .genesis_wave
                .waves[0]
                .stage,
            crate::genesis_wave_public_v69::StageV69::GraveyardOrder
        );
        let mut remainder = sampled(&clone, 8173);
        finish(&mut remainder);
        assert_eq!(
            remainder.fixture_state_v69().objects.get(wave).zone,
            Zone::Graveyard
        );
        let terminal = sampled(&remainder, 8174);
        assert_eq!(
            terminal
                .decision_v69()
                .unwrap()
                .observation()
                .genesis_wave
                .waves[0]
                .selected_indices,
            vec![1]
        );
        // Explicit current hidden-order primitive on genuine immutable history:
        // a fixed seed depends on the current canonical pool, never its old
        // private ordering. No receipt or already revealed slot is rewritten.
        let mut reordered = pair.own.sampled_clone_v69(&original, 0).unwrap();
        reordered.inner.state.players[p.index()].library.swap(2, 3);
        let history = reordered.inner.state.engine.event_history.clone();
        let d = reordered.decision_v69().unwrap();
        let same = reordered.sampled_clone_v69(&d, 8172).unwrap();
        let expected = pair.own.sampled_clone_v69(&original, 8172).unwrap();
        assert_eq!(
            same.fixture_state_v69().players,
            expected.fixture_state_v69().players
        );
        assert_eq!(
            same.fixture_state_v69().objects,
            expected.fixture_state_v69().objects
        );
        assert_eq!(same.fixture_state_v69().engine.event_history, history);
    }
}
#[test]
fn genuine_paid_wave_sampler_as_enters_remains_readonly_until_exact_checkpoint_successor_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = crate::effect::genesis_wave_v1::tests::ready(
            p,
            &["Heraldic Banner", "Forest", "Island", "Mountain"],
            8,
            "Island",
        );
        let wave = put(&mut s, p, "Genesis Wave", Zone::Hand);
        let mut pair = Pair::new(s);
        pair.cast(wave, p, 3);
        pair.to_subset();
        pair.act(|a|matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.card_db_id==crate::card_def::card_id_by_name("Heraldic Banner").unwrap()));
        pair.act(|a| matches!(a, A::FinishEffectSelection { .. }));
        let d = pair.own.decision_v69().unwrap();
        assert_eq!(
            d.observation().genesis_wave.waves[0].stage,
            crate::genesis_wave_public_v69::StageV69::EntryChoices
        );
        let before = pair.own.fixture_state_v69().clone();
        assert!(pair.own.sampled_clone_v69(&d, 8172).is_err());
        assert_eq!(pair.own.fixture_state_v69(), &before);
        pair.equal();
    }
}

#[test]
fn genuine_paid_wave_sampler_keeps_time_stop_archives_and_current_unknown_slots_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::engine::time_stop_v1::tests::ready(p);
        let wave = put(&mut state, p, "Genesis Wave", Zone::Hand);
        let stop = put(&mut state, p, "Time Stop", Zone::Hand);
        let mut pair = Pair::new(state);
        pair.cast(wave, p, 1);
        pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==stop.0));
        for _ in 0..32 {
            if !pair
                .own
                .decision_v69()
                .unwrap()
                .observation()
                .end_turn
                .disposals
                .is_empty()
            {
                break;
            }
            pair.quiet();
        }
        let original = pair.own.decision_v69().unwrap();
        assert_eq!(
            original.observation().end_turn.disposals[0]
                .disposed_rows
                .len(),
            2
        );
        assert_eq!(
            pair.own.fixture_state_v69().objects.get(wave).zone,
            Zone::Exile
        );
        assert!(original.observation().genesis_wave.waves.is_empty());
        let clone = sampled(&pair.own, 8180);
        assert_eq!(
            clone.decision_v69().unwrap().observation().end_turn,
            original.observation().end_turn
        );
        assert!(
            crate::end_turn_observation_v68::validate_state(clone.fixture_state_v69()).is_err()
        );
        let before = pair.own.fixture_state_v69().clone();
        assert!(pair.own.restore_v69(&clone.snapshot_v69()).is_err());
        assert_eq!(pair.own.fixture_state_v69(), &before);
        pair.equal();
    }
}
