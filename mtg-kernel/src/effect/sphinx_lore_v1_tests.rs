//! Genuine paid native printing/cost/attack fixtures. Native setup/control/copy
//! primitives below are labelled separately from genuinely paid spells.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
    state::{StackItemKind, Step},
};
const NAME: &str = "Sphinx of Forgotten Lore";
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
        110_477,
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
        Decision::Discard { player, count, .. } => Action::Discard(
            s.players[player.index()]
                .hand
                .iter()
                .copied()
                .take(count as usize)
                .collect(),
        ),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        d => panic!("Sphinx Lore native decision {d:?}"),
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
    let sphinx = cast(s, p, NAME, &[], 4);
    finish(s);
    assert_eq!(s.objects.get(sphinx).zone, Zone::Battlefield);
    assert_eq!(engine::checked_effective_pt(s, sphinx).unwrap(), (3, 3));
    assert!(engine::has_effective_keyword(s, sphinx, Keywords::FLASH));
    assert!(engine::has_effective_keyword(s, sphinx, Keywords::FLYING));
    sphinx
}
fn next_main(s: &mut GameState, p: PlayerId, after: u32) {
    for _ in 0..192 {
        let d = next(s);
        if s.turn > after
            && s.active_player == p
            && s.step == Step::Main1
            && matches!(d,Decision::CastSpellOrPass{player,..}if player==p)
        {
            return;
        }
        pass(s, d)
    }
    panic!("next genuine main absent");
}
fn main2(s: &mut GameState, p: PlayerId) {
    for _ in 0..80 {
        let d = next(s);
        if s.active_player == p
            && s.step == Step::Main2
            && matches!(d,Decision::CastSpellOrPass{player,..}if player==p)
        {
            return;
        }
        pass(s, d)
    }
    panic!("main2 absent");
}
fn attack(s: &mut GameState, p: PlayerId, sphinx: ObjectId, target: ObjectId) -> StackItem {
    let mut declared = false;
    let mut targeted = false;
    for _ in 0..96 {
        let d = next(s);
        match d {
            Decision::DeclareAttackers { player, eligible } if player == p && !declared => {
                assert!(eligible.contains(&sphinx));
                engine::step(s, Action::DeclareAttackers(vec![sphinx])).unwrap();
                declared = true;
            }
            Decision::ChooseTargets {
                player,
                spell,
                legal_targets,
                can_finish,
                ..
            } if declared => {
                assert_eq!((player, spell, can_finish), (p, sphinx, false));
                assert!(legal_targets.contains(&Target::Object(target)));
                assert!(legal_targets.iter().all(|t|matches!(t,Target::Object(id)if s.objects.get(*id).owner==p&&s.objects.get(*id).zone==Zone::Graveyard&&card_def::CARD_DEFS[s.objects.get(*id).card_def as usize].types.iter().any(|t|matches!(t,CardType::Instant|CardType::Sorcery)))));
                let before = s.clone();
                assert!(engine::step(s, Action::ChooseTarget(Target::Object(sphinx))).is_err());
                assert_eq!(*s, before);
                let mut replay = restore(s);
                engine::step(&mut replay, Action::ChooseTarget(Target::Object(target))).unwrap();
                engine::step(s, Action::ChooseTarget(Target::Object(target))).unwrap();
                assert_eq!(*s, replay);
                targeted = true;
            }
            Decision::CastSpellOrPass { .. }
                if targeted
                    && s.stack.iter().any(|r| {
                        r.source == sphinx && r.inline_effect.as_ref().is_some_and(recipe)
                    }) =>
            {
                let row = s
                    .stack
                    .iter()
                    .find(|r| r.source == sphinx && r.inline_effect.as_ref().is_some_and(recipe))
                    .unwrap()
                    .clone();
                assert_eq!(s.engine.combat.attackers, [sphinx]);
                validate_published(s, &row).unwrap();
                engine::validated_stack_item_target_spec(&row, s).unwrap();
                return row;
            }
            d => pass(s, d),
        }
    }
    panic!("authentic targeted attack not published");
}
fn scenario(p: PlayerId, name: &str) -> (GameState, ObjectId, ObjectId, StackItem) {
    let mut s = ready(p);
    let target = put(&mut s, p, name, Zone::Graveyard);
    let sphinx = paid(&mut s, p);
    let turn = s.turn;
    next_main(&mut s, p, turn);
    let row = attack(&mut s, p, sphinx, target);
    (s, sphinx, target, row)
}
fn install(s: &mut GameState, p: PlayerId, target: ObjectId) -> u64 {
    finish(s);
    let grants = active_grants(s, target, p);
    assert_eq!(grants.len(), 1);
    installation(s, grants[0]).unwrap();
    grants[0]
}
fn pay_existing(
    s: &mut GameState,
    p: PlayerId,
    card: ObjectId,
    granted: bool,
    targets: &[Target],
    selections: (u16, bool, Option<ObjectId>),
) -> StackItem {
    let (x, kick, discard) = selections;
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&card))
    );
    engine::step(s, Action::CastSpell(card)).unwrap();
    let mut target_index = 0;
    for _ in 0..48 {
        match next(s) {
            Decision::ChooseEffectOption {
                player,
                source,
                option_count,
            } => {
                assert_eq!((player, source), (p, card));
                let pending = s.engine.pending_cast.as_ref().unwrap();
                let ordinal = if pending.sphinx_lore_v1.is_some_and(|c| c.selected.is_none()) {
                    let c = pending.sphinx_lore_v1.unwrap();
                    let event::CommittedEvent::SphinxLoreCastOptionsV1 { options } =
                        &s.engine.event_history[c.options_history_index as usize]
                    else {
                        panic!()
                    };
                    options
                        .routes
                        .iter()
                        .position(|r| {
                            matches!(r, engine::sphinx_lore_cast_v1::RouteV1::Granted { .. })
                                == granted
                        })
                        .unwrap() as u16
                } else {
                    x
                };
                assert!(ordinal < option_count);
                let mut replay = restore(s);
                engine::step(s, Action::ChooseEffectOption(ordinal)).unwrap();
                engine::step(&mut replay, Action::ChooseEffectOption(ordinal)).unwrap();
                assert_eq!(*s, replay);
            }
            Decision::ChooseKicker { player, spell } => {
                assert_eq!((player, spell), (p, card));
                engine::step(s, Action::ChooseKicker(kick)).unwrap();
            }
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, p);
                assert!(legal_targets.contains(&targets[target_index]));
                engine::step(s, Action::ChooseTarget(targets[target_index])).unwrap();
                target_index += 1;
            }
            Decision::ChooseCostTargets {
                player, candidates, ..
            } => {
                assert_eq!(player, p);
                assert!(
                    !granted,
                    "granted mana base must not inherit printed land sacrifice"
                );
                let mountain = *candidates
                    .iter()
                    .find(|o| s.objects.get(**o).card_def == id("Mountain"))
                    .unwrap();
                engine::step(s, Action::ChooseCostTarget(mountain)).unwrap();
            }
            Decision::Discard { player, count, .. } => {
                assert_eq!((player, count), (p, 1));
                let chosen = discard.unwrap();
                assert!(s.players[p.index()].hand.contains(&chosen));
                engine::step(s, Action::Discard(vec![chosen])).unwrap();
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert!(s.engine.pending_cast.is_none());
                assert_eq!(target_index, targets.len());
                let row = s
                    .stack
                    .iter()
                    .find(|r| r.source == card && r.kind == StackItemKind::Spell)
                    .unwrap()
                    .clone();
                engine::validated_stack_item_target_spec(&row, s).unwrap();
                return row;
            }
            d => panic!("native selected cast {d:?}"),
        }
    }
    panic!("selected cast not paid");
}
#[test]
fn exact_printing_flash_paid_on_opponents_turn_and_legacy_refusal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let turn = s.turn;
        next_main(&mut s, p.opponent(), turn);
        let sphinx = paid(&mut s, p);
        assert!(definition(id(NAME)));
        assert_eq!(id(NAME), 477);
        assert!(refuse_legacy(&s).is_err());
        assert!(crate::optional_trigger_public_v1::validate_state_checked(&s).is_err());
        assert_eq!(
            card_def::CARD_DEFS[id(NAME) as usize].capability,
            card_def::CardCapability::Partial
        );
        assert_eq!(s.objects.get(sphinx).owner, p);
    }
}
#[test]
fn paid_attack_target_response_source_death_installs_original_controller_and_paid_bolt_exiles_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, sphinx, bolt, row) = scenario(p, "Lightning Bolt");
        let captured = row.v4.ability_source_contract.unwrap();
        cast(
            &mut s,
            p.opponent(),
            "Lightning Bolt",
            &[Target::Object(sphinx)],
            1,
        );
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(sphinx).zone, Zone::Graveyard);
        assert_eq!(active_grants(&s, bolt, p).len(), 1);
        assert!(active_grants(&s, bolt, p.opponent()).is_empty());
        assert_eq!(captured.controller, p);
        let before = tapped(&s, p);
        let life = s.players[p.opponent().index()].life;
        let castrow = pay_existing(
            &mut s,
            p,
            bolt,
            true,
            &[Target::Player(p.opponent())],
            (0, false, None),
        );
        assert!(castrow.is_flashback);
        assert!(matches!(
            castrow
                .v4
                .source_contract
                .unwrap()
                .spell_cast_origin
                .unwrap()
                .route,
            crate::state::SpellCastRouteV4::SphinxLoreFlashbackV1 { .. }
        ));
        assert_eq!(tapped(&s, p) - before, 1);
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(bolt).zone, Zone::Exile);
        assert_eq!(s.players[p.opponent().index()].life, life - 3);
    }
}
#[test]
fn actual_deep_analysis_printed_and_granted_costs_remain_distinct_with_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for granted in [false, true] {
            let (mut s, _, card, _) = scenario(p, "Deep Analysis");
            install(&mut s, p, card);
            main2(&mut s, p);
            let before = tapped(&s, p);
            let life = s.players[p.index()].life;
            let hand = s.players[p.index()].hand.len();
            let row = pay_existing(
                &mut s,
                p,
                card,
                granted,
                &[Target::Player(p)],
                (0, false, None),
            );
            assert!(row.is_flashback);
            assert_eq!(tapped(&s, p) - before, if granted { 4 } else { 2 });
            assert_eq!(
                s.players[p.index()].life,
                life - if granted { 0 } else { 3 }
            );
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(s.players[p.index()].hand.len(), hand + 2);
            assert_eq!(s.objects.get(card).zone, Zone::Exile);
        }
    }
}
#[test]
fn actual_lava_dart_granted_mana_never_inherits_printed_mountain_sacrifice_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for granted in [false, true] {
            let (mut s, _, card, _) = scenario(p, "Lava Dart");
            install(&mut s, p, card);
            let before = tapped(&s, p);
            let lands = s.players[p.index()]
                .battlefield
                .iter()
                .filter(|o| s.objects.get(**o).card_def == id("Mountain"))
                .count();
            let row = pay_existing(
                &mut s,
                p,
                card,
                granted,
                &[Target::Player(p.opponent())],
                (0, false, None),
            );
            assert!(row.is_flashback);
            assert_eq!(tapped(&s, p) - before, usize::from(granted));
            assert_eq!(
                s.players[p.index()]
                    .battlefield
                    .iter()
                    .filter(|o| s.objects.get(**o).card_def == id("Mountain"))
                    .count(),
                lands - usize::from(!granted)
            );
            finish(&mut s);
            assert_eq!(s.objects.get(card).zone, Zone::Exile);
        }
    }
}
#[test]
fn actual_cleanup_expires_new_offers_and_target_reentry_never_inherits_original_grant_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, _, card, _) = scenario(p, "Lightning Bolt");
        let index = install(&mut s, p, card);
        let turn = s.turn;
        let mut reentered = restore(&s);
        event::propose_and_commit(
            &mut reentered,
            event::ProposedEvent::zone_change(card, Zone::Hand),
        );
        event::propose_and_commit(
            &mut reentered,
            event::ProposedEvent::zone_change(card, Zone::Graveyard),
        );
        assert!(active_grants(&reentered, card, p).is_empty());
        assert!(
            matches!(next(&mut reentered),Decision::CastSpellOrPass{castable_spells,..}if !castable_spells.contains(&card))
        );
        next_main(&mut s, p, turn);
        assert!(active_grants(&s, card, p).is_empty());
        installation(&s, index).unwrap();
        assert!(s.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::SphinxLoreV1{journal:JournalV1::Cleanup{installations,..}}if installations.contains(&index))));
        let mut replay = restore(&s);
        assert!(
            matches!(next(&mut replay),Decision::CastSpellOrPass{castable_spells,..}if !castable_spells.contains(&card))
        );
    }
}
#[test]
fn paid_counter_fizzle_and_native_copy_departure_keep_flashback_costs_and_exile_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let (mut s, _, card, _) = scenario(p, "Lightning Bolt");
            install(&mut s, p, card);
            let target = put(&mut s, p.opponent(), "Bear Cub", Zone::Battlefield);
            let before = tapped(&s, p);
            let row = pay_existing(
                &mut s,
                p,
                card,
                true,
                &[Target::Object(target)],
                (0, false, None),
            );
            assert_eq!(tapped(&s, p) - before, 1);
            let mut copy = None;
            match mode {
                0 => {
                    cast(
                        &mut s,
                        p.opponent(),
                        "Counterspell",
                        &[Target::Object(card)],
                        2,
                    );
                }
                1 => {
                    // Native target-departure primitive, not a paid removal claim.
                    event::propose_and_commit(
                        &mut s,
                        event::ProposedEvent::zone_change(target, Zone::Hand),
                    );
                }
                _ => {
                    // Existing native spell-copy primitive, not a paid copying-spell claim.
                    copy = Some(engine::copy_spell_for(&mut s, card, p).unwrap().0);
                    let c = s.stack.last().unwrap();
                    assert!(c.is_copy && !c.is_flashback);
                    assert_eq!(c.v4.cast_method, Some(crate::state::CastMethodV4::Normal));
                    assert!(c.v4.source_contract.unwrap().spell_cast_origin.is_none());
                }
            }
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(s.objects.get(card).zone, Zone::Exile);
            if let Some(c) = copy {
                assert!(!s.exile.contains(&c));
                assert!(!s.players[p.index()].graveyard.contains(&c));
            }
            assert!(row.is_flashback);
        }
    }
}
#[test]
fn granted_additional_discard_x_and_kicker_use_complete_native_costs_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for name in [
            "Thrill of Possibility",
            "Goblin Negotiation",
            "Burst Lightning",
        ] {
            let (mut s, _, card, _) = scenario(p, name);
            install(&mut s, p, card);
            let discard = put(&mut s, p, "Forest", Zone::Hand);
            let bear = put(&mut s, p.opponent(), "Bear Cub", Zone::Battlefield);
            let targets = if name == "Thrill of Possibility" {
                vec![]
            } else if name == "Goblin Negotiation" {
                vec![Target::Object(bear)]
            } else {
                vec![Target::Player(p.opponent())]
            };
            let before = tapped(&s, p);
            let row = pay_existing(
                &mut s,
                p,
                card,
                true,
                &targets,
                (1, name == "Burst Lightning", Some(discard)),
            );
            assert_eq!(
                tapped(&s, p) - before,
                match name {
                    "Thrill of Possibility" => 2,
                    "Goblin Negotiation" => 3,
                    _ => 5,
                }
            );
            if name == "Goblin Negotiation" {
                assert_eq!(row.v4.x_value, 1);
            }
            if name == "Burst Lightning" {
                assert!(row.kicked);
            }
            if name == "Thrill of Possibility" {
                assert_eq!(s.objects.get(discard).zone, Zone::Graveyard);
                assert!(row.discarded.contains(&discard));
            }
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(s.objects.get(card).zone, Zone::Exile);
        }
    }
}
#[test]
fn real_published_attack_public_apis_and_grant_cast_tampering_refuse_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, _, card, row) = scenario(p, "Deep Analysis");
        let ctx = super::super::crystal_prevention_v1::context(&row);
        for mode in 0..4 {
            let mut bad = restore(&s);
            let mut item = row.clone();
            let mut context = ctx.clone();
            let op = EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            };
            match mode {
                0 => {}
                1 => {
                    item.inline_effect = Some(op.clone());
                    item.kind = StackItemKind::Spell;
                    context.stack_item_id = None;
                }
                2 => {
                    context.controller = p.opponent();
                }
                _ => {
                    context.ability_source_contract = None;
                    context.stack_item_id = Some(StackItemId(u64::MAX));
                }
            }
            let before = bad.clone();
            assert!(
                super::super::begin_resumable_resolution(&op, &context, item, &mut bad).is_err()
            );
            assert_eq!(bad, before);
            let mut resumed = before.clone();
            resumed.engine.pending_effect = Some(EffectContinuation {
                resolving_item: row.clone(),
                ctx: context.clone(),
                frames: vec![EffectFrame::Program {
                    op: op.clone(),
                    path: vec![],
                }],
                choice: None,
                answered_choice_guard: None,
            });
            let saved = resumed.clone();
            assert!(super::super::resume_resumable_resolution(&mut resumed).is_err());
            assert_eq!(resumed, saved);
            let mut execute = before.clone();
            super::super::execute(&op, &context, &mut execute);
            assert_eq!(
                execute.engine.halted,
                Some((
                    engine::UnsupportedMechanic::InvalidEffectContinuation,
                    context.source
                ))
            );
            execute.engine.halted = None;
            assert_eq!(execute, before);
        }
        install(&mut s, p, card);
        main2(&mut s, p);
        priority(&mut s, p);
        engine::step(&mut s, Action::CastSpell(card)).unwrap();
        assert!(matches!(
            next(&mut s),
            Decision::ChooseEffectOption {
                option_count: 2,
                ..
            }
        ));
        for mode in 0..3 {
            let mut bad = restore(&s);
            match mode {
                0 => bad.engine.pending_cast.as_mut().unwrap().sphinx_lore_v1 = None,
                1 => {
                    bad.engine
                        .pending_cast
                        .as_mut()
                        .unwrap()
                        .sphinx_lore_v1
                        .as_mut()
                        .unwrap()
                        .selected = Some(2)
                }
                _ => {
                    let i = bad
                        .engine
                        .pending_cast
                        .as_ref()
                        .unwrap()
                        .sphinx_lore_v1
                        .unwrap()
                        .options_history_index as usize;
                    let event::CommittedEvent::SphinxLoreCastOptionsV1 { options } =
                        &mut bad.engine.event_history[i]
                    else {
                        panic!()
                    };
                    options.graveyard_generation += 1;
                }
            }
            let before = bad.clone();
            assert!(engine::step(&mut bad, Action::ChooseEffectOption(0)).is_err());
            assert_eq!(bad, before);
        }
        let mut replay = restore(&s);
        engine::step(&mut s, Action::ChooseEffectOption(1)).unwrap();
        engine::step(&mut replay, Action::ChooseEffectOption(1)).unwrap();
        assert_eq!(next(&mut s), next(&mut replay));
        assert_eq!(s, replay);
        assert!(
            matches!(next(&mut s),Decision::ChooseTargets{legal_targets,..}if legal_targets.contains(&Target::Player(p)))
        );
        engine::step(&mut s, Action::ChooseTarget(Target::Player(p))).unwrap();
        engine::step(&mut replay, Action::ChooseTarget(Target::Player(p))).unwrap();
        assert!(matches!(next(&mut s),Decision::CastSpellOrPass{player,..}if player==p));
        assert_eq!(next(&mut s), next(&mut replay));
        assert_eq!(s, replay);
        let castrow = s.stack.last().unwrap().clone();
        engine::validated_stack_item_target_spec(&castrow, &s).unwrap();
        for mode in 0..3 {
            let mut bad = restore(&s);
            let index = bad
                .objects
                .get(card)
                .v4
                .finalized_cast_binding
                .unwrap()
                .sphinx_lore_cast_v1
                .unwrap() as usize;
            let event::CommittedEvent::SphinxLoreCastSelectedV1 { choice, source, .. } =
                &mut bad.engine.event_history[index]
            else {
                panic!()
            };
            match mode {
                0 => choice.selected = Some(0),
                1 => source.controller = p.opponent(),
                _ => {
                    source
                        .spell_cast_origin
                        .as_mut()
                        .unwrap()
                        .origin_zone_change_count += 1
                }
            }
            let before = bad.clone();
            assert!(engine::validated_stack_item_target_spec(&castrow, &bad).is_err());
            assert_eq!(bad, before);
        }
    }
}
fn copied_scene(p: PlayerId, hidden: &str) -> (GameState, ObjectId, ObjectId, StackItem) {
    let mut s = ready_hidden(p, hidden);
    let target = put(&mut s, p, "Lightning Bolt", Zone::Graveyard);
    let sphinx = paid(&mut s, p);
    cast(&mut s, p, "Self-Reflection", &[Target::Object(sphinx)], 6);
    finish(&mut s);
    let copy = s
        .objects
        .iter()
        .find_map(|(id, o)| {
            (id != sphinx && o.zone == Zone::Battlefield && o.v4.is_token && definition(o.card_def))
                .then_some(id)
        })
        .unwrap();
    cast(
        &mut s,
        p,
        "Witness Protection",
        &[Target::Object(sphinx)],
        1,
    );
    finish(&mut s);
    assert!(!crate::continuous_characteristics_v1::printed_abilities_active(&s, sphinx));
    let turn = s.turn;
    next_main(&mut s, p, turn);
    let row = attack(&mut s, p, copy, target);
    assert_eq!(row.source, copy);
    (s, copy, target, row)
}
#[test]
fn paid_self_reflection_and_witness_use_effective_attack_source_and_initial_hidden_worlds_stay_private_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, copy, target, row) = copied_scene(p, "Forest");
        let (mut other, other_copy, other_target, other_row) = copied_scene(p, "Island");
        assert_eq!((copy, target), (other_copy, other_target));
        assert_eq!(row, other_row);
        assert_eq!(next(&mut s), next(&mut other));
        let receipts = |s: &GameState| {
            s.engine
                .event_history
                .iter()
                .filter(|e| matches!(e, event::CommittedEvent::SphinxLoreV1 { .. }))
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(receipts(&s), receipts(&other));
        assert!(refuse_legacy(&s).is_err());
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(active_grants(&s, target, p).len(), 1);
    }
}

