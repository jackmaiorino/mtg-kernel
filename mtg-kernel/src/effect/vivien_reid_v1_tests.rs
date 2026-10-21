//! Genuine paid loyalty/cast fixtures. Initial battlefield/library/hand operands
//! are setup; explicit counter, zone and control primitives are labelled below.
use super::*;
use crate::{
    engine::time_stop_v1::tests::{finish, next, pass, priority, restore, tapped},
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
    state::Step,
};
const NAME: &str = "Vivien Reid";
fn id(n: &str) -> u16 {
    card_def::card_id_by_name(n).unwrap()
}
pub(crate) fn ready(p: PlayerId, library: &[&str], hand: &[&str], hidden: &str) -> GameState {
    let own = library.iter().map(|n| id(n)).collect::<Vec<_>>();
    let other = vec![id(hidden); 32];
    let (a, b) = if p == PlayerId::P0 {
        (&own, &other)
    } else {
        (&other, &own)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_488,
        p,
    );
    s.step = Step::Main1;
    for player in [p, p.opponent()] {
        for land in ["Forest", "Island", "Mountain", "Plains", "Swamp"] {
            for _ in 0..16 {
                put(&mut s, player, land, Zone::Battlefield);
            }
        }
    }
    for n in hand {
        put(&mut s, p, n, Zone::Hand);
    }
    s
}
fn paid(s: &mut GameState, p: PlayerId, name: &str, targets: &[Target], cost: usize) -> ObjectId {
    let card = *s.players[p.index()]
        .hand
        .iter()
        .find(|o| s.objects.get(**o).card_def == id(name))
        .unwrap();
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&card))
    );
    let before = tapped(s, p);
    engine::step(s, Action::CastSpell(card)).unwrap();
    let mut chosen = 0;
    for _ in 0..32 {
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
                let row = s.stack.iter().find(|r| r.source == card).unwrap();
                assert_eq!(row.kind, StackItemKind::Spell);
                assert_eq!(row.controller, p);
                assert!(row.v4.source_contract.is_some());
                assert!(s
                    .engine
                    .event_history
                    .iter()
                    .any(|e| matches!(e,E::SpellCast{spell,..}if *spell==card)));
                return card;
            }
            d => panic!("paid cast {name}: {d:?}"),
        }
    }
    panic!("paid cast did not publish")
}
pub(crate) fn walker(s: &mut GameState, p: PlayerId) -> ObjectId {
    let v = paid(s, p, NAME, &[], 5);
    finish(s);
    assert_eq!(s.objects.get(v).zone, Zone::Battlefield);
    assert!(definition(s.objects.get(v).card_def));
    v
}
pub(crate) fn activate(
    s: &mut GameState,
    p: PlayerId,
    v: ObjectId,
    index: u8,
    target: Option<Target>,
) -> StackItem {
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(v,index)))
    );
    let before = crate::planeswalker_v1::loyalty(s, v).unwrap();
    let expected = crate::planeswalker_v1::quote_loyalty_change_v48(
        s,
        v,
        [1, -3, -8][index as usize],
        crate::doubling_season_v48::CounterOriginV48::Cost,
    )
    .unwrap();
    engine::step(s, Action::ActivateAbility(v, index)).unwrap();
    let mut chosen = false;
    for _ in 0..32 {
        match next(s) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, p);
                assert!(!chosen);
                assert!(legal_targets.contains(&target.unwrap()));
                engine::step(s, Action::ChooseTarget(target.unwrap())).unwrap();
                chosen = true;
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert_eq!(chosen, target.is_some());
                assert!(s.engine.pending_activation.is_none());
                let row = s
                    .stack
                    .iter()
                    .find(|r| r.source == v && r.v4.activated_ability_index == Some(index))
                    .unwrap()
                    .clone();
                assert_eq!(row.kind, StackItemKind::ActivatedAbility);
                assert_eq!(row.controller, p);
                assert_eq!(row.inline_effect, Some(program(index as usize)));
                assert_eq!(row.targets, target.into_iter().collect::<Vec<_>>());
                let pub_input = rows(s)
                    .find_map(|(_, j)| match j {
                        JournalV1::Published { input, item } if **item == row => Some(input),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!((pub_input.before, pub_input.after), (before, expected));
                if s.objects.get(v).zone == Zone::Battlefield {
                    assert_eq!(crate::planeswalker_v1::loyalty(s, v), Some(expected));
                } else {
                    assert_eq!(expected, 0);
                    assert_eq!(s.objects.get(v).zone, Zone::Graveyard);
                }
                validate_state(s).unwrap();
                return row;
            }
            d => panic!("loyalty publication {d:?}"),
        }
    }
    panic!("loyalty did not publish")
}
pub(crate) fn prompt(s: &mut GameState, p: PlayerId) -> Vec<ObjectId> {
    for _ in 0..32 {
        let d = next(s);
        if let Decision::ChooseEffectTargets {
            player,
            min_targets,
            max_targets,
            can_finish,
            legal_targets,
            ..
        } = d
        {
            assert_eq!(player, p);
            assert_eq!(min_targets, 0);
            assert_eq!(max_targets, if legal_targets.is_empty() { 0 } else { 1 });
            assert!(can_finish);
            validate_pending_effect_choice(s).unwrap();
            return legal_targets
                .iter()
                .map(|t| {
                    if let Target::Object(o) = t {
                        *o
                    } else {
                        panic!()
                    }
                })
                .collect();
        }
        pass(s, d);
    }
    panic!("Vivien private look absent")
}
pub(crate) fn emblem_setup(s: &mut GameState, p: PlayerId, v: ObjectId) {
    // Native counter-placement primitive, not an invented loyalty activation.
    let old = crate::planeswalker_v1::loyalty(s, v).unwrap();
    if old < 8 {
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(s, v, 8 - old).unwrap();
    }
    activate(s, p, v, 2, None);
    finish(s);
    assert_eq!(emblems(s).filter(|(owner, _)| *owner == p).count(), 1);
}
fn refuses(s: &GameState) {
    let mut bad = restore(s);
    let before = bad.clone();
    assert!(validate_state(&bad).is_err());
    assert!(engine::step(&mut bad, Action::Pass).is_err());
    assert_eq!(bad, before);
}

