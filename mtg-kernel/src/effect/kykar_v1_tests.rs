//! Genuine paid native Kykar fixtures. Native zone/control primitives are
//! labelled explicitly and never described as paid spell coverage.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
    state::{StackItemKind, Step},
};
const NAME: &str = "Kykar, Zephyr Awakener";
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
        110_474,
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
        d => panic!("Kykar native decision {d:?}"),
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
    let k = cast(s, p, NAME, &[], 4);
    finish(s);
    assert_eq!(s.objects.get(k).zone, Zone::Battlefield);
    assert_eq!(
        (
            engine::effective_power(s, k),
            engine::effective_toughness(s, k)
        ),
        (3, 4)
    );
    assert!(engine::has_effective_keyword(s, k, Keywords::FLYING));
    k
}
fn start_named(s: &mut GameState, p: PlayerId, k: ObjectId, name: &str, cost: usize) -> ObjectId {
    priority(s, p);
    let spell = put(s, p, name, Zone::Hand);
    let before = tapped(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&spell))
    );
    engine::step(s, Action::CastSpell(spell)).unwrap();
    match next(s) {
        Decision::ChooseTriggerMode {
            player,
            source,
            mode_count,
            legal_modes,
        } => {
            assert_eq!((player, source, mode_count), (p, k, 2));
            assert!(legal_modes.contains(&1));
            assert!(s.engine.pending_cast.is_none());
            assert_eq!(tapped(s, p) - before, cost);
            assert!(s
                .stack
                .iter()
                .any(|r| r.source == spell && r.kind == StackItemKind::Spell));
        }
        d => panic!("mode before targets {d:?}"),
    };
    spell
}
fn start(s: &mut GameState, p: PlayerId, k: ObjectId) -> ObjectId {
    start_named(s, p, k, "Ravenous Amulet", 2)
}

fn choose(s: &mut GameState, p: PlayerId, mode: u8, target: Option<ObjectId>) -> StackItem {
    engine::step(s, Action::ChooseTriggerMode(mode)).unwrap();
    if let Some(t) = target {
        match next(s) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                can_finish,
                ..
            } => {
                assert_eq!(player, p);
                assert!(legal_targets.contains(&Target::Object(t)));
                assert!(!can_finish);
                engine::step(s, Action::ChooseTarget(Target::Object(t))).unwrap();
            }
            d => panic!("mode target {d:?}"),
        }
    }
    assert!(matches!(next(s),Decision::CastSpellOrPass{player,..}if player==p));
    let row = s.stack.last().unwrap().clone();
    assert_eq!(row.controller, p);
    assert_eq!(
        row.inline_effect,
        Some(if mode == 0 {
            EffectOp::KykarExileV1
        } else {
            EffectOp::KykarSpiritV1
        })
    );
    engine::validated_stack_item_target_spec(&row, s).unwrap();
    row
}
fn schedule(s: &GameState) -> (u64, crate::state::ObjectLinkV4) {
    s.engine
        .event_history
        .iter()
        .enumerate()
        .find_map(|(i, e)| match e {
            event::CommittedEvent::KykarExiledV1 { exiled, .. } => Some((i as u64, *exiled)),
            _ => None,
        })
        .unwrap()
}
fn return_row(s: &mut GameState, index: u64) -> StackItem {
    for _ in 0..180 {
        let d = next(s);
        if let Some(row) = s.stack.iter().find(|r| {
            r.inline_effect
                == Some(EffectOp::KykarReturnV1 {
                    schedule_history_index: index,
                })
        }) {
            return row.clone();
        }
        pass(s, d)
    }
    panic!("next end-step trigger absent")
}
fn later_end_keeps_consumed(s: &mut GameState, index: u64) {
    let old = s
        .engine
        .event_history
        .iter()
        .filter(|e| matches!(e, event::CommittedEvent::BeginningEndStep { .. }))
        .count();
    for _ in 0..200 {
        let d = next(s);
        assert!(!s.stack.iter().any(|r| r.inline_effect
            == Some(EffectOp::KykarReturnV1 {
                schedule_history_index: index
            })));
        if s.engine
            .event_history
            .iter()
            .filter(|e| matches!(e, event::CommittedEvent::BeginningEndStep { .. }))
            .count()
            > old
            && s.stack.is_empty()
        {
            assert_eq!(s.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::KykarDelayedCollectedV1{schedule_history_index,..}if *schedule_history_index==index)).count(),1);
            return;
        }
        pass(s, d);
    }
    panic!("later genuine end step absent")
}
fn unchanged_public_refusals(s: &GameState, row: &StackItem) {
    let ctx = crate::effect::crystal_prevention_v1::context(row);
    let op = row.inline_effect.as_ref().unwrap();
    for erase in [false, true] {
        for unknown in [false, true] {
            let mut bad = restore(s);
            let mut c = ctx.clone();
            if erase {
                c.ability_source_contract = None
            }
            if unknown {
                c.stack_item_id = Some(crate::ids::StackItemId(u64::MAX));
            }
            let draw = EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            };
            let before = bad.clone();
            assert!(
                crate::effect::begin_resumable_resolution(&draw, &c, row.clone(), &mut bad)
                    .is_err()
            );
            assert_eq!(bad, before);
            crate::effect::execute(&draw, &c, &mut bad);
            assert!(bad.engine.halted.is_some());
            bad.engine.halted = None;
            assert_eq!(bad, before);
            let mut bad = restore(s);
            let before = bad.clone();
            crate::effect::execute(op, &c, &mut bad);
            assert!(bad.engine.halted.is_some());
            bad.engine.halted = None;
            assert_eq!(bad, before);
            let mut item = row.clone();
            item.kind = StackItemKind::Spell;
            item.inline_effect = Some(draw.clone());
            item.v4.ability_source_contract = None;
            let mut bad = restore(s);
            bad.engine.pending_effect = Some(EffectContinuation {
                resolving_item: item.clone(),
                ctx: c.clone(),
                frames: vec![EffectFrame::Program {
                    op: draw.clone(),
                    path: vec![],
                }],
                choice: None,
                answered_choice_guard: None,
            });
            let before = bad.clone();
            assert!(crate::effect::resume_resumable_resolution(&mut bad).is_err());
            assert_eq!(bad, before);
            assert!(crate::effect::validate_pending_effect_choice(&bad).is_err());
            assert_eq!(bad, before);
            let mut bad = restore(s);
            let before = bad.clone();
            assert!(crate::effect::begin_resumable_resolution(&draw, &c, item, &mut bad).is_err());
            assert_eq!(bad, before);
        }
    }
}

