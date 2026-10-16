//! Actual land-funded printing/casts. Native setup/control/zone primitives are labelled.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};
use thousand_year_storm_v1::{validate_published, JournalV1};
const NAME: &str = "Thousand-Year Storm";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(actor: PlayerId, hidden: &str) -> GameState {
    let a = [id("Forest"); 64];
    let b = [id(hidden); 64];
    let (a, b) = if actor == PlayerId::P0 {
        (&a, &b)
    } else {
        (&b, &a)
    };
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_485,
        actor,
    );
    s.step = Step::Main1;
    for p in [actor, actor.opponent()] {
        s.players[p.index()].life = 200; // Initial longevity setup, never a gain event.
        for name in ["Island", "Mountain", "Forest", "Plains", "Swamp"] {
            for _ in 0..16 {
                put(&mut s, p, name, Zone::Battlefield);
            }
        }
    }
    s
}
fn next(s: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(s);
    assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
    d
}
fn action(s: &mut GameState, a: Action) {
    engine::step(s, a).unwrap();
    assert!(s.engine.halted.is_none());
}
fn answer(s: &mut GameState, d: Decision) {
    let a = match d {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseTargets {
            player,
            legal_targets,
            ..
        } => {
            let t = Target::Player(player.opponent());
            assert!(legal_targets.contains(&t));
            Action::ChooseTarget(t)
        }
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        other => panic!("unexpected genuine Storm fixture decision {other:?}"),
    };
    action(s, a)
}
fn priority(s: &mut GameState, p: PlayerId) {
    for _ in 0..8 {
        let d = next(s);
        match d {
            Decision::CastSpellOrPass { player, .. } if player == p => return,
            other => answer(s, other),
        }
    }
    panic!("priority was not offered")
}
fn taps(s: &GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|id| {
            let o = s.objects.get(**id);
            o.tapped && card_def::CARD_DEFS[usize::from(o.card_def)].is_land
        })
        .count()
}
fn cast(s: &mut GameState, p: PlayerId, name: &str, targets: &[Target], cost: usize) -> ObjectId {
    cast_choices(s, p, name, targets, cost, false, 0)
}
fn cast_choices(
    s: &mut GameState,
    p: PlayerId,
    name: &str,
    targets: &[Target],
    cost: usize,
    kicker: bool,
    x: u16,
) -> ObjectId {
    priority(s, p);
    let object = put(s, p, name, Zone::Hand);
    let before = taps(s, p);
    action(s, Action::CastSpell(object));
    let mut chosen = 0;
    for _ in 0..targets.len() + 64 {
        let d = next(s);
        match d {
            Decision::ChooseKicker { .. } | Decision::ChooseVariableCastKicker { .. } => {
                action(s, Action::ChooseKicker(kicker))
            }
            Decision::ChooseEffectOption { option_count, .. }
                if s.engine.pending_cast.is_some() =>
            {
                assert!(x < option_count);
                action(s, Action::ChooseEffectOption(x));
            }
            Decision::ChooseTargets {
                can_finish: true, ..
            }
            | Decision::ChooseVariableCastTargets {
                can_finish: true, ..
            } if chosen == targets.len() => action(s, Action::FinishEffectSelection),
            Decision::ChooseTargets { legal_targets, .. }
            | Decision::ChooseVariableCastTargets { legal_targets, .. } => {
                let t = targets[chosen];
                assert!(
                    legal_targets.contains(&t),
                    "actual targets {legal_targets:?}"
                );
                action(s, Action::ChooseTarget(t));
                chosen += 1;
            }
            Decision::OrderTriggers { .. } => answer(s, d),
            Decision::CastSpellOrPass { player, .. } if player == p => {
                assert_eq!(chosen, targets.len());
                assert!(s.engine.pending_cast.is_none());
                let row = s
                    .stack
                    .iter()
                    .find(|r| r.source == object && r.kind == crate::state::StackItemKind::Spell)
                    .unwrap();
                assert_eq!(row.controller, p);
                assert!(!row.is_copy);
                assert_eq!(s.objects.get(object).zone, Zone::Stack);
                assert_eq!(taps(s, p) - before, cost);
                return object;
            }
            other => panic!("actual cast stage {other:?}"),
        }
    }
    panic!("finite paid cast did not finish")
}
fn finish(s: &mut GameState) {
    for _ in 0..512 {
        let d = next(s);
        if s.stack.is_empty()
            && s.engine.pending_triggers.is_empty()
            && s.engine.pending_effect.is_none()
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        answer(s, d)
    }
    panic!("finite Storm fixture did not settle")
}
fn paid_storm(s: &mut GameState, p: PlayerId) -> ObjectId {
    let source = cast(s, p, NAME, &[], 6);
    finish(s);
    assert_eq!(s.objects.get(source).zone, Zone::Battlefield);
    source
}
fn bolt(s: &mut GameState, p: PlayerId) {
    cast(s, p, "Lightning Bolt", &[Target::Player(p.opponent())], 1);
    finish(s)
}
fn restore(s: &GameState) -> GameState {
    let r: GameState = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert_eq!(*s, r);
    assert_eq!(s.state_hash(), r.state_hash());
    r
}
fn published(s: &GameState) -> Vec<StackItem> {
    s.stack
        .iter()
        .filter(|r| {
            r.inline_effect
                .as_ref()
                .is_some_and(|op| thousand_year_storm_v1::capture(op).is_some())
        })
        .cloned()
        .collect()
}
fn copies(s: &GameState) -> Vec<StackItem> {
    s.engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            event::CommittedEvent::ThousandYearStormV1 {
                journal: JournalV1::Copy { item, .. },
            } => Some((**item).clone()),
            _ => None,
        })
        .collect()
}
fn cast_count(s: &GameState) -> usize {
    s.engine
        .event_history
        .iter()
        .filter(|e| matches!(e, event::CommittedEvent::SpellCast { .. }))
        .count()
}
fn waiting(s: &mut GameState) {
    for _ in 0..32 {
        let d = next(s);
        if matches!(
            d,
            Decision::ChooseEffectTargets {
                can_finish: true,
                ..
            }
        ) {
            assert!(s.engine.pending_effect.is_some());
            return;
        }
        answer(s, d)
    }
    panic!("genuine target-copy checkpoint absent")
}

