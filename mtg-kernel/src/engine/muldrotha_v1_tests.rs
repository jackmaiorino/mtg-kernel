//! Genuine paid native casts from both seats. Initial resources/graveyards,
//! control boundaries and adversarial mutations are named setup primitives.
use super::*;
use crate::policy_observation_v6::tests::put;
const NAME: &str = "Muldrotha, the Gravetide";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
pub(crate) fn ready(p: PlayerId, hidden: &str) -> GameState {
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
pub(crate) fn priority(s: &mut GameState, p: PlayerId) {
    for _ in 0..64 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass { player,.. } if player==p) {
            return;
        }
        pass(s, d);
    }
    panic!("priority absent");
}
pub(crate) fn finish(s: &mut GameState) {
    for _ in 0..64 {
        let d = next(s);
        if s.stack.is_empty() && s.engine.pending_effect.is_none() {
            return;
        }
        pass(s, d);
    }
    panic!("native stack unfinished");
}
pub(crate) fn tapped(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|id| s.objects.get(**id).tapped)
        .count()
}
pub(crate) fn pay(
    s: &mut GameState,
    p: PlayerId,
    card: ObjectId,
    permission: Option<(ObjectId, CardType)>,
    target: Option<Target>,
) -> StackItem {
    pay_form(s, p, card, permission, target, 0, 0)
}
pub(crate) fn pay_form(
    s: &mut GameState,
    p: PlayerId,
    card: ObjectId,
    permission: Option<(ObjectId, CardType)>,
    target: Option<Target>,
    form: u8,
    x: u16,
) -> StackItem {
    priority(s, p);
    let decision = next(s);
    assert!(
        matches!(&decision,Decision::CastSpellOrPass { castable_spells,.. } if castable_spells.contains(&card)),
        "{} not castable for {p:?} during {:?}: {decision:?}",
        s.objects.get(card).name,
        s.step
    );
    step(s, Action::CastSpell(card)).unwrap();
    complete_cast(s, p, card, permission, target, form, x)
}
fn complete_cast(
    s: &mut GameState,
    p: PlayerId,
    card: ObjectId,
    permission: Option<(ObjectId, CardType)>,
    target: Option<Target>,
    form: u8,
    x: u16,
) -> StackItem {
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
                } else if s
                    .engine
                    .pending_cast
                    .as_ref()
                    .is_some_and(|p| p.selected_x_v59().is_none())
                {
                    x
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
            Decision::ChooseSpellMode { legal_modes, .. } => {
                assert!(legal_modes.contains(&form));
                Action::ChooseSpellMode(form)
            }
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
pub(crate) fn paid(s: &mut GameState, p: PlayerId) -> ObjectId {
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
        assert!(crate::rl::observe_policy_v6(
            &s,
            &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
            p,
            0,
            0,
            0,
            1,
        )
        .is_err());
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
        // Initial green resource for the Elf after the two six-mana payments.
        put(&mut s, p, "Forest", Zone::Battlefield);
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

fn choose_permission(
    s: &mut GameState,
    p: PlayerId,
    card: ObjectId,
    source: ObjectId,
    kind: CardType,
) {
    priority(s, p);
    step(s, Action::CastSpell(card)).unwrap();
    assert!(matches!(next(s), Decision::ChooseEffectOption { source, .. } if source == card));
    let pending = s.engine.muldrotha_v1.0.unwrap();
    let o = options(s, pending.options_history_index).unwrap();
    let index = o
        .permissions
        .iter()
        .position(|q| q.source.source == source && q.permanent_type == kind)
        .unwrap();
    let index = u16::try_from(index + usize::from(o.ordinary)).unwrap();
    let mut r = restore(s);
    step(&mut r, Action::ChooseEffectOption(index)).unwrap();
    step(s, Action::ChooseEffectOption(index)).unwrap();
    assert_eq!(*s, r);
}

#[test]
fn genuine_loot_extra_land_allowance_cannot_use_artifact_budget_for_seat_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        let loot = put(&mut s, p, "Loot, Exuberant Explorer", Zone::Hand);
        let before = tapped(&s, p);
        pay(&mut s, p, loot, None, None);
        assert_eq!(tapped(&s, p) - before, 3);
        finish(&mut s);
        assert_eq!(effect::loot_v1::land_allowance(&s, p), 2);
        let seat = put(&mut s, p, "Seat of the Synod", Zone::Graveyard);
        let forest = put(&mut s, p, "Forest", Zone::Graveyard);
        let ps = permissions(&s, seat, p);
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].permanent_type, CardType::Land);
        priority(&mut s, p);
        step(&mut s, Action::PlayLand(seat)).unwrap();
        assert!(matches!(
            next(&mut s),
            Decision::ChooseEffectOption {
                option_count: 1,
                ..
            }
        ));
        let before = s.clone();
        assert!(step(&mut s, Action::ChooseEffectOption(1)).is_err());
        assert_eq!(s, before);
        step(&mut s, Action::ChooseEffectOption(0)).unwrap();
        assert_eq!(s.objects.get(seat).zone, Zone::Battlefield);
        assert_eq!(s.players[p.index()].lands_played_this_turn, 1);
        assert_eq!(effect::loot_v1::land_allowance(&s, p), 2);
        assert!(permissions(&s, forest, p).is_empty());
        assert!(!land_drop_candidates(p, &s).contains(&forest));
        let before = s.clone();
        assert!(step(&mut s, Action::PlayLand(forest)).is_err());
        assert_eq!(s, before);
        let hand = put(&mut s, p, "Forest", Zone::Hand);
        step(&mut s, Action::PlayLand(hand)).unwrap();
        assert_eq!(s.players[p.index()].lands_played_this_turn, 2);
        let pick = cast(&mut s, p, "Goldvein Pick", m, CardType::Artifact, 2);
        finish(&mut s);
        assert_eq!(s.objects.get(pick).zone, Zone::Battlefield);
        validate_state(&s).unwrap();
    }
}

