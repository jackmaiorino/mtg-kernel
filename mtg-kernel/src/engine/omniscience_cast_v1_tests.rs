//! Genuine casts and native actions; explicit setup/suppression/copy operands
//! are identified in their cases. No free mana pool or fabricated permission.
use super::*;
use crate::{policy_observation_v6::tests::put, state::Step};
const NAME: &str = "Omniscience";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(p: PlayerId, hidden: &str) -> GameState {
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
        110_478,
        p,
    );
    s.step = Step::Main1;
    // Native initial resource placement, followed by real land-funded casts.
    for player in [p, p.opponent()] {
        for name in ["Island", "Plains", "Mountain", "Forest", "Swamp"] {
            for _ in 0..8 {
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
    let action = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::Discard { player, count, .. } => Action::Discard(
            s.players[player.index()]
                .hand
                .iter()
                .copied()
                .take(count as usize)
                .collect(),
        ),
        d => panic!("Omniscience native priority {d:?}"),
    };
    step(s, action).unwrap();
}
fn priority(s: &mut GameState, p: PlayerId) {
    for _ in 0..32 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==p) {
            return;
        }
        pass(s, d);
    }
    panic!("actor priority absent");
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
    selection: (bool, u16, bool),
    targets: &[Target],
    discard: Option<ObjectId>,
) -> StackItem {
    let (free, x, kick) = selection;
    priority(s, p);
    assert!(
        matches!(next(s),Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&card))
    );
    step(s, Action::CastSpell(card)).unwrap();
    let mut chosen = 0;
    for _ in 0..48 {
        let action = match next(s) {
            Decision::ChooseEffectOption {
                player,
                source,
                option_count,
            } => {
                assert_eq!((player, source), (p, card));
                let pending = s.engine.pending_cast.as_ref().unwrap();
                let option = if pending.omniscience_v1.is_some_and(|c| c.free.is_none()) {
                    u16::from(free)
                } else if pending
                    .mandatory_additional_cost
                    .is_some_and(|c| c.pay_mana.is_none())
                {
                    1
                } else {
                    x
                };
                assert!(option < option_count);
                Action::ChooseEffectOption(option)
            }
            Decision::ChooseKicker { player, spell } => {
                assert_eq!((player, spell), (p, card));
                Action::ChooseKicker(kick)
            }
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, p);
                assert!(chosen < targets.len());
                assert!(legal_targets.contains(&targets[chosen]));
                let a = Action::ChooseTarget(targets[chosen]);
                chosen += 1;
                a
            }
            Decision::Discard { player, count, .. } => {
                assert_eq!((player, count), (p, 1));
                let object = discard.unwrap();
                assert!(s.players[p.index()].hand.contains(&object));
                Action::Discard(vec![object])
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert_eq!(chosen, targets.len());
                assert!(s.engine.pending_cast.is_none());
                assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
                let row = s
                    .stack
                    .iter()
                    .find(|r| r.source == card && r.kind == StackItemKind::Spell)
                    .unwrap()
                    .clone();
                validated_stack_item_target_spec(&row, s).unwrap();
                return row;
            }
            d => panic!("native selected cast {d:?}"),
        };
        // The restored decision is answered independently through native step.
        let mut r = restore(s);
        step(&mut r, action.clone()).unwrap();
        step(s, action).unwrap();
        assert_eq!(*s, r);
    }
    panic!("selected cast not paid");
}
fn cast(
    s: &mut GameState,
    p: PlayerId,
    name: &str,
    selection: (bool, u16, bool),
    targets: &[Target],
    discard: Option<ObjectId>,
    cost: usize,
) -> ObjectId {
    let (free, x, kick) = selection;
    priority(s, p);
    let card = put(s, p, name, Zone::Hand);
    let before = tapped(s, p);
    pay(s, p, card, (free, x, kick), targets, discard);
    assert_eq!(tapped(s, p) - before, cost);
    card
}
fn paid(s: &mut GameState, p: PlayerId) -> ObjectId {
    let card = cast(s, p, NAME, (false, 0, false), &[], None, 10);
    finish(s);
    assert_eq!(s.objects.get(card).zone, Zone::Battlefield);
    assert_eq!(active_source(s, p).unwrap().source, card);
    card
}
#[test]
fn exact_printing_genuine_ten_mana_cast_and_old_consumer_refusal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let card = paid(&mut s, p);
        assert_eq!(id(NAME), 478);
        assert!(definition(478));
        assert_eq!(
            card_def::CARD_DEFS[478].capability,
            card_def::CardCapability::Partial
        );
        assert!(card_def::preflight_fully_supported_deck(&[478]).is_err());
        assert!(refuse_legacy(&s).is_err());
        assert!(crate::optional_trigger_public_v1::validate_state_checked(&s).is_err());
        let source = active_source(&s, p).unwrap();
        assert_eq!(source.controller, p);
        assert_eq!(source.owner, p);
        assert!(active_source(&s, p.opponent()).is_none());
        assert!(
            s.players[p.index()]
                .battlefield
                .iter()
                .filter(|object| s.objects.get(**object).card_def == id("Island")
                    && s.objects.get(**object).tapped)
                .count()
                >= 3
        );
        let r = restore(&s);
        assert_eq!(active_source(&r, p), Some(source));
        assert_eq!(r.objects.get(card).owner, p);
    }
}
#[test]
fn optional_paid_and_free_bolt_keep_actual_source_target_cost_and_response_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for free in [false, true] {
            for counter in [false, true] {
                let mut s = ready(p, "Forest");
                let omni = paid(&mut s, p);
                let life = s.players[p.opponent().index()].life;
                let bolt = cast(
                    &mut s,
                    p,
                    "Lightning Bolt",
                    (free, 0, false),
                    &[Target::Player(p.opponent())],
                    None,
                    usize::from(!free),
                );
                let row = s.stack.last().unwrap().clone();
                let source = row.v4.source_contract.unwrap();
                assert_eq!(source.source, bolt);
                assert_eq!(source.controller, p);
                assert_eq!(source.zone, Zone::Stack);
                let receipt = source
                    .finalized_cast_binding
                    .unwrap()
                    .omniscience_cast_v1
                    .unwrap();
                let CommittedEvent::OmniscienceCastSelectedV1 {
                    choice,
                    source: original,
                    stack_item_id,
                } = s.engine.event_history[receipt as usize]
                else {
                    panic!()
                };
                assert_eq!(choice.free, Some(free));
                assert_eq!(stack_item_id, row.v4.stack_item_id);
                assert_eq!(original.source, bolt);
                assert_eq!(
                    options(&s, choice.options_history_index)
                        .unwrap()
                        .source
                        .source,
                    omni
                );
                assert_eq!(
                    matches!(
                        source.spell_cast_origin.unwrap().route,
                        SpellCastRouteV4::OmniscienceHandV1 { .. }
                    ),
                    free
                );
                if counter {
                    cast(
                        &mut s,
                        p.opponent(),
                        "Counterspell",
                        (false, 0, false),
                        &[Target::Object(bolt)],
                        None,
                        2,
                    );
                }
                let mut r = restore(&s);
                finish(&mut s);
                finish(&mut r);
                assert_eq!(s, r);
                assert_eq!(s.objects.get(bolt).zone, Zone::Graveyard);
                assert_eq!(
                    s.players[p.opponent().index()].life,
                    life - if counter { 0 } else { 3 }
                );
            }
        }
    }
}
#[test]
fn free_x_is_zero_paid_x_and_kicker_and_real_additional_discard_keep_native_costs_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for free in [false, true] {
            let mut s = ready(p, "Forest");
            paid(&mut s, p);
            let target = put(&mut s, p.opponent(), "Bear Cub", Zone::Battlefield);
            let x = if free { 0 } else { 1 };
            let card = cast(
                &mut s,
                p,
                "Goblin Negotiation",
                (free, x, false),
                &[Target::Object(target)],
                None,
                if free { 0 } else { 3 },
            );
            let row = s.stack.last().unwrap();
            assert_eq!(row.v4.x_value, x);
            assert_eq!(
                row.v4
                    .source_contract
                    .unwrap()
                    .finalized_cast_binding
                    .unwrap()
                    .x_value,
                x
            );
            let mut r = restore(&s);
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
            assert_eq!(s.objects.get(target).zone, Zone::Battlefield);
            assert_eq!(s.objects.get(target).damage, if free { 0 } else { 1 });
        }
        for kick in [false, true] {
            let mut s = ready(p, "Forest");
            paid(&mut s, p);
            let before = s.players[p.opponent().index()].life;
            cast(
                &mut s,
                p,
                "Burst Lightning",
                (true, 0, kick),
                &[Target::Player(p.opponent())],
                None,
                if kick { 4 } else { 0 },
            );
            assert_eq!(s.stack.last().unwrap().kicked, kick);
            finish(&mut s);
            assert_eq!(
                s.players[p.opponent().index()].life,
                before - if kick { 4 } else { 2 }
            );
        }
        let mut s = ready(p, "Forest");
        paid(&mut s, p);
        let fodder = put(&mut s, p, "Forest", Zone::Hand);
        let before = s.players[p.index()].hand.len();
        let card = cast(
            &mut s,
            p,
            "Thrill of Possibility",
            (true, 0, false),
            &[],
            Some(fodder),
            0,
        );
        let row = s.stack.last().unwrap();
        assert_eq!(row.discarded, [fodder]);
        assert_eq!(s.objects.get(fodder).zone, Zone::Graveyard);
        assert_eq!(row.v4.paid_cost_refs[0].object, fodder);
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.players[p.index()].hand.len(), before + 1);
        assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
        let mut s = ready(p, "Forest");
        paid(&mut s, p);
        let target = put(&mut s, p.opponent(), "Bear Cub", Zone::Battlefield);
        cast(
            &mut s,
            p,
            "Eaten Alive",
            (true, 0, false),
            &[Target::Object(target)],
            None,
            4,
        );
        assert!(
            s.stack
                .last()
                .unwrap()
                .v4
                .source_contract
                .unwrap()
                .finalized_cast_binding
                .unwrap()
                .mandatory_additional_cost
                .unwrap()
                .pay_mana
        );
        finish(&mut s);
        assert_eq!(s.objects.get(target).zone, Zone::Exile);
    }
}
#[test]
fn unavailable_additional_cost_and_targets_do_not_create_a_free_offer_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        paid(&mut s, p);
        // No other Hand card can pay Thrill's mandatory discard.
        let old = s.players[p.index()].hand.clone();
        for object in old {
            event::propose_and_commit(
                &mut s,
                event::ProposedEvent::zone_change(object, Zone::Graveyard),
            );
        }
        let thrill = put(&mut s, p, "Thrill of Possibility", Zone::Hand);
        let before = s.clone();
        assert!(!offered_free(&s, thrill, p));
        assert_eq!(s, before);
        let eaten = put(&mut s, p, "Eaten Alive", Zone::Hand);
        // Lands and the enchantment are not legal creature/planeswalker targets.
        let before = s.clone();
        assert!(!offered_free(&s, eaten, p));
        assert_eq!(s, before);
        let land = put(&mut s, p, "Forest", Zone::Hand);
        assert!(!offered_free(&s, land, p));
    }
}
#[test]
fn source_removal_after_payment_preserves_spell_but_future_hand_graveyard_exile_and_timing_stay_normal_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let omni = paid(&mut s, p);
        let gy = put(&mut s, p, "Lightning Bolt", Zone::Graveyard);
        let exile = put(&mut s, p, "Lightning Bolt", Zone::Exile);
        assert!(!offered_free(&s, gy, p));
        assert!(!offered_free(&s, exile, p));
        assert!(
            matches!(next(&mut s),Decision::CastSpellOrPass{castable_spells,..}if !castable_spells.contains(&gy)&&!castable_spells.contains(&exile))
        );
        let bolt = cast(
            &mut s,
            p,
            "Lightning Bolt",
            (true, 0, false),
            &[Target::Player(p.opponent())],
            None,
            0,
        );
        cast(
            &mut s,
            p.opponent(),
            "Broken Wings",
            (false, 0, false),
            &[Target::Object(omni)],
            None,
            3,
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(omni).zone, Zone::Graveyard);
        assert_eq!(s.objects.get(bolt).zone, Zone::Graveyard);
        let card = put(&mut s, p, "Lightning Bolt", Zone::Hand);
        let before = s.clone();
        assert!(!offered_free(&s, card, p));
        assert_eq!(s, before);
        priority(&mut s, p);
        let before = tapped(&s, p);
        pay(
            &mut s,
            p,
            card,
            (false, 0, false),
            &[Target::Player(p.opponent())],
            None,
        );
        assert_eq!(tapped(&s, p) - before, 1);
        // A second genuinely paid Omniscience cannot grant sorcery timing while a spell waits.
        finish(&mut s);
        paid(&mut s, p);
        let sorcery = put(&mut s, p, "Eaten Alive", Zone::Hand);
        let target = put(&mut s, p.opponent(), "Bear Cub", Zone::Battlefield);
        cast(
            &mut s,
            p,
            "Lightning Bolt",
            (true, 0, false),
            &[Target::Player(p.opponent())],
            None,
            0,
        );
        assert!(!s.stack.is_empty());
        assert_eq!(s.objects.get(target).zone, Zone::Battlefield);
        assert!(!offered_free(&s, sorcery, p));
    }
}
#[test]
fn exact_waiting_answered_finalized_bindings_refuse_forgery_readonly_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        paid(&mut s, p);
        let card = put(&mut s, p, "Lightning Bolt", Zone::Hand);
        priority(&mut s, p);
        step(&mut s, Action::CastSpell(card)).unwrap();
        assert!(matches!(
            next(&mut s),
            Decision::ChooseEffectOption {
                option_count: 2,
                ..
            }
        ));
        for mode in 0..4 {
            let mut bad = restore(&s);
            let pending = bad.engine.pending_cast.as_ref().unwrap().clone();
            let index = pending.omniscience_v1.unwrap().options_history_index as usize;
            match mode {
                0 => bad.engine.pending_cast.as_mut().unwrap().omniscience_v1 = None,
                1 => {
                    bad.engine
                        .pending_cast
                        .as_mut()
                        .unwrap()
                        .omniscience_v1
                        .as_mut()
                        .unwrap()
                        .options_history_index += 1
                }
                2 => {
                    let CommittedEvent::OmniscienceCastOptionsV1 { options } =
                        &mut bad.engine.event_history[index]
                    else {
                        panic!()
                    };
                    options.hand.zone_change_count += 1;
                }
                _ => {
                    let CommittedEvent::OmniscienceCastOptionsV1 { options } =
                        &mut bad.engine.event_history[index]
                    else {
                        panic!()
                    };
                    options.source.controller = p.opponent();
                }
            }
            let before = bad.clone();
            assert!(step(&mut bad, Action::ChooseEffectOption(1)).is_err());
            assert_eq!(bad, before);
        }
        let before = s.clone();
        assert!(step(&mut s, Action::ChooseEffectOption(2)).is_err());
        assert_eq!(s, before);
        let mut r = restore(&s);
        step(&mut s, Action::ChooseEffectOption(1)).unwrap();
        step(&mut r, Action::ChooseEffectOption(1)).unwrap();
        assert_eq!(s, r);
        let mut bad_x = restore(&s);
        bad_x.engine.pending_cast.as_mut().unwrap().x_value = Some(1);
        let before = bad_x.clone();
        assert!(
            validate_pending_cast(&bad_x, bad_x.engine.pending_cast.as_ref().unwrap()).is_err()
        );
        assert_eq!(bad_x, before);
        assert_eq!(next(&mut s), next(&mut r));
        assert_eq!(s, r);
        step(&mut s, Action::ChooseTarget(Target::Player(p.opponent()))).unwrap();
        step(&mut r, Action::ChooseTarget(Target::Player(p.opponent()))).unwrap();
        assert_eq!(next(&mut s), next(&mut r));
        assert_eq!(s, r);
        let row = s.stack.last().unwrap().clone();
        validated_stack_item_target_spec(&row, &s).unwrap();
        for mode in 0..4 {
            let mut bad = restore(&s);
            let source = row.v4.source_contract.unwrap();
            let index = source
                .finalized_cast_binding
                .unwrap()
                .omniscience_cast_v1
                .unwrap() as usize;
            match mode {
                0 => {
                    let CommittedEvent::OmniscienceCastSelectedV1 { choice, .. } =
                        &mut bad.engine.event_history[index]
                    else {
                        panic!()
                    };
                    choice.free = Some(false);
                }
                1 => {
                    let CommittedEvent::OmniscienceCastSelectedV1 { source, .. } =
                        &mut bad.engine.event_history[index]
                    else {
                        panic!()
                    };
                    source.controller = p.opponent();
                }
                2 => {
                    let CommittedEvent::OmniscienceCastSelectedV1 { stack_item_id, .. } =
                        &mut bad.engine.event_history[index]
                    else {
                        panic!()
                    };
                    stack_item_id.0 += 1;
                }
                _ => {
                    let event = bad.engine.event_history[index].clone();
                    bad.engine.event_history.push(event);
                }
            }
            let before = bad.clone();
            assert!(validated_stack_item_target_spec(&row, &bad).is_err());
            assert_eq!(bad, before);
        }
        let copy = copy_spell_for(&mut s, card, p).unwrap().0;
        let copied = s.stack.last().unwrap();
        assert!(copied.is_copy);
        assert_eq!(copied.v4.cast_method, Some(CastMethodV4::Normal));
        assert!(copied
            .v4
            .source_contract
            .unwrap()
            .spell_cast_origin
            .is_none());
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(card).zone, Zone::Graveyard);
        assert!(!s.players[p.index()].graveyard.contains(&copy));
    }
}
#[test]
fn paid_source_control_suppression_face_and_independent_hidden_world_operands_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut a = ready(p, "Forest");
        let mut b = ready(p, "Island");
        let omni = paid(&mut a, p);
        assert_eq!(paid(&mut b, p), omni);
        let bolt = put(&mut a, p, "Lightning Bolt", Zone::Hand);
        assert_eq!(put(&mut b, p, "Lightning Bolt", Zone::Hand), bolt);
        assert_ne!(a.state_hash(), b.state_hash());
        assert_eq!(next(&mut a), next(&mut b));
        let before_a = a.clone();
        let before_b = b.clone();
        assert!(offered_free(&a, bolt, p));
        assert_eq!(offered_free(&a, bolt, p), offered_free(&b, bolt, p));
        assert_eq!(a, before_a);
        assert_eq!(b, before_b);
        pay(
            &mut a,
            p,
            bolt,
            (true, 0, false),
            &[Target::Player(p.opponent())],
            None,
        );
        pay(
            &mut b,
            p,
            bolt,
            (true, 0, false),
            &[Target::Player(p.opponent())],
            None,
        );
        assert_eq!(a.stack.last(), b.stack.last());
        let mut s = restore(&a);
        // Labelled native control operand on genuinely paid source history.
        crate::standard_cards_v1::gain_control(&mut s, omni, p.opponent());
        assert!(active_source(&s, p).is_none());
        assert_eq!(active_source(&s, p.opponent()).unwrap().source, omni);
        // This is a native suppression operand, not a legal paid Tidebinder claim.
        let source = put(&mut s, p.opponent(), "Bear Cub", Zone::Battlefield);
        let link = crate::state::ObjectLinkV4 {
            object: source,
            zone_change_count: s.objects.get(source).zone_change_count,
        };
        s.objects
            .get_mut(omni)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .suppressed_by
            .push(crate::standard_creatures_v1::TidebinderSuppressionV1 {
                source: link,
                timestamp: 1,
            });
        assert!(active_source(&s, p.opponent()).is_none());
        let mut r = restore(&s);
        assert!(active_source(&r, p.opponent()).is_none());
        event::propose_and_commit(
            &mut r,
            event::ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        assert_eq!(active_source(&r, p.opponent()).unwrap().source, omni);
        // Native neutral face-down operand, with no hidden-definition lookahead.
        r.objects.get_mut(omni).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        assert!(active_source(&r, p.opponent()).is_none());
        assert!(active_source(&restore(&r), p.opponent()).is_none());
    }
}