#[test]
fn paid_optional_top_four_reveal_random_bottom_and_answered_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for choose in [false, true] {
            let mut s = ready(
                p,
                &[
                    "Llanowar Elves",
                    "Think Twice",
                    "Mountain",
                    "Healer's Hawk",
                    "Island",
                    "Forest",
                ],
                &[NAME],
                "Forest",
            );
            let v = walker(&mut s, p);
            assert_eq!(crate::planeswalker_v1::loyalty(&s, v), Some(5));
            let original = s.players[p.index()].library.clone();
            let row = activate(&mut s, p, v, 0, None);
            assert_eq!(crate::planeswalker_v1::loyalty(&s, v), Some(6));
            let options = prompt(&mut s, p);
            assert_eq!(options, vec![original[0], original[2], original[3]]);
            let pending = restore(&s);
            let mut replay = restore(&s);
            for b in [&mut s, &mut replay] {
                if choose {
                    engine::step(b, Action::ChooseEffectTarget(Target::Object(original[0])))
                        .unwrap();
                } else {
                    engine::step(b, Action::FinishEffectSelection).unwrap();
                }
                validate_pending_effect_choice(b).unwrap();
                let mut answered = restore(b);
                finish(b);
                finish(&mut answered);
                assert_eq!(*b, answered);
                let lib = &b.players[p.index()].library;
                assert_eq!(&lib[..2], &original[4..]);
                let mut rest = lib[2..].to_vec();
                rest.sort();
                let mut expected = original[..4]
                    .iter()
                    .copied()
                    .filter(|o| !choose || *o != original[0])
                    .collect::<Vec<_>>();
                expected.sort();
                assert_eq!(rest, expected);
                assert_eq!(b.players[p.index()].hand.contains(&original[0]), choose);
                assert_eq!(
                    b.known_hand_cards(p.opponent(), p).len(),
                    usize::from(choose)
                );
                assert!(
                    !matches!(next(b),Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(v,0)))
                );
                validate_state(b).unwrap();
                assert!(require_legacy(b, Some(p)).is_err());
            }
            assert_eq!(s, replay);
            assert_eq!(s.state_hash(), replay.state_hash());
            // Public/direct APIs cannot substitute a different program or context,
            // even if the generic selection guard is erased from a restored frame.
            for erase in [false, true] {
                let mut bad = pending.clone();
                let c = bad.engine.pending_effect.as_mut().unwrap();
                c.ctx.stack_item_id = None;
                c.ctx.controller = p.opponent();
                if erase {
                    c.choice = None;
                    c.answered_choice_guard = None;
                }
                let before = bad.clone();
                assert!(resume_resumable_resolution(&mut bad).is_err());
                assert_eq!(bad, before);
                assert!(engine::step(&mut bad, Action::FinishEffectSelection).is_err());
                assert_eq!(bad, before);
            }
            let mut bad = pending.clone();
            if let Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::LookTopPickToHandBottomRest { rule, .. },
                ..
            }) = bad.engine.pending_effect.as_mut().unwrap().choice.as_mut()
            {
                rule.selection.as_mut().unwrap().filter = LibraryPickFilterV1::Any;
            } else {
                panic!("actual Vivien library purpose absent");
            }
            let before = bad.clone();
            assert!(engine::step(&mut bad, Action::FinishEffectSelection).is_err());
            assert_eq!(bad, before);
            let mut bad = pending.clone();
            let ctx = super::super::crystal_prevention_v1::context(&row);
            let before = bad.clone();
            assert!(begin_resumable_resolution(&look(), &ctx, row.clone(), &mut bad).is_err());
            assert_eq!(bad, before);
            execute(&emblem(), &ctx, &mut bad);
            assert!(bad.engine.halted.is_some());
            assert_eq!(emblems(&bad).count(), 0);
        }
    }
}

