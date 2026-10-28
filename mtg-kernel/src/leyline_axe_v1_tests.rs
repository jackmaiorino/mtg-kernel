//! Actual native Leyline482. Library/land/host operands are labelled setup;
//! opening draws, London choices, casting, equip and response payments are native.
use super::*;
use crate::{engine, policy_observation_v6::tests::put, state::Target};

const AXE: &str = "Leyline Axe";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn restore(s: &GameState) -> GameState {
    let r: GameState = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(*s, r);
    assert_eq!(s.state_hash(), r.state_hash());
    r
}
fn next(s: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(s);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn answer(s: &mut GameState, a: Action) {
    let mut r = restore(s);
    engine::step(s, a.clone()).unwrap();
    engine::step(&mut r, a).unwrap();
    assert_eq!(next(s), next(&mut r));
    assert_eq!(*s, r);
}
fn quiet(s: &mut GameState) {
    let a = match next(s) {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        d => panic!("unexpected actual Axe decision {d:?}"),
    };
    answer(s, a);
}
fn settle(s: &mut GameState) {
    for _ in 0..80 {
        if matches!(next(s), Decision::CastSpellOrPass { .. })
            && s.stack.is_empty()
            && s.engine.pending_activation.is_none()
            && s.engine.pending_cast.is_none()
        {
            return;
        }
        quiet(s);
    }
    panic!("native Axe stack did not settle");
}
fn ready(p: PlayerId) -> GameState {
    let library = vec![id("Forest"); 40];
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &library,
        &library,
        crate::rl::card_name,
        110_482,
        p,
    );
    s.step = Step::Main1;
    s
}
fn deal(p: PlayerId, own_axes: usize, other_axes: usize) -> GameState {
    let deck = |count| {
        let mut d = vec![id("Forest"); 40];
        for x in d.iter_mut().take(count) {
            *x = id(AXE);
        }
        d
    };
    let own = deck(own_axes);
    let other = deck(other_axes);
    let (a, b) = if p == PlayerId::P0 {
        (&own, &other)
    } else {
        (&other, &own)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_482,
        p,
    );
    for _ in 0..7 {
        for player in [PlayerId::P0, PlayerId::P1] {
            event::propose_and_commit(&mut s, ProposedEvent::draw(player));
        }
    }
    s
}
fn paid_cast(s: &mut GameState, p: PlayerId, source: ObjectId, lands: &[ObjectId]) {
    assert_eq!(s.priority_player, p);
    for land in lands {
        engine::step(s, Action::ActivateManaAbility(*land)).unwrap();
    }
    assert!(
        matches!(next(s), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == p && castable_spells.contains(&source))
    );
    engine::step(s, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(s), Decision::CastSpellOrPass { player, .. } if player == p));
    assert!(s.engine.pending_cast.is_none());
    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
    assert!(lands.iter().all(|l| s.objects.get(*l).tapped));
    assert_eq!(s.objects.get(source).zone, Zone::Stack);
}
fn paid_axe(p: PlayerId) -> (GameState, ObjectId, ObjectId, Vec<ObjectId>) {
    let mut s = ready(p);
    let axe = put(&mut s, p, AXE, Zone::Hand);
    let host = put(&mut s, p, "Llanowar Elves", Zone::Battlefield);
    let lands = (0..7)
        .map(|_| put(&mut s, p, "Forest", Zone::Battlefield))
        .collect::<Vec<_>>();
    paid_cast(&mut s, p, axe, &lands[..4]);
    settle(&mut s);
    assert_eq!(s.objects.get(axe).zone, Zone::Battlefield);
    (s, axe, host, lands)
}
fn equip(s: &mut GameState, p: PlayerId, axe: ObjectId, host: ObjectId, lands: &[ObjectId]) {
    for land in lands {
        engine::step(s, Action::ActivateManaAbility(*land)).unwrap();
    }
    engine::step(s, Action::ActivateAbility(axe, 0)).unwrap();
    assert!(
        matches!(next(s), Decision::ChooseTargets { player, spell, legal_targets, .. }
        if player == p && spell == axe && legal_targets.contains(&Target::Object(host)))
    );
    answer(s, Action::ChooseTarget(Target::Object(host)));
    assert!(s.engine.pending_activation.is_none());
    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
    assert!(lands.iter().all(|l| s.objects.get(*l).tapped));
    assert!(!s.objects.get(axe).tapped);
}

