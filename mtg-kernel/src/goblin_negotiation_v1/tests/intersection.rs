//! Primitive dual-type boards, followed by actual paid Goblin announcements.
//! No registered printing currently animates a planeswalker this way. Kaito
//! becomes only a creature and is deliberately not used as an intersection.
use super::*;
use crate::goblin_negotiation_v1 as native;

fn dual_primitive(
    s: &mut GameState,
    controller: PlayerId,
    toughness: i16,
    loyalty: u32,
) -> ObjectId {
    let id = put(s, controller, "Ajani, Caller of the Pride", Zone::Hand);
    // Native entry primitive, not a paid Ajani cast. The raw fixture insertion
    // alone does not create the separate incarnation-bound loyalty ledger.
    assert_eq!(crate::planeswalker_v1::loyalty(s, id), None);
    event::propose_and_commit(s, ProposedEvent::zone_change(id, Zone::Battlefield));
    assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
    assert_eq!(s.objects.get(id).zone, Zone::Battlefield);
    let old = crate::planeswalker_v1::loyalty(s, id).unwrap();
    assert_eq!(
        Some(old),
        card_def::CARD_DEFS[s.objects.get(id).card_def as usize]
            .starting_loyalty
            .map(u32::from)
    );
    if loyalty >= old {
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(s, id, loyalty - old).unwrap();
    } else {
        crate::planeswalker_v1::change_loyalty(s, id, -((old - loyalty) as i32));
    }
    // Explicit existing native state primitive, not a printed animation action.
    let timestamp = engine::next_timestamp(s);
    s.objects
        .get_mut(id)
        .v4
        .creature_upgrade
        .get_or_insert_with(Default::default)
        .temporary_creature = Some((1, toughness, timestamp));
    assert!(engine::object_has_type(s, id, CardType::Creature));
    assert!(engine::object_has_type(s, id, CardType::Planeswalker));
    assert_eq!(crate::planeswalker_v1::loyalty(s, id), Some(loyalty));
    id
}

#[test]
fn primitive_dual_type_paid_goblin_both_threshold_orders_zero_and_unsigned_loyalty_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (toughness, loyalty, x, excess) in [
            (4, 1, 3, 2),
            (2, 5, 4, 2),
            (3, 3, 3, 0),
            (4, 2, 0, 0),
            (4, u32::MAX, 5, 1),
        ] {
            let mut s = ready(actor);
            let target = dual_primitive(&mut s, actor.opponent(), toughness, loyalty);
            let spell = announce(&mut s, actor, target, x);
            let facts = crate::next_five_public_v59::capture(&s, actor).unwrap();
            let q = &facts.goblin_quotes[0];
            assert_eq!(q.creature_lethal_damage_now, Some(i64::from(toughness)));
            let input = q.intersection.as_ref().unwrap();
            assert_eq!(input.predamage_loyalty, loyalty);
            assert_eq!(
                input.effective_lethal_damage,
                i64::from(toughness).min(i64::from(loyalty))
            );
            let mut forged = facts.clone();
            forged.goblin_quotes[0]
                .intersection
                .as_mut()
                .unwrap()
                .predamage_loyalty ^= 1;
            let before = serde_json::to_vec(&s).unwrap();
            assert!(crate::next_five_public_v59::validate(&s, actor, &forged).is_err());
            assert_eq!(serde_json::to_vec(&s).unwrap(), before);
            let mut replay = restore(&s);
            settle(&mut s);
            settle(&mut replay);
            assert_eq!(
                serde_json::to_vec(&s).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            assert_eq!(damage(&s, spell, target), i32::from(x));
            assert_eq!(goblins(&s, actor).len(), excess);
            let completed = native::public_completed(&s).unwrap();
            assert_eq!(completed[0].excess as usize, excess);
            assert_eq!(completed[0].intersection, q.intersection);
            if x == 0 {
                assert_eq!(crate::planeswalker_v1::loyalty(&s, target), Some(loyalty));
                assert_eq!(s.objects.get(target).damage, 0);
            } else {
                assert_eq!(s.objects.get(target).zone, Zone::Graveyard);
            }
        }
    }
}