#[test]
fn paid_short_empty_and_no_eligible_library_decline_without_exposing_other_library_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for lib in [vec![], vec!["Think Twice"], vec!["Forest"]] {
            let mut s = ready(p, &lib, &[NAME], "Mountain");
            let other = s.players[p.opponent().index()].library.clone();
            let v = walker(&mut s, p);
            activate(&mut s, p, v, 0, None);
            assert_eq!(prompt(&mut s, p).len(), usize::from(lib == ["Forest"]));
            engine::step(&mut s, Action::FinishEffectSelection).unwrap();
            finish(&mut s);
            assert!(s.players[p.index()].hand.is_empty());
            assert_eq!(s.players[p.index()].library.len(), lib.len());
            assert_eq!(s.players[p.opponent().index()].library, other);
            validate_state(&restore(&s)).unwrap();
        }
    }
}

#[test]
fn paid_destroy_union_current_flying_and_original_target_incarnation_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for name in ["Goldvein Pick", "Doubling Season", "Healer's Hawk"] {
            let cost = if name == "Goldvein Pick" {
                2
            } else if name == "Doubling Season" {
                5
            } else {
                1
            };
            let mut s = ready(p, &["Forest"; 8], &[NAME, name], "Island");
            let target = paid(&mut s, p, name, &[], cost);
            finish(&mut s);
            let v = walker(&mut s, p);
            let old = crate::planeswalker_v1::loyalty(&s, v).unwrap();
            activate(&mut s, p, v, 1, Some(Target::Object(target)));
            assert_eq!(crate::planeswalker_v1::loyalty(&s, v), Some(old - 3));
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(s.objects.get(target).zone, Zone::Graveyard);
        }
        let mut s = ready(p, &["Forest"; 8], &[NAME, "Healer's Hawk"], "Island");
        let hawk = paid(&mut s, p, "Healer's Hawk", &[], 1);
        finish(&mut s);
        let v = walker(&mut s, p);
        let row = activate(&mut s, p, v, 1, Some(Target::Object(hawk)));
        let generation = s.objects.get(hawk).zone_change_count;
        // Native leave/reenter primitive before resolution, not a paid blink.
        event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(hawk, Zone::Exile));
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(hawk, Zone::Battlefield),
        );
        assert!(s.objects.get(hawk).zone_change_count > generation);
        assert_eq!(s.stack.last(), Some(&row));
        finish(&mut s);
        assert_eq!(s.objects.get(hawk).zone, Zone::Battlefield);
    }
}

