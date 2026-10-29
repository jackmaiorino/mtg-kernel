//! Current-slot permutations on genuine paid histories; no posterior/strength claim.
use super::tests::Pair;
use super::*;
use crate::{
    engine::tinybones_v72::tests::complete_observation::{
        sampler_disposed, sampler_ready, sampler_replace_source,
    },
    event::{self, CommittedEvent as E, ProposedEvent},
    rl::ActionSemanticV1 as A,
    state::Zone,
};
fn sampled(owner: &TinybonesSessionV72, seed: u64) -> TinybonesSessionV72 {
    let d = owner.decision_v72().unwrap();
    let before = owner.fixture_state_v72().clone();
    let mut clone = owner.sampled_clone_v72(&d, seed).unwrap();
    let repeat = owner.sampled_clone_v72(&d, seed).unwrap();
    assert_eq!(clone.fixture_state_v72(), repeat.fixture_state_v72());
    assert!(clone.fresh_binding().unwrap() == d.binding);
    assert_eq!(
        clone.fixture_state_v72().engine.event_history,
        before.engine.event_history
    );
    assert_eq!(
        clone.fixture_state_v72().engine.event_log,
        before.engine.event_log
    );
    assert_eq!(
        clone.fixture_state_v72().engine.exile_play_permissions,
        before.engine.exile_play_permissions
    );
    assert_eq!(
        clone.fixture_state_v72().counter_lki_v1,
        before.counter_lki_v1
    );
    assert_eq!(owner.fixture_state_v72(), &before);
    assert!(!std::sync::Arc::ptr_eq(&clone.owner, &owner.owner));
    assert!(clone.validate_decision_v72(&d).is_err());
    let fresh = clone.decision_v72().unwrap();
    assert!(owner.validate_decision_v72(&fresh).is_err());
    let decoded: GameState =
        serde_json::from_slice(&serde_json::to_vec(clone.fixture_state_v72()).unwrap()).unwrap();
    observation::validate_state(&decoded).unwrap();
    assert_eq!(&decoded, clone.fixture_state_v72());
    let save = clone.snapshot_v72();
    clone.restore_v72(&save).unwrap();
    assert!(clone.validate_decision_v72(&fresh).is_err());
    assert_eq!(
        clone.decision_v72().unwrap().candidates_v72(),
        fresh.candidates_v72()
    );
    clone
}
fn parity_step(owner: &mut TinybonesSessionV72, choose: impl Fn(&A) -> bool) {
    let d = owner.decision_v72().unwrap();
    let i = d.actions().iter().position(choose).unwrap();
    let original = d.fixture_original_index_v72(i as u32).unwrap();
    let mut native = owner.fixture_native_clone_v72();
    assert_eq!(
        owner.step_v72(&d, i as u32).unwrap(),
        native
            .step(d.expected().episode_id, d.expected().step, original)
            .unwrap()
    );
    assert_eq!(owner.fixture_state_v72(), native.tinybones_fixture_state());
}
#[test]
fn genuine_paid_tinybones_sampling_keeps_discard_publication_stash_and_cast_native_indices_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (state, tiny, card, _, _) = sampler_ready(p, "Forest");
        let known = state.players[p.index()].library[0];
        let mut pair = Pair::new(state);
        let mut changed = false;
        for seed in 1..=8 {
            let c = sampled(&pair.own, seed);
            assert_eq!(c.fixture_state_v72().players[p.index()].library[0], known);
            changed |= c.fixture_state_v72().players != pair.own.fixture_state_v72().players;
        }
        assert!(changed);
        pair.act(
            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==tiny.0),
        );
        for _ in 0..32 {
            if pair
                .own
                .decision_v72()
                .unwrap()
                .actions()
                .iter()
                .any(|a| matches!(a, A::Discard { .. }))
            {
                break;
            }
            pair.quiet();
        }
        let at_discard = sampled(&pair.own, 724901);
        assert_eq!(
            at_discard
                .decision_v72()
                .unwrap()
                .observation()
                .tinybones
                .viewer,
            p.opponent().into()
        );
        pair.act(|a| matches!(a,A::Discard{cards,..}if cards.iter().any(|r|r.arena_id==card.0)));
        let published = sampled(&pair.own, 724902);
        assert!(published
            .decision_v72()
            .unwrap()
            .observation()
            .tinybones
            .triggers
            .iter()
            .any(|r| r.stage == crate::tinybones_public_v72::StageV72::Published));
        for _ in 0..48 {
            if pair.own.fixture_state_v72().objects.get(card).zone == Zone::Exile {
                break;
            }
            pair.quiet();
        }
        let mut derived = sampled(&pair.own, 724903);
        let before = derived.fixture_state_v72().objects.get(card).clone();
        assert_eq!(before.counters.stash, 1);
        assert_eq!(before.owner, p.opponent());
        let facts = derived
            .decision_v72()
            .unwrap()
            .observation()
            .tinybones
            .clone();
        assert!(facts.stash[0].can_play[p.index()]);
        assert!(facts.stash[0].any_type_mana[p.index()]);
        parity_step(
            &mut derived,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0),
        );
        let paid = sampled(&derived, 724904);
        let d = paid.decision_v72().unwrap();
        let play = d.observation().tinybones.plays.last().unwrap();
        assert_eq!(play.stage, crate::tinybones_public_v72::PlayStageV72::Paid);
        assert_eq!(play.original_card.owner, p.opponent().into());
        assert_eq!(play.paid_x, Some(0));
        assert_eq!(paid.fixture_state_v72().objects.get(card).zone, Zone::Stack);
    }
}
#[test]
fn independent_prepaid_tinybones_worlds_sample_without_private_identity_in_root_bindings_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _, _, _, _) = sampler_ready(p, "Forest");
        let (b, _, _, _, _) = sampler_ready(p, "Mountain");
        let a = Pair::new(a).own;
        let b = Pair::new(b).own;
        let da = a.decision_v72().unwrap();
        let db = b.decision_v72().unwrap();
        assert_eq!(da.candidates_v72(), db.candidates_v72());
        let sa = sampled(&a, 724905);
        let sb = sampled(&b, 724905);
        assert_eq!(
            sa.decision_v72().unwrap().candidates_v72(),
            sb.decision_v72().unwrap().candidates_v72()
        );
        let a = crate::current_mixed_policy_v1::TinybonesSession::from_tinybones(sa);
        let b = crate::current_mixed_policy_v1::TinybonesSession::from_tinybones(sb);
        let FastActorResponseV1::Decision(e) = a.current_response() else {
            panic!()
        };
        let da = a.encode(e).unwrap();
        let FastActorResponseV1::Decision(e) = b.current_response() else {
            panic!()
        };
        let db = b.encode(e).unwrap();
        assert_eq!(da.packet(), db.packet());
        let mut sampled = a.sampled_clone(&da, 724906).unwrap();
        let FastActorResponseV1::Decision(e) = sampled.current_response() else {
            panic!()
        };
        assert_eq!(sampled.encode(e).unwrap().packet(), da.packet());
        assert!(sampled.apply(&da, 0).is_err());
    }
}
#[test]
fn genuine_paid_tinybones_history_hidden_original_and_forged_stash_refuse_readonly_after_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for hide_source in [false, true] {
            let (state, tiny, card, _, _) = sampler_ready(p, "Forest");
            let mut pair = Pair::new(state);
            pair.stash(tiny, card);
            let mut world = pair.own.fixture_state_v72().clone();
            let hidden = if hide_source { tiny } else { card };
            // Native zone-move primitive after real paid discard/exile; no history rebinding.
            event::propose_and_commit(
                &mut world,
                ProposedEvent::zone_change(hidden, Zone::Library),
            );
            observation::validate_state(&world).unwrap();
            assert!(
                crate::engine::tinybones_v72::sampler_conflicts_v72(&world, &[hidden]).unwrap()
            );
            let owner = Pair::new(world).own;
            let d = owner.decision_v72().unwrap();
            let before = owner.fixture_state_v72().clone();
            assert!(owner.sampled_clone_v72(&d, 724907).is_err());
            assert_eq!(owner.fixture_state_v72(), &before);
            let mut legacy = before.clone();
            let legacy_before = legacy.clone();
            assert!(super::super::flat_action_v4::redeterminize_muldrotha_v71(
                &mut legacy,
                p,
                724912
            )
            .is_err());
            assert_eq!(legacy, legacy_before);
        }
        let (state, tiny, card, _, _) = sampler_ready(p, "Forest");
        let mut pair = Pair::new(state);
        pair.stash(tiny, card);
        let mut world = pair.own.fixture_state_v72().clone();
        let E::TinybonesV72{journal}=world.engine.event_history.iter_mut().find(|e|matches!(e,E::TinybonesV72{journal}if matches!(journal.as_ref(),crate::engine::tinybones_v72::JournalV72::Stashed{..}))).unwrap()else{panic!()};
        let crate::engine::tinybones_v72::JournalV72::Stashed { after, .. } = journal.as_mut()
        else {
            panic!()
        };
        *after = 2;
        let mut decoded: GameState =
            serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        let before = decoded.clone();
        assert!(
            super::super::flat_action_v4::redeterminize_tinybones_v72(&mut decoded, p, 724908)
                .is_err()
        );
        assert_eq!(decoded, before);
    }
}
#[test]
fn genuine_paid_tinybones_replacement_grant_and_disposed_archive_keep_factual_authority_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let (state, tiny, card, second, bolt) = sampler_ready(p, "Forest");
        let mut pair = Pair::new(state);
        pair.stash(tiny, card);
        let mut world = pair.own.fixture_state_v72().clone();
        sampler_replace_source(&mut world, p, tiny, second, bolt);
        let owner = Pair::new(world).own;
        let d = owner.decision_v72().unwrap();
        let stash = &d.observation().tinybones.stash[0];
        assert!(stash.current_grant_sources[p.index()]
            .iter()
            .any(|r| r.original_source.object == second.0));
        assert!(stash.current_grant_sources[p.index()]
            .iter()
            .all(|r| r.original_source.object != tiny.0));
        assert!(stash.can_play[p.index()]);
        let clone = sampled(&owner, 724909);
        assert_eq!(
            clone.fixture_state_v72().objects.get(card).counters.stash,
            1
        );
        let mut wide = owner.fixture_state_v72().clone();
        // Labelled native full-width battlefield storage operands on real paid history.
        wide.objects.get_mut(second).counters.charge = i32::MAX;
        wide.objects.get_mut(second).counters.fellowship = i32::MAX;
        let wide_owner = Pair::new(wide).own;
        let wide_clone = sampled(&wide_owner, 724911);
        let c = &wide_clone.fixture_state_v72().objects.get(second).counters;
        assert_eq!(c.charge, i32::MAX);
        assert_eq!(c.fellowship, i32::MAX);
        let owner = Pair::new(sampler_disposed(p)).own;
        let d = owner.decision_v72().unwrap();
        assert!(d
            .observation()
            .tinybones
            .triggers
            .iter()
            .any(|r| r.stage == crate::tinybones_public_v72::StageV72::Disposed));
        let clone = sampled(&owner, 724910);
        assert_eq!(
            clone.fixture_state_v72().engine.time_stop_v1,
            owner.fixture_state_v72().engine.time_stop_v1
        );
    }
}
