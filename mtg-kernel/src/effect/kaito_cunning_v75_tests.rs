//! Genuine paid native Kaito source fixtures. Explicit counter/zone/combat
//! and virtual-copy primitives below are not invented paid printing routes.
use super::*;
use crate::{
    engine::time_stop_v1::tests::{cast, next, pass, priority, restore, tapped},
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
    state::Step,
};
const NAME: &str = "Kaito, Cunning Infiltrator";
fn ready(p: PlayerId) -> GameState {
    super::super::vivien_reid_v1::tests::ready(p, &["Forest"; 64], &[], "Island")
}
fn walker(s: &mut GameState, p: PlayerId) -> ObjectId {
    let k = cast(s, p, NAME, &[], 3);
    finish(s);
    assert_eq!(s.objects.get(k).zone, Zone::Battlefield);
    assert!(definition(s.objects.get(k).card_def));
    assert_eq!(crate::planeswalker_v1::loyalty(s, k), Some(3));
    k
}
fn finish(s: &mut GameState) {
    for _ in 0..256 {
        let d = next(s);
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
            && s.engine.pending_discard.is_none()
        {
            validate_state(s).unwrap();
            return;
        }
        match d {
            Decision::Discard { choices, .. } => {
                engine::step(s, Action::Discard(vec![choices[0]])).unwrap()
            }
            d => pass(s, d),
        }
    }
    panic!("Kaito resolution did not finish")
}
fn activate(
    s: &mut GameState,
    p: PlayerId,
    k: ObjectId,
    index: u8,
    target: Option<ObjectId>,
) -> StackItem {
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(k,index)))
    );
    let before = crate::planeswalker_v1::loyalty(s, k).unwrap();
    let after = crate::planeswalker_v1::quote_loyalty_change_v48(
        s,
        k,
        [1, -2, -9][index as usize],
        crate::doubling_season_v48::CounterOriginV48::Cost,
    )
    .unwrap();
    let taps = tapped(s, p);
    engine::step(s, Action::ActivateAbility(k, index)).unwrap();
    let mut answered = false;
    for _ in 0..32 {
        match next(s) {
            Decision::ChooseEffectTargets {
                player,
                min_targets,
                max_targets,
                legal_targets,
                can_finish,
                ..
            } => {
                assert_eq!((player, min_targets, max_targets), (p, 0, 1));
                assert_eq!(index, 0);
                assert!(!answered && can_finish);
                if let Some(id) = target {
                    assert!(legal_targets.contains(&Target::Object(id)));
                    engine::step(s, Action::ChooseEffectTarget(Target::Object(id))).unwrap();
                } else {
                    engine::step(s, Action::FinishEffectSelection).unwrap();
                }
                answered = true;
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert_eq!(answered, index == 0);
                assert!(s.engine.pending_activation.is_none());
                let row = s
                    .stack
                    .iter()
                    .find(|r| r.source == k && r.v4.activated_ability_index == Some(index))
                    .unwrap()
                    .clone();
                assert_eq!(row.kind, StackItemKind::ActivatedAbility);
                assert_eq!(row.controller, p);
                assert_eq!(row.inline_effect, Some(program(index as usize)));
                assert_eq!(
                    row.targets,
                    target.map(Target::Object).into_iter().collect::<Vec<_>>()
                );
                let at = published(s, &row).unwrap();
                let JournalV75::Published { input, .. } = super::at(s, at).unwrap() else {
                    panic!()
                };
                assert_eq!((input.before, input.after), (before, after));
                assert_eq!(tapped(s, p), taps);
                if s.objects.get(k).zone == Zone::Battlefield {
                    assert_eq!(crate::planeswalker_v1::loyalty(s, k), Some(after));
                } else {
                    assert_eq!(after, 0);
                    assert_eq!(s.objects.get(k).zone, Zone::Graveyard);
                }
                return row;
            }
            d => panic!("Kaito activation {d:?}"),
        }
    }
    panic!("Kaito activation was not published")
}
fn discard_prompt(s: &mut GameState, p: PlayerId) -> Vec<ObjectId> {
    for _ in 0..64 {
        let d = next(s);
        if let Decision::Discard {
            player,
            count,
            choices,
        } = d
        {
            assert_eq!((player, count), (p, 1));
            assert!(choices.len() > 1);
            validate_pending_effect_choice(s).unwrap();
            return choices;
        }
        pass(s, d);
    }
    panic!("Kaito discard prompt absent")
}
fn ninja_count(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|id| token_definition(s.objects.get(**id).card_def))
        .count()
}
fn ultimate(s: &mut GameState, p: PlayerId, k: ObjectId) {
    // Native counter-placement setup for the printed -9, not a fabricated +ability.
    let old = crate::planeswalker_v1::loyalty(s, k).unwrap();
    if old < 9 {
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(s, k, 9 - old).unwrap();
    }
    activate(s, p, k, 2, None);
    finish(s);
    assert_eq!(
        rows(s)
            .filter(|(_, j)| matches!(j,JournalV75::Emblem{controller,..}if *controller==p))
            .count(),
        1
    );
}
fn refuses(s: &GameState) {
    let mut bad = restore(s);
    let before = bad.clone();
    assert!(validate_state(&bad).is_err() || validate_pending_effect_choice(&bad).is_err());
    assert!(engine::step(&mut bad, Action::Pass).is_err());
    assert_eq!(bad, before);
}
fn next_own_main(s: &mut GameState, p: PlayerId) {
    let old = s.turn;
    for _ in 0..256 {
        let d = next(s);
        if s.turn > old
            && s.active_player == p
            && s.step == Step::Main1
            && matches!(d,Decision::CastSpellOrPass{player,..}if player==p)
        {
            return;
        }
        match d {
            Decision::Discard { choices, count, .. } => {
                engine::step(s, Action::Discard(choices[..count as usize].to_vec())).unwrap()
            }
            d => pass(s, d),
        }
    }
    panic!("next own main absent")
}
fn combat_trigger(s: &mut GameState, p: PlayerId, attacker: ObjectId) -> StackItem {
    let mut declared = false;
    for _ in 0..128 {
        let d = next(s);
        match d {
            Decision::DeclareAttackers { player, eligible } if player == p => {
                assert!(eligible.contains(&attacker));
                engine::step(s, Action::DeclareAttackers(vec![attacker])).unwrap();
                declared = true;
            }
            Decision::CastSpellOrPass { .. } if declared => {
                if let Some(row) = s.stack.iter().find(|r| {
                    matches!(
                        r.inline_effect,
                        Some(EffectOp::KaitoCunningV75 { kind: 3, .. })
                    )
                }) {
                    return row.clone();
                }
                pass(s, d);
            }
            d => pass(s, d),
        }
    }
    panic!("real combat Kaito trigger absent")
}

