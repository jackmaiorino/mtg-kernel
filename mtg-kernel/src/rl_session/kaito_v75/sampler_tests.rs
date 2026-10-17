//! Genuine paid current-slot sampling, not a posterior or playing-strength claim.
use super::*;
use crate::{
    effect::kaito_cunning_v75::tests::owner_v75_tests::owning_ready,
    event::{self, CommittedEvent as E, ProposedEvent},
    rl::ActionSemanticV1 as A,
    state::Zone,
};
fn owner(s: GameState) -> KaitoSessionV75 {
    KaitoSessionV75::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s)).unwrap()
}
fn sampled(o: &KaitoSessionV75, seed: u64) -> KaitoSessionV75 {
    let d = o.decision_v75().unwrap();
    let before = o.fixture_state_v75().clone();
    let mut c = o.sampled_clone_v75(&d, seed).unwrap();
    let again = o.sampled_clone_v75(&d, seed).unwrap();
    assert_eq!(c.fixture_state_v75(), again.fixture_state_v75());
    assert!(c.fresh_binding().unwrap() == d.binding);
    assert_eq!(
        c.fixture_state_v75().engine.event_history,
        before.engine.event_history
    );
    assert_eq!(
        c.fixture_state_v75().engine.event_log,
        before.engine.event_log
    );
    assert_eq!(c.fixture_state_v75().counter_lki_v1, before.counter_lki_v1);
    assert_eq!(o.fixture_state_v75(), &before);
    assert!(!std::sync::Arc::ptr_eq(&c.owner, &o.owner));
    assert!(c.validate_decision_v75(&d).is_err());
    let fresh = c.decision_v75().unwrap();
    assert!(o.validate_decision_v75(&fresh).is_err());
    let decoded: GameState =
        serde_json::from_slice(&serde_json::to_vec(c.fixture_state_v75()).unwrap()).unwrap();
    assert_eq!(&decoded, c.fixture_state_v75());
    observation::validate_state(&decoded).unwrap();
    let saved = c.snapshot_v75();
    c.restore_v75(&saved).unwrap();
    assert!(c.validate_decision_v75(&fresh).is_err());
    assert_eq!(
        c.decision_v75().unwrap().candidates_v75(),
        fresh.candidates_v75()
    );
    c
}
fn act(o: &mut KaitoSessionV75, choose: impl Fn(&A) -> bool) {
    let d = o.decision_v75().unwrap();
    let i = d.actions().iter().position(choose).unwrap();
    let original = d.fixture_original_index_v75(i as u32).unwrap();
    let mut native = o.fixture_native_clone_v75();
    assert_eq!(
        o.step_v75(&d, i as u32).unwrap(),
        native
            .step(d.expected().episode_id, d.expected().step, original)
            .unwrap()
    );
    assert_eq!(o.fixture_state_v75(), native.kaito_fixture_state());
    assert_eq!(
        o.fixture_counts_v75(),
        (native.policy_step_count(), native.physical_decision_count())
    );
}
fn quiet(o: &mut KaitoSessionV75) {
    let d = o.decision_v75().unwrap();
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
                    | A::ChooseTarget { .. }
                    | A::Discard { .. }
                    | A::ChooseAttackerInclusion { include: false, .. }
                    | A::ChooseBlockerInclusion { include: false, .. }
            )
        }
    });
}
fn complete_root(o: &mut KaitoSessionV75) {
    for _ in 0..128 {
        if o.decision_v75()
            .unwrap()
            .observation()
            .kaito
            .activations
            .last()
            .is_some_and(|a| a.stage == crate::kaito_public_v75::StageV75::Resolved)
        {
            return;
        }
        quiet(o);
    }
    panic!("genuine Kaito root did not complete");
}
fn settle(o: &mut KaitoSessionV75) {
    for _ in 0..256 {
        let s = o.fixture_state_v75();
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_discard.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            return;
        }
        quiet(o);
    }
    panic!("genuine Kaito native work did not settle");
}
fn refuses(o: &KaitoSessionV75, seed: u64) {
    let d = o.decision_v75().unwrap();
    let before = o.fixture_state_v75().clone();
    let counts = o.fixture_counts_v75();
    assert!(o.sampled_clone_v75(&d, seed).is_err());
    assert_eq!(o.fixture_state_v75(), &before);
    assert_eq!(o.fixture_counts_v75(), counts);
    o.validate_decision_v75(&d).unwrap();
}
#[test]
fn genuine_paid_kaito_zero_or_one_target_draw_discard_sampling_original_indices_and_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for selected in [false, true] {
            let (s, k, c) = owning_ready(p, false, "Island");
            let mut o = owner(s);
            let mut changed = false;
            for seed in 1..=8 {
                changed |=
                    sampled(&o, seed).fixture_state_v75().players != o.fixture_state_v75().players;
            }
            assert!(changed);
            act(
                &mut o,
                |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==k.0),
            );
            let c0 = sampled(&o, 750001);
            assert_eq!(
                c0.decision_v75().unwrap().candidates_v75(),
                o.decision_v75().unwrap().candidates_v75()
            );
            if selected {
                act(
                    &mut o,
                    |a| matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==c.0),
                );
            } else {
                act(&mut o, |a| matches!(a, A::FinishEffectSelection { .. }));
            }
            let mut waiting = false;
            for n in 0..64 {
                let d = o.decision_v75().unwrap();
                if d.observation().kaito.activations[0].stage
                    == crate::kaito_public_v75::StageV75::WaitingDiscard
                {
                    waiting = true;
                    break;
                }
                let _ = sampled(&o, 750100 + n);
                quiet(&mut o);
            }
            assert!(waiting);
            let mut o = sampled(&o, 750002);
            let d = o.decision_v75().unwrap();
            let a = &d.observation().kaito.activations[0];
            assert_eq!((a.loyalty_before, a.loyalty_after), (3, 4));
            assert_eq!(a.targets.len(), usize::from(selected));
            assert_eq!(a.evasion.as_ref().unwrap().active_on_original, selected);
            assert_eq!(a.draw, crate::kaito_public_v75::DrawV75::Drawn);
            assert!(a
                .discard
                .as_ref()
                .unwrap()
                .private_choices
                .as_ref()
                .is_some_and(|cards| cards.len() > 1));
            let other =
                crate::kaito_public_v75::capture(o.fixture_state_v75(), p.opponent()).unwrap();
            assert!(other.activations[0]
                .discard
                .as_ref()
                .unwrap()
                .private_choices
                .is_none());
            let saved = o.snapshot_v75();
            complete_root(&mut o);
            let expected = o.fixture_state_v75().clone();
            assert!(
                o.decision_v75().unwrap().observation().kaito.activations[0]
                    .discard
                    .as_ref()
                    .unwrap()
                    .answered
            );
            o.restore_v75(&saved).unwrap();
            assert!(o.step_v75(&d, 0).is_err());
            complete_root(&mut o);
            assert_eq!(o.fixture_state_v75(), &expected);
            let _ = sampled(&o, 750003);
        }
    }
}
#[test]
fn independently_paid_kaito_hidden_worlds_keep_full_numeric_packets_fresh_authority_and_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _, _) = owning_ready(p, false, "Island");
        let (b, _, _) = owning_ready(p, false, "Imprisoned in the Moon");
        let a = owner(a);
        let b = owner(b);
        assert_eq!(
            a.decision_v75().unwrap().candidates_v75(),
            b.decision_v75().unwrap().candidates_v75()
        );
        let a = crate::current_mixed_policy_v1::KaitoSession::from_kaito(sampled(&a, 750004));
        let b = crate::current_mixed_policy_v1::KaitoSession::from_kaito(sampled(&b, 750004));
        let FastActorResponseV1::Decision(h) = a.current_response() else {
            panic!()
        };
        let d = a.encode(h).unwrap();
        let FastActorResponseV1::Decision(h) = b.current_response() else {
            panic!()
        };
        assert_eq!(d.packet(), b.encode(h).unwrap().packet());
        let mut c = a.sampled_clone(&d, 750005).unwrap();
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
fn genuine_paid_kaito_ninja_batch_sampling_keeps_token_incarnations_cost_and_native_replay_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, k, _) = owning_ready(p, false, "Island");
        let mut o = owner(s);
        act(
            &mut o,
            |a| matches!(a,A::ActivateAbility{source,ability_index:1,..}if source.arena_id==k.0),
        );
        let published = sampled(&o, 750006);
        assert_eq!(
            published
                .decision_v75()
                .unwrap()
                .observation()
                .kaito
                .activations[0]
                .loyalty_after,
            1
        );
        let saved = o.snapshot_v75();
        complete_root(&mut o);
        let expected = o.fixture_state_v75().clone();
        let c = sampled(&o, 750007);
        let d = c.decision_v75().unwrap();
        assert_eq!(d.observation().kaito.ninja_batches.len(), 1);
        let ninja = &d.observation().kaito.ninja_batches[0].created;
        assert_eq!(ninja.len(), 1);
        assert!(matches!(
            ninja[0].original.identity,
            crate::optional_trigger_public_v1::IdentityV1::Known { card_id: 495 }
        ));
        assert_eq!(
            ninja[0].current_original.as_ref().unwrap().object,
            ninja[0].original.object
        );
        o.restore_v75(&saved).unwrap();
        complete_root(&mut o);
        assert_eq!(o.fixture_state_v75(), &expected);
    }
}
#[test]
fn genuine_paid_kaito_combat_emblem_and_original_cast_sampling_keep_native_roles_both_seats() {
    use crate::effect::kaito_cunning_v75::tests::owner_v75_tests::{
        combat_boundary, emblem_cast_boundary,
    };
    for p in [PlayerId::P0, PlayerId::P1] {
        for departing in [false, true] {
            // false is declared native combat; true is labelled simultaneous combat/departure primitive.
            let mut o = owner(combat_boundary(p, departing));
            let c = sampled(&o, 750008);
            let d = c.decision_v75().unwrap();
            assert_eq!(d.observation().kaito.combats.len(), 1);
            assert_eq!(d.observation().kaito.combats[0].final_damage, 2);
            assert_eq!(
                d.observation().kaito.triggers[0].origin,
                crate::kaito_public_v75::OriginV75::Combat(0)
            );
            let source = crate::ids::ObjectId(
                d.observation().kaito.combats[0]
                    .observer
                    .original_source
                    .object,
            );
            let creature =
                crate::ids::ObjectId(d.observation().kaito.combats[0].original_creature.object);
            for id in [source, creature] {
                let mut world = o.fixture_state_v75().clone();
                // Explicit native hidden return after the genuine original combat capture.
                event::propose_and_commit(
                    &mut world,
                    ProposedEvent::zone_change(id, Zone::Library),
                );
                observation::validate_state(&world).unwrap();
                assert!(
                    crate::effect::kaito_cunning_v75::sampler_conflicts_v75(&world, &[id]).unwrap()
                );
                refuses(&owner(world), 750009);
            }
            settle(&mut o);
            assert_eq!(
                o.decision_v75().unwrap().observation().kaito.triggers[0].stage,
                crate::kaito_public_v75::StageV75::Resolved
            );
            let _ = sampled(&o, 750010);
        }
        // Actual paid Kaito/Think Twice/Counterspell; ultimate counter setup and virtual spell copy are labelled native primitives.
        let mut o = owner(emblem_cast_boundary(p));
        let c = sampled(&o, 750011);
        let d = c.decision_v75().unwrap();
        assert_eq!(d.observation().kaito.emblems.len(), 1);
        assert_eq!(d.observation().kaito.casts.len(), 2);
        assert_eq!(d.observation().kaito.emblems[0].controller, p.into());
        let cast = crate::ids::ObjectId(d.observation().kaito.casts[0].original_spell.object);
        let before = o.snapshot_v75();
        settle(&mut o);
        let expected = o.fixture_state_v75().clone();
        assert_eq!(
            o.decision_v75()
                .unwrap()
                .observation()
                .kaito
                .ninja_batches
                .len(),
            2
        );
        let _ = sampled(&o, 750012);
        o.restore_v75(&before).unwrap();
        settle(&mut o);
        assert_eq!(o.fixture_state_v75(), &expected);
        let mut world = o.fixture_state_v75().clone();
        event::propose_and_commit(&mut world, ProposedEvent::zone_change(cast, Zone::Library));
        observation::validate_state(&world).unwrap();
        assert!(crate::effect::kaito_cunning_v75::sampler_conflicts_v75(&world, &[cast]).unwrap());
        refuses(&owner(world), 750013);
    }
}
#[test]
fn genuine_paid_kaito_original_discard_hand_hidden_returns_forged_journal_and_old_scope_refuse_readonly_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, k, _) = owning_ready(p, false, "Island");
        let mut o = owner(s);
        act(
            &mut o,
            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==k.0),
        );
        act(&mut o, |a| matches!(a, A::FinishEffectSelection { .. }));
        for _ in 0..64 {
            if o.decision_v75().unwrap().observation().kaito.activations[0].stage
                == crate::kaito_public_v75::StageV75::WaitingDiscard
            {
                break;
            }
            quiet(&mut o);
        }
        let original_hand = o.fixture_state_v75().players[p.index()].hand.clone();
        assert!(original_hand.len() > 1);
        assert_eq!(
            o.decision_v75().unwrap().observation().kaito.activations[0].stage,
            crate::kaito_public_v75::StageV75::WaitingDiscard
        );
        complete_root(&mut o);
        let hidden = *original_hand
            .iter()
            .find(|id| o.fixture_state_v75().objects.get(**id).zone == Zone::Hand)
            .unwrap();
        for id in [k, hidden] {
            let mut world = o.fixture_state_v75().clone();
            // Original private hand alternatives remain factual, not a future-library placement constraint.
            event::propose_and_commit(&mut world, ProposedEvent::zone_change(id, Zone::Library));
            observation::validate_state(&world).unwrap();
            assert!(
                crate::effect::kaito_cunning_v75::sampler_conflicts_v75(&world, &[id]).unwrap()
            );
            refuses(&owner(world), 750014);
        }
        let d = o.decision_v75().unwrap();
        let before = o.fixture_state_v75().clone();
        let mut foreign = d.clone();
        foreign.binding.public_to_original.clear();
        assert!(o.sampled_clone_v75(&foreign, 750015).is_err());
        assert_eq!(o.fixture_state_v75(), &before);
        let mut old = before.clone();
        assert!(
            super::super::flat_action_v4::redeterminize_chandra_v74(&mut old, p, 750016).is_err()
        );
        assert_eq!(old, before);
        let mut forged = before.clone();
        let E::KaitoCunningV75{journal}=forged.engine.event_history.iter_mut().find(|e|matches!(e,E::KaitoCunningV75{journal}if matches!(journal.as_ref(),crate::effect::kaito_cunning_v75::JournalV75::Payment{..}))).unwrap()else{panic!()};
        let crate::effect::kaito_cunning_v75::JournalV75::Payment { input } = journal.as_mut()
        else {
            panic!()
        };
        let mut value = serde_json::to_value(input.as_ref()).unwrap();
        value["source"]["zone_change_count"] = u32::MAX.into();
        *input = Box::new(serde_json::from_value(value).unwrap());
        let mut decoded: GameState =
            serde_json::from_slice(&serde_json::to_vec(&forged).unwrap()).unwrap();
        let unchanged = decoded.clone();
        assert!(
            super::super::flat_action_v4::redeterminize_kaito_v75(&mut decoded, p, 750017).is_err()
        );
        assert_eq!(decoded, unchanged);
        let mut log = before.clone();
        log.engine.event_log.push(before.engine.event_history.iter().find(|e|matches!(e,E::KaitoCunningV75{journal}if matches!(journal.as_ref(),crate::effect::kaito_cunning_v75::JournalV75::Payment{..}))).unwrap().clone());
        let unchanged = log.clone();
        assert!(
            super::super::flat_action_v4::redeterminize_kaito_v75(&mut log, p, 750018).is_err()
        );
        assert_eq!(log, unchanged);
    }
}
#[test]
fn genuine_paid_kaito_and_inherited_crystal_whole_world_boundaries_keep_conservative_refusal_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _, _) = owning_ready(p, true, "Island");
        // Season's exact original-world journals are still outside this current-slot sampler.
        refuses(&owner(s), 750019);
        // Existing actual paid Chandra/Tyrant replacement-choice fixture, not Kaito damage.
        let s =
            crate::effect::chandra_flameshaper_v1::tests::owner_v74_tests::pending_damage_boundary(
                p,
            );
        refuses(&owner(s), 750020);
    }
}