#[test]
fn primitive_dual_type_ordered_damage_season_pending_and_after_damage_restore_tamper() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for prevent in [false, true] {
            let mut s = ready(actor);
            let target = dual_primitive(&mut s, actor.opponent(), 4, 2);
            let tyrant = put(&mut s, actor, "Twinflame Tyrant", Zone::Battlefield);
            put(&mut s, actor, "Doubling Season", Zone::Battlefield);
            if prevent {
                put(
                    &mut s,
                    actor.opponent(),
                    "Crystal Barricade",
                    Zone::Battlefield,
                );
            }
            shield_primitive(&mut s, tyrant, target, 1);
            let spell = announce(&mut s, actor, target, 3);
            wait_ordering(&mut s, actor.opponent());
            for tamper in 0..6 {
                let mut forged = restore(&s);
                match tamper {
                    0 => crate::planeswalker_v1::change_loyalty(&mut forged, target, 1),
                    1 => {
                        for event in &mut forged.engine.event_history {
                            if let CommittedEvent::CrystalExecutionV1 { execution } = event {
                                for step in &mut execution.steps {
                                    if let Some(c) = &mut step.excess_capture {
                                        c.intersection.as_mut().unwrap().predamage_loyalty += 1;
                                    }
                                }
                            }
                        }
                    }
                    2 => forged.objects.get_mut(target).controller = actor,
                    3 => forged.objects.get_mut(target).zone_change_count += 1,
                    4 => {
                        forged
                            .objects
                            .get_mut(target)
                            .v4
                            .creature_upgrade
                            .as_mut()
                            .unwrap()
                            .temporary_creature = None
                    }
                    _ => {
                        for event in &mut forged.engine.event_history {
                            if let CommittedEvent::CrystalExecutionV1 { execution } = event {
                                for step in &mut execution.steps {
                                    if let Some(c) = &mut step.excess_capture {
                                        c.intersection = None;
                                    }
                                }
                            }
                        }
                    }
                }
                let before = serde_json::to_vec(&forged).unwrap();
                assert!(native::public_ordering_stage(&forged).is_err());
                assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
            }
            let mut first = Some(if prevent { "crystal" } else { "numeric" });
            loop {
                let mut replay = restore(&s);
                let slot = first.take().map_or(0, |kind| choice_slot(&s, kind));
                engine::step(&mut s, Action::ChooseEffectOption(slot)).unwrap();
                engine::step(&mut replay, Action::ChooseEffectOption(slot)).unwrap();
                assert_eq!(s.state_hash(), replay.state_hash());
                if s.engine.pending_effect.as_ref().unwrap().choice.is_none() {
                    let amount = if prevent { 0 } else { 4 };
                    assert_eq!(damage(&s, spell, target), amount);
                    assert_eq!(s.objects.get(target).damage, amount as u32);
                    assert_eq!(
                        crate::planeswalker_v1::loyalty(&s, target),
                        Some(2u32.saturating_sub(amount as u32))
                    );
                    assert!(goblins(&s, actor).is_empty());
                    let mut after_damage = restore(&s);
                    settle(&mut s);
                    settle(&mut replay);
                    settle(&mut after_damage);
                    assert_eq!(s.state_hash(), replay.state_hash());
                    assert_eq!(s.state_hash(), after_damage.state_hash());
                    assert_eq!(goblins(&s, actor).len(), if prevent { 0 } else { 4 });
                    assert_eq!(
                        native::public_completed(&s).unwrap()[0].excess,
                        if prevent { 0 } else { 2 }
                    );
                    break;
                }
                assert!(matches!(next(&mut s), Decision::ChooseEffectOption { .. }));
            }
        }
    }
}

#[test]
fn primitive_dual_type_marked_damage_deathtouch_and_zero_loyalty_thresholds() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let source = put(&mut s, actor, "Vampire Nighthawk", Zone::Battlefield);
        let target = dual_primitive(&mut s, actor.opponent(), 4, 3);
        // Native damage primitive, not a paid spell or an artificial printing.
        event::propose_and_commit(
            &mut s,
            ProposedEvent::damage(source, Target::Object(target), 1),
        );
        assert_eq!(s.objects.get(target).damage, 1);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, target), Some(2));
        assert!(engine::has_effective_keyword(
            &s,
            source,
            Keywords::DEATHTOUCH
        ));
        assert_eq!(native::creature_lethal_threshold(&s, source, target), 1);
        assert_eq!(native::lethal_threshold(&s, source, target).unwrap(), 1);
        crate::planeswalker_v1::change_loyalty(&mut s, target, -2);
        assert_eq!(native::lethal_threshold(&s, source, target).unwrap(), 0);
    }
}

#[test]
fn ordinary_capture_absent_intersection_retains_original_json_and_hash_sequence() {
    use std::hash::{Hash, Hasher};
    #[derive(serde::Serialize, Hash)]
    struct OriginalCapture<'a> {
        previous: u32,
        history_start: u32,
        ctx: &'a crate::effect::ExecCtx,
        op: &'a crate::effect::EffectOp,
        path: &'a Vec<u16>,
        source: &'a crate::state::GameObject,
        target: &'a crate::state::GameObject,
        toughness: i32,
        deathtouch: bool,
        lethal: i64,
        replacement: &'a crate::doubling_season_v48::ReplacementInputV48,
    }
    let actor = PlayerId::P0;
    let mut s = ready(actor);
    let tyrant = put(&mut s, actor, "Twinflame Tyrant", Zone::Battlefield);
    let target = put(
        &mut s,
        actor.opponent(),
        "Axgard Cavalry",
        Zone::Battlefield,
    );
    shield_primitive(&mut s, tyrant, target, 1);
    announce(&mut s, actor, target, 3);
    wait_ordering(&mut s, actor.opponent());
    let c = s
        .engine
        .event_history
        .iter()
        .find_map(|event| match event {
            CommittedEvent::CrystalExecutionV1 { execution } => execution
                .steps
                .iter()
                .find_map(|step| step.excess_capture.as_ref()),
            _ => None,
        })
        .unwrap();
    assert!(c.intersection.is_none());
    let old = OriginalCapture {
        previous: c.previous,
        history_start: c.history_start,
        ctx: &c.ctx,
        op: &c.op,
        path: &c.path,
        source: &c.source,
        target: &c.target,
        toughness: c.toughness,
        deathtouch: c.deathtouch,
        lethal: c.lethal,
        replacement: &c.replacement,
    };
    assert_eq!(
        serde_json::to_vec(c).unwrap(),
        serde_json::to_vec(&old).unwrap()
    );
    let mut old_hash = std::collections::hash_map::DefaultHasher::new();
    old.hash(&mut old_hash);
    let mut current = std::collections::hash_map::DefaultHasher::new();
    c.hash(&mut current);
    assert_eq!(current.finish(), old_hash.finish());
    let public = native::public_ordering_stage(&s).unwrap().unwrap();
    assert!(serde_json::to_value(public)
        .unwrap()
        .get("intersection")
        .is_none());
}
