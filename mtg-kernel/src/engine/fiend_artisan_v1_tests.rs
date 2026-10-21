//! Genuine casts and native actions; explicit setup/suppression/copy operands
//! are identified in their cases. No free mana pool or fabricated permission.
use super::*;
use crate::{policy_observation_v6::tests::put, state::Step};
const NAME: &str = "Fiend Artisan";
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
        110_483,
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
        Decision::ChooseTargets {
            player,
            legal_targets,
            ..
        } => {
            let t = Target::Player(player.opponent());
            assert!(legal_targets.contains(&t));
            Action::ChooseTarget(t)
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            if can_finish {
                Action::FinishEffectSelection
            } else {
                Action::ChooseEffectTarget(legal_targets[0])
            }
        }
        Decision::ChooseLegendPermanent { candidates, .. } => Action::ChooseLegendPermanent(
            candidates
                .iter()
                .copied()
                .find(|id| !s.objects.get(*id).v4.is_token)
                .unwrap_or(candidates[0]),
        ),
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
        d => panic!("Fiend Artisan native priority {d:?}"),
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
        if matches!(d, Decision::CastSpellOrPass { .. })
            && s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
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
    let decision = next(s);
    assert!(
        matches!(&decision,Decision::CastSpellOrPass{castable_spells,..}if castable_spells.contains(&card)),
        "{} not castable for {p:?} during {:?}: {decision:?}; untapped mana sources: {:?}",
        s.objects.get(card).name,
        s.step,
        s.players[p.index()]
            .battlefield
            .iter()
            .filter_map(|id| {
                let object = s.objects.get(*id);
                (!object.tapped && card_def::CARD_DEFS[object.card_def as usize].is_land)
                    .then_some(object.name.as_str())
            })
            .collect::<Vec<_>>()
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
    let c = cast(s, p, NAME, (false, 0, false), &[], None, 2);
    finish(s);
    assert_eq!(s.objects.get(c).zone, Zone::Battlefield);
    assert_eq!(checked_effective_pt(s, c).unwrap(), (1, 1));
    c
}
fn mature(s: &mut GameState, p: PlayerId, source: ObjectId) {
    for _ in 0..128 {
        let d = next(s);
        if matches!(d,Decision::CastSpellOrPass{player,..}if player==p)
            && s.active_player == p
            && s.step == Step::Main1
            && !creature_summoning_sick(
                s,
                source,
                &card_def::CARD_DEFS[s.objects.get(source).card_def as usize],
            )
        {
            return;
        }
        pass(s, d);
    }
    panic!("source did not reach its genuine next main phase");
}
fn activate(
    s: &mut GameState,
    p: PlayerId,
    source: ObjectId,
    sacrifice: ObjectId,
    x: u16,
) -> StackItem {
    priority(s, p);
    assert!(available_activatable_abilities(p, s).contains(&(source, 0)));
    step(s, Action::ActivateAbility(source, 0)).unwrap();
    assert!(
        matches!(next(s),Decision::ChooseEffectOption{player,source:c,option_count}
        if player==p && c==source && x<option_count)
    );
    assert!(s
        .engine
        .pending_activation
        .as_ref()
        .unwrap()
        .mana_x_v1
        .is_none());
    let mut restored = restore(s);
    step(s, Action::ChooseEffectOption(x)).unwrap();
    step(&mut restored, Action::ChooseEffectOption(x)).unwrap();
    assert_eq!(*s, restored);
    for _ in 0..8 {
        match next(s) {
            Decision::ChooseCostTargets {
                player, candidates, ..
            } => {
                assert_eq!(player, p);
                assert!(candidates.contains(&sacrifice));
                let mut r = restore(s);
                step(s, Action::ChooseCostTarget(sacrifice)).unwrap();
                step(&mut r, Action::ChooseCostTarget(sacrifice)).unwrap();
                assert_eq!(*s, r);
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, p);
                assert!(s.engine.pending_activation.is_none());
                let item = s
                    .stack
                    .iter()
                    .find(|r| r.source == source && r.kind == StackItemKind::ActivatedAbility)
                    .unwrap()
                    .clone();
                assert!(s.objects.get(source).tapped);
                assert!(s.objects.get(sacrifice).zone != Zone::Battlefield);
                assert_eq!(item.v4.x_value, x);
                assert_eq!(
                    crate::effect::fiend_artisan_v1::paid_x(s, &item).unwrap(),
                    x as u8
                );
                validated_stack_item_target_spec(&item, s).unwrap();
                return item;
            }
            d => panic!("actual Fiend activation stage {d:?}"),
        }
    }
    panic!("native activation not published");
}
fn search(s: &mut GameState, p: PlayerId) -> Decision {
    for _ in 0..16 {
        let d = next(s);
        if matches!(d,Decision::ChooseEffectTargets{player,..}if player==p) {
            return d;
        }
        pass(s, d);
    }
    panic!("native original search missing");
}
fn answer(s: &mut GameState, target: Option<ObjectId>) {
    let a = target.map_or(Action::FinishEffectSelection, |c| {
        Action::ChooseEffectTarget(Target::Object(c))
    });
    let mut r = restore(s);
    step(&mut r, a.clone()).unwrap();
    step(s, a).unwrap();
    assert_eq!(*s, r);
    // Snapshot after a real answer, before its bound frame executes.
    crate::effect::validate_pending_effect_choice(s).unwrap();
    for mode in 0..3 {
        let mut bad = restore(s);
        let pending = bad.engine.pending_effect.as_mut().unwrap();
        if mode == 0 {
            pending.answered_choice_guard = None;
        } else {
            let changed_x = if pending.ctx.x_value == 0 { 1 } else { 0 };
            let frame = pending.frames.last_mut().unwrap();
            let effect::EffectFrame::SearchLibraryCardsToDestination {
                filter,
                max_targets,
                ..
            } = frame
            else {
                panic!("paid Fiend answer lost its original search frame")
            };
            if mode == 1 {
                *filter = effect::LibraryCardFilter::CreatureManaValueAtMost(changed_x);
            } else {
                *max_targets = 2;
            }
            // Coherently changing both copies must still fail the paid native
            // search contract, even though the generic guard copies agree.
            pending.answered_choice_guard =
                Some(effect::EffectAnsweredChoiceGuard::StandardSelection {
                    frame: Box::new(frame.clone()),
                });
        }
        let before = bad.clone();
        assert!(effect::validate_pending_effect_choice(&bad).is_err());
        assert_eq!(bad, before);
    }
    let mut r = restore(s);
    finish(s);
    finish(&mut r);
    assert_eq!(*s, r);
}
#[test]
fn paid_hybrid_printing_sorcery_mana_x_tap_sacrifice_search_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        assert_eq!(id(NAME), 483);
        assert!(crate::effect::fiend_artisan_v1::definition(id(NAME)));
        let source = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", (false, 0, false), &[], None, 1);
        finish(&mut s);
        assert!(!available_activatable_abilities(p, &s).contains(&(source, 0)));
        mature(&mut s, p, source);
        // A genuine new Elf may be sacrificed immediately; its cost has no T.
        let young = cast(&mut s, p, "Llanowar Elves", (false, 0, false), &[], None, 1);
        finish(&mut s);
        assert!(creature_summoning_sick(
            &s,
            young,
            &card_def::CARD_DEFS[s.objects.get(young).card_def as usize]
        ));
        let found = put(&mut s, p, "Axgard Cavalry", Zone::Library);
        let too_large = put(&mut s, p, "Meteor Golem", Zone::Library);
        let noncreature = put(&mut s, p, "Lightning Bolt", Zone::Library);
        let before = tapped(&s, p);
        activate(&mut s, p, source, young, 2);
        assert_eq!(tapped(&s, p) - before, 4);
        assert_eq!(s.objects.get(young).zone, Zone::Graveyard);
        assert_eq!(checked_effective_pt(&s, source).unwrap(), (2, 2));
        assert!(s.objects.get(elf).zone == Zone::Battlefield);
        let d = search(&mut s, p);
        assert!(
            matches!(d,Decision::ChooseEffectTargets{legal_targets,can_finish:true,..}
            if legal_targets.contains(&Target::Object(found)) && !legal_targets.contains(&Target::Object(too_large))
                && !legal_targets.contains(&Target::Object(noncreature)))
        );
        answer(&mut s, Some(found));
        assert_eq!(s.objects.get(found).zone, Zone::Battlefield);
        assert!(!s.objects.get(found).tapped);
    }
}
#[test]
fn paid_zero_x_fail_to_find_and_real_mana_before_sacrificing_elf_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let source = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", (false, 0, false), &[], None, 1);
        finish(&mut s);
        mature(&mut s, p, source);
        // Explicit native resource-exhaustion primitive, not a claimed paid spell.
        let lands = s.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|id| object_has_type(&s, *id, CardType::Land))
            .collect::<Vec<_>>();
        for c in lands {
            event::propose_and_commit(&mut s, event::ProposedEvent::tap(c));
        }
        let start = s.engine.event_history.len();
        activate(&mut s, p, source, elf, 0);
        let events = &s.engine.event_history[start..];
        let mana_tap = events
            .iter()
            .position(|e| matches!(e,event::CommittedEvent::Tap{object}if *object==elf))
            .unwrap();
        let sac = events
            .iter()
            .position(|e| matches!(e,event::CommittedEvent::Sacrificed{object,..}if *object==elf))
            .unwrap();
        assert!(mana_tap < sac);
        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
        assert!(
            matches!(search(&mut s,p),Decision::ChooseEffectTargets{legal_targets,can_finish:true,..}if legal_targets.is_empty())
        );
        let before = s.players[p.index()].library.len();
        answer(&mut s, None);
        assert_eq!(s.players[p.index()].library.len(), before);
        assert_eq!(s.objects.get(elf).zone, Zone::Graveyard);
    }
}
#[test]
fn paid_original_x_survives_source_departure_and_native_counter_keeps_all_costs_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let source = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", (false, 0, false), &[], None, 1);
        finish(&mut s);
        mature(&mut s, p, source);
        let found = put(&mut s, p, "Axgard Cavalry", Zone::Library);
        let row = activate(&mut s, p, source, elf, 2);
        // Existing native counter primitive on the genuinely paid activation.
        // Counterspell cannot target an ability and is not claimed here.
        let mut countered = restore(&s);
        assert_eq!(
            counter_stack_item_by_id(&mut countered, row.v4.stack_item_id).unwrap(),
            Some(row.clone())
        );
        assert!(countered.objects.get(source).tapped);
        assert_eq!(countered.objects.get(elf).zone, Zone::Graveyard);
        assert_eq!(countered.objects.get(found).zone, Zone::Library);
        // Genuine opposing paid Lightning Bolt response removes the source.
        cast(
            &mut s,
            p.opponent(),
            "Lightning Bolt",
            (false, 0, false),
            &[Target::Object(source)],
            None,
            1,
        );
        let d = search(&mut s, p);
        assert!(
            matches!(d,Decision::ChooseEffectTargets{legal_targets,..}if legal_targets.contains(&Target::Object(found)))
        );
        assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(
            crate::effect::fiend_artisan_v1::paid_x(&s, &row).unwrap(),
            2
        );
        answer(&mut s, Some(found));
        assert_eq!(s.objects.get(found).controller, p);
    }
}
#[test]
fn paid_x_row_input_pending_and_search_forgeries_refuse_readonly_both_seats() {
    use crate::effect::{self, EffectFrame, EffectOp};
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let source = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", (false, 0, false), &[], None, 1);
        finish(&mut s);
        mature(&mut s, p, source);
        step(&mut s, Action::ActivateAbility(source, 0)).unwrap();
        assert!(matches!(next(&mut s), Decision::ChooseEffectOption { .. }));
        let original = restore(&s);
        let mut wrong_timing = original.clone();
        wrong_timing.step = Step::BeginCombat;
        let before = wrong_timing.clone();
        assert!(validate_pending_activation(
            &wrong_timing,
            wrong_timing.engine.pending_activation.as_ref().unwrap()
        )
        .is_err());
        assert_eq!(wrong_timing, before);

        assert!(
            !serde_json::to_value(s.engine.pending_activation.as_ref().unwrap())
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("mana_x_v1")
        );
        let before = s.clone();
        assert!(step(&mut s, Action::ChooseEffectOption(256)).is_err());
        assert_eq!(s, before);
        let mut bad = original.clone();
        bad.engine.pending_activation.as_mut().unwrap().mana_x_v1 = Some(255);
        let before = bad.clone();
        assert!(
            validate_pending_activation(&bad, bad.engine.pending_activation.as_ref().unwrap())
                .is_err()
        );
        assert_eq!(bad, before);
        s = original;
        step(&mut s, Action::ChooseEffectOption(2)).unwrap();
        loop {
            match next(&mut s) {
                Decision::ChooseCostTargets { candidates, .. } => {
                    assert!(candidates.contains(&elf));
                    step(&mut s, Action::ChooseCostTarget(elf)).unwrap();
                }
                Decision::CastSpellOrPass { .. } => break,
                d => panic!("{d:?}"),
            }
        }
        let row = s.stack.last().unwrap().clone();
        for mode in 0..4 {
            let mut bad = restore(&s);
            let mut forged = row.clone();
            match mode {
                0 => {
                    forged.v4.x_value = 1;
                    *bad.stack.last_mut().unwrap() = forged.clone();
                }
                1 => {
                    for e in &mut bad.engine.event_history {
                        if let event::CommittedEvent::FiendArtisanPaidV1 { item, .. } = e {
                            item.v4.x_value = 1;
                        }
                    }
                }
                2 => {
                    for e in &mut bad.engine.event_history {
                        if let event::CommittedEvent::FiendArtisanInputV1 { input } = e {
                            input.pending.mana_x_v1 = Some(1);
                        }
                    }
                }
                _ => {
                    for e in &mut bad.engine.event_history {
                        if let event::CommittedEvent::FiendArtisanInputV1 { input } = e {
                            input.pending.object_cost_chosen[0].expected_zone_change_count += 1;
                        }
                    }
                }
            }
            let before = bad.clone();
            assert!(validated_stack_item_target_spec(&forged, &bad).is_err());
            assert_eq!(bad, before);
        }
        search(&mut s, p);
        let pending = s.engine.pending_effect.as_ref().unwrap().clone();
        let before = s.clone();
        assert!(effect::resume_resumable_resolution(&mut s).is_err());
        assert_eq!(s, before);
        assert!(effect::begin_resumable_resolution(
            &effect::fiend_artisan_v1::program(),
            &pending.ctx,
            pending.resolving_item.clone(),
            &mut s
        )
        .is_err());
        assert_eq!(s, before);
        for mode in 0..4 {
            let mut bad = restore(&s);
            let q = bad.engine.pending_effect.as_mut().unwrap();
            match mode {
                0 => q.ctx.x_value = 1,
                1 => {
                    if let Some(effect::PendingEffectChoice::SelectTargets {
                        purpose:
                            effect::EffectTargetSelectionPurpose::SearchLibraryCardsToDestination {
                                filter,
                                ..
                            },
                        ..
                    }) = &mut q.choice
                    {
                        *filter = effect::LibraryCardFilter::CreatureManaValueAtMost(1);
                    }
                }
                2 => {
                    q.choice = None;
                    q.frames = vec![EffectFrame::Program {
                        op: effect::fiend_artisan_v1::program(),
                        path: vec![],
                    }];
                }
                _ => {
                    q.resolving_item.inline_effect = Some(EffectOp::DrawCards {
                        player: effect::PlayerRef::Controller,
                        count: 1,
                    });
                    q.resolving_item.kind = StackItemKind::Spell;
                    q.ctx.stack_item_id = None;
                    q.choice = None;
                    q.answered_choice_guard = None;
                    q.frames = vec![EffectFrame::Program {
                        op: q.resolving_item.inline_effect.clone().unwrap(),
                        path: vec![],
                    }];
                }
            }
            let before = bad.clone();
            assert!(effect::validate_pending_effect_choice(&bad).is_err());
            assert_eq!(bad, before);
            assert!(effect::resume_resumable_resolution(&mut bad).is_err());
            assert_eq!(bad, before);
        }
    }
}
#[test]
fn paid_battlefield_only_graveyard_cards_count_control_suppression_and_checked_boundary_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let source = paid(&mut s, p);
        // Native zone operands, no invented paid discard or death claim.
        for (holder, name) in [
            (p, "Llanowar Elves"),
            (p, "Axgard Cavalry"),
            (p, "Lightning Bolt"),
            (p.opponent(), "Llanowar Elves"),
        ] {
            let c = put(&mut s, holder, name, Zone::Hand);
            event::propose_and_commit(
                &mut s,
                event::ProposedEvent::zone_change(c, Zone::Graveyard),
            );
        }
        assert_eq!(checked_effective_pt(&s, source).unwrap(), (3, 3));
        // Native transient token-zone operand: a creature token is not a card.
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::create_token(id("Elf Warrior Token"), p),
        );
        let token = *s.players[p.index()].battlefield.last().unwrap();
        assert!(s.objects.get(token).v4.is_token);
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(token, Zone::Graveyard),
        );
        assert_eq!(checked_effective_pt(&s, source).unwrap(), (3, 3));
        // Existing genuine spell-copy/counter primitives on a second real paid
        // Fiend spell. A virtual creature spell is not a graveyard creature card.
        let original = cast(&mut s, p, NAME, (false, 0, false), &[], None, 2);
        let (copy, row) = copy_spell_for(&mut s, original, p).unwrap();
        assert!(s.objects.get(copy).spell_copy_origin.is_some());
        counter_stack_item_by_id(&mut s, row).unwrap();
        assert_eq!(checked_effective_pt(&s, source).unwrap(), (3, 3));
        let original_row = s
            .stack
            .iter()
            .find(|r| r.source == original)
            .unwrap()
            .v4
            .stack_item_id;
        counter_stack_item_by_id(&mut s, original_row).unwrap();
        assert_eq!(checked_effective_pt(&s, source).unwrap(), (4, 4));
        crate::standard_cards_v1::gain_control(&mut s, source, p.opponent());
        assert_eq!(checked_effective_pt(&s, source).unwrap(), (2, 2));
        // Explicit neutral face-down primitive hides the printed static.
        let mut neutral = restore(&s);
        neutral.objects.get_mut(source).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        assert_eq!(checked_effective_pt(&neutral, source).unwrap(), (2, 2));
        // Genuine opposing Witness Protection suppresses the static in layer6.
        cast(
            &mut s,
            p,
            "Witness Protection",
            (false, 0, false),
            &[Target::Object(source)],
            None,
            1,
        );
        finish(&mut s);
        assert_eq!(checked_effective_pt(&s, source).unwrap(), (1, 1));
        let mut width = ready(p, "Forest");
        let c = paid(&mut width, p);
        // Explicit native full-width counter operand at i32's exact upper edge.
        width.objects.get_mut(c).counters.plus1_plus1 = i32::MAX - 1;
        assert_eq!(
            checked_effective_pt(&width, c).unwrap(),
            (i32::MAX, i32::MAX)
        );
        let card = put(&mut width, p, "Llanowar Elves", Zone::Hand);
        let before = width.clone();
        event::propose_and_commit(
            &mut width,
            event::ProposedEvent::zone_change(card, Zone::Graveyard),
        );
        assert!(width.engine.halted.is_some());
        width.engine.halted = before.engine.halted;
        assert_eq!(width, before);
        event::propose_and_commit(
            &mut s,
            event::ProposedEvent::zone_change(source, Zone::Hand),
        );
        assert_eq!(checked_effective_pt(&s, source).unwrap(), (1, 1));
    }
}
#[test]
fn paid_closed_copies_season_copy_of_copy_and_native_virtual_ancestry_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        // Season's generic cost and both Reflections share the same turn.
        // Preserve enough initial blue sources after automatic generic payment.
        for _ in 0..4 {
            put(&mut s, p, "Island", Zone::Battlefield);
        }
        let source = paid(&mut s, p);
        cast(
            &mut s,
            p,
            "Doubling Season",
            (false, 0, false),
            &[],
            None,
            5,
        );
        finish(&mut s);
        cast(
            &mut s,
            p,
            "Self-Reflection",
            (false, 0, false),
            &[Target::Object(source)],
            None,
            6,
        );
        finish(&mut s);
        let copies = s.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|c| s.objects.get(*c).card_def == id(NAME) && s.objects.get(*c).v4.is_token)
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 2);
        for &c in &copies {
            assert_eq!(checked_effective_pt(&s, c).unwrap(), (1, 1));
            assert_eq!(s.objects.get(c).counters.plus1_plus1, 0);
        }
        let reflection = cast(
            &mut s,
            p,
            "Self-Reflection",
            (false, 0, false),
            &[Target::Object(copies[0])],
            None,
            6,
        );
        // Genuine existing copy-spell primitive on the original paid source;
        // no absent Limited copy-spell printing or paid copy spell is claimed.
        let (virtual_copy, _) = copy_spell_for(&mut s, reflection, p).unwrap();
        assert!(s.objects.get(virtual_copy).spell_copy_origin.is_some());
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(
            s.players[p.index()]
                .battlefield
                .iter()
                .filter(
                    |c| s.objects.get(**c).card_def == id(NAME) && s.objects.get(**c).v4.is_token
                )
                .count(),
            6
        );
        cast(
            &mut s,
            p,
            "Electroduplicate",
            (false, 0, false),
            &[Target::Object(source)],
            None,
            3,
        );
        finish(&mut s);
        assert!(s.players[p.index()]
            .battlefield
            .iter()
            .any(|c| s.objects.get(*c).card_def == id(NAME)
                && event::copiable_token_abilities_v1(&s, *c).unwrap() > 0));
    }
}
#[test]
fn paid_original_search_uses_captured_controller_and_independent_hidden_worlds_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut a = ready(p, "Forest");
        let mut b = ready(p, "Island");
        let source = paid(&mut a, p);
        assert_eq!(paid(&mut b, p), source);
        let mut rows = vec![];
        for s in [&mut a, &mut b] {
            let elf = cast(s, p, "Llanowar Elves", (false, 0, false), &[], None, 1);
            finish(s);
            mature(s, p, source);
            rows.push(activate(s, p, source, elf, 2));
        }
        assert_eq!(rows[0], rows[1]);
        assert_ne!(a.state_hash(), b.state_hash());
        assert_eq!(search(&mut a, p), search(&mut b, p));
        assert_eq!(
            a.engine.pending_effect.as_ref().unwrap().choice,
            b.engine.pending_effect.as_ref().unwrap().choice
        );
        answer(&mut a, None);
        answer(&mut b, None);
        let mut s = ready(p, "Forest");
        let source = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", (false, 0, false), &[], None, 1);
        finish(&mut s);
        mature(&mut s, p, source);
        let found = put(&mut s, p, "Axgard Cavalry", Zone::Library);
        activate(&mut s, p, source, elf, 2);
        // Native control primitive after payment, not a paid control spell.
        crate::standard_cards_v1::gain_control(&mut s, source, p.opponent());
        assert!(matches!(search(&mut s,p),Decision::ChooseEffectTargets{player,..}if player==p));
        answer(&mut s, Some(found));
        assert_eq!(s.objects.get(found).controller, p);
    }
}
#[test]
fn paid_selected_sacrifice_final_pt_overflow_refuses_before_any_payment_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let source = paid(&mut s, p);
        let elf = cast(&mut s, p, "Llanowar Elves", (false, 0, false), &[], None, 1);
        finish(&mut s);
        mature(&mut s, p, source);
        // Exact native counter-width operand on the genuinely paid source.
        s.objects.get_mut(source).counters.plus1_plus1 = i32::MAX - 1;
        assert_eq!(
            checked_effective_pt(&s, source).unwrap(),
            (i32::MAX, i32::MAX)
        );
        step(&mut s, Action::ActivateAbility(source, 0)).unwrap();
        assert!(matches!(next(&mut s), Decision::ChooseEffectOption { .. }));
        step(&mut s, Action::ChooseEffectOption(0)).unwrap();
        // The sole actual another creature is automatically selected by Advance;
        // snapshot before that boundary and preserve its real selection on error.
        assert_eq!(s.objects.get(elf).zone, Zone::Battlefield);
        let mana_before = s.players[p.index()].mana_pool;
        let taps_before = tapped(&s, p);
        let history_before = s.engine.event_history.len();
        assert!(matches!(
            advance_until_decision(&mut s),
            Decision::Halted { .. }
        ));
        assert_eq!(s.players[p.index()].mana_pool, mana_before);
        assert_eq!(tapped(&s, p), taps_before);
        assert_eq!(s.objects.get(elf).zone, Zone::Battlefield);
        assert_eq!(s.engine.event_history.len(), history_before);
        assert!(!s.objects.get(source).tapped);
        assert!(s.stack.is_empty());
    }
}
#[test]
fn paid_replication_real_upkeep_copy_and_removed_spell_copy_target_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let source = paid(&mut s, p);
        cast(
            &mut s,
            p,
            "Extravagant Replication",
            (false, 0, false),
            &[],
            None,
            6,
        );
        finish(&mut s);
        let mut chosen = false;
        for _ in 0..128 {
            let d = next(&mut s);
            if let Decision::ChooseTargets {
                player,
                ref legal_targets,
                ..
            } = d
            {
                assert_eq!(player, p);
                assert!(legal_targets.contains(&Target::Object(source)));
                let mut r = restore(&s);
                step(&mut s, Action::ChooseTarget(Target::Object(source))).unwrap();
                step(&mut r, Action::ChooseTarget(Target::Object(source))).unwrap();
                assert_eq!(s, r);
                chosen = true;
                break;
            }
            pass(&mut s, d);
        }
        assert!(
            chosen,
            "real next upkeep never requested its mandatory target"
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(
            s.players[p.index()]
                .battlefield
                .iter()
                .filter(
                    |c| s.objects.get(**c).card_def == id(NAME) && s.objects.get(**c).v4.is_token
                )
                .count(),
            1
        );
        // A separate real paid cast/response scene leaves the original target
        // incarnation. No manufactured stack item or substituted printing.
        let mut s = ready(p, "Forest");
        let source = paid(&mut s, p);
        cast(
            &mut s,
            p,
            "Self-Reflection",
            (false, 0, false),
            &[Target::Object(source)],
            None,
            6,
        );
        cast(
            &mut s,
            p.opponent(),
            "Lightning Bolt",
            (false, 0, false),
            &[Target::Object(source)],
            None,
            1,
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
        assert!(!s.players[p.index()]
            .battlefield
            .iter()
            .any(|c| s.objects.get(*c).card_def == id(NAME)));
    }
}
