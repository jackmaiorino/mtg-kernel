//! Genuine paid native Greatwurm fixtures. Native counter/control primitives are
//! labelled explicitly and never described as paid spell coverage.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
    state::{StackItemKind, Step},
};
const NAME: &str = "Quilled Greatwurm";
fn id(n: &str) -> u16 {
    card_def::card_id_by_name(n).unwrap()
}
fn ready(p: PlayerId) -> GameState {
    ready_hidden(p, "Forest")
}
fn ready_hidden(p: PlayerId, hidden: &str) -> GameState {
    let library = [id("Forest"); 80];
    let other = [id(hidden); 80];
    let (a, b) = if p == PlayerId::P0 {
        (&library, &other)
    } else {
        (&other, &library)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_476,
        p,
    );
    s.step = Step::Main1;
    for player in [p, p.opponent()] {
        for n in ["Island", "Plains", "Mountain", "Forest"] {
            for _ in 0..8 {
                put(&mut s, player, n, Zone::Battlefield);
            }
        }
    }
    s
}
fn restore(s: &GameState) -> GameState {
    let r: GameState = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(*s, r);
    assert_eq!(s.state_hash(), r.state_hash());
    r
}
fn next(s: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(s);
    assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn pass(s: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        d => panic!("Greatwurm native decision {d:?}"),
    };
    engine::step(s, a).unwrap();
}
fn priority(s: &mut GameState, p: PlayerId) {
    for _ in 0..32 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==p) {
            return;
        }
        pass(s, d)
    }
    panic!("priority absent")
}
fn tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|o| s.objects.get(**o).tapped)
        .count()
}
fn cast(s: &mut GameState, p: PlayerId, n: &str, targets: &[Target], cost: usize) -> ObjectId {
    priority(s, p);
    let card = put(s, p, n, Zone::Hand);
    let before = tapped(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&card))
    );
    engine::step(s, Action::CastSpell(card)).unwrap();
    let mut chosen = 0;
    for _ in 0..16 {
        match next(s) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, p);
                assert!(legal_targets.contains(&targets[chosen]));
                engine::step(s, Action::ChooseTarget(targets[chosen])).unwrap();
                chosen += 1;
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert_eq!(chosen, targets.len());
                assert!(s.engine.pending_cast.is_none());
                assert_eq!(tapped(s, p) - before, cost);
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                assert!(s
                    .stack
                    .iter()
                    .any(|r| r.kind == StackItemKind::Spell && r.source == card));
                return card;
            }
            d => panic!("paid cast {d:?}"),
        }
    }
    panic!("paid cast absent")
}
fn finish(s: &mut GameState) {
    for _ in 0..48 {
        let d = next(s);
        if s.stack.is_empty() && s.engine.pending_effect.is_none() {
            return;
        }
        pass(s, d)
    }
    panic!("stack unfinished")
}