#[test]
fn genuine_paid_printing_zero_copy_and_predecessor_refusal_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        let source = paid_storm(&mut s, p);
        assert_eq!(id(NAME), 485);
        assert!(thousand_year_storm_v1::definition(485));
        assert_eq!(
            card_def::CARD_DEFS[485].capability,
            card_def::CardCapability::Partial
        );
        assert!(published(&s).is_empty());
        assert!(copies(&s).is_empty());
        assert!(thousand_year_storm_v1::refuse_legacy(&s).is_err());
        cast(
            &mut s,
            p,
            "Lightning Bolt",
            &[Target::Player(p.opponent())],
            1,
        );
        let rows = published(&s);
        assert_eq!(rows.len(), 1);
        validate_published(&s, &rows[0]).unwrap();
        assert_eq!(rows[0].v4.ability_source_contract.unwrap().source, source);
        let before = s.players[p.opponent().index()].life;
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert!(copies(&s).is_empty());
        assert_eq!(s.players[p.opponent().index()].life, before - 3);
        assert!(!card_def::CARD_DEFS[485].capability.is_fully_supported());
    }
}
#[test]
fn actual_prior_own_casts_exclude_opponent_permanents_current_cast_and_copies_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        bolt(&mut s, p);
        bolt(&mut s, p.opponent());
        cast(&mut s, p, "Llanowar Elves", &[], 1);
        finish(&mut s);
        paid_storm(&mut s, p);
        let casts = cast_count(&s);
        let before = s.players[p.opponent().index()].life;
        cast(
            &mut s,
            p,
            "Lightning Bolt",
            &[Target::Player(p.opponent())],
            1,
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(copies(&s).len(), 1);
        assert_eq!(cast_count(&s), casts + 1);
        assert_eq!(s.players[p.opponent().index()].life, before - 6);
    }
}
#[test]
fn paid_countered_original_and_destroyed_observer_keep_frozen_copies_and_controller_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        bolt(&mut s, p);
        let source = paid_storm(&mut s, p);
        let original = cast(
            &mut s,
            p,
            "Lightning Bolt",
            &[Target::Player(p.opponent())],
            1,
        );
        let row = published(&s)[0].clone();
        cast(
            &mut s,
            p.opponent(),
            "Counterspell",
            &[Target::Object(original)],
            2,
        );
        // Only the real top response resolves. The independently published Storm row remains.
        for _ in 0..12 {
            let d = next(&mut s);
            if s.objects.get(original).zone == Zone::Graveyard {
                break;
            }
            answer(&mut s, d)
        }
        assert_eq!(s.objects.get(original).zone, Zone::Graveyard);
        validate_published(&s, &row).unwrap();
        cast(
            &mut s,
            p.opponent(),
            "Broken Wings",
            &[Target::Object(source)],
            3,
        );
        let mut r = restore(&s);
        let before = s.players[p.opponent().index()].life;
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(copies(&s).len(), 1);
        assert_eq!(copies(&s)[0].controller, p);
        assert_eq!(s.players[p.opponent().index()].life, before - 3);
    }
}
#[test]
fn actual_two_target_copies_keep_original_slots_and_optional_new_targets_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        let a = cast(&mut s, p, "Llanowar Elves", &[], 1);
        finish(&mut s);
        let b = cast(&mut s, p.opponent(), "Llanowar Elves", &[], 1);
        finish(&mut s);
        bolt(&mut s, p);
        paid_storm(&mut s, p);
        cast(
            &mut s,
            p,
            "Run Away Together",
            &[Target::Object(a), Target::Object(b)],
            2,
        );
        waiting(&mut s);
        let original = s.engine.pending_effect.as_ref().unwrap().clone();
        let mut r = restore(&s);
        for branch in [&mut s, &mut r] {
            // Keeping each original target is optional and remains valid when it later leaves.
            action(branch, Action::FinishEffectSelection);
            let d = next(branch);
            assert!(matches!(
                d,
                Decision::ChooseEffectTargets {
                    can_finish: true,
                    ..
                }
            ));
            let ctx = &branch.engine.pending_effect.as_ref().unwrap().ctx;
            assert_eq!(ctx.controller, p);
            action(branch, Action::FinishEffectSelection);
            finish(branch);
        }
        assert_eq!(s, r);
        assert_eq!(copies(&s).len(), 1);
        assert_eq!(
            copies(&s)[0].targets,
            [Target::Object(a), Target::Object(b)]
        );
        assert_eq!(s.objects.get(a).zone, Zone::Hand);
        assert_eq!(s.objects.get(b).zone, Zone::Hand);
        assert_eq!(original.resolving_item.controller, p);
    }
}
#[test]
fn genuine_waiting_answered_copy_forgeries_and_erased_stages_are_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        bolt(&mut s, p);
        bolt(&mut s, p);
        paid_storm(&mut s, p);
        cast(
            &mut s,
            p,
            "Lightning Bolt",
            &[Target::Player(p.opponent())],
            1,
        );
        waiting(&mut s);
        for answered in [false, true] {
            let mut input = restore(&s);
            if answered {
                action(&mut input, Action::FinishEffectSelection)
            }
            for mutation in 0..8 {
                let mut bad = input.clone();
                let pending = bad.engine.pending_effect.as_mut().unwrap();
                match mutation {
                    0 => {
                        pending.choice = None;
                        pending.frames.clear();
                        pending.answered_choice_guard = None
                    }
                    1 => pending.ctx.controller = p.opponent(),
                    2 => {
                        pending.resolving_item.kind = crate::state::StackItemKind::Spell;
                        pending.ctx.stack_item_id = None;
                        pending.resolving_item.inline_effect = Some(EffectOp::DrawCards {
                            player: PlayerRef::Controller,
                            count: 1,
                        });
                        pending.choice = None;
                        pending.answered_choice_guard = None;
                        pending.frames = vec![EffectFrame::Program {
                            op: EffectOp::DrawCards {
                                player: PlayerRef::Controller,
                                count: 1,
                            },
                            path: vec![],
                        }]
                    }
                    3 => {
                        pending
                            .resolving_item
                            .v4
                            .ability_source_contract
                            .as_mut()
                            .unwrap()
                            .zone_change_count += 1
                    }
                    4 => pending.ctx.stack_item_id = Some(StackItemId(u64::MAX)),
                    5 => pending.frames.clear(),
                    6 => {
                        let event=bad.engine.event_history.iter_mut().find(|e|matches!(e,event::CommittedEvent::ThousandYearStormV1{journal:JournalV1::Cast{observers,..}}if !observers.is_empty())).unwrap();
                        if let event::CommittedEvent::ThousandYearStormV1 {
                            journal: JournalV1::Cast { spell, .. },
                        } = event
                        {
                            spell.controller = p.opponent()
                        }
                    }
                    7 => {
                        let event = bad
                            .engine
                            .event_history
                            .iter()
                            .find(|e| {
                                matches!(
                                    e,
                                    event::CommittedEvent::ThousandYearStormV1 {
                                        journal: JournalV1::Published { .. }
                                    }
                                )
                            })
                            .unwrap()
                            .clone();
                        bad.engine.event_history.push(event)
                    }
                    _ => unreachable!(),
                }
                let before = bad.clone();
                assert!(
                    validate_pending_effect_choice(&bad).is_err(),
                    "mutation {mutation}"
                );
                assert!(resume_resumable_resolution(&mut bad).is_err());
                assert_eq!(bad, before);
            }
            let mut replay = restore(&input);
            finish(&mut input);
            finish(&mut replay);
            assert_eq!(input, replay);
            assert_eq!(copies(&input).len(), 2);
        }
    }
}
#[test]
fn source_control_and_departure_reentry_primitives_keep_original_paid_cast_occurrence_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let mut s = ready(p, "Island");
            bolt(&mut s, p);
            let source = paid_storm(&mut s, p);
            cast(
                &mut s,
                p,
                "Lightning Bolt",
                &[Target::Player(p.opponent())],
                1,
            );
            let row = published(&s)[0].clone();
            // Explicit native control/zone primitives, no claimed enchantment-control printing.
            crate::standard_cards_v1::gain_control(&mut s, source, p.opponent());
            if reenter {
                event::propose_and_commit(
                    &mut s,
                    event::ProposedEvent::zone_change(source, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut s,
                    event::ProposedEvent::zone_change(source, Zone::Battlefield),
                );
            }
            validate_published(&s, &row).unwrap();
            let mut r = restore(&s);
            finish(&mut s);
            finish(&mut r);
            assert_eq!(s, r);
            assert_eq!(copies(&s).len(), 1);
            assert_eq!(copies(&s)[0].controller, p);
        }
    }
}
#[test]
fn genuine_paid_retarget_changes_one_copy_without_recasting_and_other_copy_keeps_original_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        bolt(&mut s, p);
        bolt(&mut s, p);
        paid_storm(&mut s, p);
        let original = cast(
            &mut s,
            p,
            "Lightning Bolt",
            &[Target::Player(p.opponent())],
            1,
        );
        waiting(&mut s);
        let casts = cast_count(&s);
        let d = next(&mut s);
        assert!(
            matches!(d,Decision::ChooseEffectTargets{ref legal_targets,..}if legal_targets.contains(&Target::Player(p)))
        );
        action(&mut s, Action::ChooseEffectTarget(Target::Player(p)));
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        let created = copies(&s);
        assert_eq!(created.len(), 2);
        assert_eq!(created[0].targets, [Target::Player(p)]);
        assert_eq!(created[1].targets, [Target::Player(p.opponent())]);
        assert_eq!(cast_count(&s), casts);
        assert_eq!(s.objects.get(original).zone, Zone::Graveyard);
    }
}
#[test]
fn public_begin_execute_and_hidden_world_guards_do_not_supply_native_storm_authority_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        bolt(&mut s, p);
        paid_storm(&mut s, p);
        cast(
            &mut s,
            p,
            "Lightning Bolt",
            &[Target::Player(p.opponent())],
            1,
        );
        let row = published(&s)[0].clone();
        let ctx = crystal_prevention_v1::context(&row);
        let op = row.inline_effect.clone().unwrap();
        for changed in [false, true] {
            let mut b = s.clone();
            let mut r = row.clone();
            let program = if changed {
                r.kind = crate::state::StackItemKind::Spell;
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                }
            } else {
                op.clone()
            };
            r.inline_effect = Some(program.clone());
            let before = b.clone();
            assert!(begin_resumable_resolution(&program, &ctx, r, &mut b).is_err());
            assert_eq!(b, before);
            execute(&program, &ctx, &mut b);
            assert_eq!(
                b.engine.halted.take(),
                Some((
                    engine::UnsupportedMechanic::InvalidEffectContinuation,
                    row.source
                ))
            );
            assert_eq!(b, before);
        }
        // Projection-only independent hidden content, never an invented paid event.
        let mut hidden = s.clone();
        let card = hidden.players[p.opponent().index()].library[0];
        hidden.objects.get_mut(card).card_def = id("Lightning Bolt");
        validate_published(&hidden, &row).unwrap();
        assert_eq!(published(&s), published(&hidden));
        assert!(thousand_year_storm_v1::refuse_legacy(&hidden).is_err());
    }
}