#[test]
fn paid_emblem_cost_source_departure_season_copy_and_current_control_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(
            p,
            &["Forest"; 8],
            &[NAME, "Llanowar Elves", "Self-Reflection", "Doubling Season"],
            "Island",
        );
        paid(&mut s, p, "Doubling Season", &[], 5);
        finish(&mut s);
        let elf = paid(&mut s, p, "Llanowar Elves", &[], 1);
        finish(&mut s);
        let v = walker(&mut s, p);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, v), Some(10));
        let row = activate(&mut s, p, v, 2, None);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, v), Some(2));
        // Native source departure preserves the captured original controller.
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(v, Zone::Graveyard),
        );
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(row.controller, p);
        assert_eq!(engine::checked_effective_pt(&s, elf).unwrap(), (3, 3));
        for kw in [
            Keywords::VIGILANCE,
            Keywords::TRAMPLE,
            Keywords::INDESTRUCTIBLE,
        ] {
            assert!(engine::has_effective_keyword(&s, elf, kw));
            assert_eq!(engine::effective_keywords(&s, elf, kw), kw);
            assert_eq!(engine::effective_keywords(&replay, elf, kw), kw);
        }
        let before = s.objects.len();
        paid(&mut s, p, "Self-Reflection", &[Target::Object(elf)], 6);
        finish(&mut s);
        let copies = s
            .objects
            .iter()
            .filter(|(o, x)| o.0 as usize >= before && x.zone == Zone::Battlefield && x.v4.is_token)
            .map(|(o, _)| o)
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 2);
        for copy in copies {
            assert_eq!(engine::checked_effective_pt(&s, copy).unwrap(), (3, 3));
        }
        // Native current-control primitive. The external emblem is not a
        // copiable value and follows its controller, not the recipient owner.
        s.players[p.index()].battlefield.retain(|o| *o != elf);
        s.players[p.opponent().index()].battlefield.push(elf);
        s.objects.get_mut(elf).controller = p.opponent();
        assert_eq!(engine::checked_effective_pt(&s, elf).unwrap(), (1, 1));
        assert!(!engine::has_effective_keyword(&s, elf, Keywords::TRAMPLE));
        assert_eq!(
            engine::effective_keywords(&s, elf, Keywords::TRAMPLE),
            Keywords::NONE
        );
        validate_state(&restore(&s)).unwrap();
        assert!(require_legacy(&s, Some(p)).is_err());
    }
}