#[test]
fn paid_printing_modes_announce_before_targets_and_spirit_batch_season_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let mut s = ready(p);
            if season {
                let d = cast(&mut s, p, "Doubling Season", &[], 5);
                finish(&mut s);
                assert_eq!(s.objects.get(d).zone, Zone::Battlefield)
            }
            let k = paid(&mut s, p);
            start(&mut s, p, k);
            assert!(
                matches!(next(&mut s),Decision::ChooseTriggerMode{legal_modes,..}if legal_modes==vec![1])
            );
            let before = s.clone();
            assert!(engine::step(&mut s, Action::ChooseTriggerMode(0)).is_err());
            assert_eq!(s, before);
            assert!(engine::step(&mut s, Action::ChooseTriggerMode(2)).is_err());
            assert_eq!(s, before);
            let mut r = restore(&s);
            let row = choose(&mut s, p, 1, None);
            assert_eq!(row, choose(&mut r, p, 1, None));
            unchanged_public_refusals(&s, &row);
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            let spirits: Vec<_> = s
                .objects
                .iter()
                .filter(|(_, o)| {
                    o.zone == Zone::Battlefield && o.card_def == id("Kykar Spirit Token")
                })
                .collect();
            assert_eq!(spirits.len(), if season { 2 } else { 1 });
            for (o, live) in spirits {
                assert!(live.v4.is_token);
                assert_eq!(live.controller, p);
                assert_eq!(
                    (
                        engine::effective_power(&s, o),
                        engine::effective_toughness(&s, o)
                    ),
                    (1, 1)
                );
                assert!(engine::has_effective_keyword(&s, o, Keywords::FLYING));
            }
            assert!(crate::optional_trigger_public_v1::validate_state_checked(&s).is_err());
            assert!(card_def::preflight_fully_supported_deck(&[id(NAME)]).is_err());
            assert!(card_def::preflight_fully_supported_deck(&[id("Kykar Spirit Token")]).is_err());
        }
    }
}
#[test]
fn paid_exile_response_source_departure_owner_return_and_delayed_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", &[], 1);
        finish(&mut s);
        start(&mut s, p, k);
        let row = choose(&mut s, p, 0, Some(elf));
        unchanged_public_refusals(&s, &row);
        // A paid opposing response removes Kykar after target announcement.
        let bolt = cast(
            &mut s,
            p.opponent(),
            "Luminous Rebuke",
            &[Target::Object(k)],
            5,
        );
        finish(&mut s);
        assert_eq!(s.objects.get(bolt).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(k).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(elf).zone, Zone::Exile);
        let (index, link) = schedule(&s);
        assert_eq!(link.object, elf);
        validate_schedule(&s, index).unwrap();
        let mut r = restore(&s);
        let row = return_row(&mut s, index);
        assert_eq!(row, return_row(&mut r, index));
        assert_eq!(s.step, Step::End);
        assert_eq!(s.active_player, p);
        unchanged_public_refusals(&s, &row);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(elf).zone, Zone::Battlefield);
        assert_eq!(s.objects.get(elf).controller, p);
        assert_eq!(
            s.objects.get(elf).zone_change_count,
            link.zone_change_count + 1
        );
        assert_eq!(s.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::KykarDelayedCollectedV1{schedule_history_index,..}if *schedule_history_index==index)).count(),1);
        assert!(crate::optional_trigger_public_v1::validate_state_checked(&s).is_err());
        later_end_keeps_consumed(&mut s, index);
        later_end_keeps_consumed(&mut r, index);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(elf).zone, Zone::Battlefield);
    }
}
#[test]
fn paid_target_bounce_fizzles_without_exile_schedule_and_noncreature_cast_only_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", &[], 1);
        finish(&mut s);
        assert!(s.engine.pending_triggers.is_empty());
        let other = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
        start(&mut s, p, k);
        choose(&mut s, p, 0, Some(elf));
        cast(
            &mut s,
            p.opponent(),
            "Run Away Together",
            &[Target::Object(elf), Target::Object(other)],
            2,
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(elf).zone, Zone::Hand);
        assert!(!s
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, event::CommittedEvent::KykarExiledV1 { .. })));
    }
}
#[test]
fn exact_exile_generation_stale_countered_return_and_token_cease_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for kind in 0..3 {
            let mut s = ready(p);
            let k = paid(&mut s, p);
            let elf = if kind == 2 {
                start(&mut s, p, k);
                choose(&mut s, p, 1, None);
                finish(&mut s);
                s.objects
                    .iter()
                    .find(|(_, o)| {
                        o.zone == Zone::Battlefield && o.card_def == id("Kykar Spirit Token")
                    })
                    .unwrap()
                    .0
            } else {
                let o = cast(&mut s, p, "Llanowar Elves", &[], 1);
                finish(&mut s);
                o
            };
            start(&mut s, p, k);
            choose(&mut s, p, 0, Some(elf));
            finish(&mut s);
            let (index, link) = schedule(&s);
            if kind == 0 {
                // Native zone primitives prove exact exile-generation refusal;
                // there is no claim of a paid exile-to-hand producer here.
                event::propose_and_commit(
                    &mut s,
                    event::ProposedEvent::zone_change(elf, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut s,
                    event::ProposedEvent::zone_change(elf, Zone::Exile),
                );
                assert_ne!(s.objects.get(elf).zone_change_count, link.zone_change_count);
            }
            let mut r = restore(&s);
            return_row(&mut s, index);
            return_row(&mut r, index);
            if kind == 1 {
                // The native counter primitive removes this exact published trigger.
                let row = s.stack.last().unwrap().clone();
                assert_eq!(
                    row.inline_effect,
                    Some(EffectOp::KykarReturnV1 {
                        schedule_history_index: index
                    })
                );
                engine::counter_stack_item_by_id(&mut s, row.v4.stack_item_id).unwrap();
                engine::counter_stack_item_by_id(&mut r, row.v4.stack_item_id).unwrap();
            }
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            assert!(!s.players[p.index()].battlefield.contains(&elf));
            let count=s.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::KykarDelayedCollectedV1{schedule_history_index,..}if *schedule_history_index==index)).count();
            assert_eq!(count, 1);
            later_end_keeps_consumed(&mut s, index);
            later_end_keeps_consumed(&mut r, index);
            assert_eq!(s, r);
            assert!(!s.players[p.index()].battlefield.contains(&elf));
        }
    }
}
#[test]
fn native_control_primitive_returns_physical_owner_and_original_controller_ignores_later_control() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = paid(&mut s, p);
        let elf = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
        // Genuine native control layer primitive; this is not a paid control spell.
        crate::standard_cards_v1::gain_control(&mut s, elf, p);
        start(&mut s, p, k);
        let row = choose(&mut s, p, 0, Some(elf));
        crate::standard_cards_v1::gain_control(&mut s, k, p.opponent());
        finish(&mut s);
        let (index, _) = schedule(&s);
        let mut r = restore(&s);
        let delayed = return_row(&mut s, index);
        assert_eq!(delayed.controller, p);
        assert_eq!(
            delayed.v4.ability_source_contract,
            row.v4.ability_source_contract
        );
        return_row(&mut r, index);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(elf).controller, p.opponent());
        assert_eq!(s.objects.get(elf).owner, p.opponent());
    }
}
#[test]
fn delayed_receipt_forgery_and_hidden_opponent_roster_refuse_without_change_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", &[], 1);
        finish(&mut s);
        start(&mut s, p, k);
        choose(&mut s, p, 0, Some(elf));
        finish(&mut s);
        let (index, _) = schedule(&s);
        let row = return_row(&mut s, index);
        let before = serde_json::to_value(&s).unwrap();
        assert!(refuse_legacy(&s).is_err());
        assert_eq!(serde_json::to_value(&s).unwrap(), before);
        for mode in 0..4 {
            let mut bad = restore(&s);
            match mode {
                0 => {
                    if let event::CommittedEvent::KykarExiledV1 { owner, .. } =
                        &mut bad.engine.event_history[index as usize]
                    {
                        *owner = owner.opponent()
                    }
                }
                1 => {
                    if let event::CommittedEvent::KykarExiledV1 { exiled, .. } =
                        &mut bad.engine.event_history[index as usize]
                    {
                        exiled.zone_change_count += 1
                    }
                }
                2 => bad.engine.event_history.retain(|e| {
                    !matches!(e, event::CommittedEvent::KykarDelayedCollectedV1 { .. })
                }),
                _ => {
                    let e = bad
                        .engine
                        .event_history
                        .iter()
                        .find(|e| {
                            matches!(e, event::CommittedEvent::KykarDelayedCollectedV1 { .. })
                        })
                        .unwrap()
                        .clone();
                    bad.engine.event_history.push(e)
                }
            };
            let old = bad.clone();
            assert!(engine::validated_stack_item_target_spec(&row, &bad).is_err());
            assert_eq!(bad, old);
        }
        // No new observer grants access to the opponent's private hand/library.
        let mut other = restore(&s);
        let private = put(&mut other, p.opponent(), "Forest", Zone::Hand);
        let _ = private;
        assert_eq!(row, s.stack.last().unwrap().clone());
        assert!(refuse_legacy(&other).is_err());
    }
}