#[test]
fn genuine_paid_zero_or_one_target_draw_discard_and_waiting_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for choose in [false, true] {
            let mut s = ready(p);
            put(&mut s, p, "Forest", Zone::Hand);
            put(&mut s, p, "Island", Zone::Hand);
            let k = walker(&mut s, p);
            let creature = cast(&mut s, p, "Axgard Cavalry", &[], 2);
            finish(&mut s);
            let row = activate(&mut s, p, k, 0, choose.then_some(creature));
            let hand = s.players[p.index()].hand.len();
            let choices = discard_prompt(&mut s, p);
            assert_eq!(s.players[p.index()].hand.len(), hand + 1);
            assert_eq!(
                crate::vampire_gourmand_v1::cant_be_blocked(&s, creature),
                choose
            );
            let mut resumed = restore(&s);
            for state in [&mut s, &mut resumed] {
                engine::step(state, Action::Discard(vec![choices[0]])).unwrap();
                finish(state);
                assert_eq!(state.players[p.index()].hand.len(), hand);
                assert!(finished(state, &row));
            }
            assert_eq!(s, resumed);
            assert_eq!(crate::planeswalker_v1::loyalty(&s, k), Some(4));
        }
    }
}

#[test]
fn genuine_paid_only_target_departure_fizzles_whole_ability_and_reentry_is_distinct_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = walker(&mut s, p);
        let creature = cast(&mut s, p, "Axgard Cavalry", &[], 2);
        finish(&mut s);
        let row = activate(&mut s, p, k, 0, Some(creature));
        let original = s.objects.get(creature).zone_change_count;
        let hand = s.players[p.index()].hand.len();
        // Native zone-change primitive during the real response window.
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(creature, Zone::Hand),
        );
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(creature, Zone::Battlefield),
        );
        assert_eq!(s.objects.get(creature).zone_change_count, original + 2);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.players[p.index()].hand.len(), hand);
        assert!(!crate::vampire_gourmand_v1::cant_be_blocked(&s, creature));
        assert!(rows(&s).any(
            |(_, j)| matches!(j,JournalV75::NotResolved{item,countered:false}if item.as_ref()==&row)
        ));
    }
}