#[test]
fn actual_four_mana_printing_three_mana_equip_full_native_bonus_and_restore_both_seats() {
    assert_eq!(id(AXE), 482);
    assert!(definition(482));
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, axe, host, lands) = paid_axe(p);
        assert!(s.objects.get(axe).v4.attached_to.is_none());
        equip(&mut s, p, axe, host, &lands[4..]);
        let waiting = restore(&s);
        settle(&mut s);
        let mut r = waiting;
        settle(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(axe).v4.attached_to.unwrap().object, host);
        assert_eq!(engine::checked_effective_pt(&s, host).unwrap(), (2, 2));
        assert!(engine::has_effective_keyword(
            &s,
            host,
            Keywords::DOUBLE_STRIKE
        ));
        assert!(engine::has_effective_keyword(&s, host, Keywords::TRAMPLE));
        assert!(require_legacy_absence(&s).is_err());
    }
}

#[test]
fn real_opening_draws_optional_each_copy_starting_seat_order_and_restored_answers_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = deal(p, 2, 1);
        let originals = s.players[p.index()].hand[..2].to_vec();
        let opponent = s.players[p.opponent().index()].hand[0];
        for (source, actor, option) in [
            (originals[0], p, 1),
            (originals[1], p, 0),
            (opponent, p.opponent(), 1),
        ] {
            assert!(
                matches!(next(&mut s), Decision::ChooseEffectOption { player, source: card, option_count: 2 }
                if player == actor && card == source)
            );
            assert!(s.stack.is_empty());
            assert_eq!(s.turn, 1);
            assert_eq!(s.step, Step::Untap);
            assert!(require_legacy_absence(&s).is_err());
            answer(&mut s, Action::ChooseEffectOption(option));
        }
        assert!(s.leyline_opening_v1.is_none());
        assert_eq!(s.objects.get(originals[0]).zone, Zone::Battlefield);
        assert_eq!(s.objects.get(originals[1]).zone, Zone::Hand);
        assert_eq!(s.objects.get(opponent).zone, Zone::Battlefield);
        assert_eq!(s.objects.get(originals[0]).zone_change_count, 2);
        assert!(s.objects.get(originals[0]).v4.attached_to.is_none());
        assert!(!s
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, CommittedEvent::SpellCast { .. })));
        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
        assert!(s
            .engine
            .event_log
            .iter()
            .all(|e| !matches!(e, CommittedEvent::LeylineOpeningV1 { .. })));
        validate(&restore(&s)).unwrap();
    }
}

#[test]
fn actual_london_redraw_bottom_then_keep_precedes_opening_actions_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        // Actual library setup ensures every redraw is a real Axe. No hand
        // or checkpoint is installed after the native deal.
        let mut s = deal(p, 40, 0);
        crate::london_mulligan_v1::enable_london_mulligans_v1(&mut s).unwrap();
        assert!(
            matches!(next(&mut s), Decision::ChooseLondonMulligan { player, mulligan_count: 0 } if player == p)
        );
        answer(&mut s, Action::ChooseLondonMulligan { mulligan: true });
        answer(&mut s, Action::ChooseLondonMulligan { mulligan: false });
        assert!(
            matches!(next(&mut s), Decision::ChooseLondonBottom { player, remaining: 1, .. } if player == p)
        );
        let bottom = s.players[p.index()].hand[0];
        answer(&mut s, Action::ChooseLondonBottom(bottom));
        assert!(
            matches!(next(&mut s), Decision::ChooseLondonMulligan { player, mulligan_count: 1 } if player == p)
        );
        answer(&mut s, Action::ChooseLondonMulligan { mulligan: false });
        assert_eq!(s.players[p.index()].hand.len(), 6);
        assert!(crate::london_mulligan_v1::kept_hands_for_opening_v1(&s).is_ok());
        let sources = s.players[p.index()].hand.clone();
        for (i, source) in sources.iter().enumerate() {
            assert!(
                matches!(next(&mut s), Decision::ChooseEffectOption { player, source: card, .. } if player == p && card == *source)
            );
            answer(&mut s, Action::ChooseEffectOption(u16::from(i == 0)));
        }
        assert_eq!(s.players[p.index()].battlefield.len(), 1);
        assert_eq!(s.objects.get(bottom).zone, Zone::Library);
        validate(&restore(&s)).unwrap();
    }
}

#[test]
fn original_opening_choice_erasure_duplicates_foreign_indices_and_bounds_refuse_readonly_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = deal(p, 2, 0);
        next(&mut s);
        let original = restore(&s);
        for kind in 0..6 {
            let mut bad = original.clone();
            let pending = bad.leyline_opening_v1.unwrap();
            match kind {
                0 => bad.leyline_opening_v1 = None,
                1 => bad.leyline_opening_v1.as_mut().unwrap().next = 1,
                2 => bad.leyline_opening_v1.as_mut().unwrap().input_history_index += 1,
                3 => {
                    let row =
                        bad.engine.event_history[pending.input_history_index as usize].clone();
                    bad.engine.event_history.push(row);
                }
                4 | 5 => {
                    let CommittedEvent::LeylineOpeningV1 {
                        record: RecordV1::Input(input),
                    } = &mut bad.engine.event_history[pending.input_history_index as usize]
                    else {
                        panic!()
                    };
                    if kind == 4 {
                        input.hands[p.index()][0].object = ObjectId(u32::MAX);
                    } else {
                        input.candidates.reverse();
                    }
                }
                _ => unreachable!(),
            }
            let before = bad.clone();
            assert!(engine::step(&mut bad, Action::ChooseEffectOption(1)).is_err());
            assert_eq!(bad, before);
        }
        for action in [Action::ChooseEffectOption(2), Action::Pass] {
            let mut bad = original.clone();
            assert!(engine::step(&mut bad, action).is_err());
            assert_eq!(bad, original);
        }
    }
}