#[test]
fn paid_instant_on_opponent_turn_and_after_end_step_begins_uses_next_any_end_step_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for already_begun in [false, true] {
            let mut s = ready(p);
            let k = paid(&mut s, p);
            let elf = cast(&mut s, p, "Llanowar Elves", &[], 1);
            finish(&mut s);
            for _ in 0..180 {
                let d = next(&mut s);
                let wanted = if already_begun {
                    s.active_player == p && s.step == Step::End
                } else {
                    s.active_player == p.opponent() && s.step == Step::Main1
                };
                if wanted && matches!(d, Decision::CastSpellOrPass { .. }) {
                    break;
                }
                pass(&mut s, d);
            }
            assert_eq!(
                s.active_player,
                if already_begun { p } else { p.opponent() }
            );
            assert_eq!(
                s.step,
                if already_begun {
                    Step::End
                } else {
                    Step::Main1
                }
            );
            start_named(&mut s, p, k, "Think Twice", 2);
            choose(&mut s, p, 0, Some(elf));
            finish(&mut s);
            let (index, _) = schedule(&s);
            assert_eq!(s.objects.get(elf).zone, Zone::Exile);
            let mut r = restore(&s);
            return_row(&mut s, index);
            return_row(&mut r, index);
            assert_eq!(s.active_player, p.opponent());
            assert_eq!(s.step, Step::End);
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            assert_eq!(s.objects.get(elf).zone, Zone::Battlefield);
        }
    }
}
#[test]
fn paid_countered_printing_and_generic_partial_spirit_creation_refuse_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = cast(&mut s, p, NAME, &[], 4);
        cast(
            &mut s,
            p.opponent(),
            "Counterspell",
            &[Target::Object(k)],
            2,
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(k).zone, Zone::Graveyard);
        assert_eq!(tapped(&s, p), 4);
        assert_eq!(tapped(&s, p.opponent()), 2);
        assert!(s.engine.pending_triggers.is_empty());
        assert!(definition(474));
        assert!(token_definition(475));
        let proposal = event::ProposedEvent::create_token(475, p);
        for route in 0..3 {
            let mut bad = restore(&s);
            let original = bad.clone();
            match route {
                0 => event::propose_and_commit(&mut bad, proposal.clone()),
                1 => event::commit(&mut bad, proposal.clone()),
                _ => event::propose_and_commit_batch(&mut bad, vec![proposal.clone()]),
            };
            assert!(bad.engine.halted.is_some());
            bad.engine.halted = None;
            assert_eq!(bad, original);
        }
    }
}