#[test]
fn adventure_and_omen_cards_pay_for_permanent_only_and_refuse_nonpermanent_form_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for (name, cost) in [("Fang Dragon", 7), ("Sagu Wildling", 5)] {
            let mut s = ready(p, "Forest");
            let m = paid(&mut s, p);
            let card = put(&mut s, p, name, Zone::Graveyard);
            choose_permission(&mut s, p, card, m, CardType::Creature);
            let pending = s.engine.pending_cast.as_ref().unwrap();
            let def = &card_def::CARD_DEFS[id(name) as usize];
            assert_eq!(viable_pending_spell_forms(def, pending, &s), vec![0]);
            assert!(pending_has_graveyard_creature_permission(pending, &s));
            assert!(!form_allowed(&s, pending, CastMethodV4::Omen));
            let before = s.clone();
            assert!(step(&mut s, Action::ChooseSpellMode(1)).is_err());
            assert_eq!(s, before);
            let before = tapped(&s, p);
            let row = complete_cast(&mut s, p, card, Some((m, CardType::Creature)), None, 0, 0);
            assert_eq!(tapped(&s, p) - before, cost);
            assert_eq!(row.v4.cast_method, Some(CastMethodV4::Normal));
            assert_eq!(row.mode_chosen, 0);
            let mut r = restore(&s);
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            assert_eq!(s.objects.get(card).zone, Zone::Battlefield);
            assert!(object_has_type(&s, card, CardType::Creature));
            validate_state(&s).unwrap();
        }
    }
}

#[test]
fn bestow_uses_real_xgg_enchantment_budget_and_survives_target_loss_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for lose_target in [false, true] {
            let mut s = ready(p, "Forest");
            let m = paid(&mut s, p);
            let host = put(&mut s, p, "Llanowar Elves", Zone::Hand);
            pay(&mut s, p, host, None, None);
            finish(&mut s);
            let hydra = put(&mut s, p, "Nyxborn Hydra", Zone::Graveyard);
            let before = tapped(&s, p);
            let row = pay_form(
                &mut s,
                p,
                hydra,
                Some((m, CardType::Enchantment)),
                Some(Target::Object(host)),
                1,
                3,
            );
            assert_eq!(tapped(&s, p) - before, 5);
            assert_eq!(row.v4.cast_method, Some(CastMethodV4::Bestow));
            assert_eq!(row.v4.x_value, 3);
            let types = crate::source_type_hexproof_v1::spell_types(
                &card_def::CARD_DEFS[id("Nyxborn Hydra") as usize],
                CastMethodV4::Bestow,
            )
            .unwrap();
            assert!(types.contains(CardType::Enchantment));
            assert!(!types.contains(CardType::Creature));
            if lose_target {
                // Native target-departure primitive after actual successful payment.
                event::propose_and_commit(
                    &mut s,
                    ProposedEvent::zone_change(host, Zone::Graveyard),
                );
            }
            let mut r = restore(&s);
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            assert_eq!(s.objects.get(hydra).zone, Zone::Battlefield);
            assert_eq!(s.objects.get(hydra).counters.plus1_plus1, 3);
            assert_eq!(object_has_type(&s, hydra, CardType::Creature), lose_target);
            assert!(object_has_type(&s, hydra, CardType::Enchantment));
            assert_eq!(
                s.objects.get(hydra).v4.attached_to.map(|link| link.object),
                (!lose_target).then_some(host)
            );
            let another = put(&mut s, p, "Nyxborn Hydra", Zone::Graveyard);
            let ps = permissions(&s, another, p);
            assert_eq!(ps.len(), 1);
            assert_eq!(ps[0].permanent_type, CardType::Creature);
            validate_state(&s).unwrap();
        }
    }
}

#[test]
fn creature_budget_cannot_choose_or_relabel_bestow_and_pays_normal_hydra_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let m = paid(&mut s, p);
        let hydra = put(&mut s, p, "Nyxborn Hydra", Zone::Graveyard);
        choose_permission(&mut s, p, hydra, m, CardType::Creature);
        let pending = s.engine.pending_cast.as_ref().unwrap();
        let def = &card_def::CARD_DEFS[id("Nyxborn Hydra") as usize];
        assert_eq!(viable_pending_spell_forms(def, pending, &s), vec![0]);
        assert!(!form_allowed(&s, pending, CastMethodV4::Bestow));
        let before = s.clone();
        assert!(step(&mut s, Action::ChooseSpellMode(1)).is_err());
        assert_eq!(s, before);
        let before = tapped(&s, p);
        let row = complete_cast(&mut s, p, hydra, Some((m, CardType::Creature)), None, 0, 2);
        assert_eq!(tapped(&s, p) - before, 3);
        let mut bad = row.v4.source_contract.unwrap();
        // Explicit forged finalized-form operand, preserving original permission.
        bad.cast_method = CastMethodV4::Bestow;
        bad.spell_cast_origin.as_mut().unwrap().finalized_method = Some(CastMethodV4::Bestow);
        assert!(validate_contract(&s, bad).is_err());
        finish(&mut s);
        assert!(object_has_type(&s, hydra, CardType::Creature));
        assert_eq!(s.objects.get(hydra).counters.plus1_plus1, 2);
        let another = put(&mut s, p, "Nyxborn Hydra", Zone::Graveyard);
        let ps = permissions(&s, another, p);
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].permanent_type, CardType::Enchantment);
        validate_state(&s).unwrap();
    }
}