#[test]
fn paid_emblem_timestamp_witness_suppression_and_later_creatures_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for aura_first in [false, true] {
            let mut s = ready(
                p,
                &["Forest"; 8],
                &[
                    NAME,
                    NAME,
                    "Llanowar Elves",
                    "Witness Protection",
                    "Healer's Hawk",
                ],
                "Island",
            );
            let elf = paid(&mut s, p, "Llanowar Elves", &[], 1);
            finish(&mut s);
            if aura_first {
                paid(&mut s, p, "Witness Protection", &[Target::Object(elf)], 1);
                finish(&mut s);
            }
            let v = walker(&mut s, p);
            emblem_setup(&mut s, p, v);
            assert_eq!(s.objects.get(v).zone, Zone::Graveyard);
            if !aura_first {
                paid(&mut s, p, "Witness Protection", &[Target::Object(elf)], 1);
                finish(&mut s);
            }
            assert_eq!(engine::checked_effective_pt(&s, elf).unwrap(), (3, 3));
            for kw in [
                Keywords::VIGILANCE,
                Keywords::TRAMPLE,
                Keywords::INDESTRUCTIBLE,
            ] {
                assert_eq!(engine::has_effective_keyword(&s, elf, kw), aura_first);
                let expected = if aura_first { kw } else { Keywords::NONE };
                assert_eq!(engine::effective_keywords(&s, elf, kw), expected);
                assert_eq!(engine::effective_keywords(&restore(&s), elf, kw), expected);
            }
            let hawk = paid(&mut s, p, "Healer's Hawk", &[], 1);
            finish(&mut s);
            assert_eq!(engine::checked_effective_pt(&s, hawk).unwrap(), (3, 3));
            assert!(engine::has_effective_keyword(&s, hawk, Keywords::FLYING));
            assert!(engine::has_effective_keyword(
                &s,
                hawk,
                Keywords::INDESTRUCTIBLE
            ));
            let aura = *s.objects.get(elf).attachments.first().unwrap();
            // Genuine paid Aura departure exposes the persistent external grant.
            event::propose_and_commit(
                &mut s,
                event::ProposedEvent::zone_change(aura, Zone::Graveyard),
            );
            assert!(engine::has_effective_keyword(&s, elf, Keywords::TRAMPLE));
            // A second genuine paid source creates an independently stacking
            // emblem. Its extra three loyalty use the labelled native counter
            // placement primitive, not a fabricated loyalty activation.
            let second = walker(&mut s, p);
            crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, second, 3).unwrap();
            activate(&mut s, p, second, 2, None);
            finish(&mut s);
            assert_eq!(emblems(&s).filter(|(owner, _)| *owner == p).count(), 2);
            assert_eq!(engine::checked_effective_pt(&s, elf).unwrap(), (5, 5));
            assert!(engine::has_effective_keyword(&s, elf, Keywords::TRAMPLE));
            validate_state(&restore(&s)).unwrap();
        }
    }
}

#[test]
fn paid_journal_echo_tamper_private_dispatch_and_full_u32_loyalty_refuse_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, &["Forest"; 8], &[NAME], "Island");
        let v = walker(&mut s, p);
        emblem_setup(&mut s, p, v);
        for which in 0..6 {
            let mut bad = restore(&s);
            match which {
                0 => {
                    let j = bad
                        .engine
                        .event_history
                        .iter_mut()
                        .find_map(|e| match e {
                            E::VivienReidV1 { journal }
                                if matches!(journal.as_ref(), JournalV1::Published { .. }) =>
                            {
                                Some(journal)
                            }
                            _ => None,
                        })
                        .unwrap();
                    if let JournalV1::Published { input, .. } = j.as_mut() {
                        input.after += 1;
                    }
                }
                1..=3 => {
                    let j = bad
                        .engine
                        .event_history
                        .iter_mut()
                        .find_map(|e| match e {
                            E::VivienReidV1 { journal }
                                if matches!(journal.as_ref(), JournalV1::EmblemCreated { .. }) =>
                            {
                                Some(journal)
                            }
                            _ => None,
                        })
                        .unwrap();
                    if let JournalV1::EmblemCreated {
                        controller,
                        timestamp,
                        publication,
                        ..
                    } = j.as_mut()
                    {
                        match which {
                            1 => *controller = p.opponent(),
                            2 => *timestamp += 1,
                            _ => *publication += 1,
                        }
                    }
                }
                5 => {
                    let row = rows(&bad)
                        .find_map(|(_, j)| match j {
                            JournalV1::Published { item, .. } => Some(item.as_ref().clone()),
                            _ => None,
                        })
                        .unwrap();
                    bad.stack.push(row);
                }
                _ => {
                    let event = bad
                        .engine
                        .event_history
                        .iter()
                        .find(|e| matches!(e, E::VivienReidV1 { .. }))
                        .unwrap()
                        .clone();
                    bad.engine.event_log.push(event);
                }
            }
            refuses(&bad);
        }
        let mut s = ready(p, &["Forest"; 8], &[NAME], "Island");
        let v = walker(&mut s, p);
        // Explicit unsigned ledger boundary setup. No saturating +1 is offered.
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, v, u32::MAX - 5).unwrap();
        assert_eq!(crate::planeswalker_v1::loyalty(&s, v), Some(u32::MAX));
        assert!(
            !matches!(next(&mut s),Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(v,0)))
        );
        let before = s.clone();
        assert!(engine::step(&mut s, Action::ActivateAbility(v, 0)).is_err());
        assert_eq!(s, before);

        let mut s = ready(p, &["Forest"; 8], &[NAME, "Llanowar Elves"], "Island");
        let elf = paid(&mut s, p, "Llanowar Elves", &[], 1);
        finish(&mut s);
        let v = walker(&mut s, p);
        // Native integer-boundary primitives. The ultimate is still a genuine
        // paid activation; an unrepresentable final PT may not partly install
        // its timestamp or emblem after the cost has actually been paid.
        s.objects.get_mut(elf).counters.plus1_plus1 = i32::MAX - 1;
        assert_eq!(
            engine::checked_effective_pt(&s, elf).unwrap(),
            (i32::MAX, i32::MAX)
        );
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, v, 3).unwrap();
        activate(&mut s, p, v, 2, None);
        let timestamp = s.engine.next_effect_timestamp;
        for _ in 0..8 {
            let d = engine::advance_until_decision(&mut s);
            if s.engine.halted.is_some() {
                break;
            }
            pass(&mut s, d);
        }
        assert!(s.engine.halted.is_some());
        assert_eq!(emblems(&s).count(), 0);
        assert_eq!(s.engine.next_effect_timestamp, timestamp);
        assert_eq!(
            engine::checked_effective_pt(&s, elf).unwrap(),
            (i32::MAX, i32::MAX)
        );
        assert!(!rows(&s).any(|(_, j)| matches!(j, JournalV1::EmblemInput { .. })));
    }
}