#[test]
fn paid_independent_hidden_worlds_preserve_native_modes_targets_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut a = ready_hidden(p, "Forest");
        let mut b = ready_hidden(p, "Counterspell");
        put(&mut a, p.opponent(), "Forest", Zone::Hand);
        put(&mut b, p.opponent(), "Counterspell", Zone::Hand);
        let ka = paid(&mut a, p);
        let kb = paid(&mut b, p);
        assert_eq!(ka, kb);
        let ea = cast(&mut a, p, "Llanowar Elves", &[], 1);
        let eb = cast(&mut b, p, "Llanowar Elves", &[], 1);
        assert_eq!(ea, eb);
        finish(&mut a);
        finish(&mut b);
        assert_eq!(start(&mut a, p, ka), start(&mut b, p, kb));
        assert_eq!(next(&mut a), next(&mut b));
        assert_eq!(
            choose(&mut a, p, 0, Some(ea)),
            choose(&mut b, p, 0, Some(eb))
        );
        assert_eq!(refuse_legacy(&a), refuse_legacy(&b));
        let mut ar = restore(&a);
        let mut br = restore(&b);
        finish(&mut a);
        finish(&mut b);
        finish(&mut ar);
        finish(&mut br);
        assert_eq!(a, ar);
        assert_eq!(b, br);
        assert_ne!(a.state_hash(), b.state_hash());
        assert_eq!(schedule(&a), schedule(&b));
    }
}

