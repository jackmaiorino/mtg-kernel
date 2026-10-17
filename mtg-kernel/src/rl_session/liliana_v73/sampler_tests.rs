//! Genuine paid current-slot sampling, not a posterior or playing-strength claim.
use super::*;
use crate::{
    effect::liliana_dreadhorde_v1::tests::owner_v73_tests::sampler_ready,
    event::{self, CommittedEvent as E, ProposedEvent},
    rl::ActionSemanticV1 as A,
    state::Zone,
};
fn owner(s: GameState) -> LilianaSessionV73 {
    LilianaSessionV73::from_session(FastActorSessionV1::from_liliana_v73_fixture_state(s)).unwrap()
}
fn sampled(o: &LilianaSessionV73, seed: u64) -> LilianaSessionV73 {
    let d = o.decision_v73().unwrap();
    let before = o.fixture_state_v73().clone();
    let mut c = o.sampled_clone_v73(&d, seed).unwrap();
    let again = o.sampled_clone_v73(&d, seed).unwrap();
    assert_eq!(c.fixture_state_v73(), again.fixture_state_v73());
    assert!(c.fresh_binding().unwrap() == d.binding);
    assert_eq!(
        c.fixture_state_v73().engine.event_history,
        before.engine.event_history
    );
    assert_eq!(
        c.fixture_state_v73().engine.event_log,
        before.engine.event_log
    );
    assert_eq!(c.fixture_state_v73().counter_lki_v1, before.counter_lki_v1);
    assert_eq!(o.fixture_state_v73(), &before);
    assert!(!std::sync::Arc::ptr_eq(&c.owner, &o.owner));
    assert!(c.validate_decision_v73(&d).is_err());
    let fresh = c.decision_v73().unwrap();
    assert!(o.validate_decision_v73(&fresh).is_err());
    let decoded: GameState =
        serde_json::from_slice(&serde_json::to_vec(c.fixture_state_v73()).unwrap()).unwrap();
    assert_eq!(&decoded, c.fixture_state_v73());
    observation::validate_state(&decoded).unwrap();
    let saved = c.snapshot_v73();
    c.restore_v73(&saved).unwrap();
    assert!(c.validate_decision_v73(&fresh).is_err());
    assert_eq!(
        c.decision_v73().unwrap().candidates_v73(),
        fresh.candidates_v73()
    );
    c
}
fn act(o: &mut LilianaSessionV73, choose: impl Fn(&A) -> bool) {
    let d = o.decision_v73().unwrap();
    let i = d.actions().iter().position(choose).unwrap();
    let original = d.fixture_original_index_v73(i as u32).unwrap();
    let mut native = o.fixture_native_clone_v73();
    assert_eq!(
        o.step_v73(&d, i as u32).unwrap(),
        native
            .step(d.expected().episode_id, d.expected().step, original)
            .unwrap()
    );
    assert_eq!(o.fixture_state_v73(), native.liliana_fixture_state());
    assert_eq!(
        o.fixture_counts_v73(),
        (native.policy_step_count(), native.physical_decision_count())
    );
}
fn quiet(o: &mut LilianaSessionV73) {
    let d = o.decision_v73().unwrap();
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
fn genuine_paid_liliana_sampling_keeps_loyalty_zombie_sacrifice_choices_and_native_indices_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1] {
            let (s, id) = sampler_ready(p, "Island");
            let mut o = owner(s);
            let mut changed = false;
            for seed in 1..=8 {
                let c = sampled(&o, seed);
                changed |= c.fixture_state_v73().players != o.fixture_state_v73().players;
            }
            assert!(changed);
            act(
                &mut o,
                |a| matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index),
            );
            let published = sampled(&o, 730001);
            assert_eq!(
                published
                    .decision_v73()
                    .unwrap()
                    .observation()
                    .liliana
                    .activations[0]
                    .loyalty_after,
                if index == 0 { 7 } else { 2 }
            );
            let mut saw_choice = false;
            let mut saw_completed = false;
            for step in 0..128 {
                let d = o.decision_v73().unwrap();
                if d.observation().liliana.activations[0].stage
                    == crate::liliana_public_v73::ActivationStageV73::NativeCompleted
                {
                    saw_completed = true;
                    break;
                }
                if d.observation()
                    .liliana
                    .resolutions
                    .iter()
                    .any(|r| r.selections.iter().any(|s| s.current))
                {
                    let c = sampled(&o, 730100 + step);
                    let current = c.decision_v73().unwrap();
                    assert_eq!(current.candidates_v73(), d.candidates_v73());
                    saw_choice = true;
                }
                quiet(&mut o);
            }
            assert!(saw_completed);
            assert_eq!(saw_choice, index == 1);
            let completed = sampled(&o, 730002);
            let d = completed.decision_v73().unwrap();
            if index == 0 {
                assert_eq!(
                    d.observation().liliana.activations[0].created_zombies.len(),
                    1
                );
            } else {
                assert!(d.observation().liliana.resolutions[0].finished);
                assert_eq!(
                    d.observation().liliana.resolutions[0]
                        .completed_sacrifices
                        .len(),
                    4
                );
            }
        }
    }
}
#[test]
fn independently_paid_liliana_hidden_worlds_keep_numeric_packets_and_fresh_sampler_authority_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _) = sampler_ready(p, "Island");
        let (b, _) = sampler_ready(p, "Imprisoned in the Moon");
        let a = owner(a);
        let b = owner(b);
        assert_eq!(
            a.decision_v73().unwrap().candidates_v73(),
            b.decision_v73().unwrap().candidates_v73()
        );
        let a = crate::current_mixed_policy_v1::LilianaSession::from_liliana(sampled(&a, 730003));
        let b = crate::current_mixed_policy_v1::LilianaSession::from_liliana(sampled(&b, 730003));
        let FastActorResponseV1::Decision(h) = a.current_response() else {
            panic!()
        };
        let d = a.encode(h).unwrap();
        let FastActorResponseV1::Decision(h) = b.current_response() else {
            panic!()
        };
        assert_eq!(d.packet(), b.encode(h).unwrap().packet());
        let mut c = a.sampled_clone(&d, 730004).unwrap();
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
fn genuine_paid_liliana_hidden_original_and_changed_journal_refuse_readonly_after_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (season, _) =
            crate::effect::liliana_dreadhorde_v1::tests::owner_v73_tests::owning_ready(
                p, true, "Island",
            );
        let season = owner(season);
        let d = season.decision_v73().unwrap();
        let before = season.fixture_state_v73().clone();
        assert!(season.sampled_clone_v73(&d, 730010).is_err());
        assert_eq!(season.fixture_state_v73(), &before);
        let (s, id) = sampler_ready(p, "Island");
        let mut o = owner(s);
        act(
            &mut o,
            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==id.0),
        );
        let mut world = o.fixture_state_v73().clone();
        // Explicit native zone boundary after actual paid publication, not public cancellation.
        event::propose_and_commit(&mut world, ProposedEvent::zone_change(id, Zone::Library));
        observation::validate_state(&world).unwrap();
        assert!(
            crate::effect::liliana_dreadhorde_v1::sampler_conflicts_v73(&world, &[id]).unwrap()
        );
        let o = owner(world);
        let d = o.decision_v73().unwrap();
        let before = o.fixture_state_v73().clone();
        assert!(o.sampled_clone_v73(&d, 730005).is_err());
        assert_eq!(o.fixture_state_v73(), &before);
        let mut old = before.clone();
        assert!(
            super::super::flat_action_v4::redeterminize_tinybones_v72(&mut old, p, 730006).is_err()
        );
        assert_eq!(old, before);
        let mut forged = d.clone();
        forged.binding.original.clear();
        assert!(o.sampled_clone_v73(&forged, 730007).is_err());
        let mut forged = before.clone();
        let E::LilianaDreadhordeV1{journal}=forged.engine.event_history.iter_mut().find(|e|matches!(e,E::LilianaDreadhordeV1{journal}if matches!(journal.as_ref(),crate::effect::liliana_dreadhorde_v1::JournalV1::Payment{..}))).unwrap()else{panic!()};
        let crate::effect::liliana_dreadhorde_v1::JournalV1::Payment { input } = journal.as_mut()
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
            super::super::flat_action_v4::redeterminize_liliana_v73(&mut decoded, p, 730008)
                .is_err()
        );
        assert_eq!(decoded, unchanged);
    }
}
#[test]
fn genuine_paid_liliana_death_observers_and_current_choice_scene_protect_hidden_later_incarnations_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, id) = sampler_ready(p, "Island");
        let mut o = owner(s);
        act(
            &mut o,
            |a| matches!(a,A::ActivateAbility{source,ability_index:1,..}if source.arena_id==id.0),
        );
        for _ in 0..128 {
            if o.decision_v73().unwrap().observation().liliana.activations[0].stage
                == crate::liliana_public_v73::ActivationStageV73::NativeCompleted
            {
                break;
            }
            quiet(&mut o);
        }
        let d = o.decision_v73().unwrap();
        assert!(d.observation().liliana.resolutions[0].finished);
        let died = d.observation().liliana.resolutions[0].completed_sacrifices[0].object;
        assert!(d
            .observation()
            .liliana
            .deaths
            .iter()
            .any(|r| r.creature.object == died));
        let mut world = o.fixture_state_v73().clone();
        let object = crate::ids::ObjectId(died);
        // Labelled native return-to-hidden primitive after a real simultaneous paid sacrifice.
        event::propose_and_commit(
            &mut world,
            ProposedEvent::zone_change(object, Zone::Library),
        );
        observation::validate_state(&world).unwrap();
        assert!(
            crate::effect::liliana_dreadhorde_v1::sampler_conflicts_v73(&world, &[object]).unwrap()
        );
        let o = owner(world);
        let d = o.decision_v73().unwrap();
        let before = o.fixture_state_v73().clone();
        assert!(o.sampled_clone_v73(&d, 730009).is_err());
        assert_eq!(o.fixture_state_v73(), &before);
    }
}
