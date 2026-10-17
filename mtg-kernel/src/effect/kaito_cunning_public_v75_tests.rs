//! Genuine paid Kaito inputs. Native counter/zone/combat/copy primitives are labelled.
use super::tests::{
    activate, combat_trigger, discard_prompt, finish, next_own_main, ready, ultimate, walker,
};
use super::*;
use crate::{
    engine::time_stop_v1::tests::{cast, next, pass, restore},
    engine::{self, Action},
    kaito_public_v75 as facts,
    policy_observation_v6::tests::put,
};
fn capture(s: &GameState, p: PlayerId) -> facts::FactsV75 {
    let before = s.clone();
    let f = facts::capture(s, p).unwrap();
    assert_eq!(*s, before);
    facts::validate_current(s, p, &f).unwrap();
    let restored: facts::FactsV75 =
        serde_json::from_slice(&serde_json::to_vec(&f).unwrap()).unwrap();
    assert_eq!(restored, f);
    assert_eq!(facts::capture(&restore(s), p).unwrap(), f);
    f
}
#[test]
fn genuine_paid_optional_target_draw_private_discard_full_counters_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for selected in [false, true] {
            let mut s = ready(p);
            put(&mut s, p, "Forest", Zone::Hand);
            put(&mut s, p, "Island", Zone::Hand);
            let k = walker(&mut s, p);
            let c = cast(&mut s, p, "Axgard Cavalry", &[], 2);
            finish(&mut s);
            // Native storage operands exercise all eighteen exact-width counter fields.
            let o = s.objects.get_mut(c);
            o.counters.plus1_plus1 = 3;
            o.counters.minus1_minus1 = 1;
            o.counters.minus0_minus1 = 1;
            o.counters.stun = 2;
            o.counters.lore = 3;
            o.counters.oil = 4;
            o.counters.charge = i32::MAX;
            o.counters.net = 5;
            o.counters.bait = 6;
            o.counters.soul = 7;
            o.counters.incubation = 8;
            o.counters.revival = 9;
            o.counters.fellowship = 10;
            o.counters.stash = 11;
            o.v4.lifelink_keyword_counters = 12;
            o.v4.time_counters_v1 = u8::MAX;
            // Native storage and checked placement supply finality and full-width loyalty.
            s.objects
                .get_mut(c)
                .v4
                .creature_upgrade
                .get_or_insert_with(Default::default)
                .finality = u16::MAX;
            crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, k, u32::MAX - 4)
                .unwrap();
            let row = activate(&mut s, p, k, 0, selected.then_some(c));
            let initial = capture(&s, p);
            assert_eq!(initial.activations[0].loyalty_after, u32::MAX);
            let b = initial.battlefield[p.index()]
                .iter()
                .find(|x| x.card.object == c.0)
                .unwrap();
            assert_eq!(
                serde_json::to_value(&b.counters)
                    .unwrap()
                    .as_object()
                    .unwrap()
                    .len(),
                18
            );
            assert_eq!(b.counters.charge, i32::MAX);
            assert_eq!(b.counters.stash, 11);
            assert_eq!(b.counters.time, u8::MAX);
            assert_eq!(b.counters.finality, u16::MAX);
            assert_eq!(initial.activations[0].targets.len(), usize::from(selected));
            assert_eq!(initial.activations[0].stage, facts::StageV75::Published);
            let choices = discard_prompt(&mut s, p);
            let own = capture(&s, p);
            let other = capture(&s, p.opponent());
            assert_eq!(own.activations[0].stage, facts::StageV75::WaitingDiscard);
            assert_eq!(own.activations[0].draw, facts::DrawV75::Drawn);
            assert_eq!(
                own.activations[0]
                    .discard
                    .as_ref()
                    .unwrap()
                    .private_choices
                    .as_ref()
                    .unwrap()
                    .len(),
                choices.len()
            );
            assert!(other.activations[0]
                .discard
                .as_ref()
                .unwrap()
                .private_choices
                .is_none());
            assert!(other.activations[0]
                .discard
                .as_ref()
                .unwrap()
                .private_chosen
                .is_none());
            let mut replay = restore(&s);
            for state in [&mut s, &mut replay] {
                engine::step(state, Action::Discard(vec![choices[0]])).unwrap();
                finish(state);
            }
            assert_eq!(s, replay);
            let done = capture(&s, p);
            assert_eq!(done.activations[0].stage, facts::StageV75::Resolved);
            assert!(done.activations[0].discard.as_ref().unwrap().answered);
            assert_eq!(
                done.activations[0]
                    .discard
                    .as_ref()
                    .unwrap()
                    .private_chosen
                    .as_ref()
                    .unwrap()[0]
                    .object,
                choices[0].0
            );
            assert!(capture(&s, p.opponent()).activations[0]
                .discard
                .as_ref()
                .unwrap()
                .private_chosen
                .is_none());
            assert!(finished(&s, &row));
            let before = s.clone();
            let mut bad = done.clone();
            bad.activations.clear();
            assert!(facts::validate_current(&s, p, &bad).is_err());
            let mut bad = done.clone();
            bad.activations[0].source.original_source.generation += 1;
            assert!(facts::validate_current(&s, p, &bad).is_err());
            assert_eq!(s, before);
            let mut raw = serde_json::to_value(&done).unwrap();
            raw["battlefield"][p.index()][0]["counters"]["loyalty"] =
                serde_json::json!(4294967296u64);
            assert!(serde_json::from_value::<facts::FactsV75>(raw).is_err());
        }
    }
}
#[test]
fn genuine_paid_departed_plus_one_and_reentered_target_keep_original_roles_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        put(&mut s, p, "Forest", Zone::Hand);
        put(&mut s, p, "Island", Zone::Hand);
        let k = walker(&mut s, p);
        let c = cast(&mut s, p, "Axgard Cavalry", &[], 2);
        finish(&mut s);
        activate(&mut s, p, k, 0, Some(c));
        // Native source departure during the actual response window.
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(k, Zone::Hand));
        let choices = discard_prompt(&mut s, p);
        let f = capture(&s, p.opponent());
        assert!(f.activations[0].source.current_original_source.is_none());
        assert_eq!(f.activations[0].source.captured_controller, p.into());
        assert!(
            f.activations[0]
                .evasion
                .as_ref()
                .unwrap()
                .active_on_original
        );
        engine::step(
            &mut s,
            Action::Discard(vec![*choices.iter().find(|x| **x != k).unwrap()]),
        )
        .unwrap();
        finish(&mut s);
        // The rule effect applies only to its original target incarnation and expires.
        let mut replay = restore(&s);
        next_own_main(&mut s, p);
        next_own_main(&mut replay, p);
        assert_eq!(s, replay);
        assert!(
            !capture(&s, p).activations[0]
                .evasion
                .as_ref()
                .unwrap()
                .active_on_original
        );
        let k = walker(&mut s, p);
        activate(&mut s, p, k, 0, Some(c));
        let old = s.objects.get(c).zone_change_count;
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(c, Zone::Hand));
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(c, Zone::Battlefield),
        );
        finish(&mut s);
        let f = capture(&s, p);
        let a = f.activations.last().unwrap();
        assert_eq!(a.stage, facts::StageV75::Fizzled);
        assert_eq!(a.targets[0].original.generation, old);
        assert!(a.targets[0].current_original.is_none());
        assert!(!a.targets[0].legal_now);
        assert_eq!(a.draw, facts::DrawV75::NotReached);
    }
}
#[test]
fn genuine_paid_declared_and_source_departing_combat_have_only_bound_native_occurrences_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        for depart in [false, true] {
            let mut s = ready(p);
            let k = walker(&mut s, p);
            let c = cast(&mut s, p, "Axgard Cavalry", &[], 2);
            finish(&mut s);
            if depart {
                // Native simultaneous final-combat-wave primitive, not invented paid combat.
                let mut d = event::ProposedEvent::damage(c, Target::Player(p.opponent()), 2);
                if let event::ProposedEvent::Damage(d) = &mut d {
                    d.is_combat = true;
                }
                engine::commit_combat_damage_events(
                    &mut s,
                    vec![d, event::ProposedEvent::zone_change(k, Zone::Graveyard)],
                );
                let held = capture(&s, p);
                assert_eq!(held.combats.len(), 1);
                assert!(held.combats[0].observer.current_original_source.is_none());
                assert_eq!(held.triggers[0].stage, facts::StageV75::Captured);
                for _ in 0..32 {
                    let d = next(&mut s);
                    if s.stack.iter().any(|r| {
                        matches!(
                            r.inline_effect,
                            Some(EffectOp::KaitoCunningV75 { kind: 3, .. })
                        )
                    }) {
                        break;
                    }
                    pass(&mut s, d);
                }
            } else {
                combat_trigger(&mut s, p, c);
            }
            let f = capture(&s, p);
            assert_eq!(f.combats.len(), 1);
            assert_eq!(f.combats[0].final_damage, 2);
            assert_eq!(f.combats[0].original_creature.object, c.0);
            assert_eq!(f.combats[0].original_creature.controller, p.into());
            assert_eq!(f.triggers[0].origin, facts::OriginV75::Combat(0));
            assert_eq!(f.triggers[0].stage, facts::StageV75::Published);
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(capture(&s, p).triggers[0].stage, facts::StageV75::Resolved);
            let original=s.engine.event_history.iter().find(|e|matches!(e,E::KaitoCunningV75{journal}if matches!(journal.as_ref(),JournalV75::Combat{..}))).unwrap().clone();
            let mut bad = restore(&s);
            bad.engine.event_log.push(original);
            let before = bad.clone();
            assert!(facts::capture(&bad, p).is_err());
            assert_eq!(bad, before);
        }
    }
}
#[test]
fn genuine_paid_emblems_any_caster_countering_virtual_copy_and_exact_ninja_batches_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = walker(&mut s, p);
        ultimate(&mut s, p, k);
        let k2 = walker(&mut s, p);
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, k2, 6).unwrap();
        activate(&mut s, p, k2, 2, None);
        finish(&mut s);
        let baseline = capture(&s, p);
        assert_eq!(baseline.emblems.len(), 2);
        let old_casts = baseline.casts.len();
        let old_batches = baseline.ninja_batches.len();
        let spell = cast(&mut s, p.opponent(), "Think Twice", &[], 2);
        let original = capture(&s, p);
        assert_eq!(original.casts.len(), old_casts + 2);
        assert_eq!(
            original.casts[old_casts].actual_cast,
            original.casts[old_casts + 1].actual_cast
        );
        assert_eq!(
            original.casts[old_casts].original_spell.controller,
            p.opponent().into()
        );
        assert_ne!(
            original.casts[old_casts].emblem,
            original.casts[old_casts + 1].emblem
        );
        // Genuine engine virtual-copy primitive, not a fabricated paid copy spell.
        engine::copy_spell_for(&mut s, spell, p.opponent()).unwrap();
        assert_eq!(capture(&s, p).casts.len(), old_casts + 2);
        cast(&mut s, p, "Counterspell", &[Target::Object(spell)], 2);
        let f = capture(&s, p);
        assert_eq!(f.casts.len(), old_casts + 4);
        assert_eq!(f.emblems.len(), 2);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        let f = capture(&s, p);
        assert_eq!(f.ninja_batches.len(), old_batches + 4);
        assert!(f
            .ninja_batches
            .iter()
            .all(|b| b.controller == p.into() && b.created.len() == 1));
        assert!(f
            .triggers
            .iter()
            .all(|t| t.stage == facts::StageV75::Resolved));
        assert_eq!(f.casts.len(), old_casts + 4);
        let before = s.clone();
        let mut erased = f.clone();
        erased.ninja_batches.clear();
        assert!(facts::validate_current(&s, p, &erased).is_err());
        assert_eq!(s, before);
    }
}
#[test]
fn genuine_paid_ninja_season_and_full_private_world_neutrality_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut worlds = [
            super::super::vivien_reid_v1::tests::ready(p, &["Forest"; 64], &[], "Forest"),
            super::super::vivien_reid_v1::tests::ready(p, &["Forest"; 64], &[], "Island"),
        ];
        let mut outputs = vec![];
        for s in &mut worlds {
            cast(s, p, "Doubling Season", &[], 5);
            finish(s);
            let k = cast(s, p, "Kaito, Cunning Infiltrator", &[], 3);
            finish(s);
            assert_eq!(crate::planeswalker_v1::loyalty(s, k), Some(6));
            activate(s, p, k, 1, None);
            finish(s);
            let f = capture(s, p);
            assert_eq!(f.ninja_batches.len(), 1);
            assert_eq!(f.ninja_batches[0].created.len(), 2);
            assert_eq!(f.meaning.printed.starting_loyalty, 3);
            assert!(!f.owning_consumer_admitted);
            assert!(require_legacy(s, Some(p)).is_err());
            outputs.push(f);
        }
        assert_eq!(outputs[0], outputs[1]);
    }
}
#[test]
fn genuine_paid_failed_draw_and_countered_ninja_have_distinct_actual_terminal_facts_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = super::super::vivien_reid_v1::tests::ready(p, &[], &[], "Island");
        let k = walker(&mut s, p);
        activate(&mut s, p, k, 0, None);
        finish(&mut s);
        let f = capture(&s, p);
        assert_eq!(f.activations[0].draw, facts::DrawV75::EmptyLibrary);
        assert!(f.activations[0].discard.is_none());
        assert_eq!(f.activations[0].stage, facts::StageV75::Resolved);
        assert!(s.players[p.index()].drew_from_empty);
        let mut s = ready(p);
        let k = walker(&mut s, p);
        let row = activate(&mut s, p, k, 1, None);
        // Native stack countering primitive, not an invented paid ability-counter spell.
        assert!(
            engine::counter_stack_item_by_id(&mut s, row.v4.stack_item_id)
                .unwrap()
                .is_some()
        );
        let f = capture(&s, p);
        assert_eq!(f.activations[0].stage, facts::StageV75::Countered);
        assert!(f.ninja_batches.is_empty());
        assert_eq!(f.activations[0].loyalty_after, 1);
        let mut forged = f.clone();
        forged.activations[0].stage = facts::StageV75::Resolved;
        assert!(facts::validate(&forged).is_err());
    }
}
#[test]
fn genuine_paid_prior_public_source_then_neutral_face_down_current_form_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = walker(&mut s, p);
        activate(&mut s, p, k, 1, None);
        finish(&mut s);
        // Native face-down projection primitive, not a paid disguise route for Kaito.
        s.objects.get_mut(k).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        let f = capture(&s, p.opponent());
        assert!(f.authorized_printings.iter().all(|r| r.object != k.0));
        assert!(matches!(
            f.activations[0].source.original_source.identity,
            crate::optional_trigger_public_v1::IdentityV1::Known { card_id: 494 }
        ));
        assert!(matches!(
            f.activations[0]
                .source
                .current_original_source
                .as_ref()
                .unwrap()
                .identity,
            crate::optional_trigger_public_v1::IdentityV1::FaceDown { .. }
        ));
        let c = f.battlefield[p.index()]
            .iter()
            .find(|c| c.card.object == k.0)
            .unwrap();
        assert!(!c.form.names.iter().any(|n| n.contains("Kaito")));
        assert!(!c
            .form
            .subtype_ids
            .contains(&(crate::card_def::Subtype::Kaito as u16)));
    }
}
#[test]
fn genuine_paid_independent_opposing_emblem_controllers_share_cast_not_ninja_ownership_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = walker(&mut s, p);
        ultimate(&mut s, p, k);
        next_own_main(&mut s, p.opponent());
        let second = walker(&mut s, p.opponent());
        ultimate(&mut s, p.opponent(), second);
        let baseline = capture(&s, p);
        assert_eq!(
            baseline
                .emblems
                .iter()
                .map(|e| e.controller)
                .collect::<Vec<_>>(),
            vec![p.into(), p.opponent().into()]
        );
        cast(&mut s, p.opponent(), "Think Twice", &[], 2);
        let f = capture(&s, p);
        let new = &f.casts[baseline.casts.len()..];
        assert_eq!(new.len(), 2);
        assert_eq!(new[0].actual_cast, new[1].actual_cast);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        let f = capture(&s, p);
        let batches = &f.ninja_batches[baseline.ninja_batches.len()..];
        assert_eq!(batches.len(), 2);
        assert!(batches.iter().any(|b| b.controller == p.into()));
        assert!(batches.iter().any(|b| b.controller == p.opponent().into()));
        assert!(batches.iter().all(|b| b.created.len() == 1));
    }
}