#[test]
fn native_copy_of_paid_kykar_retains_effective_source_and_paid_opposite_cast_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let original = paid(&mut s, p);
        // Native copy primitive under the other seat avoids a fabricated paid
        // copy spell and the legend-rule choice between same-controller copies.
        let copied =
            crate::standard_cards_v1::create_token_copy(&mut s, original, p.opponent()).unwrap();
        assert_eq!(s.objects.get(copied).card_def, id(NAME));
        assert!(s.objects.get(copied).v4.is_token);
        assert_eq!(s.objects.get(copied).controller, p.opponent());
        assert!(refuse_legacy(&s).is_err());
        start_named(&mut s, p.opponent(), copied, "Think Twice", 2);
        let row = choose(&mut s, p.opponent(), 1, None);
        let source = row.v4.ability_source_contract.unwrap();
        assert_eq!(
            (source.source, source.card_def, source.controller),
            (copied, 474, p.opponent())
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(
            s.objects
                .iter()
                .filter(|(_, o)| o.zone == Zone::Battlefield
                    && o.card_def == 475
                    && o.controller == p.opponent())
                .count(),
            1
        );
        assert_eq!(
            s.objects
                .iter()
                .filter(|(_, o)| o.zone == Zone::Battlefield
                    && o.card_def == 475
                    && o.controller == p)
                .count(),
            0
        );
    }
}

#[test]
fn paid_countered_return_missing_marker_cannot_reuse_equal_later_end_steps_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let k = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", &[], 1);
        finish(&mut s);
        start(&mut s, p, k);
        choose(&mut s, p, 0, Some(elf));
        finish(&mut s);
        let (index, link) = schedule(&s);
        let delayed = return_row(&mut s, index);
        engine::counter_stack_item_by_id(&mut s, delayed.v4.stack_item_id).unwrap();
        finish(&mut s);
        assert_eq!(s.objects.get(elf).zone, Zone::Exile);
        assert_eq!(s.objects.get(elf).zone_change_count, link.zone_change_count);
        let (first_end_index, first_end) = s
            .engine
            .event_history
            .iter()
            .enumerate()
            .skip(index as usize + 1)
            .find(|(_, e)| matches!(e, event::CommittedEvent::BeginningEndStep { .. }))
            .map(|(i, e)| (i, e.clone()))
            .unwrap();
        let mut restored = restore(&s);
        // With no attacks or deaths, each player's end-step value repeats on
        // the following turn. Keep the genuine consumed schedule through two
        // later occurrences equal to its first end-step event.
        for _ in 0..4 {
            later_end_keeps_consumed(&mut s, index);
            later_end_keeps_consumed(&mut restored, index);
            assert_eq!(s, restored);
            assert_eq!(s.objects.get(elf).zone, Zone::Exile);
            assert_eq!(s.objects.get(elf).zone_change_count, link.zone_change_count);
        }
        let equal_later = s.engine.event_history[first_end_index + 1..]
            .iter()
            .filter(|e| **e == first_end)
            .count();
        assert_eq!(equal_later, 2);
        let latest = s
            .engine
            .event_history
            .iter()
            .rev()
            .find(|e| matches!(e, event::CommittedEvent::BeginningEndStep { .. }))
            .unwrap()
            .clone();
        assert_eq!(latest, first_end);
        let intact = s.clone();
        assert!(collect(&mut s, std::slice::from_ref(&latest))
            .unwrap()
            .is_empty());
        assert_eq!(s, intact);
        let mut bad = restore(&intact);
        let before_len = bad.engine.event_history.len();
        bad.engine.event_history.retain(|e| {
            !matches!(e,
            event::CommittedEvent::KykarDelayedCollectedV1 { schedule_history_index, .. }
                if *schedule_history_index == index)
        });
        assert_eq!(bad.engine.event_history.len() + 1, before_len);
        // Explicit single-marker corruption primitive, using the latest actual
        // native event as collector input. No valid engine path deletes this
        // receipt, and event value equality is not cryptographic provenance.
        let before = bad.clone();
        assert!(collect(&mut bad, std::slice::from_ref(&latest)).is_err());
        assert_eq!(bad, before);
        assert_eq!(bad.objects.get(elf).zone, Zone::Exile);
        assert_eq!(
            bad.objects.get(elf).zone_change_count,
            link.zone_change_count
        );
        assert!(!bad.stack.iter().any(|r| r.inline_effect
            == Some(EffectOp::KykarReturnV1 {
                schedule_history_index: index
            })));
    }
}