#[test]
fn genuine_paid_ninja_season_self_reflection_electroduplicate_and_raw_refusal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        cast(&mut s, p, "Doubling Season", &[], 5);
        finish(&mut s);
        let k = cast(&mut s, p, NAME, &[], 3);
        finish(&mut s);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, k), Some(6));
        activate(&mut s, p, k, 1, None);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(ninja_count(&s, p), 2);
        let ninja = s.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .find(|id| token_definition(s.objects.get(*id).card_def))
            .unwrap();
        assert_eq!(engine::checked_effective_pt(&s, ninja).unwrap(), (2, 1));
        assert!(engine::has_effective_subtype(
            &s,
            ninja,
            card_def::Subtype::Ninja
        ));
        assert_eq!(engine::object_color_mask(&s, ninja), 1 << 1);
        assert!(!engine::has_effective_keyword(&s, ninja, Keywords::HASTE));
        let mut bad = restore(&s);
        let before = bad.clone();
        event::propose_and_commit_batch(&mut bad, vec![event::ProposedEvent::create_token(495, p)]);
        assert!(bad.engine.halted.take().is_some());
        assert_eq!(bad, before);
        cast(&mut s, p, "Self-Reflection", &[Target::Object(ninja)], 6);
        finish(&mut s);
        assert_eq!(ninja_count(&s, p), 4);
        cast(&mut s, p, "Electroduplicate", &[Target::Object(ninja)], 3);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(ninja_count(&s, p), 6);
        let hasty = s.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|id| {
                token_definition(s.objects.get(*id).card_def)
                    && engine::has_effective_keyword(&s, *id, Keywords::HASTE)
            })
            .count();
        assert_eq!(hasty, 2);
    }
}

#[test]
fn genuine_paid_real_declared_combat_original_observer_control_and_checked_loyalty_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = walker(&mut s, p);
        let creature = cast(&mut s, p, "Axgard Cavalry", &[], 2);
        finish(&mut s);
        next_own_main(&mut s, p);
        assert!(!s.objects.get(creature).summoning_sick);
        let life = s.players[p.opponent().index()].life;
        let row = combat_trigger(&mut s, p, creature);
        assert_eq!(s.players[p.opponent().index()].life, life - 2);
        assert_eq!(row.controller, p);
        let Some(EffectOp::KaitoCunningV75 {
            kind: 3,
            origin: Some(index),
        }) = row.inline_effect
        else {
            panic!()
        };
        let c = combat(&s, index).unwrap();
        assert_eq!(c.creature.source, creature);
        assert_eq!(c.source.source, k);
        assert_eq!(c.amount, 2);
        // The genuine independent history/marker remains intact. Additional
        // changed/orphan or already-collected current-log rows cannot mint a
        // second trigger, even before another collector call.
        let original = s.engine.event_history[index as usize].clone();
        for mode in 0..3 {
            let mut bad = restore(&s);
            let mut extra = original.clone();
            if let E::KaitoCunningV75 { journal } = &mut extra {
                if let JournalV75::Combat { capture } = journal.as_mut() {
                    match mode {
                        0 => capture.amount += 1,
                        1 => capture.damage = u64::MAX,
                        _ => {}
                    }
                }
            }
            bad.engine.event_log.push(extra);
            refuses(&bad);
        }
        let old = crate::planeswalker_v1::loyalty(&s, k).unwrap();
        // Native control change while the genuine captured trigger is pending.
        crate::standard_cards_v1::gain_control(&mut s, k, p.opponent());
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(k).controller, p.opponent());
        assert_eq!(crate::planeswalker_v1::loyalty(&s, k), Some(old + 1));
        let mut bad = restore(&s);
        if let E::KaitoCunningV75 { journal } = &mut bad.engine.event_history[index as usize] {
            if let JournalV75::Combat { capture } = journal.as_mut() {
                capture.creature.zone_change_count += 1;
            }
        }
        refuses(&bad);
    }
}

