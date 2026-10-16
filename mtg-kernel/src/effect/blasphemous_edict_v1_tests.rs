//! Genuine FDN57 native Partial fixtures. Board rosters are labelled native
//! setup; every Edict, response and observer described as paid is really cast.
use super::*;
use crate::{
    card_def::{self, card_id_by_name, CardCapability, Keywords},
    engine::{self, Action, CastMode, Decision},
    event::{self},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, StackItemKind, Step, Zone},
};
fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap()
}
fn ready(actor: PlayerId) -> GameState {
    let mut s = GameState::new_from_libraries_with_starting_player_v1(
        &[id("Swamp"); 40],
        &[id("Swamp"); 40],
        crate::rl::card_name,
        110_469,
        actor,
    );
    s.step = Step::Main1;
    for seat in [actor, actor.opponent()] {
        for name in ["Swamp", "Island"] {
            for _ in 0..10 {
                put(&mut s, seat, name, Zone::Battlefield);
            }
        }
    }
    s
}
fn roster(s: &mut GameState, player: PlayerId, count: usize) -> Vec<ObjectId> {
    // Registered native battlefield setup, not a claim of paying for this roster.
    (0..count)
        .map(|_| put(s, player, "Llanowar Elves", Zone::Battlefield))
        .collect()
}
fn taps(s: &GameState, player: PlayerId) -> usize {
    s.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&o| s.objects.get(o).tapped)
        .count()
}
fn restore(s: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap()
}
fn act_pair(s: &mut GameState, replay: &mut GameState, action: Action) {
    engine::step(s, action.clone()).unwrap();
    engine::step(replay, action).unwrap();
    assert_eq!(s, replay);
}
fn advance_pair(s: &mut GameState, replay: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(s);
    assert_eq!(d, engine::advance_until_decision(replay));
    assert_eq!(s, replay);
    assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
    d
}
fn cast(
    s: &mut GameState,
    player: PlayerId,
    name: &str,
    mode: CastMode,
    target: Option<Target>,
    cost: usize,
) -> ObjectId {
    assert!(
        matches!(engine::advance_until_decision(s), Decision::CastSpellOrPass { player: p, .. } if p == player)
    );
    let spell = put(s, player, name, Zone::Hand);
    let before = taps(s, player);
    engine::step(s, Action::CastSpell(spell)).unwrap();
    for _ in 0..12 {
        match engine::advance_until_decision(s) {
            Decision::ChooseCastMode {
                player: p,
                spell: card,
                options,
            } => {
                assert_eq!((p, card), (player, spell));
                assert!(options.contains(&mode));
                engine::step(s, Action::ChooseCastMode(mode)).unwrap();
            }
            Decision::ChooseTargets {
                player: p,
                spell: card,
                legal_targets,
                ..
            } => {
                assert_eq!((p, card), (player, spell));
                let t = target.unwrap();
                assert!(legal_targets.contains(&t));
                engine::step(s, Action::ChooseTarget(t)).unwrap();
            }
            Decision::CastSpellOrPass { player: p, .. } => {
                assert_eq!(p, player);
                assert!(s.engine.pending_cast.is_none());
                assert_eq!(taps(s, player) - before, cost);
                assert_eq!(s.objects.get(spell).zone, Zone::Stack);
                let row = s.stack.iter().find(|r| r.source == spell).unwrap();
                assert_eq!(row.kind, StackItemKind::Spell);
                assert_eq!(
                    row.v4.cast_method,
                    Some(if mode == CastMode::Alternative {
                        crate::state::CastMethodV4::Alternative
                    } else {
                        crate::state::CastMethodV4::Normal
                    })
                );
                assert_eq!(row.v4.source_contract.unwrap().controller, player);
                return spell;
            }
            other => panic!("genuine cast publication: {other:?}"),
        }
    }
    panic!("cast did not publish")
}
fn finish(s: &mut GameState) {
    for _ in 0..256 {
        let d = engine::advance_until_decision(s);
        assert!(s.engine.halted.is_none(), "{:?}", s.engine.halted);
        if s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            assert!(matches!(d, Decision::CastSpellOrPass { .. }));
            return;
        }
        let action = match d {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseEffectTargets {
                legal_targets,
                can_finish: false,
                ..
            } => Action::ChooseEffectTarget(*legal_targets.first().unwrap()),
            other => panic!("unexpected native resolution: {other:?}"),
        };
        engine::step(s, action).unwrap();
    }
    panic!("native resolution did not terminate")
}
fn to_choice(s: &mut GameState) -> PlayerId {
    for _ in 0..8 {
        match engine::advance_until_decision(s) {
            Decision::CastSpellOrPass { .. } => engine::step(s, Action::Pass).unwrap(),
            Decision::ChooseEffectTargets {
                player,
                min_targets,
                max_targets,
                selected_count,
                can_finish,
                ..
            } => {
                assert_eq!(
                    (min_targets, max_targets, selected_count, can_finish),
                    (13, 13, 0, false)
                );
                validate_pending_effect_choice(s).unwrap();
                return player;
            }
            d => panic!("expected actual Edict choice: {d:?}"),
        }
    }
    panic!("Edict response window did not finish")
}
fn choose_thirteen(s: &mut GameState, replay: &mut GameState, chooser: PlayerId, leave: ObjectId) {
    for _ in 0..13 {
        let Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } = advance_pair(s, replay)
        else {
            panic!("native sacrifice choice missing")
        };
        assert_eq!(player, chooser);
        let chosen = *legal_targets
            .iter()
            .find(|t| **t != Target::Object(leave))
            .unwrap();
        act_pair(s, replay, Action::ChooseEffectTarget(chosen));
    }
    assert!(s.engine.pending_effect.as_ref().unwrap().choice.is_none());
    validate_pending_effect_choice(s).unwrap();
}
#[test]
fn genuine_paid_normal_and_thirteen_creature_alternative_costs_zero_to_thirteen_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (count, mode, cost) in [
            (0, CastMode::Normal, 5),
            (12, CastMode::Normal, 5),
            (13, CastMode::Alternative, 1),
        ] {
            let mut s = ready(actor);
            let own = roster(&mut s, actor, count / 2);
            let other = roster(&mut s, actor.opponent(), count - count / 2);
            let spell = cast(&mut s, actor, "Blasphemous Edict", mode, None, cost);
            let start = s.engine.event_history.len();
            let mut replay = restore(&s);
            finish(&mut s);
            finish(&mut replay);
            assert_eq!(s, replay);
            assert_eq!(s.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(s.objects.get(spell).owner, actor);
            assert!(own
                .iter()
                .chain(&other)
                .all(|&o| s.objects.get(o).zone == Zone::Graveyard));
            assert_eq!(
                s.engine.event_history[start..]
                    .iter()
                    .filter(|e| matches!(e, event::CommittedEvent::Sacrificed { .. }))
                    .count(),
                count
            );
        }
    }
}
#[test]
fn genuine_paid_apnap_thirteen_each_original_candidates_and_answered_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let a = roster(&mut s, actor, 14);
        let b = roster(&mut s, actor.opponent(), 14);
        cast(
            &mut s,
            actor,
            "Blasphemous Edict",
            CastMode::Alternative,
            None,
            1,
        );
        assert_eq!(to_choice(&mut s), actor);
        let before = s.clone();
        let mut replay = restore(&s);
        choose_thirteen(&mut s, &mut replay, actor, a[13]);
        assert!(a
            .iter()
            .chain(&b)
            .all(|&o| s.objects.get(o).zone == Zone::Battlefield));
        let mut answered = restore(&s);
        let d = advance_pair(&mut s, &mut replay);
        assert_eq!(d, engine::advance_until_decision(&mut answered));
        assert_eq!(s, answered);
        assert!(matches!(d,Decision::ChooseEffectTargets{player,..} if player==actor.opponent()));
        choose_thirteen(&mut s, &mut replay, actor.opponent(), b[13]);
        let mut restored = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        finish(&mut restored);
        assert_eq!(s, replay);
        assert_eq!(s, restored);
        for &o in a[..13].iter().chain(&b[..13]) {
            assert_eq!(s.objects.get(o).zone, Zone::Graveyard);
            assert_eq!(
                s.objects.get(o).zone_change_count,
                before.objects.get(o).zone_change_count + 1
            );
        }
        for o in [a[13], b[13]] {
            assert_eq!(s.objects.get(o), before.objects.get(o));
        }
        let events = &s.engine.event_history[before.engine.event_history.len()..];
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, event::CommittedEvent::Sacrificed { .. }))
                .count(),
            26
        );
    }
}
#[test]
fn genuine_paid_waiting_answered_and_changed_root_context_forgeries_refuse_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let a = roster(&mut s, actor, 14);
        let b = roster(&mut s, actor.opponent(), 14);
        cast(
            &mut s,
            actor,
            "Blasphemous Edict",
            CastMode::Alternative,
            None,
            1,
        );
        to_choice(&mut s);
        let waiting = s.clone();
        let mut replay = restore(&s);
        choose_thirteen(&mut s, &mut replay, actor, a[13]);
        for original in [&waiting, &s] {
            for variant in 0..7 {
                let mut bad = original.clone();
                let p = bad.engine.pending_effect.as_mut().unwrap();
                match variant {
                    0 => p.ctx.controller = actor.opponent(),
                    1 => p.ctx.stack_item_id = None,
                    2 => {
                        p.choice = None;
                        p.frames.clear();
                        p.answered_choice_guard = None;
                    }
                    3 => {
                        p.choice = None;
                        p.resolving_item.kind = StackItemKind::ActivatedAbility;
                        p.resolving_item.inline_effect = Some(EffectOp::DrawCards {
                            player: PlayerRef::Controller,
                            count: 1,
                        });
                        p.ctx.stack_item_id = None;
                        p.frames = vec![EffectFrame::Program {
                            op: EffectOp::DrawCards {
                                player: PlayerRef::Controller,
                                count: 1,
                            },
                            path: vec![],
                        }];
                        p.answered_choice_guard = None;
                    }
                    4 => {
                        p.ctx.source = a[13];
                    }
                    5 => {
                        if let Some(PendingEffectChoice::SelectTargets { min_targets, .. }) =
                            &mut p.choice
                        {
                            *min_targets = 0;
                        } else {
                            p.answered_choice_guard = None;
                        }
                    }
                    _ => {
                        if let Some(PendingEffectChoice::SelectTargets { legal, .. }) =
                            &mut p.choice
                        {
                            legal[0]
                                .expected_object
                                .as_mut()
                                .unwrap()
                                .expected_zone_change_count += 1;
                        } else if let Some(EffectFrame::BlasphemousEdictV1 { selected, .. }) =
                            p.frames.last_mut()
                        {
                            selected[0].expected_zone_change_count += 1;
                        }
                    }
                }
                let unchanged = bad.clone();
                assert!(validate_pending_effect_choice(&bad).is_err());
                assert!(resume_resumable_resolution(&mut bad).is_err());
                assert_eq!(bad, unchanged);
            }
            let p = original.engine.pending_effect.as_ref().unwrap();
            let mut bad = original.clone();
            assert!(begin_resumable_resolution(
                &EffectOp::BlasphemousEdictV1,
                &p.ctx,
                p.resolving_item.clone(),
                &mut bad
            )
            .is_err());
            assert_eq!(&bad, original);
            let mut bad = original.clone();
            execute(
                &EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                &p.ctx,
                &mut bad,
            );
            assert!(bad.engine.halted.is_some());
            bad.engine.halted = None;
            assert_eq!(&bad, original);
        }
        // Genuine current answers still work after all readonly negative probes.
        let d = advance_pair(&mut s, &mut replay);
        assert!(matches!(d,Decision::ChooseEffectTargets{player,..} if player==actor.opponent()));
        choose_thirteen(&mut s, &mut replay, actor.opponent(), b[13]);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
    }
}
#[test]
fn genuine_paid_countered_edict_keeps_both_rosters_and_real_cast_payments_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let a = roster(&mut s, actor, 7);
        let b = roster(&mut s, actor.opponent(), 6);
        let source = cast(
            &mut s,
            actor,
            "Blasphemous Edict",
            CastMode::Alternative,
            None,
            1,
        );
        let row = s.stack.last().unwrap().v4.stack_item_id;
        engine::step(&mut s, Action::Pass).unwrap();
        cast(
            &mut s,
            actor.opponent(),
            "Counterspell",
            CastMode::Normal,
            Some(Target::StackItem(row)),
            2,
        );
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
        assert!(a
            .iter()
            .chain(&b)
            .all(|&o| s.objects.get(o).zone == Zone::Battlefield));
        assert!(!s
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, event::CommittedEvent::Sacrificed { .. })));
        assert_eq!((taps(&s, actor), taps(&s, actor.opponent())), (1, 2));
    }
}
#[test]
fn genuine_paid_response_removes_thirteenth_creature_without_refunding_alternative_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let a = roster(&mut s, actor, 7);
        let b = roster(&mut s, actor.opponent(), 6);
        let source = cast(
            &mut s,
            actor,
            "Blasphemous Edict",
            CastMode::Alternative,
            None,
            1,
        );
        engine::step(&mut s, Action::Pass).unwrap();
        cast(
            &mut s,
            actor.opponent(),
            "Murder",
            CastMode::Normal,
            Some(Target::Object(a[0])),
            3,
        );
        let mut replay = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(source).zone, Zone::Graveyard);
        assert!(a
            .iter()
            .chain(&b)
            .all(|&o| s.objects.get(o).zone == Zone::Graveyard));
        assert_eq!(
            s.engine
                .event_history
                .iter()
                .filter(|e| matches!(e, event::CommittedEvent::Sacrificed { .. }))
                .count(),
            12
        );
        assert_eq!(taps(&s, actor), 1);
    }
}
#[test]
fn exact_partial_printing_keeps_full_deck_preflight_refused() {
    let card = id("Blasphemous Edict");
    assert_eq!(card, 469);
    let d = &card_def::CARD_DEFS[usize::from(card)];
    assert_eq!(d.capability, CardCapability::Partial);
    assert_eq!(d.mana_value, 5);
    assert_eq!(d.keywords, Keywords::NONE);
    assert_eq!(d.target_spec, TargetSpec::None);
    assert_eq!((d.spell_effect)(), Some(EffectOp::BlasphemousEdictV1));
    assert!(matches!(
        d.alt_cost,
        Some(card_def::AltCostDef {
            condition: card_def::AltCostCondition::BattlefieldCreatureCountAtLeast(13),
            ..
        })
    ));
    assert!(card_def::preflight_fully_supported_deck(&[card]).is_err());
}