#[test]
fn real_turn_boundary_resets_own_count_and_countered_trigger_does_not_undo_paid_spell_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        bolt(&mut s, p);
        paid_storm(&mut s, p);
        let original = cast(
            &mut s,
            p,
            "Lightning Bolt",
            &[Target::Player(p.opponent())],
            1,
        );
        let row = published(&s)[0].clone();
        // Existing native counter primitive, not a claimed paid Stifle printing.
        let before = s.players[p.opponent().index()].life;
        assert!(
            engine::counter_stack_item_by_id(&mut s, row.v4.stack_item_id)
                .unwrap()
                .is_some()
        );
        finish(&mut s);
        assert_eq!(s.objects.get(original).zone, Zone::Graveyard);
        assert!(copies(&s).is_empty());
        assert_eq!(s.players[p.opponent().index()].life, before - 3);
        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut s, p);
        cast(
            &mut s,
            p,
            "Lightning Bolt",
            &[Target::Player(p.opponent())],
            1,
        );
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert!(copies(&s).is_empty());
    }
}

#[test]
fn paid_run_away_swap_cannot_keep_duplicate_and_historical_departure_uses_real_lki_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for departure in [false, true] {
            let mut s = ready(p, "Island");
            let extra = put(&mut s, p, "Ajani's Pridemate", Zone::Battlefield); // Initial native roster primitive.
            let a = cast(&mut s, p, "Llanowar Elves", &[], 1);
            finish(&mut s);
            let b = cast(&mut s, p.opponent(), "Llanowar Elves", &[], 1);
            finish(&mut s);
            bolt(&mut s, p);
            paid_storm(&mut s, p);
            let original = cast(
                &mut s,
                p,
                "Run Away Together",
                &[Target::Object(a), Target::Object(b)],
                2,
            );
            if departure {
                cast(
                    &mut s,
                    p.opponent(),
                    "Counterspell",
                    &[Target::Object(original)],
                    2,
                );
                for _ in 0..12 {
                    let d = next(&mut s);
                    if s.objects.get(original).zone == Zone::Graveyard {
                        break;
                    }
                    answer(&mut s, d)
                }
                assert_eq!(s.objects.get(original).zone, Zone::Graveyard);
                // Existing native control/departure primitives while authentic Storm remains published.
                crate::standard_cards_v1::gain_control(&mut s, a, p.opponent());
                event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(a, Zone::Hand));
                assert!(s.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::DifferentControllerTargetDepartureV1{target:crate::state::StackTargetContractV4::Object{object,..},controller_before}if *object==a&&*controller_before==p.opponent())));
                waiting(&mut s);
                let pending = s.engine.pending_effect.as_ref().unwrap();
                let EffectOp::Sequence(ops) =
                    pending.resolving_item.inline_effect.as_ref().unwrap()
                else {
                    panic!()
                };
                let EffectOp::CopySpellSnapshot { spell } = &ops[1] else {
                    panic!()
                };
                let spell = spell.clone();
                // Retain the exact departed first target; the new second target is
                // controlled by p, opposite the genuine departure LKI (p.opponent).
                let targets = [Target::Object(a), Target::Object(extra)];
                let contracts = [
                    spell.v4.target_contracts[0],
                    crate::state::StackTargetContractV4::capture(&s, targets[1]),
                ];
                assert!(engine::copy_snapshot_assignment_is_legal(
                    &s, &spell, p, &targets, &contracts
                )
                .unwrap());
                let mut r = restore(&s);
                finish(&mut s);
                finish(&mut r);
                assert_eq!(s, r);
            } else {
                waiting(&mut s);
                let d = next(&mut s);
                assert!(
                    matches!(d,Decision::ChooseEffectTargets{ref legal_targets,..}if legal_targets.contains(&Target::Object(b)))
                );
                action(&mut s, Action::ChooseEffectTarget(Target::Object(b)));
                let d = next(&mut s);
                assert!(
                    matches!(d,Decision::ChooseEffectTargets{can_finish:false,ref legal_targets,..}if legal_targets.contains(&Target::Object(a))&&!legal_targets.contains(&Target::Object(b)))
                );
                let before = s.clone();
                assert!(engine::step(&mut s, Action::FinishEffectSelection).is_err());
                assert_eq!(s, before);
                let mut r = restore(&s);
                for branch in [&mut s, &mut r] {
                    action(branch, Action::ChooseEffectTarget(Target::Object(a)));
                    finish(branch)
                }
                assert_eq!(s, r);
                assert_eq!(
                    copies(&s)[0].targets,
                    [Target::Object(b), Target::Object(a)]
                );
            }
        }
    }
}
#[test]
fn genuine_paid_kicked_divine_260_slot_copy_matching_keeps_width_and_incarnation_identity_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        // Explicit initial native creature roster. Only Divine/Storm/Bolt below are claimed paid casts.
        let creatures = (0..260)
            .map(|_| put(&mut s, p, "Ajani's Pridemate", Zone::Battlefield))
            .collect::<Vec<_>>();
        bolt(&mut s, p);
        paid_storm(&mut s, p);
        let targets = creatures
            .iter()
            .copied()
            .map(Target::Object)
            .collect::<Vec<_>>();
        cast_choices(&mut s, p, "Divine Resilience", &targets, 4, true, 0);
        // Native zone primitives in the genuine response window, before any
        // copy-choice is offered. CR400.7 gives the returned physical card a
        // new identity; CR115.7d still permits keeping its illegal old target.
        let mut reentered = s.clone();
        event::propose_and_commit(
            &mut reentered,
            event::ProposedEvent::zone_change(creatures[0], Zone::Hand),
        );
        event::propose_and_commit(
            &mut reentered,
            event::ProposedEvent::zone_change(creatures[0], Zone::Battlefield),
        );
        waiting(&mut reentered);
        let original = match reentered
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .resolving_item
            .inline_effect
            .as_ref()
            .unwrap()
        {
            EffectOp::Sequence(ops) => match &ops[1] {
                EffectOp::CopySpellSnapshot { spell } => spell.as_ref().clone(),
                _ => panic!("original Divine copy root"),
            },
            _ => panic!("original Storm root"),
        };
        let prefix = [targets[0], targets[0]];
        let contracts = [
            original.v4.target_contracts[0],
            crate::state::StackTargetContractV4::capture(&reentered, targets[0]),
        ];
        assert_ne!(contracts[0], contracts[1]);
        assert!(engine::copy_snapshot_prefix_can_complete(
            &reentered, &original, p, &prefix, &contracts
        )
        .unwrap());
        assert!(!engine::copy_snapshot_prefix_can_complete(
            &reentered,
            &original,
            p,
            &prefix,
            &[contracts[1], contracts[1]]
        )
        .unwrap());
        action(&mut reentered, Action::FinishEffectSelection); // Keep departed original A.
        let d = next(&mut reentered);
        assert!(
            matches!(d, Decision::ChooseEffectTargets{ref legal_targets,..}if legal_targets.contains(&targets[0]))
        );
        action(&mut reentered, Action::ChooseEffectTarget(targets[0])); // Choose returned new A.
        let mut replay = restore(&reentered);
        finish(&mut reentered);
        finish(&mut replay);
        assert_eq!(reentered, replay);
        let c = copies(&reentered);
        assert_eq!(c[0].targets[..2], prefix);
        assert_eq!(c[0].v4.target_contracts[..2], contracts);
        assert!(engine::has_effective_keyword(
            &reentered,
            creatures[0],
            Keywords::INDESTRUCTIBLE
        ));
        assert!(engine::validate_copy_snapshot(&reentered, &c[0]).is_ok());
        assert!(engine::copy_snapshot_prefix_can_complete(&reentered, &c[0], p, &[], &[]).unwrap());
        let generations = reentered
            .engine
            .until_end_of_turn
            .iter()
            .filter_map(|effect| match effect {
                engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                    object_id,
                    object_zone_change_count,
                    keywords,
                    ..
                } if *object_id == creatures[0] && keywords.contains(Keywords::INDESTRUCTIBLE) => {
                    Some(*object_zone_change_count)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            generations,
            [reentered.objects.get(creatures[0]).zone_change_count]
        );
        let mut duplicate = c[0].clone();
        duplicate.v4.target_contracts[0] = contracts[1];
        let before = reentered.clone();
        assert!(engine::validate_copy_snapshot(&reentered, &duplicate).is_err());
        assert_eq!(reentered, before);
        waiting(&mut s);
        let mut r = restore(&s);
        for branch in [&mut s, &mut r] {
            action(branch, Action::ChooseEffectTarget(targets[1]));
            let d = next(branch);
            assert!(
                matches!(d,Decision::ChooseEffectTargets{can_finish:false,ref legal_targets,..}if legal_targets.contains(&targets[0])&&!legal_targets.contains(&targets[1]))
            );
            action(branch, Action::ChooseEffectTarget(targets[0]));
            finish(branch);
        }
        assert_eq!(s, r);
        let c = copies(&s);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].targets.len(), 260);
        assert!(c[0].kicked);
        assert_eq!(c[0].targets[0], targets[1]);
        assert_eq!(c[0].targets[1], targets[0]);
        assert!(creatures.iter().all(|id| engine::has_effective_keyword(
            &s,
            *id,
            Keywords::INDESTRUCTIBLE
        )));
    }
}
#[test]
fn genuine_paid_x_copies_retain_original_amount_without_mana_or_new_casts_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        bolt(&mut s, p);
        paid_storm(&mut s, p);
        let before = s.players.each_ref().map(|p| p.life);
        let casts = cast_count(&s);
        cast_choices(&mut s, p, "Exsanguinate", &[], 5, false, 3);
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        let c = copies(&s);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].v4.x_value, 3);
        assert_eq!(cast_count(&s), casts + 1);
        assert_eq!(s.players[p.index()].life, before[p.index()] + 6);
        assert_eq!(
            s.players[p.opponent().index()].life,
            before[p.opponent().index()] - 6
        );
    }
}

