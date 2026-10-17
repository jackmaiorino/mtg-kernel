//! Genuine paid Loot fixtures; all libraries precede the first native action.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    ids::PlayerId,
    policy_observation_v6::tests::put,
    state::{StackItemKind, Step},
};
const NAME: &str = "Loot, Exuberant Explorer";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(actor: PlayerId, prefix: &[&str]) -> GameState {
    // A real intervening draw consumes this first card before the next main.
    let mut cards = vec![id("Forest")];
    cards.extend(prefix.iter().map(|n| id(n)));
    let other = vec![id("Forest"); 32];
    let (a, b) = if actor == PlayerId::P0 {
        (&cards, &other)
    } else {
        (&other, &cards)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_473,
        actor,
    );
    s.step = Step::Main1;
    for _ in 0..6 {
        put(&mut s, actor, "Forest", Zone::Battlefield);
    }
    for name in ["Island", "Mountain"] {
        for _ in 0..6 {
            put(&mut s, actor.opponent(), name, Zone::Battlefield);
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
        d => panic!("native Loot decision {d:?}"),
    };
    engine::step(s, a).unwrap();
}
fn priority(s: &mut GameState, p: PlayerId) {
    for _ in 0..24 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==p) {
            return;
        }
        pass(s, d)
    }
    panic!("priority absent")
}
fn finish_stack(s: &mut GameState) {
    for _ in 0..48 {
        let d = next(s);
        if s.stack.is_empty() && s.engine.pending_effect.is_none() {
            return;
        }
        pass(s, d)
    }
    panic!("stack not completed")
}
fn paid(s: &mut GameState, p: PlayerId) -> ObjectId {
    let source = cast(s, p, NAME, &[], 3);
    finish_stack(s);
    assert_eq!(s.objects.get(source).zone, Zone::Battlefield);
    assert_eq!(
        (
            engine::effective_power(s, source),
            engine::effective_toughness(s, source)
        ),
        (1, 4)
    );
    source
}
fn next_own_main(s: &mut GameState, p: PlayerId, source: ObjectId) {
    for _ in 0..160 {
        let d = next(s);
        if s.active_player == p
            && s.step == Step::Main1
            && !s.objects.get(source).summoning_sick
            && matches!(d,Decision::CastSpellOrPass{player,..}if player==p)
        {
            return;
        }
        pass(s, d)
    }
    panic!("next genuine main absent")
}
fn activate(s: &mut GameState, p: PlayerId, source: ObjectId) -> StackItem {
    priority(s, p);
    let before = tapped(s, p);
    let d = next(s);
    assert!(
        matches!(d,Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(source,0)))
    );
    engine::step(s, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(next(s),Decision::CastSpellOrPass{player,..}if player==p));
    assert!(s.engine.pending_activation.is_none());
    assert_eq!(tapped(s, p) - before, 7);
    assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
    let row = s.stack.last().unwrap().clone();
    assert_eq!(row.kind, StackItemKind::ActivatedAbility);
    assert_eq!(row.source, source);
    assert_eq!(row.inline_effect, Some(program()));
    assert_eq!(row.v4.activated_ability_index, Some(0));
    assert_eq!(row.controller, p);
    row
}
fn prompt(s: &mut GameState, p: PlayerId) -> Vec<ObjectId> {
    for _ in 0..24 {
        let d = next(s);
        if let Decision::ChooseEffectTargets {
            player,
            legal_targets,
            min_targets,
            max_targets,
            can_finish,
            ..
        } = d
        {
            assert_eq!(player, p);
            assert_eq!(min_targets, 0);
            assert_eq!(usize::from(max_targets), legal_targets.len().min(1));
            assert!(can_finish);
            validate_pending_effect_choice(s).unwrap();
            return legal_targets
                .into_iter()
                .map(|t| match t {
                    Target::Object(o) => o,
                    _ => panic!(),
                })
                .collect();
        }
        if s.stack.is_empty() {
            return vec![];
        }
        pass(s, d)
    }
    panic!("Loot prompt absent")
}
fn select(s: &mut GameState, card: Option<ObjectId>) {
    if let Some(c) = card {
        engine::step(s, Action::ChooseEffectTarget(Target::Object(c))).unwrap();
    }
    if s.engine
        .pending_effect
        .as_ref()
        .is_some_and(|p| p.choice.is_some())
    {
        engine::step(s, Action::FinishEffectSelection).unwrap();
    }
    finish_stack(s);
}
fn land_menu(s: &mut GameState, p: PlayerId) -> Vec<ObjectId> {
    priority(s, p);
    match next(s) {
        Decision::CastSpellOrPass { land_drops, .. } => land_drops,
        _ => panic!(),
    }
}
fn tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count()
}
fn cast(s: &mut GameState, p: PlayerId, name: &str, targets: &[Target], cost: usize) -> ObjectId {
    let card = put(s, p, name, Zone::Hand);
    priority(s, p);
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
                chosen += 1
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
                    .any(|r| r.source == card && r.kind == StackItemKind::Spell));
                return card;
            }
            d => panic!("paid cast {d:?}"),
        }
    }
    panic!("paid cast did not publish")
}
fn resolve_response(s: &mut GameState, source: ObjectId) {
    for _ in 0..24 {
        let d = next(s);
        if s.objects.get(source).zone == Zone::Graveyard && s.engine.pending_effect.is_none() {
            return;
        }
        pass(s, d)
    }
    panic!("response absent")
}