#[test]
fn original_target_generation_and_captured_controller_survive_real_attack_response_window_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for stale in [false, true] {
            let (mut s, sphinx, target, row) = scenario(p, "Lightning Bolt");
            // Native control/zone-change primitives above an actual paid attack root.
            crate::standard_cards_v1::gain_control(&mut s, sphinx, p.opponent());
            if stale {
                event::propose_and_commit(
                    &mut s,
                    event::ProposedEvent::zone_change(target, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut s,
                    event::ProposedEvent::zone_change(target, Zone::Graveyard),
                );
            }
            assert_eq!(row.controller, p);
            validate_published(&s, &row).unwrap();
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(active_grants(&s, target, p).len(), usize::from(!stale));
            assert!(active_grants(&s, target, p.opponent()).is_empty());
        }
    }
}

#[test]
fn unaffordable_installed_base_preserves_genuine_printed_flashback_without_new_choice_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let lib = [id("Forest"); 80];
        let mut s = GameState::new_from_libraries_with_starting_player_v1(
            &lib,
            &lib,
            crate::rl::card_name,
            110_477,
            p,
        );
        s.step = Step::Main1;
        for _ in 0..4 {
            put(&mut s, p, "Island", Zone::Battlefield);
        }
        let card = put(&mut s, p, "Deep Analysis", Zone::Graveyard);
        let sphinx = paid(&mut s, p);
        let turn = s.turn;
        next_main(&mut s, p, turn);
        cast(&mut s, p, "Think Twice", &[], 2);
        finish(&mut s);
        attack(&mut s, p, sphinx, card);
        install(&mut s, p, card);
        main2(&mut s, p);
        let before = tapped(&s, p);
        let life = s.players[p.index()].life;
        let row = pay_existing(
            &mut s,
            p,
            card,
            false,
            &[Target::Player(p)],
            (0, false, None),
        );
        assert_eq!(tapped(&s, p) - before, 2);
        assert_eq!(s.players[p.index()].life, life - 3);
        assert_eq!(
            row.v4
                .source_contract
                .unwrap()
                .spell_cast_origin
                .unwrap()
                .route,
            crate::state::SpellCastRouteV4::GraveyardFlashback
        );
        assert!(row
            .v4
            .source_contract
            .unwrap()
            .finalized_cast_binding
            .unwrap()
            .sphinx_lore_cast_v1
            .is_none());
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(card).zone, Zone::Exile);
    }
}