#[test]
fn genuine_paid_combat_native_final_wave_captures_departing_source_and_rejects_boolean_only_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = walker(&mut s, p);
        let creature = cast(&mut s, p, "Axgard Cavalry", &[], 2);
        finish(&mut s);
        // Native simultaneous final-combat-wave primitive. The real declared combat
        // route is exercised separately; this forces observer departure in one batch.
        let mut damage = event::ProposedEvent::damage(creature, Target::Player(p.opponent()), 2);
        if let event::ProposedEvent::Damage(d) = &mut damage {
            d.is_combat = true;
        }
        let mut raw = restore(&s);
        event::propose_and_commit_batch(&mut raw, vec![damage.clone()]);
        let before = raw.clone();
        assert!(validate_state(&raw).is_err());
        assert_eq!(raw, before);
        engine::commit_combat_damage_events(
            &mut s,
            vec![
                damage,
                event::ProposedEvent::zone_change(k, Zone::Graveyard),
            ],
        );
        assert!(s.engine.halted.is_none());
        let mut held = false;
        for _ in 0..32 {
            let d = next(&mut s);
            if s.stack.iter().any(|r| {
                matches!(
                    r.inline_effect,
                    Some(EffectOp::KaitoCunningV75 { kind: 3, .. })
                )
            }) {
                held = true;
                break;
            }
            pass(&mut s, d);
        }
        assert!(held);
        assert_eq!(s.objects.get(k).zone, Zone::Graveyard);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(k).zone, Zone::Graveyard);
    }
}

#[test]
fn genuine_paid_ultimate_source_sba_any_player_cast_countered_spell_and_virtual_copy_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = walker(&mut s, p);
        ultimate(&mut s, p, k);
        assert_eq!(s.objects.get(k).zone, Zone::Graveyard);
        let spell = cast(&mut s, p, "Think Twice", &[], 2);
        assert_eq!(
            rows(&s)
                .filter(|(_, j)| matches!(j, JournalV75::Cast { .. }))
                .count(),
            1
        );
        // Genuine native virtual-spell-copy primitive, not a paid copy-spell claim.
        let (_, copy) = engine::copy_spell_for(&mut s, spell, p).unwrap();
        assert!(s
            .stack
            .iter()
            .any(|r| r.v4.stack_item_id == copy && r.is_copy));
        assert_eq!(
            rows(&s)
                .filter(|(_, j)| matches!(j, JournalV75::Cast { .. }))
                .count(),
            1
        );
        cast(
            &mut s,
            p.opponent(),
            "Counterspell",
            &[Target::Object(spell)],
            2,
        );
        assert_eq!(
            rows(&s)
                .filter(|(_, j)| matches!(j, JournalV75::Cast { .. }))
                .count(),
            2
        );
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(ninja_count(&s, p), 2);
        assert_eq!(ninja_count(&s, p.opponent()), 0);
        assert_eq!(
            rows(&s)
                .filter(|(_, j)| matches!(j, JournalV75::Emblem { .. }))
                .count(),
            1
        );
    }
}