fn paid(s: &mut GameState, p: PlayerId) -> ObjectId {
    let card = cast(s, p, NAME, &[], 6);
    finish(s);
    assert_eq!(s.objects.get(card).zone, Zone::Battlefield);
    assert_eq!(engine::checked_effective_pt(s, card).unwrap(), (7, 7));
    card
}
fn graveyard(s: &mut GameState, card: ObjectId) {
    // Native departure setup, not an additional paid removal spell.
    event::propose_and_commit(s, event::ProposedEvent::zone_change(card, Zone::Graveyard));
    assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
}
fn begin_graveyard(s: &mut GameState, p: PlayerId, card: ObjectId) {
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&card))
    );
    engine::step(s, Action::CastSpell(card)).unwrap();
    assert!(
        matches!(next(s),Decision::ChooseEffectOption{player,source,..}if player==p && source==card)
    );
}
fn pick_unit(s: &mut GameState, p: PlayerId, permanent: ObjectId, kind: CounterKindV1) {
    let prefix = s
        .engine
        .pending_cast
        .as_ref()
        .unwrap()
        .greatwurm_v1
        .as_ref()
        .unwrap()
        .units
        .clone();
    let menu = units(s, p, &prefix).unwrap();
    let index = menu
        .iter()
        .position(|u| u.permanent.object == permanent && u.kind == kind)
        .unwrap();
    let Decision::ChooseEffectOption {
        player,
        option_count,
        ..
    } = next(s)
    else {
        panic!("counter choice")
    };
    assert_eq!(player, p);
    assert_eq!(usize::from(option_count), menu.len());
    let before = s.clone();
    let mut replay = restore(s);
    engine::step(s, Action::ChooseEffectOption(u16::try_from(index).unwrap())).unwrap();
    engine::step(
        &mut replay,
        Action::ChooseEffectOption(u16::try_from(index).unwrap()),
    )
    .unwrap();
    assert_eq!(*s, replay);
    assert_eq!(
        s.objects.get(permanent).counters,
        before.objects.get(permanent).counters,
        "selection never pays"
    );
}
fn publish_graveyard(s: &mut GameState, p: PlayerId, card: ObjectId, before: usize) {
    assert!(matches!(next(s),Decision::CastSpellOrPass{player,..}if player==p));
    assert!(s.engine.pending_cast.is_none());
    assert_eq!(tapped(s, p) - before, 6);
    let item = s
        .stack
        .iter()
        .find(|r| r.source == card && r.kind == StackItemKind::Spell)
        .unwrap();
    assert!(matches!(
        item.v4
            .source_contract
            .unwrap()
            .spell_cast_origin
            .unwrap()
            .route,
        crate::state::SpellCastRouteV4::GreatwurmGraveyardV1 { .. }
    ));
    let index = item
        .v4
        .source_contract
        .unwrap()
        .finalized_cast_binding
        .unwrap()
        .greatwurm_payment_v1
        .unwrap();
    assert!(matches!(
        s.engine.event_history[index as usize],
        event::CommittedEvent::GreatwurmCounterPaymentV1 { .. }
    ));
    engine::validated_stack_item_target_spec(item, s).unwrap();
    let _ = restore(s);
}
#[test]
fn exact_partial_printing_paid_hand_and_all_predecessor_refusals_both_seats() {
    assert_eq!(id(NAME), 476);
    assert!(definition(476));
    assert!(!definition(475));
    assert!(card_def::preflight_fully_supported_deck(&[476]).is_err());
    let [ability] = crate::trigger::triggers_for(476) else {
        panic!("one trigger")
    };
    assert_eq!(
        ability.condition,
        crate::trigger::TriggerCondition::ControlledCreatureCombatDamageDuringOwnTurnV1
    );
    assert_eq!((ability.effect)(), marker());
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let w = paid(&mut s, p);
        assert!(engine::has_effective_keyword(&s, w, Keywords::TRAMPLE));
        assert!(refuse_legacy(&s).is_err());
        // The V53 appendix is not a whole-world legacy consumer gate.
        assert!(crate::rl::observe_v1(&s, p, 0).is_err());
        let _ = restore(&s);
    }
}
#[test]
fn paid_graveyard_six_repeated_units_original_incarnation_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let w = paid(&mut s, p);
        graveyard(&mut s, w);
        let donor = put(&mut s, p, "Axgard Cavalry", Zone::Battlefield);
        // Counter setup primitive, followed by a genuine 4GG cast/payment.
        s.objects.get_mut(donor).counters.plus1_plus1 = 6;
        let before = tapped(&s, p);
        begin_graveyard(&mut s, p, w);
        for _ in 0..6 {
            pick_unit(&mut s, p, donor, CounterKindV1::PlusOne);
        }
        publish_graveyard(&mut s, p, w, before);
        assert_eq!(s.objects.get(donor).counters.plus1_plus1, 0);
        let item = s.stack.iter().find(|r| r.source == w).unwrap().clone();
        let paid_index = item
            .v4
            .source_contract
            .unwrap()
            .finalized_cast_binding
            .unwrap()
            .greatwurm_payment_v1
            .unwrap() as usize;
        for mode in 0..4 {
            let mut bad = restore(&s);
            let event::CommittedEvent::GreatwurmCounterPaymentV1 { paid, .. } =
                &mut bad.engine.event_history[paid_index]
            else {
                panic!("paid receipt")
            };
            match mode {
                0 => paid.units[0].kind = CounterKindV1::Oil,
                1 => paid.original_counts[0] += 1,
                2 => paid.units[0].permanent.expected_zone_change_count += 1,
                _ => paid.units[0].permanent.object = w,
            }
            let original = bad.clone();
            assert!(engine::validated_stack_item_target_spec(&item, &bad).is_err());
            assert_eq!(bad, original);
        }
        finish(&mut s);
        assert_eq!(s.objects.get(w).zone, Zone::Battlefield);
        assert_eq!(engine::checked_effective_pt(&s, w).unwrap(), (7, 7));
    }
}
#[test]
fn genuine_paid_cast_accepts_every_native_counter_kind_without_narrowing_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for kind in KINDS {
            let mut s = ready(p);
            let w = paid(&mut s, p);
            graveyard(&mut s, w);
            let donor = put(&mut s, p, NAME, Zone::Battlefield);
            let o = s.objects.get_mut(donor);
            // Exhaustive native counter-storage setup, not a printed producer claim.
            // A printed 7/7 donor survives six negative counters without an
            // illegal uncancelled +1/-1 pair in an ordinary priority scene.
            match kind {
                CounterKindV1::PlusOne => o.counters.plus1_plus1 = 6,
                CounterKindV1::MinusOne => o.counters.minus1_minus1 = 6,
                CounterKindV1::MinusToughness => o.counters.minus0_minus1 = 6,
                CounterKindV1::Stun => o.counters.stun = 6,
                CounterKindV1::Lore => o.counters.lore = 6,
                CounterKindV1::Oil => o.counters.oil = 6,
                CounterKindV1::Charge => o.counters.charge = 6,
                CounterKindV1::Net => o.counters.net = 6,
                CounterKindV1::Bait => {}
                CounterKindV1::Soul => o.counters.soul = 6,
                CounterKindV1::Incubation => o.counters.incubation = 6,
                CounterKindV1::Revival => o.counters.revival = 6,
                CounterKindV1::Fellowship => o.counters.fellowship = 6,
                CounterKindV1::Stash => o.counters.stash = 6,
                CounterKindV1::Loyalty => {}
                CounterKindV1::Lifelink => o.v4.lifelink_keyword_counters = 6,
                CounterKindV1::Time => o.v4.time_counters_v1 = 6,
                CounterKindV1::Finality => {
                    o.v4.creature_upgrade
                        .get_or_insert_with(Default::default)
                        .finality = 6
                }
            }
            if kind == CounterKindV1::Loyalty {
                crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, donor, u32::MAX)
                    .unwrap();
            }
            if kind == CounterKindV1::Bait {
                // Existing authenticated placement primitive, not a printed
                // bait producer. Raw bait injection must remain refused.
                let mut raw = restore(&s);
                raw.objects.get_mut(donor).counters.bait = 6;
                let original = raw.clone();
                assert!(crate::fishing_pole_v39::lifecycle::validate_state(&raw).is_err());
                assert_eq!(raw, original);
                crate::standard_creatures_v1::place_counters_checked_v1(
                    &mut s,
                    donor,
                    p,
                    crate::standard_creatures_v1::CounterTransferV1 {
                        counters: crate::state::Counters {
                            bait: 6,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .unwrap();
                crate::fishing_pole_v39::lifecycle::validate_state(&s).unwrap();
            }
            let old = count(&s, donor, kind).unwrap();
            let before = tapped(&s, p);
            begin_graveyard(&mut s, p, w);
            for _ in 0..6 {
                pick_unit(&mut s, p, donor, kind);
            }
            publish_graveyard(&mut s, p, w, before);
            assert_eq!(count(&s, donor, kind).unwrap(), old - 6);
            if kind == CounterKindV1::Bait {
                crate::fishing_pole_v39::lifecycle::validate_state(&s).unwrap();
                let at = s
                    .engine
                    .event_history
                    .iter()
                    .position(|e| {
                        matches!(e, event::CommittedEvent::GreatwurmCounterPaymentV1 { .. })
                    })
                    .unwrap();
                for mode in 0..4 {
                    let mut bad = restore(&s);
                    let event::CommittedEvent::GreatwurmCounterPaymentV1 { paid, .. } =
                        &mut bad.engine.event_history[at]
                    else {
                        unreachable!()
                    };
                    match mode {
                        0 => paid.units[0].kind = CounterKindV1::Oil,
                        1 => paid.original_counts[0] += 1,
                        2 => paid.units[0].permanent.expected_zone_change_count += 1,
                        _ => paid.units[0].permanent.object = w,
                    }
                    let original = bad.clone();
                    assert!(crate::fishing_pole_v39::lifecycle::validate_state(&bad).is_err());
                    assert_eq!(bad, original);
                }
            }
            finish(&mut s);
            if kind == CounterKindV1::Bait {
                assert_eq!(s.objects.get(w).zone, Zone::Battlefield);
                // Real paid cast has resolved. Its original caster remains
                // bound independently of the matching input/payment pair.
                let mut forged = restore(&s);
                for event in &mut forged.engine.event_history {
                    match event {
                        event::CommittedEvent::GreatwurmCounterInputV1 { input }
                        | event::CommittedEvent::GreatwurmCounterPaymentV1 {
                            paid: input, ..
                        } => {
                            input.source.source = ObjectId(u32::MAX);
                        }
                        _ => {}
                    }
                }
                let original = forged.clone();
                assert!(crate::fishing_pole_v39::lifecycle::validate_state(&forged).is_err());
                assert_eq!(forged, original);
                // Native departure/reentry and control primitives, not another
                // paid spell. They cannot relabel the historical cast source.
                event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(w, Zone::Hand));
                event::propose_and_commit(
                    &mut s,
                    event::ProposedEvent::zone_change(w, Zone::Battlefield),
                );
                crate::standard_cards_v1::gain_control(&mut s, w, p.opponent());
                let later = restore(&s);
                crate::fishing_pole_v39::lifecycle::validate_state(&later).unwrap();
            }
        }
    }
}
#[test]
fn partial_counter_plan_forgery_stale_control_and_unaffordable_cast_are_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let w = paid(&mut s, p);
        graveyard(&mut s, w);
        let donor = put(&mut s, p, "Axgard Cavalry", Zone::Battlefield);
        s.objects.get_mut(donor).counters.charge = 5;
        let before = s.clone();
        assert!(engine::step(&mut s, Action::CastSpell(w)).is_err());
        assert_eq!(s, before);
        s.objects.get_mut(donor).counters.charge = 6;
        begin_graveyard(&mut s, p, w);
        pick_unit(&mut s, p, donor, CounterKindV1::Charge);
        for mode in 0..5 {
            let mut bad = restore(&s);
            match mode {
                0 => bad.engine.pending_cast.as_mut().unwrap().greatwurm_v1 = None,
                1 => {
                    bad.engine
                        .pending_cast
                        .as_mut()
                        .unwrap()
                        .greatwurm_v1
                        .as_mut()
                        .unwrap()
                        .units[0]
                        .permanent
                        .expected_zone_change_count += 1
                }
                2 => crate::standard_cards_v1::gain_control(&mut bad, donor, p.opponent()),
                3 => {
                    bad.engine
                        .pending_cast
                        .as_mut()
                        .unwrap()
                        .greatwurm_v1
                        .as_mut()
                        .unwrap()
                        .units = vec![
                        s.engine
                            .pending_cast
                            .as_ref()
                            .unwrap()
                            .greatwurm_v1
                            .as_ref()
                            .unwrap()
                            .units[0];
                        7
                    ]
                }
                _ => {
                    bad.engine
                        .pending_cast
                        .as_mut()
                        .unwrap()
                        .greatwurm_v1
                        .as_mut()
                        .unwrap()
                        .printed_route = Some(false)
                }
            }
            let original = bad.clone();
            assert!(engine::step(&mut bad, Action::ChooseEffectOption(0)).is_err());
            assert_eq!(bad, original);
        }
        let original = s.clone();
        assert!(engine::step(&mut s, Action::ChooseEffectOption(u16::MAX)).is_err());
        assert_eq!(s, original);
    }
}
fn primitive_wave(s: &mut GameState, p: PlayerId, source: ObjectId, packets: &[(Target, i32)]) {
    // Existing native combat-wave primitive; no declaration/power claim.
    engine::commit_combat_damage_events(
        s,
        packets
            .iter()
            .map(|(t, n)| event::ProposedEvent::damage(source, *t, *n))
            .collect(),
    );
    assert!(s.engine.halted.is_none());
    let _ = next(s);
    assert!(s
        .stack
        .iter()
        .any(|r| r.controller == p && r.inline_effect.as_ref().is_some_and(recipe)));
}
#[test]
fn paid_observer_native_wave_sums_all_recipients_once_and_current_placement_control_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let _w = paid(&mut s, p);
        let donor = put(&mut s, p, "Axgard Cavalry", Zone::Battlefield);
        let blocker = put(&mut s, p.opponent(), "Axgard Cavalry", Zone::Battlefield);
        primitive_wave(
            &mut s,
            p,
            donor,
            &[
                (Target::Object(blocker), 1),
                (Target::Player(p.opponent()), 2),
            ],
        );
        let rows = s
            .stack
            .iter()
            .filter(|r| r.inline_effect.as_ref().is_some_and(recipe))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        let EffectOp::GreatwurmCountersV1 {
            capture_history_index,
            ..
        } = rows[0].inline_effect.clone().unwrap()
        else {
            panic!()
        };
        assert_eq!(capture(&s, capture_history_index).unwrap().amount, 3);
        let _ = restore(&s);
        // Same-generation control primitive. Employment is a sorcery and cannot
        // honestly be cast in response to this combat trigger.
        crate::standard_cards_v1::gain_control(&mut s, donor, p.opponent());
        put(&mut s, p.opponent(), "Doubling Season", Zone::Battlefield);
        finish(&mut s);
        assert_eq!(s.objects.get(donor).counters.plus1_plus1, 6);
    }
}
#[test]
fn raw_combat_boolean_cannot_mint_trigger_and_departed_reentered_creature_gets_none_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let _w = paid(&mut s, p);
        let donor = put(&mut s, p, "Axgard Cavalry", Zone::Battlefield);
        let mut raw = s.clone();
        let mut damage = event::ProposedEvent::damage(donor, Target::Player(p.opponent()), 2);
        let event::ProposedEvent::Damage(d) = &mut damage else {
            unreachable!()
        };
        d.is_combat = true;
        event::propose_and_commit(&mut raw, damage);
        let e = raw.engine.event_log.clone();
        assert!(collect(&mut raw, &e).is_err());
        primitive_wave(&mut s, p, donor, &[(Target::Player(p.opponent()), 2)]);
        let old = s.objects.get(donor).zone_change_count;
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(donor, Zone::Hand));
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(donor, Zone::Battlefield),
        );
        assert!(s.objects.get(donor).zone_change_count > old);
        finish(&mut s);
        assert_eq!(s.objects.get(donor).counters.plus1_plus1, 0);
    }
}
#[test]
fn paid_observer_native_wave_public_api_changed_root_and_capture_tamper_refuse_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let donor = paid(&mut s, p);
        primitive_wave(&mut s, p, donor, &[(Target::Player(p.opponent()), 2)]);
        let item = s.stack.last().unwrap().clone();
        let ctx = super::super::crystal_prevention_v1::context(&item);
        let draw = EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        };
        let original = s.clone();
        assert!(
            super::super::begin_resumable_resolution(&draw, &ctx, item.clone(), &mut s).is_err()
        );
        assert_eq!(s, original);
        let mut bad = s.clone();
        let mut changed = item.clone();
        changed.kind = StackItemKind::Spell;
        changed.inline_effect = Some(draw.clone());
        let mut erased = ctx.clone();
        erased.stack_item_id = None;
        erased.ability_source_contract = None;
        assert!(
            super::super::begin_resumable_resolution(&draw, &erased, changed, &mut bad).is_err()
        );
        assert_eq!(bad, s);
        let mut direct = s.clone();
        super::super::execute(&draw, &erased, &mut direct);
        assert!(direct.engine.halted.is_some());
        assert_eq!(
            direct.players[p.index()].library,
            s.players[p.index()].library
        );
        let EffectOp::GreatwurmCountersV1 {
            capture_history_index,
            ..
        } = item.inline_effect.clone().unwrap()
        else {
            panic!()
        };
        for mode in 0..5 {
            let mut bad = s.clone();
            let receipt = bad.engine.event_history[capture_history_index as usize].clone();
            match mode {
                0 => {
                    if let event::CommittedEvent::GreatwurmCombatCaptureV1 { capture } =
                        &mut bad.engine.event_history[capture_history_index as usize]
                    {
                        capture.amount += 1;
                    }
                }
                1 => bad.engine.event_history.push(receipt),
                2 => {
                    if let event::CommittedEvent::GreatwurmCombatCaptureV1 { capture } =
                        &mut bad.engine.event_history[capture_history_index as usize]
                    {
                        capture.creature.zone_change_count += 1;
                    }
                }
                3 => {
                    if let event::CommittedEvent::GreatwurmCombatCaptureV1 { capture } =
                        &mut bad.engine.event_history[capture_history_index as usize]
                    {
                        capture.creature.zone_change_count =
                            capture.creature.zone_change_count.saturating_sub(1);
                    }
                }
                _ => {
                    bad.engine.event_history.retain(|e|!matches!(e,event::CommittedEvent::GreatwurmCombatBoundV1{capture_history_index:i,..}if *i==capture_history_index));
                }
            }
            assert!(validate_published(&bad, &item).is_err());
        }
    }
}

