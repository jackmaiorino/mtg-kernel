//! Genuine paid native casts from both seats. Initial resources/graveyards,
//! control boundaries and adversarial mutations are named setup primitives.
use super::*;
use crate::policy_observation_v6::tests::put;
const NAME: &str = "Muldrotha, the Gravetide";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(p: PlayerId, hidden: &str) -> GameState {
    let a = [id("Forest"); 80];
    let b = [id(hidden); 80];
    let (a, b) = if p == PlayerId::P0 {
        (&a, &b)
    } else {
        (&b, &a)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_489,
        p,
    );
    s.step = Step::Main1;
    // Initial native resource declarations. Every spell subsequently pays real lands.
    for player in [p, p.opponent()] {
        for name in ["Forest", "Swamp", "Island", "Plains", "Mountain"] {
            for _ in 0..10 {
                put(&mut s, player, name, Zone::Battlefield);
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
    let d = advance_until_decision(s);
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
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        Decision::Discard { player, count, .. } => Action::Discard(
            s.players[player.index()]
                .hand
                .iter()
                .copied()
                .take(count as usize)
                .collect(),
        ),
        d => panic!("native priority {d:?}"),
    };
    step(s, a).unwrap();
}
fn priority(s: &mut GameState, p: PlayerId) {
    for _ in 0..64 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass { player,.. } if player==p) {
            return;
        }
        pass(s, d);
    }
    panic!("priority absent");
}
fn finish(s: &mut GameState) {
    for _ in 0..64 {
        let d = next(s);
        if s.stack.is_empty() && s.engine.pending_effect.is_none() {
            return;
        }
        pass(s, d);
    }
    panic!("native stack unfinished");
}
fn tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count()
}
fn pay(
    s: &mut GameState,
    p: PlayerId,
    card: ObjectId,
    permission: Option<(ObjectId, CardType)>,
    target: Option<Target>,
) -> StackItem {
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass { castable_spells,.. } if castable_spells.contains(&card))
    );
    step(s, Action::CastSpell(card)).unwrap();
    for _ in 0..48 {
        let a = match next(s) {
            Decision::ChooseEffectOption {
                source,
                option_count,
                ..
            } => {
                assert_eq!(source, card);
                let index = if let Some(pending) = s.engine.muldrotha_v1.0 {
                    let o = options(s, pending.options_history_index).unwrap();
                    match permission {
                        Some((source, permanent_type)) => u16::try_from(
                            o.permissions
                                .iter()
                                .position(|p| {
                                    p.source.source == source && p.permanent_type == permanent_type
                                })
                                .unwrap()
                                + usize::from(o.ordinary),
                        )
                        .unwrap(),
                        None => {
                            assert!(o.ordinary);
                            0
                        }
                    }
                } else {
                    0
                };
                assert!(index < option_count);
                Action::ChooseEffectOption(index)
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                let t = target.unwrap();
                assert!(legal_targets.contains(&t));
                Action::ChooseTarget(t)
            }
            Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert!(s.engine.pending_cast.is_none());
                let item = s
                    .stack
                    .iter()
                    .find(|r| r.source == card && r.kind == StackItemKind::Spell)
                    .unwrap()
                    .clone();
                validated_stack_item_target_spec(&item, s).unwrap();
                return item;
            }
            d => panic!("native cast {d:?}"),
        };
        let mut r = restore(s);
        step(&mut r, a.clone()).unwrap();
        step(s, a).unwrap();
        assert_eq!(*s, r);
    }
    panic!("native cast unfinished");
}
fn paid(s: &mut GameState, p: PlayerId) -> ObjectId {
    let card = put(s, p, NAME, Zone::Hand);
    let before = tapped(s, p);
    pay(s, p, card, None, None);
    assert_eq!(tapped(s, p) - before, 6);
    finish(s);
    assert_eq!(s.objects.get(card).zone, Zone::Battlefield);
    card
}
fn cast(
    s: &mut GameState,
    p: PlayerId,
    name: &str,
    source: ObjectId,
    kind: CardType,
    cost: usize,
) -> ObjectId {
    let c = put(s, p, name, Zone::Graveyard);
    let before = tapped(s, p);
    pay(s, p, c, Some((source, kind)), None);
    assert_eq!(tapped(s, p) - before, cost);
    c
}
#[test]
fn exact_printing_paid_six_mana_and_full_consumer_refusal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        assert_eq!(id(NAME), 489);
        assert!(definition(489));
        assert_eq!(
            card_def::CARD_DEFS[489].capability,
            card_def::CardCapability::Partial
        );
        assert!(card_def::preflight_fully_supported_deck(&[489]).is_err());
        assert!(require_legacy(&s, Some(p)).is_err());
        assert!(crate::optional_trigger_public_v1::validate_state_checked(&s).is_err());
        assert!(active(&s, AbilitySourceContractV4::capture(&s, m), p));
        assert!(s.players[p.index()].mana_pool.iter().all(|n| *n == 0));
        validate_state(&restore(&s)).unwrap();
    }
}
#[test]
fn each_permanent_type_paid_and_independent_once_budgets_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        for (name, kind, cost) in [
            ("Llanowar Elves", CardType::Creature, 1),
            ("Goldvein Pick", CardType::Artifact, 2),
            ("Anthem of Champions", CardType::Enchantment, 2),
            ("Ajani, Caller of the Pride", CardType::Planeswalker, 3),
        ] {
            let c = cast(&mut s, p, name, m, kind, cost);
            let extra = put(&mut s, p, name, Zone::Graveyard);
            assert!(permissions(&s, extra, p).is_empty());
            finish(&mut s);
            assert_eq!(s.objects.get(c).zone, Zone::Battlefield);
        }
        assert!(require_legacy(&s, Some(p)).is_err());
        validate_state(&s).unwrap();
    }
}
#[test]
fn land_is_real_land_play_with_one_drop_and_no_stack_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        priority(&mut s, p);
        let land = put(&mut s, p, "Forest", Zone::Graveyard);
        assert!(land_drop_candidates(p, &s).contains(&land));
        step(&mut s, Action::PlayLand(land)).unwrap();
        assert!(matches!(
            next(&mut s),
            Decision::ChooseEffectOption {
                option_count: 1,
                ..
            }
        ));
        let r = restore(&s);
        assert_eq!(r.engine.muldrotha_v1, s.engine.muldrotha_v1);
        step(&mut s, Action::ChooseEffectOption(0)).unwrap();
        assert_eq!(s.objects.get(land).zone, Zone::Battlefield);
        assert!(s.stack.is_empty());
        assert_eq!(s.players[p.index()].lands_played_this_turn, 1);
        let extra = put(&mut s, p, "Forest", Zone::Graveyard);
        assert!(!permissions(&s, extra, p)
            .iter()
            .any(|q| q.source.source == m));
        let hand = put(&mut s, p, "Forest", Zone::Hand);
        assert!(!land_drop_candidates(p, &s).contains(&hand));
        let before = s.clone();
        assert!(step(&mut s, Action::PlayLand(hand)).is_err());
        assert_eq!(s, before);
        validate_state(&s).unwrap();
    }
}
#[test]
fn artifact_creature_spends_only_chosen_type_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for kind in [CardType::Creature, CardType::Artifact] {
            let mut s = ready(p, "Forest");
            let m = paid(&mut s, p);
            let hart = cast(&mut s, p, "Burnished Hart", m, kind, 3);
            finish(&mut s);
            let other = put(&mut s, p, "Burnished Hart", Zone::Graveyard);
            let ps = permissions(&s, other, p);
            assert_eq!(ps.len(), 1);
            assert_ne!(ps[0].permanent_type, kind);
            assert_eq!(s.objects.get(hart).zone, Zone::Battlefield);
            let before = tapped(&s, p);
            pay(&mut s, p, other, Some((m, ps[0].permanent_type)), None);
            assert_eq!(tapped(&s, p) - before, 3);
            finish(&mut s);
            let third = put(&mut s, p, "Burnished Hart", Zone::Graveyard);
            assert!(permissions(&s, third, p).is_empty());
        }
    }
}
#[test]
fn countered_cast_consumes_budget_and_source_removal_after_cast_preserves_row_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", m, CardType::Creature, 1);
        let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
        pay(
            &mut s,
            p.opponent(),
            counter,
            None,
            Some(Target::Object(elf)),
        );
        // Native source-departure primitive after actual successful casting.
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(m, Zone::Graveyard));
        for row in &s.stack {
            validated_stack_item_target_spec(row, &s).unwrap();
        }
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(elf).zone, Zone::Graveyard);
        assert!(permissions(&s, elf, p).is_empty());
        // Native source reentry creates a genuinely different source incarnation.
        event::propose_and_commit(&mut s, ProposedEvent::zone_change(m, Zone::Battlefield));
        assert!(permissions(&s, elf, p)
            .iter()
            .any(|q| q.source.source == m && q.permanent_type == CardType::Creature));
    }
}
#[test]
fn own_turn_graveyard_timing_payment_and_no_activation_permission_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        let elf = put(&mut s, p, "Llanowar Elves", Zone::Graveyard);
        let instant = put(&mut s, p, "Lightning Bolt", Zone::Graveyard);
        let foreign = put(&mut s, p.opponent(), "Llanowar Elves", Zone::Graveyard);
        assert!(permissions(&s, instant, p).is_empty());
        assert!(permissions(&s, foreign, p).is_empty());
        assert!(!available_activatable_abilities(p, &s)
            .iter()
            .any(|(source, _)| *source == elf));
        s.step = Step::BeginCombat;
        assert!(!castable_spells(p, &s).contains(&elf));
        let flash = put(&mut s, p, "Brineborn Cutthroat", Zone::Graveyard);
        assert!(castable_spells(p, &s).contains(&flash));
        s.active_player = p.opponent();
        assert!(!castable_spells(p, &s).contains(&flash));
        s.active_player = p;
        s.step = Step::Main1;
        s.active_player = p.opponent();
        assert!(permissions(&s, elf, p).is_empty());
        s.active_player = p;
        // Explicit no-mana boundary primitive, preserving the already-paid source.
        for source in s.players[p.index()].battlefield.clone() {
            s.objects.get_mut(source).tapped = true;
        }
        assert!(!castable_spells(p, &s).contains(&elf));
        let before = s.clone();
        assert!(step(&mut s, Action::CastSpell(elf)).is_err());
        assert_eq!(s, before);
        assert_eq!(s.objects.get(m).zone, Zone::Battlefield);
    }
}
#[test]
fn stale_erased_foreign_and_out_of_range_choices_refuse_without_mutation_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        priority(&mut s, p);
        let elf = put(&mut s, p, "Llanowar Elves", Zone::Graveyard);
        step(&mut s, Action::CastSpell(elf)).unwrap();
        next(&mut s);
        let original = restore(&s);
        let before = s.clone();
        assert!(step(&mut s, Action::ChooseEffectOption(99)).is_err());
        assert_eq!(s, before);
        for mode in 0..5 {
            let mut bad = original.clone();
            match mode {
                0 => bad.engine.muldrotha_v1.0 = None,
                1 => bad.objects.get_mut(m).zone_change_count += 1,
                2 => bad.objects.get_mut(elf).zone_change_count += 1,
                3 => bad.objects.get_mut(m).controller = p.opponent(),
                _ => bad.active_player = p.opponent(),
            }
            let before = bad.clone();
            assert!(step(&mut bad, Action::ChooseEffectOption(0)).is_err());
            assert_eq!(bad, before);
        }
        let mut r = restore(&s);
        step(&mut r, Action::ChooseEffectOption(0)).unwrap();
        step(&mut s, Action::ChooseEffectOption(0)).unwrap();
        assert_eq!(s, r);
    }
}
#[test]
fn current_generation_source_controller_changes_and_next_turn_reset_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        cast(&mut s, p, "Llanowar Elves", m, CardType::Creature, 1);
        finish(&mut s);
        let elf = put(&mut s, p, "Llanowar Elves", Zone::Graveyard);
        assert!(permissions(&s, elf, p).is_empty());
        // Explicit native control-change boundary, same source incarnation.
        s.players[p.index()].battlefield.retain(|x| *x != m);
        s.players[p.opponent().index()].battlefield.push(m);
        s.objects.get_mut(m).controller = p.opponent();
        assert!(permissions(&s, elf, p).is_empty());
        s.players[p.opponent().index()]
            .battlefield
            .retain(|x| *x != m);
        s.players[p.index()].battlefield.push(m);
        s.objects.get_mut(m).controller = p;
        assert!(permissions(&s, elf, p).is_empty());
        validate_state(&s).unwrap();
        // Drive actual turn/untap work rather than editing the budget key.
        let old_turn = s.turn_key_v1();
        for _ in 0..160 {
            let d = next(&mut s);
            if s.turn_key_v1() != old_turn && s.active_player == p && matches!(s.step, Step::Main1)
            {
                break;
            }
            pass(&mut s, d);
        }
        assert_ne!(s.turn_key_v1(), old_turn);
        assert_eq!(s.active_player, p);
        assert!(permissions(&s, elf, p)
            .iter()
            .any(|q| q.permanent_type == CardType::Creature));
        pay(&mut s, p, elf, Some((m, CardType::Creature)), None);
        finish(&mut s);
    }
}
#[test]
fn genuinely_paid_new_muldrotha_has_independent_budget_through_real_legend_choice_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        let other = cast(&mut s, p, NAME, m, CardType::Creature, 6);
        let mut reached = false;
        for _ in 0..32 {
            let d = next(&mut s);
            if matches!(d, Decision::ChooseLegendPermanent { .. }) {
                reached = true;
                break;
            }
            pass(&mut s, d);
        }
        assert!(reached);
        let elf = put(&mut s, p, "Llanowar Elves", Zone::Graveyard);
        let ps = permissions(&s, elf, p);
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].source.source, other);
        let hart = put(&mut s, p, "Burnished Hart", Zone::Graveyard);
        let ps = permissions(&s, hart, p);
        assert_eq!(ps.len(), 3);
        assert!(ps
            .iter()
            .any(|q| q.source.source == m && q.permanent_type == CardType::Artifact));
        assert!(ps
            .iter()
            .any(|q| q.source.source == other && q.permanent_type == CardType::Artifact));
        step(&mut s, Action::ChooseLegendPermanent(other)).unwrap();
        priority(&mut s, p);
        pay(&mut s, p, elf, Some((other, CardType::Creature)), None);
        finish(&mut s);
        assert_eq!(s.objects.get(m).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(elf).zone, Zone::Battlefield);
    }
}
#[test]
fn erased_consumption_and_relabelled_finalized_route_are_readonly_refusals_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        cast(&mut s, p, "Llanowar Elves", m, CardType::Creature, 1);
        let mut bad = s.clone();
        bad.engine
            .event_history
            .retain(|e| !matches!(e, CommittedEvent::MuldrothaPlayedV1 { .. }));
        let before = bad.clone();
        assert!(step(&mut bad, Action::Pass).is_err());
        assert_eq!(bad, before);
        let mut bad = s.clone();
        let row = bad.stack.last_mut().unwrap();
        let card = row.source;
        let mut source = row.v4.source_contract.unwrap();
        source.spell_cast_origin.as_mut().unwrap().route = SpellCastRouteV4::Hand;
        row.v4.source_contract = Some(source);
        bad.objects.get_mut(card).v4.spell_cast_origin = source.spell_cast_origin;
        assert!(validated_stack_item_target_spec(bad.stack.last().unwrap(), &bad).is_err());
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
    }
}