#[test]
fn genuine_paid_edict_orders_stolen_creatures_by_physical_owner_after_all_choices_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let own = roster(&mut s, actor, 12);
        let other = roster(&mut s, actor.opponent(), 2);
        // Genuine native control primitive, not a claim of paying a control spell.
        crate::standard_cards_v1::gain_control(&mut s, own[0], actor.opponent());
        assert_eq!(
            (
                s.objects.get(own[0]).owner,
                s.objects.get(own[0]).controller
            ),
            (actor, actor.opponent())
        );
        cast(
            &mut s,
            actor,
            "Blasphemous Edict",
            CastMode::Alternative,
            None,
            1,
        );
        let mut saw_owner = false;
        for _ in 0..8 {
            match engine::advance_until_decision(&mut s) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut s, Action::Pass).unwrap(),
                Decision::ChooseEffectTargets {
                    player,
                    min_targets,
                    max_targets,
                    ..
                } => {
                    assert_eq!((player, min_targets, max_targets), (actor, 12, 12));
                    saw_owner = true;
                    break;
                }
                d => panic!("actual owner graveyard order absent: {d:?}"),
            }
        }
        assert!(saw_owner);
        let mut replay = restore(&s);
        let mut ordered = Vec::new();
        while s.engine.pending_effect.as_ref().unwrap().choice.is_some() {
            let Decision::ChooseEffectTargets {
                player,
                legal_targets,
                ..
            } = advance_pair(&mut s, &mut replay)
            else {
                panic!("actual owner order missing")
            };
            assert_eq!(player, actor);
            let target = *legal_targets.last().unwrap();
            let Target::Object(object) = target else {
                panic!("owner order requires physical card")
            };
            ordered.push(object);
            act_pair(&mut s, &mut replay, Action::ChooseEffectTarget(target));
        }
        // Ordered choices auto-fill only a forced final card.
        let frame = s
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .frames
            .last()
            .unwrap();
        let EffectFrame::BlasphemousEdictV1 { selected, .. } = frame else {
            panic!("exact owner answer frame absent")
        };
        let expected = selected.iter().map(|b| b.object).collect::<Vec<_>>();
        assert!(expected.contains(&own[0]));
        assert!(expected.starts_with(&ordered));
        let mut answered = restore(&s);
        finish(&mut s);
        finish(&mut replay);
        finish(&mut answered);
        assert_eq!(s, replay);
        assert_eq!(s, answered);
        let actual = s.players[actor.index()]
            .graveyard
            .iter()
            .copied()
            .filter(|o| own.contains(o))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert!(other
            .iter()
            .all(|&o| s.players[actor.opponent().index()].graveyard.contains(&o)));
    }
}
#[test]
fn genuine_paid_dying_bloodwitch_observes_the_whole_simultaneous_sacrifice_group_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(actor);
        let observer = cast(
            &mut s,
            actor,
            "Vengeful Bloodwitch",
            CastMode::Normal,
            None,
            2,
        );
        finish(&mut s);
        let own = roster(&mut s, actor, 13);
        let other = roster(&mut s, actor.opponent(), 14);
        cast(
            &mut s,
            actor,
            "Blasphemous Edict",
            CastMode::Alternative,
            None,
            1,
        );
        assert_eq!(to_choice(&mut s), actor);
        let mut replay = restore(&s);
        choose_thirteen(&mut s, &mut replay, actor, own[12]);
        assert!(
            matches!(advance_pair(&mut s,&mut replay),Decision::ChooseEffectTargets{player,..}if player==actor.opponent())
        );
        choose_thirteen(&mut s, &mut replay, actor.opponent(), other[13]);
        finish(&mut s);
        finish(&mut replay);
        assert_eq!(s, replay);
        assert_eq!(s.objects.get(observer).zone, Zone::Graveyard);
        assert_eq!(s.players[actor.opponent().index()].life, 7);
        assert_eq!(s.objects.get(own[12]).zone, Zone::Battlefield);
        assert_eq!(s.objects.get(other[13]).zone, Zone::Battlefield);
        assert_eq!(taps(&s, actor), 3);
    }
}