#[test]
fn paid_response_time_stop_disposes_loyalty_without_emblem_or_countering_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, &["Forest"; 8], &[NAME, "Time Stop"], "Island");
        let v = walker(&mut s, p);
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, v, 3).unwrap();
        let row = activate(&mut s, p, v, 2, None);
        assert_eq!(s.objects.get(v).zone, Zone::Graveyard);
        paid(&mut s, p, "Time Stop", &[], 6);
        let mut replay = restore(&s);
        for b in [&mut s, &mut replay] {
            crate::engine::time_stop_v1::tests::end_turn(b);
            assert!(crate::engine::time_stop_v1::disposed(b, &row).unwrap());
            assert_eq!(emblems(b).count(), 0);
            validate_state(b).unwrap();
            // A retained paid row cannot be reintroduced after its actual
            // EndTurnDisposed outcome and gain another execution authority.
            let mut forged = restore(b);
            forged.stack.push(row.clone());
            refuses(&forged);
        }
        assert_eq!(s, replay);
    }
}

#[test]
fn independently_initialized_hidden_worlds_and_source_departure_keep_original_private_look_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut states = [
            ready(
                p,
                &[
                    "Forest",
                    "Llanowar Elves",
                    "Think Twice",
                    "Island",
                    "Mountain",
                ],
                &[NAME],
                "Island",
            ),
            ready(
                p,
                &[
                    "Forest",
                    "Llanowar Elves",
                    "Think Twice",
                    "Island",
                    "Mountain",
                ],
                &[NAME],
                "Mountain",
            ),
        ];
        let mut menus = Vec::new();
        for s in &mut states {
            assert!(require_legacy(s, None).is_ok());
            assert!(require_legacy(s, Some(p)).is_err());
            assert!(require_legacy(s, Some(p.opponent())).is_ok());
            let v = walker(s, p);
            let row = activate(s, p, v, 0, None);
            // Native source departure/control scene; the actual ability owns
            // its captured player and private prefix even after source leaves.
            event::propose_and_commit(s, event::ProposedEvent::zone_change(v, Zone::Graveyard));
            assert_eq!(row.controller, p);
            menus.push(prompt(s, p));
            let mut replay = restore(s);
            for b in [s, &mut replay] {
                engine::step(b, Action::FinishEffectSelection).unwrap();
                finish(b);
                validate_state(b).unwrap();
            }
        }
        assert_eq!(menus[0], menus[1]);
        for viewer in [p, p.opponent()] {
            assert_eq!(
                crate::rl::observe_v1(&states[0], viewer, 0).unwrap_err(),
                crate::rl::observe_v1(&states[1], viewer, 0).unwrap_err()
            );
        }
    }
}