#[test]
fn genuine_paid_source_departure_recast_and_countered_loyalty_keep_independent_occurrences_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        put(&mut s, p, "Forest", Zone::Hand);
        put(&mut s, p, "Island", Zone::Hand);
        let k = walker(&mut s, p);
        let row = activate(&mut s, p, k, 0, None);
        discard_prompt(&mut s, p);
        let ctx = super::super::crystal_prevention_v1::context(&row);
        let draw = EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        };
        let mut bad = restore(&s);
        let before = bad.clone();
        assert!(begin_resumable_resolution(&draw, &ctx, row.clone(), &mut bad).is_err());
        assert_eq!(bad, before);
        let choices = s.players[p.index()].hand.clone();
        engine::step(&mut s, Action::Discard(vec![choices[0]])).unwrap();
        finish(&mut s);
        // Native departure primitive after the genuine completed ability. Recast the
        // same physical card through its real paid hand route below.
        let generation = s.objects.get(k).zone_change_count;
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(k, Zone::Hand));
        priority(&mut s, p);
        let before = tapped(&s, p);
        engine::step(&mut s, Action::CastSpell(k)).unwrap();
        priority(&mut s, p);
        assert_eq!(tapped(&s, p) - before, 3);
        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
        finish(&mut s);
        assert!(s.objects.get(k).zone_change_count > generation);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, k), Some(3));
        let new = activate(&mut s, p, k, 1, None);
        // Countering an activated ability is an explicitly labelled native primitive.
        engine::counter_stack_item_by_id(&mut s, new.v4.stack_item_id).unwrap();
        finish(&mut s);
        assert_eq!(ninja_count(&s, p), 0);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, k), Some(1));
        assert!(rows(&s).any(
            |(_, j)| matches!(j,JournalV75::NotResolved{item,countered:true}if item.as_ref()==&new)
        ));
    }
}

#[test]
fn genuine_paid_waiting_discard_and_current_capture_forgeries_refuse_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        put(&mut s, p, "Forest", Zone::Hand);
        put(&mut s, p, "Island", Zone::Hand);
        let k = walker(&mut s, p);
        activate(&mut s, p, k, 0, None);
        discard_prompt(&mut s, p);
        let mut erased = restore(&s);
        erased.engine.pending_discard = None;
        erased.engine.pending_effect.as_mut().unwrap().frames = vec![EffectFrame::Program {
            op: leaf(5, None),
            path: vec![3],
        }];
        refuses(&erased);
        let mut foreign = restore(&s);
        foreign
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .ctx
            .controller = p.opponent();
        refuses(&foreign);
        let at = rows(&s)
            .find_map(|(i, j)| matches!(j, JournalV75::Published { .. }).then_some(i))
            .unwrap();
        let mut bad = restore(&s);
        if let E::KaitoCunningV75 { journal } = &mut bad.engine.event_history[at] {
            if let JournalV75::Published { input, .. } = journal.as_mut() {
                input.after += 1;
            }
        }
        refuses(&bad);
        let choices = s.players[p.index()].hand.clone();
        engine::step(&mut s, Action::Discard(vec![choices[0]])).unwrap();
        finish(&mut s);
        let input = rows(&s)
            .find_map(|(i, j)| matches!(j, JournalV75::DiscardInput { .. }).then_some(i))
            .unwrap();
        let mut bad = restore(&s);
        if let E::KaitoCunningV75 { journal } = &mut bad.engine.event_history[input] {
            if let JournalV75::DiscardInput { chosen, .. } = journal.as_mut() {
                chosen[0].expected_zone_change_count += 1;
            }
        }
        refuses(&bad);
    }
}