#[test]
fn printed_observer_follows_current_controller_and_real_replication_copy_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        bolt(&mut s, p.opponent());
        let source = paid_storm(&mut s, p);
        // Native control primitive, not an invented enchantment-control spell.
        crate::standard_cards_v1::gain_control(&mut s, source, p.opponent());
        cast(
            &mut s,
            p.opponent(),
            "Lightning Bolt",
            &[Target::Player(p)],
            1,
        );
        let rows = published(&s);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].controller, p.opponent());
        assert_eq!(rows[0].v4.ability_source_contract.unwrap().owner, p);
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(copies(&s)[0].controller, p.opponent());

        let mut s = ready(p, "Island");
        let source = paid_storm(&mut s, p);
        cast(&mut s, p, "Extravagant Replication", &[], 6);
        finish(&mut s);
        let turn = s.turn_key_v1();
        for _ in 0..192 {
            let d = next(&mut s);
            if s.turn_key_v1() != turn
                && s.active_player == p
                && s.step == Step::Main1
                && s.stack.is_empty()
            {
                break;
            }
            if let Decision::ChooseTargets {
                ref legal_targets, ..
            } = d
            {
                assert!(legal_targets.contains(&Target::Object(source)));
                action(&mut s, Action::ChooseTarget(Target::Object(source)))
            } else {
                answer(&mut s, d)
            }
        }
        assert!(s.turn_key_v1() != turn && s.active_player == p && s.step == Step::Main1);
        let sources = s.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|id| s.objects.get(*id).card_def == 485)
            .collect::<Vec<_>>();
        assert_eq!(sources.len(), 2);
        assert!(sources.iter().any(|id| s.objects.get(*id).v4.is_token));
        bolt(&mut s, p);
        let before = copies(&s).len();
        cast(
            &mut s,
            p,
            "Lightning Bolt",
            &[Target::Player(p.opponent())],
            1,
        );
        let rows = published(&s);
        assert_eq!(rows.len(), 2);
        let mut r = restore(&s);
        finish(&mut s);
        finish(&mut r);
        assert_eq!(s, r);
        assert_eq!(copies(&s).len() - before, 2);
        assert_ne!(rows[0].source, rows[1].source);
    }
}
#[test]
fn paid_fiery_attachment_retarget_uses_feasible_final_assignment_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        let a = cast(&mut s, p, "Llanowar Elves", &[], 1);
        finish(&mut s);
        let b = cast(&mut s, p, "Llanowar Elves", &[], 1);
        finish(&mut s);
        let mut equipment = vec![];
        for host in [a, b] {
            let bow = cast(&mut s, p, "Viridian Longbow", &[], 1);
            finish(&mut s);
            priority(&mut s, p);
            let before = taps(&s, p);
            assert!(engine::available_activatable_abilities(p, &s).contains(&(bow, 0)));
            action(&mut s, Action::ActivateAbility(bow, 0));
            let d = next(&mut s);
            assert!(
                matches!(d,Decision::ChooseTargets{ref legal_targets,..}if legal_targets.contains(&Target::Object(host)))
            );
            action(&mut s, Action::ChooseTarget(Target::Object(host)));
            assert!(matches!(next(&mut s),Decision::CastSpellOrPass{player,..}if player==p));
            assert_eq!(taps(&s, p) - before, 3);
            finish(&mut s);
            assert_eq!(s.objects.get(bow).v4.attached_to.unwrap().object, host);
            equipment.push(bow);
        }
        bolt(&mut s, p);
        paid_storm(&mut s, p);
        cast(
            &mut s,
            p,
            "Fiery Annihilation",
            &[Target::Object(a), Target::Object(equipment[0])],
            3,
        );
        waiting(&mut s);
        let d = next(&mut s);
        assert!(
            matches!(d,Decision::ChooseEffectTargets{ref legal_targets,..}if legal_targets.contains(&Target::Object(b)))
        );
        action(&mut s, Action::ChooseEffectTarget(Target::Object(b)));
        let d = next(&mut s);
        assert!(
            matches!(d,Decision::ChooseEffectTargets{can_finish:false,ref legal_targets,..}if legal_targets.contains(&Target::Object(equipment[1]))&&!legal_targets.contains(&Target::Object(equipment[0])))
        );
        let mut r = restore(&s);
        for branch in [&mut s, &mut r] {
            action(
                branch,
                Action::ChooseEffectTarget(Target::Object(equipment[1])),
            );
            finish(branch)
        }
        assert_eq!(s, r);
        let copied = copies(&s);
        assert_eq!(copied.len(), 1);
        assert_eq!(
            copied[0].targets,
            [Target::Object(b), Target::Object(equipment[1])]
        );
        assert_eq!(s.objects.get(a).zone, Zone::Exile);
        assert_eq!(s.objects.get(b).zone, Zone::Exile);
        assert_eq!(s.objects.get(equipment[0]).zone, Zone::Exile);
        assert_eq!(s.objects.get(equipment[1]).zone, Zone::Exile);
    }
}