#[test]
fn paid_completed_look_pup_bounce_and_same_card_recast_has_fresh_native_occurrence_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(
            p,
            &[
                "Forest",
                "Llanowar Elves",
                "Think Twice",
                "Island",
                "Mountain",
                "Forest",
            ],
            &[NAME, "Mischievous Pup"],
            "Island",
        );
        let v = walker(&mut s, p);
        let old_generation = s.objects.get(v).zone_change_count;
        let old = activate(&mut s, p, v, 0, None);
        prompt(&mut s, p);
        engine::step(&mut s, Action::FinishEffectSelection).unwrap();
        finish(&mut s);
        assert!(s.engine.pending_effect.is_none());
        assert!(!s
            .stack
            .iter()
            .any(|r| r.v4.stack_item_id == old.v4.stack_item_id));
        for id in [
            Some(old.v4.stack_item_id),
            None,
            Some(StackItemId(u64::MAX)),
        ] {
            let mut ctx = super::super::crystal_prevention_v1::context(&old);
            ctx.stack_item_id = id;
            let draw = EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            };
            let mut bad = restore(&s);
            let before = bad.clone();
            assert!(begin_resumable_resolution(&draw, &ctx, old.clone(), &mut bad).is_err());
            assert_eq!(bad, before);
            execute(&draw, &ctx, &mut bad);
            assert!(bad.engine.halted.is_some());
            let mut expected = before;
            expected.engine.halted = bad.engine.halted;
            assert_eq!(bad, expected);
        }
        let pup = paid(&mut s, p, "Mischievous Pup", &[], 3);
        let mut issued = false;
        for _ in 0..32 {
            let d = next(&mut s);
            if let Decision::ChooseTargets {
                player,
                spell,
                legal_targets,
                can_finish,
                ..
            } = d
            {
                assert_eq!((player, spell), (p, pup));
                assert!(can_finish);
                assert!(legal_targets.contains(&Target::Object(v)));
                engine::step(&mut s, Action::ChooseTarget(Target::Object(v))).unwrap();
                issued = true;
                break;
            }
            pass(&mut s, d);
        }
        assert!(issued);
        finish(&mut s);
        assert_eq!(s.objects.get(v).zone, Zone::Hand);
        assert_eq!(s.objects.get(v).zone_change_count, old_generation + 1);
        let recast = paid(&mut s, p, NAME, &[], 5);
        assert_eq!(recast, v);
        let new = s
            .stack
            .iter()
            .find(|r| r.source == v && r.kind == StackItemKind::Spell)
            .unwrap()
            .clone();
        assert_ne!(new.v4.stack_item_id, old.v4.stack_item_id);
        assert!(new.v4.ability_source_contract.is_none());
        assert!(!context_selected(
            &s,
            &super::super::crystal_prevention_v1::context(&new)
        ));
        let mut replay = restore(&s);
        for b in [&mut s, &mut replay] {
            finish(b);
            assert_eq!(b.objects.get(v).zone, Zone::Battlefield);
            assert_eq!(b.objects.get(v).zone_change_count, old_generation + 3);
            assert_eq!(crate::planeswalker_v1::loyalty(b, v), Some(5));
            assert!(
                matches!(next(b),Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(v,0)))
            );
            validate_state(b).unwrap();
        }
        assert_eq!(s, replay);
        assert_eq!(s.state_hash(), replay.state_hash());
    }
}