#[test]
fn genuine_paid_printing_real_declared_trample_combat_counts_one_entire_wave_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let w = paid(&mut s, p);
        let blocker = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
        let turn = s.turn;
        // Real passes advance through the opponent's turn and clear sickness.
        for _ in 0..160 {
            let d = next(&mut s);
            if s.turn > turn
                && s.active_player == p
                && s.step == Step::Main1
                && matches!(d,Decision::CastSpellOrPass{player,..}if player==p)
            {
                break;
            }
            pass(&mut s, d);
        }
        assert!(!s.objects.get(w).summoning_sick);
        let mut attacked = false;
        let mut blocked = false;
        let mut trigger = false;
        for _ in 0..128 {
            let d = next(&mut s);
            match d {
                Decision::DeclareAttackers { player, eligible } if player == p => {
                    assert!(eligible.contains(&w));
                    engine::step(&mut s, Action::DeclareAttackers(vec![w])).unwrap();
                    attacked = true;
                }
                Decision::DeclareBlockers {
                    player,
                    legal_blockers,
                    ..
                } if attacked && player == p.opponent() => {
                    assert!(legal_blockers
                        .iter()
                        .any(|(attacker, bs)| *attacker == w && bs.contains(&blocker)));
                    engine::step(&mut s, Action::DeclareBlockers(vec![(blocker, w)])).unwrap();
                    blocked = true;
                }
                Decision::ChooseCombatDamageRange { .. } => engine::step(
                    &mut s,
                    Action::ChooseCombatDamageRange { upper_half: false },
                )
                .unwrap(),
                Decision::CastSpellOrPass { .. }
                    if s.stack
                        .iter()
                        .any(|r| r.inline_effect.as_ref().is_some_and(recipe)) =>
                {
                    trigger = true;
                    break;
                }
                d => pass(&mut s, d),
            }
        }
        assert!(attacked && blocked && trigger);
        let rows = s
            .stack
            .iter()
            .filter(|r| r.inline_effect.as_ref().is_some_and(recipe))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        let Some(EffectOp::GreatwurmCountersV1 {
            capture_history_index,
            ..
        }) = rows[0].inline_effect.as_ref()
        else {
            panic!()
        };
        assert_eq!(capture(&s, *capture_history_index).unwrap().amount, 7);
        assert_eq!(capture(&s, *capture_history_index).unwrap().damage.len(), 2);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(w).counters.plus1_plus1, 7);
    }
}