#[test]
fn genuine_paid_initial_hidden_worlds_legacy_refusal_and_rule_evasion_survives_witness_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut worlds = [
            super::super::vivien_reid_v1::tests::ready(p, &["Forest"; 64], &[], "Forest"),
            super::super::vivien_reid_v1::tests::ready(p, &["Forest"; 64], &[], "Island"),
        ];
        let mut menus = vec![];
        for s in &mut worlds {
            put(s, p, NAME, Zone::Hand);
            assert!(require_legacy(s, None).is_ok());
            assert!(require_legacy(s, Some(p)).is_err());
            assert!(require_legacy(s, Some(p.opponent())).is_ok());
            // The helper appends its own actual spell, so remove no fixture cards: cast
            // the already-present printing by the genuine native action instead.
            let k = *s.players[p.index()].hand.last().unwrap();
            priority(s, p);
            let before = tapped(s, p);
            engine::step(s, Action::CastSpell(k)).unwrap();
            priority(s, p);
            assert_eq!(tapped(s, p) - before, 3);
            finish(s);
            let c = cast(s, p, "Axgard Cavalry", &[], 2);
            finish(s);
            activate(s, p, k, 0, Some(c));
            finish(s);
            assert!(crate::vampire_gourmand_v1::cant_be_blocked(s, c));
            cast(s, p, "Witness Protection", &[Target::Object(c)], 1);
            finish(s);
            assert!(crate::vampire_gourmand_v1::cant_be_blocked(s, c));
            assert!(!crate::continuous_characteristics_v1::printed_abilities_active(s, c));
            menus.push(crate::rl::observe_v1(s, p, 0).unwrap_err());
            let mut replay = restore(s);
            next_own_main(s, p);
            next_own_main(&mut replay, p);
            assert_eq!(*s, replay);
            assert!(!crate::vampire_gourmand_v1::cant_be_blocked(s, c));
        }
        assert_eq!(menus[0], menus[1]);
    }
}

#[test]
fn genuine_paid_plus_one_departed_source_retains_original_controller_and_original_target_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        put(&mut s, p, "Forest", Zone::Hand);
        put(&mut s, p, "Island", Zone::Hand);
        let k = walker(&mut s, p);
        let creature = cast(&mut s, p, "Axgard Cavalry", &[], 2);
        finish(&mut s);
        let row = activate(&mut s, p, k, 0, Some(creature));
        // Native source-departure primitive during the real response window.
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(k, Zone::Hand));
        let hand = s.players[p.index()].hand.len();
        let choices = discard_prompt(&mut s, p);
        assert_eq!(s.players[p.index()].hand.len(), hand + 1);
        assert_eq!(s.engine.pending_effect.as_ref().unwrap().ctx.controller, p);
        assert!(crate::vampire_gourmand_v1::cant_be_blocked(&s, creature));
        let mut replay = restore(&s);
        for state in [&mut s, &mut replay] {
            let chosen = *choices.iter().find(|id| **id != k).unwrap();
            engine::step(state, Action::Discard(vec![chosen])).unwrap();
            finish(state);
            assert!(finished(state, &row));
            assert_eq!(state.objects.get(k).zone, Zone::Hand);
            assert_eq!(state.players[p.index()].hand.len(), hand);
        }
        assert_eq!(s, replay);
    }
}

#[test]
fn genuine_paid_two_independent_emblems_each_create_for_one_real_cast_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let first = walker(&mut s, p);
        ultimate(&mut s, p, first);
        let second = walker(&mut s, p);
        // Counter-placement primitive sets the second printed -9's actual payable cost.
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, second, 6).unwrap();
        activate(&mut s, p, second, 2, None);
        finish(&mut s);
        assert_eq!(
            rows(&s)
                .filter(|(_, j)| matches!(j, JournalV75::Emblem { .. }))
                .count(),
            2
        );
        let before = ninja_count(&s, p);
        cast(&mut s, p.opponent(), "Think Twice", &[], 2);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(ninja_count(&s, p), before + 2);
        assert_eq!(ninja_count(&s, p.opponent()), 0);
    }
}