#[test]
fn genuine_paid_printing_land_allowance_and_sickness_no_refund_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, &["Forest"; 12]);
        let loot = paid(&mut s, p);
        assert!(s.objects.get(loot).summoning_sick);
        assert!(
            !matches!(next(&mut s),Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(loot,0)))
        );
        assert!(crate::optional_trigger_public_v1::validate_state_checked(&s).is_err());
        assert!(card_def::preflight_fully_supported_deck(&[id(NAME)]).is_err());
        for n in 1..=2 {
            let land = put(&mut s, p, "Forest", Zone::Hand);
            assert!(land_menu(&mut s, p).contains(&land));
            engine::step(&mut s, Action::PlayLand(land)).unwrap();
            assert_eq!(s.players[p.index()].lands_played_this_turn, n);
        }
        let third = put(&mut s, p, "Forest", Zone::Hand);
        assert!(!land_menu(&mut s, p).contains(&third));
        let before = s.clone();
        assert!(engine::step(&mut s, Action::PlayLand(third)).is_err());
        assert_eq!(s, before);
        // Genuine paid instant removes the actual allowance source after both plays.
        let other = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
        let response = cast(
            &mut s,
            p.opponent(),
            "Run Away Together",
            &[Target::Object(loot), Target::Object(other)],
            2,
        );
        resolve_response(&mut s, response);
        assert_eq!(s.objects.get(loot).zone, Zone::Hand);
        assert_eq!(land_allowance(&s, p), 1);
        assert_eq!(s.players[p.index()].lands_played_this_turn, 2);
        assert!(!land_menu(&mut s, p).contains(&third));
        let mut r = restore(&s);
        assert_eq!(land_menu(&mut s, p), land_menu(&mut r, p));
        assert_eq!(s, r);
    }
}
#[test]
fn genuine_paid_activation_zero_one_six_and_failed_find_random_bottom_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for names in [
            vec![],
            vec!["Llanowar Elves"],
            vec![
                "Forest",
                "Llanowar Elves",
                "Island",
                "Healer's Hawk",
                "Mountain",
                "Koma, World-Eater",
            ],
            vec![
                "Forest",
                "Island",
                "Mountain",
                "Counterspell",
                "Forest",
                "Island",
            ],
        ] {
            for take in [false, true] {
                let mut s = ready(p, &names);
                let loot = paid(&mut s, p);
                next_own_main(&mut s, p, loot);
                let before_library = s.players[p.index()].library.clone();
                assert_eq!(before_library.len(), names.len());
                let row = activate(&mut s, p, loot);
                let legal = prompt(&mut s, p);
                let expected = before_library
                    .iter()
                    .copied()
                    .filter(|o| {
                        let d = &card_def::CARD_DEFS[s.objects.get(*o).card_def as usize];
                        d.has_type(CardType::Creature) && usize::from(d.mana_value) <= 6
                    })
                    .collect::<Vec<_>>();
                assert_eq!(legal, expected);
                if s.engine.pending_effect.is_some() {
                    let mut r = restore(&s);
                    let chosen = if take { legal.first().copied() } else { None };
                    select(&mut s, chosen);
                    select(&mut r, chosen);
                    assert_eq!(s, r);
                    assert_eq!(s.state_hash(), r.state_hash());
                    if let Some(c) = chosen {
                        assert_eq!(s.objects.get(c).zone, Zone::Battlefield);
                        assert_eq!(s.objects.get(c).controller, p);
                        assert!(!s.players[p.index()].library.contains(&c));
                    }
                    let mut remaining = s.players[p.index()].library.clone();
                    remaining.sort();
                    let mut expected = before_library
                        .into_iter()
                        .filter(|o| Some(*o) != chosen)
                        .collect::<Vec<_>>();
                    expected.sort();
                    assert_eq!(remaining, expected);
                }
                assert!(s
                    .stack
                    .iter()
                    .all(|r| r.v4.stack_item_id != row.v4.stack_item_id));
                assert!(s.objects.get(loot).tapped);
            }
        }
    }
}
#[test]
fn genuine_paid_response_captures_controller_and_resolution_land_threshold_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for control in [false, true] {
            let mut s = ready(
                p,
                &[
                    "Koma, World-Eater",
                    "Curator of Destinies",
                    "Llanowar Elves",
                    "Forest",
                    "Island",
                    "Mountain",
                ],
            );
            let loot = paid(&mut s, p);
            next_own_main(&mut s, p, loot);
            let row = activate(&mut s, p, loot);
            if control {
                put(
                    &mut s,
                    p.opponent(),
                    "High Fae Trickster",
                    Zone::Battlefield,
                );
                let spell = cast(
                    &mut s,
                    p.opponent(),
                    "Involuntary Employment",
                    &[Target::Object(loot)],
                    4,
                );
                resolve_response(&mut s, spell);
                assert_eq!(s.objects.get(loot).controller, p.opponent());
                assert_eq!(land_allowance(&s, p), 1);
                assert_eq!(land_allowance(&s, p.opponent()), 2);
            } else {
                let land = s.players[p.index()]
                    .battlefield
                    .iter()
                    .copied()
                    .find(|o| engine::object_has_type(&s, *o, CardType::Land))
                    .unwrap();
                let other = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Battlefield);
                let spell = cast(
                    &mut s,
                    p.opponent(),
                    "Run Away Together",
                    &[Target::Object(loot), Target::Object(other)],
                    2,
                );
                resolve_response(&mut s, spell);
                assert_eq!(s.objects.get(loot).zone, Zone::Hand);
                // Native zone primitive distinguishes resolution land count from cast-time count.
                crate::event::propose_and_commit(
                    &mut s,
                    crate::event::ProposedEvent::zone_change(land, Zone::Hand),
                );
            }
            assert_eq!(s.stack.last(), Some(&row));
            let legal = prompt(&mut s, p);
            assert!(legal.iter().all(|o| s.objects.get(*o).owner == p));
            assert!(!legal
                .iter()
                .any(|o| s.objects.get(*o).card_def == id("Koma, World-Eater")));
            assert_eq!(
                legal
                    .iter()
                    .any(|o| s.objects.get(*o).card_def == id("Curator of Destinies")),
                control
            );
            let chosen = legal
                .iter()
                .copied()
                .find(|o| s.objects.get(*o).card_def == id("Llanowar Elves"))
                .unwrap();
            let mut r = restore(&s);
            select(&mut s, Some(chosen));
            select(&mut r, Some(chosen));
            assert_eq!(s, r);
            assert_eq!(s.objects.get(chosen).controller, p);
        }
    }
}
#[test]
fn genuine_paid_private_candidates_unknown_prefix_and_public_api_tamper_refuse_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(
            p,
            &[
                "Forest",
                "Llanowar Elves",
                "Island",
                "Healer's Hawk",
                "Mountain",
                "Counterspell",
            ],
        );
        let loot = paid(&mut s, p);
        next_own_main(&mut s, p, loot);
        let row = activate(&mut s, p, loot);
        let ctx = crystal_prevention_v1::context(&row);
        let before = s.clone();
        assert!(begin_resumable_resolution(&program(), &ctx, row.clone(), &mut s).is_err());
        assert_eq!(s, before);
        let legal = prompt(&mut s, p);
        let prefix = s.players[p.index()].library.clone();
        assert_eq!(legal.len(), 2);
        assert!(crate::optional_trigger_public_v1::validate_state_checked(&s).is_err());
        let d = next(&mut s);
        if let Decision::ChooseEffectTargets { legal_targets, .. } = d {
            assert_eq!(
                legal_targets,
                legal
                    .iter()
                    .copied()
                    .map(Target::Object)
                    .collect::<Vec<_>>()
            );
        } else {
            panic!()
        }
        assert!(s.known_library_cards(p.opponent(), p).is_empty());
        for mode in 0..7 {
            let mut bad = restore(&s);
            match mode {
                0 => bad.engine.pending_effect.as_mut().unwrap().ctx.controller = p.opponent(),
                1 => {
                    bad.players[p.index()].library.swap(0, 1);
                }
                2 => {
                    bad.objects.get_mut(prefix[1]).zone_change_count += 1;
                }
                3 => {
                    if let Some(PendingEffectChoice::SelectTargets { legal, .. }) =
                        &mut bad.engine.pending_effect.as_mut().unwrap().choice
                    {
                        legal.pop();
                    }
                }
                4 => {
                    let q = bad.engine.pending_effect.as_mut().unwrap();
                    q.choice = None;
                    q.frames.clear();
                    q.answered_choice_guard = None;
                }
                5 => {
                    let q = bad.engine.pending_effect.as_mut().unwrap();
                    q.resolving_item.kind = StackItemKind::Spell;
                    q.ctx.stack_item_id = Some(StackItemId(u64::MAX));
                    q.resolving_item.inline_effect = Some(EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    });
                    q.frames = vec![EffectFrame::Program {
                        op: q.resolving_item.inline_effect.clone().unwrap(),
                        path: vec![],
                    }];
                    q.choice = None;
                    q.answered_choice_guard = None;
                }
                _ => {
                    bad.engine.event_history.retain(|e| {
                        !matches!(
                            e,
                            crate::event::CommittedEvent::SquadRallierResolutionBoundaryV1 { .. }
                        )
                    });
                }
            }
            let before = bad.clone();
            assert!(validate_pending_effect_choice(&bad).is_err());
            assert!(resume_resumable_resolution(&mut bad).is_err());
            assert_eq!(bad, before);
        }
        let chosen = legal[0];
        engine::step(&mut s, Action::ChooseEffectTarget(Target::Object(chosen))).unwrap();
        if s.engine
            .pending_effect
            .as_ref()
            .is_some_and(|p| p.choice.is_some())
        {
            engine::step(&mut s, Action::FinishEffectSelection).unwrap();
        }
        let mut bad = restore(&s);
        bad.engine
            .pending_effect
            .as_mut()
            .unwrap()
            .answered_choice_guard = None;
        let before = bad.clone();
        assert!(validate_pending_effect_choice(&bad).is_err());
        assert!(resume_resumable_resolution(&mut bad).is_err());
        assert_eq!(bad, before);
        finish_stack(&mut s);
        assert_eq!(s.objects.get(chosen).zone, Zone::Battlefield);
    }
}
#[test]
fn genuine_paid_witness_suppression_removes_land_permission_and_activation_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, &["Forest"; 12]);
        let loot = paid(&mut s, p);
        next_own_main(&mut s, p, loot);
        put(
            &mut s,
            p.opponent(),
            "High Fae Trickster",
            Zone::Battlefield,
        );
        let aura = cast(
            &mut s,
            p.opponent(),
            "Witness Protection",
            &[Target::Object(loot)],
            1,
        );
        finish_stack(&mut s);
        assert_eq!(s.objects.get(aura).zone, Zone::Battlefield);
        assert!(!crate::continuous_characteristics_v1::printed_abilities_active(&s, loot));
        assert_eq!(land_allowance(&s, p), 1);
        priority(&mut s, p);
        assert!(
            !matches!(next(&mut s),Decision::CastSpellOrPass{activatable_abilities,..}if activatable_abilities.contains(&(loot,0)))
        );
        let r = restore(&s);
        assert_eq!(land_allowance(&r, p), 1);
    }
}
#[test]
fn native_pre_sba_multiplicity_counts_sources_and_u8_boundary_refuses_readonly() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, &["Forest"; 12]);
        let a = put(&mut s, p, NAME, Zone::Battlefield);
        let b = put(&mut s, p, NAME, Zone::Battlefield);
        assert_eq!(land_allowance(&s, p), 3);
        crate::standard_cards_v1::gain_control(&mut s, b, p.opponent());
        assert_eq!(land_allowance(&s, p), 2);
        assert_eq!(land_allowance(&s, p.opponent()), 2);
        crate::event::propose_and_commit(
            &mut s,
            crate::event::ProposedEvent::zone_change(a, Zone::Hand),
        );
        assert_eq!(land_allowance(&s, p), 1);
        s.players[p.index()].lands_played_this_turn = u8::MAX;
        let land = put(&mut s, p, "Forest", Zone::Hand);
        let before = s.clone();
        assert!(engine::step(&mut s, Action::PlayLand(land)).is_err());
        assert_eq!(s, before);
    }
}