#[test]
fn mixed_counter_units_pay_one_atomic_cost_and_hidden_worlds_do_not_change_menu_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut a = ready_hidden(p, "Forest");
        let mut b = ready_hidden(p, "Island");
        let wa = paid(&mut a, p);
        let wb = paid(&mut b, p);
        assert_eq!(wa, wb);
        graveyard(&mut a, wa);
        graveyard(&mut b, wb);
        let da = put(&mut a, p, "Axgard Cavalry", Zone::Battlefield);
        let db = put(&mut b, p, "Axgard Cavalry", Zone::Battlefield);
        assert_eq!(da, db);
        for s in [&mut a, &mut b] {
            // Native six-kind setup; these are public counters on one creature.
            let c = &mut s.objects.get_mut(da).counters;
            c.charge = 1;
            c.oil = 1;
            c.lore = 1;
            c.net = 1;
            c.fellowship = 1;
            // Keep the six distinct units, with bait entering through its real
            // native transfer journal rather than an unauthenticated assignment.
            crate::standard_creatures_v1::place_counters_checked_v1(
                s,
                da,
                p,
                crate::standard_creatures_v1::CounterTransferV1 {
                    counters: crate::state::Counters {
                        bait: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        }
        begin_graveyard(&mut a, p, wa);
        begin_graveyard(&mut b, p, wb);
        for kind in [
            CounterKindV1::Charge,
            CounterKindV1::Oil,
            CounterKindV1::Lore,
            CounterKindV1::Net,
            CounterKindV1::Bait,
            CounterKindV1::Fellowship,
        ] {
            let pa = &a
                .engine
                .pending_cast
                .as_ref()
                .unwrap()
                .greatwurm_v1
                .as_ref()
                .unwrap()
                .units;
            let pb = &b
                .engine
                .pending_cast
                .as_ref()
                .unwrap()
                .greatwurm_v1
                .as_ref()
                .unwrap()
                .units;
            assert_eq!(units(&a, p, pa).unwrap(), units(&b, p, pb).unwrap());
            pick_unit(&mut a, p, da, kind);
            pick_unit(&mut b, p, db, kind);
        }
        let before = tapped(&a, p);
        publish_graveyard(&mut a, p, wa, before);
        publish_graveyard(&mut b, p, wb, before);
        assert_eq!(
            a.objects.get(da).counters,
            crate::state::Counters::default()
        );
        assert_eq!(
            b.objects.get(db).counters,
            crate::state::Counters::default()
        );
        finish(&mut a);
        finish(&mut b);
        assert_eq!(a.objects.get(wa), b.objects.get(wb));
    }
}

#[test]
fn paid_counterspell_keeps_six_removed_counters_and_all_mana_paid_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let w = paid(&mut s, p);
        graveyard(&mut s, w);
        let donor = put(&mut s, p, "Axgard Cavalry", Zone::Battlefield);
        s.objects.get_mut(donor).counters.charge = 6;
        let before = tapped(&s, p);
        begin_graveyard(&mut s, p, w);
        for _ in 0..6 {
            pick_unit(&mut s, p, donor, CounterKindV1::Charge);
        }
        publish_graveyard(&mut s, p, w, before);
        let counter = cast(
            &mut s,
            p.opponent(),
            "Counterspell",
            &[Target::Object(w)],
            2,
        );
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(w).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(counter).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(donor).counters.charge, 0);
        assert_eq!(tapped(&s, p) - before, 6);
    }
}