fn paid_spirit_copy_scene(s: &mut GameState, p: PlayerId) -> ObjectId {
    let k = paid(s, p);
    start(s, p, k);
    choose(s, p, 1, None);
    finish(s);
    let spirit = s
        .objects
        .iter()
        .find_map(|(id, o)| {
            (o.zone == Zone::Battlefield && o.controller == p && token_definition(o.card_def))
                .then_some(id)
        })
        .unwrap();
    // Native departure primitive removes unrelated future cast triggers. The
    // Spirit itself was created by the genuinely paid Kykar/cast/mode route.
    event::propose_and_commit(s, event::ProposedEvent::zone_change(k, Zone::Hand));
    assert_eq!(s.objects.get(k).zone, Zone::Hand);
    spirit
}
fn spirit_objects(s: &GameState, p: PlayerId) -> Vec<ObjectId> {
    s.objects
        .iter()
        .filter_map(|(id, o)| {
            (o.zone == Zone::Battlefield && o.controller == p && token_definition(o.card_def))
                .then_some(id)
        })
        .collect()
}
#[test]
fn paid_spirit_self_reflection_electroduplicate_copy_of_copy_and_season_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let mut s = ready(p);
            // Keep four real initial blue sources for both paid Self-Reflections
            // after the earlier generic payments have consumed the first Islands.
            for _ in 0..4 {
                put(&mut s, p, "Island", Zone::Battlefield);
            }
            if season {
                cast(&mut s, p, "Doubling Season", &[], 5);
                finish(&mut s);
            }
            let spirit = paid_spirit_copy_scene(&mut s, p);
            let multiplier = if season { 2 } else { 1 };
            let before = spirit_objects(&s, p).len();
            cast(&mut s, p, "Self-Reflection", &[Target::Object(spirit)], 6);
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(spirit_objects(&s, p).len(), before + multiplier);
            let before = spirit_objects(&s, p);
            cast(&mut s, p, "Electroduplicate", &[Target::Object(spirit)], 3);
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            let copies = spirit_objects(&s, p)
                .into_iter()
                .filter(|id| !before.contains(id))
                .collect::<Vec<_>>();
            assert_eq!(copies.len(), multiplier);
            for id in &copies {
                assert_eq!(event::copiable_token_abilities_v1(&s, *id).unwrap(), 1);
                assert!(engine::has_effective_keyword(&s, *id, Keywords::HASTE));
            }
            let before = spirit_objects(&s, p);
            cast(
                &mut s,
                p,
                "Self-Reflection",
                &[Target::Object(copies[0])],
                6,
            );
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            let inherited = spirit_objects(&s, p)
                .into_iter()
                .filter(|id| !before.contains(id))
                .collect::<Vec<_>>();
            assert_eq!(inherited.len(), multiplier);
            for id in inherited.iter().chain(copies.iter()) {
                let o = s.objects.get(*id);
                assert!(o.v4.is_token);
                assert_eq!(o.owner, p);
                assert_eq!(engine::checked_effective_pt(&s, *id).unwrap(), (1, 1));
                assert_eq!(
                    card_def::CARD_DEFS[o.card_def as usize].colors,
                    [crate::mana::ManaColor::W]
                );
                assert!(engine::has_effective_keyword(&s, *id, Keywords::FLYING));
                assert_eq!(event::copiable_token_abilities_v1(&s, *id).unwrap(), 1);
            }
            // Real pass progression publishes and resolves each inherited
            // sacrifice ability; the original ordinary Spirits survive.
            for _ in 0..200 {
                let d = next(&mut s);
                if inherited
                    .iter()
                    .chain(copies.iter())
                    .all(|id| s.objects.get(*id).zone != Zone::Battlefield)
                {
                    break;
                }
                pass(&mut s, d);
            }
            assert!(inherited
                .iter()
                .chain(copies.iter())
                .all(|id| s.objects.get(*id).zone != Zone::Battlefield));
            assert_eq!(s.objects.get(spirit).zone, Zone::Battlefield);
            assert!(refuse_legacy(&s).is_err());
        }
    }
}
#[test]
fn paid_spirit_copy_stale_target_and_generic_or_forged_copy_authority_refuse_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for name in ["Self-Reflection", "Electroduplicate"] {
            let mut s = ready(p);
            let spirit = paid_spirit_copy_scene(&mut s, p);
            cast(
                &mut s,
                p,
                name,
                &[Target::Object(spirit)],
                if name == "Self-Reflection" { 6 } else { 3 },
            );
            let item = s.stack.last().unwrap().clone();
            let ctx = crystal_prevention_v1::context(&item);
            let op = if name == "Self-Reflection" {
                EffectOp::CreateControlledCreatureTargetCopyV1
            } else {
                EffectOp::ElectroduplicateCopyV1
            };
            let mut raw = s.clone();
            event::propose_and_commit(
                &mut raw,
                event::ProposedEvent::create_token(id("Kykar Spirit Token"), p),
            );
            assert!(raw.engine.halted.is_some());
            assert_eq!(raw.objects, s.objects);
            let mut raw = s.clone();
            super::super::execute(&op, &ctx, &mut raw);
            assert!(raw.engine.halted.is_some());
            assert_eq!(raw.objects, s.objects);
            let mut raw = s.clone();
            assert!(
                super::super::begin_resumable_resolution(&op, &ctx, item.clone(), &mut raw)
                    .is_err()
            );
            assert_eq!(raw, s);
            // An actual paid row supplies the private owner. Only its exact
            // proposal/context/target are eligible, never caller-supplied flags.
            let owner = super::super::acquire_native_execution_owner_v1(&s, &item)
                .unwrap()
                .unwrap();
            let mut input = s.clone();
            input.stack.pop();
            let proof = NativeExecutionProofV1 {
                owner: &owner,
                ctx: &ctx,
            };
            let before = input.clone();
            // Public validation still refuses an absent row. Only the private
            // native owner may lend its exact original for copy validation.
            assert!(crate::engine::validated_stack_item_target_spec(&item, &input).is_err());
            let proposal = spirit_copy_proposal_v1(&input, &proof).unwrap().unwrap();
            assert_eq!(input, before);
            let good = SpiritCopyPermitV1 {
                proof: &proof,
                proposal: proposal.clone(),
            };
            assert!(good.allows(&input, &good.events()));
            let mut foreign_stack = input.clone();
            foreign_stack.stack.push(item.clone());
            let before = foreign_stack.clone();
            assert!(!good.allows(&foreign_stack, &good.events()));
            assert!(spirit_copy_proposal_v1(&foreign_stack, &proof).is_err());
            assert_eq!(foreign_stack, before);
            let mut stale = input.clone();
            event::propose_and_commit(
                &mut stale,
                event::ProposedEvent::zone_change(spirit, Zone::Hand),
            );
            assert!(!good.allows(&stale, &good.events()));
            let stale_before = stale.clone();
            assert!(event::commit_spirit_copy_owned_v1(&mut stale, good).is_err());
            assert_eq!(stale, stale_before);
            let mut changed = proposal.clone();
            if let event::ProposedEvent::CreateToken(t) = &mut changed {
                t.controller = p.opponent();
            }
            let bad = SpiritCopyPermitV1 {
                proof: &proof,
                proposal: changed,
            };
            assert!(!bad.allows(&input, &bad.events()));
            let mut foreign = ctx.clone();
            foreign.controller = p.opponent();
            let foreign_proof = NativeExecutionProofV1 {
                owner: &owner,
                ctx: &foreign,
            };
            assert!(spirit_copy_proposal_v1(&input, &foreign_proof).is_err());
            let mut changed = proposal;
            if let event::ProposedEvent::CreateToken(t) = &mut changed {
                t.initialization_v48
                    .as_mut()
                    .unwrap()
                    .copied_end_step_sacrifices_v1 += 1;
            }
            let bad = SpiritCopyPermitV1 {
                proof: &proof,
                proposal: changed,
            };
            assert!(!bad.allows(&input, &bad.events()));
            let before = input.clone();
            assert!(event::commit_spirit_copy_owned_v1(&mut input, bad).is_err());
            assert_eq!(input, before);
            // Explicit native target-removal primitive after genuine payment.
            let before = spirit_objects(&s, p).len();
            event::propose_and_commit(
                &mut s,
                event::ProposedEvent::zone_change(spirit, Zone::Hand),
            );
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(spirit_objects(&s, p).len(), before - 1);
        }
    }
}
#[test]
fn independent_hidden_worlds_keep_genuine_paid_spirit_copy_results_equal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut a = ready_hidden(p, "Forest");
        let mut b = ready_hidden(p, "Island");
        let sa = paid_spirit_copy_scene(&mut a, p);
        let sb = paid_spirit_copy_scene(&mut b, p);
        assert_eq!(sa, sb);
        for name in ["Self-Reflection", "Electroduplicate"] {
            cast(
                &mut a,
                p,
                name,
                &[Target::Object(sa)],
                if name == "Self-Reflection" { 6 } else { 3 },
            );
            cast(
                &mut b,
                p,
                name,
                &[Target::Object(sb)],
                if name == "Self-Reflection" { 6 } else { 3 },
            );
            assert_eq!(a.stack, b.stack);
            finish(&mut a);
            finish(&mut b);
            let ids = spirit_objects(&a, p);
            assert_eq!(ids, spirit_objects(&b, p));
            for id in ids {
                assert_eq!(a.objects.get(id), b.objects.get(id));
            }
        }
    }
}