#[test]
fn genuine_paid_empty_library_empty_hand_plus_one_completes_then_native_loss_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = super::super::vivien_reid_v1::tests::ready(p, &[], &[], "Island");
        let k = walker(&mut s, p);
        assert!(s.players[p.index()].hand.is_empty());
        let row = activate(&mut s, p, k, 0, None);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert!(finished(&s, &row));
        assert!(s.players[p.index()].hand.is_empty());
        assert_eq!(
            rows(&s)
                .filter(|(_, j)| matches!(j, JournalV75::DiscardInput { .. }))
                .count(),
            0
        );
        assert!(s
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e,E::Draw{player,object:None}if *player==p)));
        assert!(matches!(next(&mut s),Decision::GameOver{winner:Some(w)}if w==p.opponent()));
    }
}

#[test]
fn genuine_paid_combat_multiplier_or_native_full_prevention_uses_final_positive_damage_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        for prevent in [false, true] {
            let mut s = ready(p);
            if !prevent {
                cast(&mut s, p, "Twinflame Tyrant", &[], 5);
                finish(&mut s);
            }
            let k = walker(&mut s, p);
            let creature = cast(&mut s, p, "Axgard Cavalry", &[], 2);
            finish(&mut s);
            next_own_main(&mut s, p);
            if prevent {
                // Existing typed next-damage shield primitive, not a claimed paid pool spell.
                s.engine.active_replacements.push(event::ActiveReplacement {
                    id: 90_494,
                    source: creature,
                    kind: event::ReplacementEffectKind::PreventNextDamage {
                        target: Target::Player(p.opponent()),
                        remaining: 2,
                    },
                });
            }
            let life = s.players[p.opponent().index()].life;
            let mut replay = restore(&s);
            for state in [&mut s, &mut replay] {
                let mut declared = false;
                let mut ended = false;
                for _ in 0..128 {
                    let d = next(state);
                    match d {
                        Decision::DeclareAttackers { player, eligible } if player == p => {
                            assert!(eligible.contains(&creature));
                            engine::step(state, Action::DeclareAttackers(vec![creature])).unwrap();
                            declared = true;
                        }
                        Decision::CastSpellOrPass { .. }
                            if declared && state.step == Step::Main2 && state.stack.is_empty() =>
                        {
                            ended = true;
                            break;
                        }
                        d => pass(state, d),
                    }
                }
                assert!(declared && ended);
                assert_eq!(
                    state.players[p.opponent().index()].life,
                    life - if prevent { 0 } else { 4 }
                );
                assert_eq!(
                    crate::planeswalker_v1::loyalty(state, k),
                    Some(if prevent { 3 } else { 4 })
                );
                let captures = rows(state)
                    .filter_map(|(_, j)| match j {
                        JournalV75::Combat { capture } => Some(capture),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(captures.len(), if prevent { 0 } else { 1 });
                if !prevent {
                    assert_eq!(captures[0].amount, 4);
                }
            }
            assert_eq!(s, replay);
        }
    }
}

#[test]
fn genuine_paid_full_u32_loyalty_cost_keeps_exact_payment_and_overflow_quote_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = walker(&mut s, p);
        // Checked native counter-placement setup exercises the native unsigned width.
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, k, u32::MAX - 3).unwrap();
        let before = s.clone();
        assert!(crate::planeswalker_v1::quote_loyalty_change_v48(
            &s,
            k,
            1,
            crate::doubling_season_v48::CounterOriginV48::Cost
        )
        .is_err());
        assert_eq!(s, before);
        priority(&mut s, p);
        assert!(
            matches!(next(&mut s),Decision::CastSpellOrPass{activatable_abilities,..}if !activatable_abilities.contains(&(k,0))&&activatable_abilities.contains(&(k,1)))
        );
        let row = activate(&mut s, p, k, 1, None);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, k), Some(u32::MAX - 2));
        let at = published(&s, &row).unwrap();
        let JournalV75::Published { input, .. } = super::at(&s, at).unwrap() else {
            panic!()
        };
        assert_eq!((input.before, input.after), (u32::MAX, u32::MAX - 2));
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(ninja_count(&s, p), 1);
    }
}
