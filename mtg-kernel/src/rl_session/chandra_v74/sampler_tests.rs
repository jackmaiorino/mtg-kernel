//! Genuine paid current-slot sampling, not a posterior or playing-strength claim.
use super::*;
use crate::{
    effect::chandra_flameshaper_v1::tests::owner_v74_tests::owning_ready,
    event::{self, CommittedEvent as E, ProposedEvent},
    rl::ActionSemanticV1 as A,
    state::Zone,
};
fn owner(s: GameState) -> ChandraSessionV74 {
    ChandraSessionV74::from_session(FastActorSessionV1::from_chandra_v74_fixture_state(s)).unwrap()
}
fn sampled(o: &ChandraSessionV74, seed: u64) -> ChandraSessionV74 {
    let d = o.decision_v74().unwrap();
    let before = o.fixture_state_v74().clone();
    let mut c = o.sampled_clone_v74(&d, seed).unwrap();
    let again = o.sampled_clone_v74(&d, seed).unwrap();
    assert_eq!(c.fixture_state_v74(), again.fixture_state_v74());
    assert!(c.fresh_binding().unwrap() == d.binding);
    assert_eq!(
        c.fixture_state_v74().engine.event_history,
        before.engine.event_history
    );
    assert_eq!(
        c.fixture_state_v74().engine.event_log,
        before.engine.event_log
    );
    assert_eq!(c.fixture_state_v74().counter_lki_v1, before.counter_lki_v1);
    assert_eq!(o.fixture_state_v74(), &before);
    assert!(!std::sync::Arc::ptr_eq(&c.owner, &o.owner));
    assert!(c.validate_decision_v74(&d).is_err());
    let fresh = c.decision_v74().unwrap();
    assert!(o.validate_decision_v74(&fresh).is_err());
    let decoded: GameState =
        serde_json::from_slice(&serde_json::to_vec(c.fixture_state_v74()).unwrap()).unwrap();
    assert_eq!(&decoded, c.fixture_state_v74());
    observation::validate_state(&decoded).unwrap();
    let saved = c.snapshot_v74();
    c.restore_v74(&saved).unwrap();
    assert!(c.validate_decision_v74(&fresh).is_err());
    assert_eq!(
        c.decision_v74().unwrap().candidates_v74(),
        fresh.candidates_v74()
    );
    c
}
fn act(o: &mut ChandraSessionV74, choose: impl Fn(&A) -> bool) {
    let d = o.decision_v74().unwrap();
    let i = d.actions().iter().position(choose).unwrap();
    let original = d.fixture_original_index_v74(i as u32).unwrap();
    let mut native = o.fixture_native_clone_v74();
    assert_eq!(
        o.step_v74(&d, i as u32).unwrap(),
        native
            .step(d.expected().episode_id, d.expected().step, original)
            .unwrap()
    );
    assert_eq!(o.fixture_state_v74(), native.chandra_fixture_state());
    assert_eq!(
        o.fixture_counts_v74(),
        (native.policy_step_count(), native.physical_decision_count())
    );
}
fn quiet(o: &mut ChandraSessionV74) {
    let d = o.decision_v74().unwrap();
    let finish = d
        .actions()
        .iter()
        .any(|a| matches!(a, A::FinishEffectSelection { .. }));
    act(o, |a| {
        if finish {
            matches!(a, A::FinishEffectSelection { .. })
        } else {
            matches!(
                a,
                A::Pass { .. }
                    | A::OrderTriggers { .. }
                    | A::ChooseTriggerOrderNext { .. }
                    | A::ChooseEffectTarget { .. }
                    | A::ChooseAttackerInclusion { include: false, .. }
                    | A::ChooseBlockerInclusion { include: false, .. }
            )
        }
    });
}
#[test]
fn genuine_paid_chandra_exile_subset_grant_sampling_and_native_indices_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, id, _, _) = owning_ready(p, false, "Island");
        let mut o = owner(s);
        let mut changed = false;
        for seed in 1..=8 {
            let c = sampled(&o, seed);
            changed |= c.fixture_state_v74().players != o.fixture_state_v74().players;
        }
        assert!(changed);
        act(
            &mut o,
            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==id.0),
        );
        let published = sampled(&o, 740001);
        assert_eq!(
            published
                .decision_v74()
                .unwrap()
                .observation()
                .chandra
                .activations[0]
                .loyalty_after,
            8
        );
        let mut saw_choice = false;
        let mut complete = false;
        for n in 0..64 {
            let d = o.decision_v74().unwrap();
            if d.observation().chandra.activations[0].stage
                == crate::chandra_public_v74::ActivationStageV74::NativeCompleted
            {
                complete = true;
                break;
            }
            if d.observation()
                .chandra
                .exiles
                .iter()
                .any(|r| r.choice_current)
            {
                let c = sampled(&o, 740100 + n);
                assert_eq!(
                    c.decision_v74().unwrap().candidates_v74(),
                    d.candidates_v74()
                );
                saw_choice = true;
            }
            quiet(&mut o);
        }
        assert!(saw_choice && complete);
        let mut c = sampled(&o, 740002);
        let d = c.decision_v74().unwrap();
        let ex = &d.observation().chandra.exiles[0];
        assert_eq!(ex.cards.len(), 3);
        assert!(ex.chosen.is_some() && ex.permission_present && !ex.expired);
        let chosen = ex.cards[ex.chosen.unwrap() as usize].original.object;
        assert!(c
            .fixture_state_v74()
            .engine
            .exile_play_permissions
            .iter()
            .any(|g| g.object.0 == chosen && g.holder == p));
        let turn = c.fixture_state_v74().turn_key_v1();
        for _ in 0..128 {
            if c.fixture_state_v74().turn_key_v1() != turn {
                break;
            }
            quiet(&mut c);
        }
        assert_ne!(c.fixture_state_v74().turn_key_v1(), turn);
        let c = sampled(&c, 740003);
        let d = c.decision_v74().unwrap();
        assert!(d.observation().chandra.exiles[0].expired);
        assert!(!d.observation().chandra.exiles[0].permission_present);
    }
}
#[test]
fn independently_paid_chandra_hidden_worlds_preserve_numeric_packets_fresh_owner_and_stale_refusal_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _, _, _) = owning_ready(p, false, "Island");
        let (b, _, _, _) = owning_ready(p, false, "Imprisoned in the Moon");
        let a = owner(a);
        let b = owner(b);
        assert_eq!(
            a.decision_v74().unwrap().candidates_v74(),
            b.decision_v74().unwrap().candidates_v74()
        );
        let a = crate::current_mixed_policy_v1::ChandraSession::from_chandra(sampled(&a, 740004));
        let b = crate::current_mixed_policy_v1::ChandraSession::from_chandra(sampled(&b, 740004));
        let FastActorResponseV1::Decision(h) = a.current_response() else {
            panic!()
        };
        let d = a.encode(h).unwrap();
        let FastActorResponseV1::Decision(h) = b.current_response() else {
            panic!()
        };
        assert_eq!(d.packet(), b.encode(h).unwrap().packet());
        let mut c = a.sampled_clone(&d, 740005).unwrap();
        let FastActorResponseV1::Decision(h) = c.current_response() else {
            panic!()
        };
        assert_eq!(c.encode(h).unwrap().packet(), d.packet());
        let saved = c.snapshot();
        c.restore(&saved).unwrap();
        assert!(c.apply(&d, 0).is_err());
    }
}
#[test]
fn genuine_paid_chandra_all_eight_damage_units_keep_original_targets_allocation_and_cost_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, id, ours, theirs) = owning_ready(p, false, "Island");
        let mut o = owner(s);
        act(
            &mut o,
            |a| matches!(a,A::ActivateAbility{source,ability_index:2,..}if source.arena_id==id.0),
        );
        for n in 0..8 {
            let c = sampled(&o, 740200 + n);
            let d = c.decision_v74().unwrap();
            let allocation = d.observation().chandra.pending_allocation.as_ref().unwrap();
            assert_eq!(allocation.remaining_units, 8 - n as u8);
            assert_eq!(allocation.loyalty_before, 6);
            let target = if n < 3 { ours } else { theirs };
            act(
                &mut o,
                |a| matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==target.0),
            );
        }
        let c = sampled(&o, 740006);
        let d = c.decision_v74().unwrap();
        let a = &d.observation().chandra.activations[0];
        assert_eq!((a.loyalty_before, a.loyalty_after), (6, 2));
        assert_eq!(
            a.targets
                .iter()
                .map(|t| t.allocated_damage.unwrap())
                .collect::<Vec<_>>(),
            vec![3, 5]
        );
        let mut world = o.fixture_state_v74().clone();
        // Labelled native target-departure primitive after real positive paid allocation.
        event::propose_and_commit(
            &mut world,
            ProposedEvent::zone_change(theirs, Zone::Library),
        );
        crate::chandra_observation_v74::validate_state(&world).unwrap();
        assert!(
            crate::effect::chandra_flameshaper_v1::sampler_conflicts_v74(&world, &[theirs])
                .unwrap()
        );
        let o = owner(world);
        let d = o.decision_v74().unwrap();
        let before = o.fixture_state_v74().clone();
        assert!(o.sampled_clone_v74(&d, 740007).is_err());
        assert_eq!(o.fixture_state_v74(), &before);
    }
}
#[test]
fn genuine_paid_chandra_revealed_original_and_source_later_hidden_and_forged_receipt_refuse_readonly_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, id, _, _) = owning_ready(p, false, "Island");
        let mut o = owner(s);
        act(
            &mut o,
            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==id.0),
        );
        for _ in 0..64 {
            if o.decision_v74().unwrap().observation().chandra.activations[0].stage
                == crate::chandra_public_v74::ActivationStageV74::NativeCompleted
            {
                break;
            }
            quiet(&mut o);
        }
        let d = o.decision_v74().unwrap();
        assert_eq!(d.observation().chandra.exiles[0].cards.len(), 3);
        let exile = &d.observation().chandra.exiles[0];
        let unchosen = (0..exile.cards.len())
            .find(|i| Some(*i as u16) != exile.chosen)
            .unwrap();
        let revealed = crate::ids::ObjectId(exile.cards[unchosen].original.object);
        for hidden in [id, revealed] {
            let mut world = o.fixture_state_v74().clone();
            // Native move after genuine paid/exiled history; original journal stays immutable.
            event::propose_and_commit(
                &mut world,
                ProposedEvent::zone_change(hidden, Zone::Library),
            );
            observation::validate_state(&world).unwrap();
            assert!(
                crate::effect::chandra_flameshaper_v1::sampler_conflicts_v74(&world, &[hidden])
                    .unwrap()
            );
            let a = owner(world);
            let d = a.decision_v74().unwrap();
            let before = a.fixture_state_v74().clone();
            assert!(a.sampled_clone_v74(&d, 740008).is_err());
            assert_eq!(a.fixture_state_v74(), &before);
        }
        let before = o.fixture_state_v74().clone();
        let mut old = before.clone();
        assert!(
            super::super::flat_action_v4::redeterminize_liliana_v73(&mut old, p, 740009).is_err()
        );
        assert_eq!(old, before);
        let mut altered = d.clone();
        altered.binding.public_to_original.clear();
        assert!(o.sampled_clone_v74(&altered, 740010).is_err());
        let mut forged = before.clone();
        let E::ChandraFlameshaperV74{journal}=forged.engine.event_history.iter_mut().find(|e|matches!(e,E::ChandraFlameshaperV74{journal}if matches!(journal.as_ref(),crate::effect::chandra_flameshaper_v1::JournalV74::Payment{..}))).unwrap()else{panic!()};
        let crate::effect::chandra_flameshaper_v1::JournalV74::Payment { input } = journal.as_mut()
        else {
            panic!()
        };
        let mut value = serde_json::to_value(input.as_ref()).unwrap();
        value["source"]["zone_change_count"] = u32::MAX.into();
        **input = serde_json::from_value(value).unwrap();
        let mut decoded: GameState =
            serde_json::from_slice(&serde_json::to_vec(&forged).unwrap()).unwrap();
        let unchanged = decoded.clone();
        assert!(
            super::super::flat_action_v4::redeterminize_chandra_v74(&mut decoded, p, 740011)
                .is_err()
        );
        assert_eq!(decoded, unchanged);
    }
}
#[test]
fn genuine_paid_chandra_copy_endstep_and_crystal_replacement_owners_keep_conservative_sampling_refusal_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, id, ours, _) = owning_ready(p, false, "Island");
        let mut o = owner(s);
        act(
            &mut o,
            |a| matches!(a,A::ActivateAbility{source,ability_index:1,..}if source.arena_id==id.0),
        );
        act(
            &mut o,
            |a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==ours.0),
        );
        let _ = sampled(&o, 740012);
        for _ in 0..64 {
            if o.decision_v74().unwrap().observation().chandra.activations[0].stage
                == crate::chandra_public_v74::ActivationStageV74::NativeCompleted
            {
                break;
            }
            quiet(&mut o);
        }
        let d = o.decision_v74().unwrap();
        assert!(!d.observation().chandra.copied_tokens.is_empty());
        let mut reached = false;
        for _ in 0..128 {
            let d = o.decision_v74().unwrap();
            if d.observation()
                .chandra
                .end_steps
                .iter()
                .any(|r| r.published_count > 0 && r.finished_count == 0)
            {
                reached = true;
                break;
            }
            quiet(&mut o);
        }
        assert!(reached);
        let d = o.decision_v74().unwrap();
        let before = o.fixture_state_v74().clone();
        assert!(o.sampled_clone_v74(&d, 740013).is_err());
        assert_eq!(o.fixture_state_v74(), &before);
        let state =
            crate::effect::chandra_flameshaper_v1::tests::owner_v74_tests::pending_damage_boundary(
                p,
            );
        let o = owner(state);
        let d = o.decision_v74().unwrap();
        assert!(d.observation().chandra.pending_damage.is_some());
        let before = o.fixture_state_v74().clone();
        assert!(o.sampled_clone_v74(&d, 740014).is_err());
        assert_eq!(o.fixture_state_v74(), &before);
    }
}