#[test]
fn genuine_native_spell_copies_of_paid_spirit_copy_spells_keep_ancestry_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            for name in ["Self-Reflection", "Electroduplicate"] {
                let mut s = ready(p);
                // Season, Kykar and Amulet can consume the original blue sources.
                // Fund the selected paid Self-Reflection with actual initial lands.
                for _ in 0..2 {
                    put(&mut s, p, "Island", Zone::Battlefield);
                }
                if season {
                    cast(&mut s, p, "Doubling Season", &[], 5);
                    finish(&mut s);
                }
                let spirit = paid_spirit_copy_scene(&mut s, p);
                let before = spirit_objects(&s, p).len();
                let original = cast(
                    &mut s,
                    p,
                    name,
                    &[Target::Object(spirit)],
                    if name == "Self-Reflection" { 6 } else { 3 },
                );
                // Existing native spell-copy primitive, not a paid copy-spell
                // producer claim. It inherits the genuine chosen target and
                // creates the actual arena/parent/source contract ancestry.
                let (copy, stack_id) = engine::copy_spell_for(&mut s, original, p).unwrap();
                let row = s
                    .stack
                    .iter()
                    .find(|r| r.v4.stack_item_id == stack_id)
                    .unwrap()
                    .clone();
                assert!(row.is_copy);
                assert!(!row.is_flashback);
                assert_eq!(row.targets, [Target::Object(spirit)]);
                assert!(s.objects.get(copy).spell_copy_origin.is_some());
                engine::validated_stack_item_target_spec(&row, &s).unwrap();
                let mut bad = restore(&s);
                bad.objects.get_mut(copy).spell_copy_origin = None;
                let corrupt = bad.clone();
                assert!(engine::step(&mut bad, Action::Pass).is_err());
                assert_eq!(bad, corrupt);
                let mut replay = restore(&s);
                finish(&mut s);
                finish(&mut replay);
                assert_eq!(s, replay);
                assert_eq!(
                    spirit_objects(&s, p).len(),
                    before + 2 * if season { 2 } else { 1 }
                );
                let expected = usize::from(name == "Electroduplicate") as u32;
                assert_eq!(
                    spirit_objects(&s, p)
                        .iter()
                        .filter(
                            |id| event::copiable_token_abilities_v1(&s, **id).unwrap() == expected
                        )
                        .count(),
                    if expected == 0 {
                        spirit_objects(&s, p).len()
                    } else {
                        2 * if season { 2 } else { 1 }
                    }
                );
                assert!(!s.stack.iter().any(|r| r.source == copy));
                assert_eq!(s.objects.get(original).zone, Zone::Graveyard);
            }
        }
    }
}