#[test]
fn printed_graveyard_route_remains_distinct_and_does_not_spend_muldrotha_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        // Native counter-resource declaration. The printed route actually
        // removes all six counters, in addition to real six-mana payment.
        s.objects.get_mut(m).counters.plus1_plus1 = 6;
        let wurm = put(&mut s, p, "Quilled Greatwurm", Zone::Graveyard);
        let before = tapped(&s, p);
        pay(&mut s, p, wurm, None, None);
        assert_eq!(tapped(&s, p) - before, 6);
        assert_eq!(s.objects.get(m).counters.plus1_plus1, 0);
        assert!(matches!(
            s.stack
                .last()
                .unwrap()
                .v4
                .source_contract
                .unwrap()
                .spell_cast_origin
                .unwrap()
                .route,
            SpellCastRouteV4::GreatwurmGraveyardV1 { .. }
        ));
        finish(&mut s);
        let elf = cast(&mut s, p, "Llanowar Elves", m, CardType::Creature, 1);
        finish(&mut s);
        assert_eq!(s.objects.get(elf).zone, Zone::Battlefield);
    }
}

#[test]
fn source_ability_suppression_by_genuinely_paid_aura_revokes_permission_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        let elf = put(&mut s, p, "Llanowar Elves", Zone::Graveyard);
        assert!(!permissions(&s, elf, p).is_empty());
        let aura = put(&mut s, p, "Witness Protection", Zone::Hand);
        let before = tapped(&s, p);
        pay(&mut s, p, aura, None, Some(Target::Object(m)));
        assert_eq!(tapped(&s, p) - before, 1);
        finish(&mut s);
        assert!(permissions(&s, elf, p).is_empty());
        assert!(!castable_spells(p, &s).contains(&elf));
    }
}

#[test]
fn independent_prepaid_hidden_worlds_have_same_public_permission_choices_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut a = ready(p, "Forest");
        let mut b = ready(p, "Swamp");
        let ma = paid(&mut a, p);
        let mb = paid(&mut b, p);
        let ca = put(&mut a, p, "Burnished Hart", Zone::Graveyard);
        let cb = put(&mut b, p, "Burnished Hart", Zone::Graveyard);
        assert_eq!(ma, mb);
        assert_eq!(ca, cb);
        assert_eq!(permissions(&a, ca, p), permissions(&b, cb, p));
        for s in [&mut a, &mut b] {
            pay(s, p, ca, Some((ma, CardType::Artifact)), None);
            finish(s);
            assert!(require_legacy(s, Some(p)).is_err());
            validate_state(s).unwrap();
        }
        let extra_a = put(&mut a, p, "Burnished Hart", Zone::Graveyard);
        let extra_b = put(&mut b, p, "Burnished Hart", Zone::Graveyard);
        assert_eq!(permissions(&a, extra_a, p), permissions(&b, extra_b, p));
    }
}