#[test]
fn genuine_paid_replication_copy_and_free_only_resource_window_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let original = paid(&mut s, p);
        let replication = cast(
            &mut s,
            p,
            "Extravagant Replication",
            (false, 0, false),
            &[],
            None,
            6,
        );
        finish(&mut s);
        let mut selected = false;
        for _ in 0..256 {
            let d = next(&mut s);
            if let Decision::ChooseTargets {
                spell,
                legal_targets,
                ..
            } = &d
            {
                assert_eq!(*spell, replication);
                assert!(legal_targets.contains(&Target::Object(original)));
                selected = true;
                break;
            }
            pass(&mut s, d);
        }
        assert!(selected);
        let mut replay = restore(&s);
        step(&mut s, Action::ChooseTarget(Target::Object(original))).unwrap();
        step(&mut replay, Action::ChooseTarget(Target::Object(original))).unwrap();
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        let copies: Vec<_> = s.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|id| s.objects.get(*id).v4.is_token && definition(s.objects.get(*id).card_def))
            .collect();
        assert_eq!(copies.len(), 1);
        let copy = copies[0];
        cast(
            &mut s,
            p.opponent(),
            "Broken Wings",
            (false, 0, false),
            &[Target::Object(original)],
            None,
            3,
        );
        finish(&mut s);
        assert_eq!(s.objects.get(original).zone, Zone::Graveyard);
        assert_eq!(active_source(&s, p).unwrap().source, copy);
        // Explicit native exhausted-land operand after genuine paid entry/copy.
        // The actual cast supplies its own real announcement and zero payment.
        for object in s.players[p.index()].battlefield.clone() {
            if card_def::CARD_DEFS[s.objects.get(object).card_def as usize].is_land {
                s.objects.get_mut(object).tapped = true;
            }
        }
        priority(&mut s, p);
        let bolt = put(&mut s, p, "Lightning Bolt", Zone::Hand);
        assert!(!is_castable_now(p, bolt, CastMethodV4::Normal, &s));
        assert!(offered_free(&s, bolt, p));
        step(&mut s, Action::CastSpell(bolt)).unwrap();
        let pending = s.engine.pending_cast.as_ref().unwrap();
        assert_eq!(pending.omniscience_v1.unwrap().free, Some(true));
        assert_eq!(pending.x_value, Some(0));
        assert!(matches!(next(&mut s), Decision::ChooseTargets { .. }));
        let mut replay = restore(&s);
        for branch in [&mut s, &mut replay] {
            step(branch, Action::ChooseTarget(Target::Player(p.opponent()))).unwrap();
            assert!(matches!(next(branch),Decision::CastSpellOrPass{player,..}if player==p));
            let row = branch.stack.last().unwrap();
            validated_stack_item_target_spec(row, branch).unwrap();
            let c = row.v4.source_contract.unwrap();
            let index = c
                .finalized_cast_binding
                .unwrap()
                .omniscience_cast_v1
                .unwrap();
            let CommittedEvent::OmniscienceCastSelectedV1 { choice, .. } =
                branch.engine.event_history[index as usize]
            else {
                panic!()
            };
            assert_eq!(
                options(branch, choice.options_history_index)
                    .unwrap()
                    .source
                    .source,
                copy
            );
            assert_eq!(branch.players[p.index()].mana_pool, [0; 6]);
        }
        assert_eq!(s, replay);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
    }
}