#[test]
fn genuine_paid_copy_and_witness_suppression_use_effective_observer_and_departure_lki_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        // Automatic generic payment can use Islands. Leave actual blue sources
        // for Witness after the genuine 4GG and 4UU payments in this same turn.
        for _ in 0..4 {
            put(&mut s, p, "Island", Zone::Battlefield);
        }
        let w = paid(&mut s, p);
        cast(&mut s, p, "Self-Reflection", &[Target::Object(w)], 6);
        finish(&mut s);
        let copy = s
            .objects
            .iter()
            .find_map(|(i, o)| {
                (i != w && o.zone == Zone::Battlefield && o.v4.is_token && definition(o.card_def))
                    .then_some(i)
            })
            .unwrap();
        assert!(refuse_legacy(&s).is_err());
        cast(&mut s, p, "Witness Protection", &[Target::Object(w)], 1);
        finish(&mut s);
        assert!(!crate::continuous_characteristics_v1::printed_abilities_active(&s, w));
        let donor = put(&mut s, p, "Axgard Cavalry", Zone::Battlefield);
        primitive_wave(&mut s, p, donor, &[(Target::Player(p.opponent()), 2)]);
        let rows = s
            .stack
            .iter()
            .filter(|r| r.inline_effect.as_ref().is_some_and(recipe))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, copy);
        // Native token departure primitive after the actual trigger published.
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(copy, Zone::Hand));
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(donor).counters.plus1_plus1, 2);
    }
}