#[test]
fn genuine_paid_countered_spell_and_native_countered_activation_keep_costs_without_look_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, &["Forest"; 12]);
        let loot = cast(&mut s, p, NAME, &[], 3);
        let counter = cast(
            &mut s,
            p.opponent(),
            "Counterspell",
            &[Target::Object(loot)],
            2,
        );
        resolve_response(&mut s, counter);
        finish_stack(&mut s);
        assert_eq!(s.objects.get(loot).zone, Zone::Graveyard);
        assert_eq!(land_allowance(&s, p), 1);
        assert_eq!(tapped(&s, p), 3);
        assert!(s.known_library_cards(p, p).is_empty());
        let mut s = ready(
            p,
            &[
                "Forest",
                "Llanowar Elves",
                "Island",
                "Healer's Hawk",
                "Mountain",
                "Counterspell",
            ],
        );
        let loot = paid(&mut s, p);
        next_own_main(&mut s, p, loot);
        let original = s.players[p.index()].library.clone();
        let row = activate(&mut s, p, loot);
        let mut r = restore(&s);
        // Native ability-counter primitive, not an unavailable paid counter spell.
        for world in [&mut s, &mut r] {
            engine::counter_stack_item_by_id(world, row.v4.stack_item_id).unwrap();
            let before = world.clone();
            assert_eq!(
                engine::counter_stack_item_by_id(world, row.v4.stack_item_id).unwrap(),
                None
            );
            assert_eq!(*world, before);
            assert_eq!(world.players[p.index()].library, original);
            assert!(world.objects.get(loot).tapped);
            assert_eq!(tapped(world, p), 7);
            assert!(world.known_library_cards(p, p).is_empty());
        }
        assert_eq!(s, r);
    }
}
#[test]
fn genuine_paid_private_look_independent_hidden_worlds_expose_no_opponent_prefix_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut a = ready(
            p,
            &[
                "Forest",
                "Llanowar Elves",
                "Island",
                "Healer's Hawk",
                "Mountain",
                "Counterspell",
            ],
        );
        let mut b = ready(
            p,
            &[
                "Mountain",
                "Llanowar Elves",
                "Forest",
                "Healer's Hawk",
                "Island",
                "Lightning Bolt",
            ],
        );
        let sa = paid(&mut a, p);
        let sb = paid(&mut b, p);
        assert_eq!(sa, sb);
        next_own_main(&mut a, p, sa);
        next_own_main(&mut b, p, sb);
        activate(&mut a, p, sa);
        activate(&mut b, p, sb);
        let ca = prompt(&mut a, p);
        let cb = prompt(&mut b, p);
        assert_eq!(ca, cb);
        assert_eq!(next(&mut a), next(&mut b));
        assert_eq!(a.known_library_cards(p, p).len(), 6);
        assert_eq!(b.known_library_cards(p, p).len(), 6);
        assert!(a.known_library_cards(p.opponent(), p).is_empty());
        assert!(b.known_library_cards(p.opponent(), p).is_empty());
        let selected = ca[0];
        let mut r = restore(&a);
        select(&mut a, Some(selected));
        select(&mut r, Some(selected));
        select(&mut b, Some(selected));
        assert_eq!(a, r);
        assert_eq!(a.objects.get(selected).zone, Zone::Battlefield);
        assert_eq!(b.objects.get(selected).zone, Zone::Battlefield);
        assert_eq!(
            a.known_library_cards(p.opponent(), p),
            b.known_library_cards(p.opponent(), p)
        );
    }
}
