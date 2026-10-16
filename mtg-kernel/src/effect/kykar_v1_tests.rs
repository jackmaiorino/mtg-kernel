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
        assert_eq!(s.step, Step::EndStep);
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
                    s.active_player == p && s.step == Step::EndStep
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
                    Step::EndStep
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
            assert_eq!(s.step, Step::EndStep);
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
        assert_eq!(
            serde_json::to_value(next(&mut a)).unwrap(),
            serde_json::to_value(next(&mut b)).unwrap()
        );
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