#[test]
fn native_generic_grant_preserves_distinct_genuinely_paid_graveyard_routes_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for printed in [false, true] {
            let mut s = ready(p);
            let w = paid(&mut s, p);
            graveyard(&mut s, w);
            let donor = put(&mut s, p, "Axgard Cavalry", Zone::Battlefield);
            s.objects.get_mut(donor).counters.charge = 6;
            // Existing generic graveyard-permission effect primitive, explicitly
            // not a paid Case or Zul Ashur claim. The following 4GG cast is paid.
            let grant = EffectOp::StandardV1(
                crate::standard_cards_v1::StandardOpV1::GraveyardCreaturesCastableThisTurn,
            );
            super::super::execute(&grant, &ExecCtx::no_targets(w, p), &mut s);
            assert!(crate::standard_cards_v1::graveyard_cast_granted(
                &s,
                w,
                p,
                s.objects.get(w).zone_change_count
            ));
            let before = tapped(&s, p);
            begin_graveyard(&mut s, p, w);
            assert!(matches!(
                next(&mut s),
                Decision::ChooseEffectOption {
                    option_count: 2,
                    ..
                }
            ));
            assert!(s
                .engine
                .pending_cast
                .as_ref()
                .unwrap()
                .greatwurm_v1
                .as_ref()
                .unwrap()
                .printed_route
                .is_none());
            let mut replay = restore(&s);
            engine::step(&mut s, Action::ChooseEffectOption(u16::from(printed))).unwrap();
            engine::step(&mut replay, Action::ChooseEffectOption(u16::from(printed))).unwrap();
            assert_eq!(s, replay);
            if printed {
                for _ in 0..6 {
                    pick_unit(&mut s, p, donor, CounterKindV1::Charge);
                }
                publish_graveyard(&mut s, p, w, before);
                assert_eq!(s.objects.get(donor).counters.charge, 0);
            } else {
                assert!(matches!(next(&mut s),Decision::CastSpellOrPass{player,..}if player==p));
                assert_eq!(tapped(&s, p) - before, 6);
                assert_eq!(s.objects.get(donor).counters.charge, 6);
                let item = s.stack.iter().find(|r| r.source == w).unwrap();
                let contract = item.v4.source_contract.unwrap();
                assert!(matches!(
                    contract.spell_cast_origin.unwrap().route,
                    crate::state::SpellCastRouteV4::GraveyardPermissionV1 { .. }
                ));
                assert!(contract
                    .finalized_cast_binding
                    .unwrap()
                    .greatwurm_payment_v1
                    .is_none());
                engine::validated_stack_item_target_spec(item, &s).unwrap();
            }
            finish(&mut s);
            assert_eq!(s.objects.get(w).zone, Zone::Battlefield);
        }
    }
}