#[test]
fn hidden_other_hand_and_library_never_enter_public_opening_events_or_old_views_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut a = deal(p, 1, 0);
        let mut b = deal(p, 1, 1);
        assert_eq!(next(&mut a), next(&mut b));
        assert_eq!(a.engine.event_log, b.engine.event_log);
        assert_ne!(a.engine.event_history, b.engine.event_history);
        assert_eq!(require_legacy_absence(&a), require_legacy_absence(&b));
        assert!(crate::drake_public_v53::capture(&a).is_err());
        assert!(crate::drake_public_v53::capture(&b).is_err());
        assert!(a.engine.pending_effect.is_none());
        assert!(b.engine.pending_effect.is_none());
    }
}

#[test]
fn genuine_counterspell_response_to_paid_ordinary_axe_keeps_costs_and_no_opening_shortcut_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let axe = put(&mut s, p, AXE, Zone::Hand);
        let lands = (0..4)
            .map(|_| put(&mut s, p, "Forest", Zone::Battlefield))
            .collect::<Vec<_>>();
        let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
        let islands = (0..2)
            .map(|_| put(&mut s, p.opponent(), "Island", Zone::Battlefield))
            .collect::<Vec<_>>();
        paid_cast(&mut s, p, axe, &lands);
        answer(&mut s, Action::Pass);
        for land in &islands {
            engine::step(&mut s, Action::ActivateManaAbility(*land)).unwrap();
        }
        engine::step(&mut s, Action::CastSpell(counter)).unwrap();
        assert!(
            matches!(next(&mut s), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(axe)))
        );
        answer(&mut s, Action::ChooseTarget(Target::Object(axe)));
        let saved = restore(&s);
        settle(&mut s);
        let mut r = saved;
        settle(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(axe).zone, Zone::Graveyard);
        assert!(lands
            .iter()
            .chain(&islands)
            .all(|l| s.objects.get(*l).tapped));
        assert!(!already_started(&s));
    }
}

#[test]
fn paid_equip_rechecks_original_host_incarnation_and_control_with_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let (mut s, axe, host, lands) = paid_axe(p);
            equip(&mut s, p, axe, host, &lands[4..]);
            // Labelled native zone/control response primitives on a real,
            // completely paid equip row, not paid control-spell claims.
            if reenter {
                event::propose_and_commit(&mut s, ProposedEvent::zone_change(host, Zone::Hand));
                event::propose_and_commit(
                    &mut s,
                    ProposedEvent::zone_change(host, Zone::Battlefield),
                );
            } else {
                crate::standard_cards_v1::gain_control(&mut s, host, p.opponent());
            }
            let saved = restore(&s);
            settle(&mut s);
            let mut r = saved;
            settle(&mut r);
            assert_eq!(s, r);
            assert!(s.objects.get(axe).v4.attached_to.is_none());
            assert!(lands.iter().all(|l| s.objects.get(*l).tapped));
        }
    }
}

#[test]
fn paid_equipment_sorcery_timing_and_absent_opening_shape_preserve_readonly_refusal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, axe, _, lands) = paid_axe(p);
        for land in &lands[4..] {
            engine::step(&mut s, Action::ActivateManaAbility(*land)).unwrap();
        }
        assert_eq!(
            s.players[p.index()]
                .mana_pool
                .iter()
                .map(|x| u32::from(*x))
                .sum::<u32>(),
            3
        );
        let mut legal = s.clone();
        engine::step(&mut legal, Action::ActivateAbility(axe, 0)).unwrap();
        for (active, phase) in [(p.opponent(), Step::Main1), (p, Step::BeginCombat)] {
            let mut bad = s.clone();
            bad.active_player = active;
            bad.priority_player = p;
            bad.step = phase;
            let before = bad.clone();
            assert!(engine::step(&mut bad, Action::ActivateAbility(axe, 0)).is_err());
            assert_eq!(bad, before);
        }
        let clean = ready(p);
        let encoded = serde_json::to_value(&clean).unwrap();
        assert!(encoded.get("leyline_opening_v1").is_none());
        assert!(!already_started(&clean));
        validate(&restore(&clean)).unwrap();
    }
}
