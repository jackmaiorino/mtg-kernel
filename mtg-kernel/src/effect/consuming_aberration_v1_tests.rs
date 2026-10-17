//! Genuine native FDN238 Partial fixtures. No owning/model admission.
use super::*;
use crate::{
    card_def::{self, CardCapability},
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, Step, Zone},
};
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(actor: PlayerId, library: &[&str]) -> GameState {
    let own = vec![id("Island"); 40];
    let other = library.iter().map(|name| id(name)).collect::<Vec<_>>();
    let decks = if actor == PlayerId::P0 {
        [&own[..], &other[..]]
    } else {
        [&other[..], &own[..]]
    };
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        decks[0],
        decks[1],
        crate::rl::card_name,
        110_467,
        actor,
    );
    state.step = Step::Main1;
    for seat in [actor, actor.opponent()] {
        for name in ["Island", "Swamp", "Forest", "Mountain"] {
            for _ in 0..12 {
                put(&mut state, seat, name, Zone::Battlefield);
            }
        }
    }
    // Original public setup gives the 0/0 printed creature a genuine CDA base.
    put(
        &mut state,
        actor.opponent(),
        "Lightning Bolt",
        Zone::Graveyard,
    );
    state
}
fn taps(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| state.objects.get(id).tapped)
        .count()
}
fn respond(state: &mut GameState, decision: Decision, reverse: bool) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: false,
            ..
        } => Action::ChooseEffectTarget(
            *if reverse {
                legal_targets.last()
            } else {
                legal_targets.first()
            }
            .unwrap(),
        ),
        other => panic!("Aberration genuine native decision: {other:?}"),
    };
    engine::step(state, action).unwrap();
    assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
}
fn finish(state: &mut GameState, reverse: bool) {
    for _ in 0..256 {
        let decision = engine::advance_until_decision(state);
        assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && state.engine.pending_effect.is_none()
        {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        respond(state, decision, reverse);
    }
    panic!("finite Aberration native resolution did not finish");
}
fn cast(
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    target: Option<Target>,
    cost: usize,
) -> ObjectId {
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    let source = put(state, actor, name, Zone::Hand);
    let before = taps(state, actor);
    engine::step(state, Action::CastSpell(source)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(engine::advance_until_decision(state), Decision::ChooseTargets { ref legal_targets, .. } if legal_targets.contains(&target))
        );
        engine::step(state, Action::ChooseTarget(target)).unwrap();
    }
    loop {
        match engine::advance_until_decision(state) {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::CastSpellOrPass { player, .. } => {
                assert_eq!(player, actor);
                break;
            }
            other => panic!("paid cast publication decision: {other:?}"),
        }
    }
    assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(taps(state, actor) - before, cost);
    let row = state
        .stack
        .iter()
        .find(|row| row.source == source && row.kind == crate::state::StackItemKind::Spell)
        .unwrap();
    assert_eq!(row.controller, actor);
    assert_eq!(state.objects.get(source).zone, Zone::Stack);
    source
}
fn paid_aberration(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = cast(state, actor, "Consuming Aberration", None, 5);
    assert_eq!(
        state.stack.len(),
        1,
        "the source's own cast grants no battlefield trigger"
    );
    assert_eq!(engine::checked_effective_pt(state, source).unwrap(), (1, 1));
    finish(state, false);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert_eq!(engine::checked_effective_pt(state, source).unwrap(), (1, 1));
    source
}
fn restore(state: &GameState) -> GameState {
    let restored = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(&restored, state);
    restored
}
fn to_order(state: &mut GameState) {
    for _ in 0..32 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::ChooseEffectTargets { .. }) {
            validate_pending_effect_choice(state).unwrap();
            assert!(matches!(
                state.engine.pending_effect.as_ref().unwrap().choice,
                Some(PendingEffectChoice::SelectTargets {
                    purpose: EffectTargetSelectionPurpose::OrderRevealedIntoGraveyard { .. },
                    ..
                })
            ));
            return;
        }
        respond(state, decision, false);
    }
    panic!("original public revealed-prefix ordering missing");
}
fn trigger_rows(state: &GameState) -> Vec<&crate::state::StackItem> {
    state
        .stack
        .iter()
        .filter(|row| {
            row.v4
                .ability_source_contract
                .is_some_and(|s| s.card_def == id("Consuming Aberration"))
        })
        .collect()
}

#[test]
fn genuine_paid_printing_each_opponent_first_land_empty_and_no_land_both_seats() {
    let definition = &card_def::CARD_DEFS[usize::from(id("Consuming Aberration"))];
    assert_eq!(id("Consuming Aberration"), 467);
    assert_eq!(definition.capability, CardCapability::Partial);
    assert_eq!(definition.mana_value, 5);
    assert_eq!((definition.power, definition.toughness), (Some(0), Some(0)));
    assert_eq!(definition.subtypes, &[card_def::Subtype::Horror]);
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (library, milled) in [
            (vec![], 0),
            (vec!["Forest", "Think Twice"], 1),
            (vec!["Think Twice", "Elvish Mystic", "Forest", "Swamp"], 3),
            (vec!["Think Twice", "Ravenous Amulet"], 2),
        ] {
            let mut state = ready(actor, &library);
            let source = paid_aberration(&mut state, actor);
            // The V53 appendix is not a whole-world legacy consumer gate.
            assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
            cast(&mut state, actor, "Think Twice", None, 2);
            let rows = trigger_rows(&state);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].controller, actor);
            assert!(rows[0].targets.is_empty());
            assert_eq!(
                rows[0].inline_effect,
                Some(EffectOp::RevealUntilCardTypeAndMill {
                    player: PlayerRef::Opponent,
                    card_type: CardType::Land
                })
            );
            let mut replay = restore(&state);
            finish(&mut state, true);
            finish(&mut replay, true);
            assert_eq!(state, replay);
            assert_eq!(
                state.players[actor.opponent().index()].graveyard.len(),
                1 + milled
            );
            assert_eq!(
                state.players[actor.opponent().index()].library.len(),
                library.len() - milled
            );
            assert_eq!(
                engine::checked_effective_pt(&state, source).unwrap(),
                ((1 + milled) as i32, (1 + milled) as i32)
            );
        }
    }
}

#[test]
fn paid_original_prefix_order_restore_and_readonly_context_frame_forgeries_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Think Twice", "Elvish Mystic", "Forest", "Swamp"]);
        paid_aberration(&mut state, actor);
        let original = state.players[actor.opponent().index()].library.clone();
        cast(&mut state, actor, "Think Twice", None, 2);
        to_order(&mut state);
        let saved = restore(&state);
        let prefix = original[..3].to_vec();
        for n in 0..6 {
            let mut bad = saved.clone();
            let p = bad.engine.pending_effect.as_mut().unwrap();
            match n {
                0 => p.ctx.controller = actor.opponent(),
                1 => {
                    p.ctx
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                2 => p.frames.push(EffectFrame::Program {
                    op: EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    },
                    path: vec![],
                }),
                3 => p.choice = None,
                4 => {
                    if let Some(PendingEffectChoice::SelectTargets {
                        purpose:
                            EffectTargetSelectionPurpose::OrderRevealedIntoGraveyard {
                                original_prefix,
                                ..
                            },
                        ..
                    }) = p.choice.as_mut()
                    {
                        original_prefix.swap(0, 1);
                    }
                }
                5 => {
                    if let Some(PendingEffectChoice::SelectTargets { path, .. }) = p.choice.as_mut()
                    {
                        path.push(0);
                    }
                }
                _ => unreachable!(),
            }
            let before = bad.clone();
            assert!(
                validate_pending_effect_choice(&bad).is_err(),
                "corruption {n}"
            );
            assert_eq!(bad, before);
        }
        // Both public viewers know the revealed prefix, never its unrevealed suffix.
        for viewer in [actor, actor.opponent()] {
            for &object in &prefix {
                assert!(state
                    .known_library_cards(viewer, actor.opponent())
                    .iter()
                    .any(|entry| entry.object == object));
            }
        }
        for viewer in [actor, actor.opponent()] {
            assert!(!state
                .known_library_cards(viewer, actor.opponent())
                .iter()
                .any(|entry| entry.object == original[3]));
        }
        while state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .is_some()
        {
            let d = engine::advance_until_decision(&mut state);
            respond(&mut state, d, true);
        }
        validate_pending_effect_choice(&state).unwrap();
        let answered = restore(&state);
        for kind in 0..3 {
            let mut bad = answered.clone();
            let p = bad.engine.pending_effect.as_mut().unwrap();
            match kind {
                0 => p.answered_choice_guard = None,
                1 => p.frames.clear(),
                2 => {
                    if let EffectFrame::RevealedLibraryToGraveyardBatch { objects, .. } =
                        p.frames.last_mut().unwrap()
                    {
                        objects.swap(0, 1);
                    }
                }
                _ => unreachable!(),
            }
            let before = bad.clone();
            assert!(validate_pending_effect_choice(&bad).is_err());
            assert_eq!(bad, before);
        }
        let mut replay = restore(&state);
        finish(&mut state, false);
        finish(&mut replay, false);
        assert_eq!(state, replay);
        assert_eq!(
            &state.players[actor.opponent().index()].graveyard[1..],
            &[prefix[2], prefix[1], prefix[0]]
        );
    }
}

#[test]
fn paid_trigger_keeps_original_controller_after_real_bolt_source_death_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Think Twice", "Forest", "Swamp"]);
        let source = paid_aberration(&mut state, actor);
        let generation = state.objects.get(source).zone_change_count;
        cast(&mut state, actor, "Think Twice", None, 2);
        assert_eq!(trigger_rows(&state).len(), 1);
        engine::step(&mut state, Action::Pass).unwrap();
        cast(
            &mut state,
            actor.opponent(),
            "Lightning Bolt",
            Some(Target::Object(source)),
            1,
        );
        let mut replay = restore(&state);
        finish(&mut state, false);
        finish(&mut replay, false);
        assert_eq!(state, replay);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(source).zone_change_count, generation + 1);
        assert_eq!(state.players[actor.opponent().index()].library.len(), 1);
        assert_eq!(state.players[actor.opponent().index()].graveyard.len(), 4); // original Bolt, paid Bolt, revealed two.
        assert_eq!(state.objects.get(source).owner, actor);
    }
}

#[test]
fn all_zone_cda_owner_control_departure_and_all_card_kinds_native_primitives_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        // A published original trigger survives a native same-generation control change.
        // This is a control primitive, not a claim that a control spell was paid.
        let mut captured = ready(actor, &["Think Twice", "Forest", "Swamp"]);
        put(&mut captured, actor, "Forest", Zone::Graveyard);
        let controlled = paid_aberration(&mut captured, actor);
        cast(&mut captured, actor, "Think Twice", None, 2);
        assert_eq!(trigger_rows(&captured)[0].controller, actor);
        crate::standard_cards_v1::gain_control(&mut captured, controlled, actor.opponent());
        assert_eq!(
            engine::checked_effective_pt(&captured, controlled).unwrap(),
            (1, 1)
        );
        let mut replay = restore(&captured);
        finish(&mut captured, false);
        finish(&mut replay, false);
        assert_eq!(captured, replay);
        assert_eq!(captured.players[actor.opponent().index()].library.len(), 1);
        assert_eq!(
            engine::checked_effective_pt(&captured, controlled).unwrap(),
            (2, 2)
        );

        let mut state = ready(actor, &["Forest"; 12]);
        let source = paid_aberration(&mut state, actor);
        put(&mut state, actor.opponent(), "Forest", Zone::Graveyard);
        put(
            &mut state,
            actor.opponent(),
            "Ravenous Amulet",
            Zone::Graveyard,
        );
        // Token identity is not a card even in the transient native pre-SBA scene.
        let token = put(
            &mut state,
            actor.opponent(),
            "Goblin Token",
            Zone::Graveyard,
        );
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (3, 3)
        );
        put(&mut state, actor, "Forest", Zone::Graveyard);
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (1, 1)
        );
        let previous = state.objects.get(source).zone_change_count;
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        assert_eq!(state.objects.get(source).owner, actor);
        assert_eq!(state.objects.get(source).zone_change_count, previous + 1);
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (3, 3)
        );
        for zone in [Zone::Hand, Zone::Exile, Zone::Library] {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, zone));
            assert_eq!(
                engine::checked_effective_pt(&state, source).unwrap(),
                (3, 3)
            );
        }
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(token, Zone::Exile));
        let replay = restore(&state);
        assert_eq!(
            engine::checked_effective_pt(&replay, source).unwrap(),
            (3, 3)
        );
    }
}

#[test]
fn genuine_paid_copy_and_witness_suppression_preserve_cda_and_cast_occurrences_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest"; 24]);
        // Original declared mana setup covers the complete same-turn paid
        // chain. Its first three spells can spend all twelve original Islands.
        for _ in 0..4 {
            put(&mut state, actor, "Island", Zone::Battlefield);
        }
        let source = paid_aberration(&mut state, actor);
        cast(
            &mut state,
            actor,
            "Self-Reflection",
            Some(Target::Object(source)),
            6,
        );
        assert_eq!(
            trigger_rows(&state).len(),
            1,
            "copy instruction has not resolved and is not an extra cast"
        );
        finish(&mut state, false);
        let copies = state.players[actor.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|&o| o != source && state.objects.get(o).card_def == id("Consuming Aberration"))
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 1);
        let copy = copies[0];
        assert!(state.objects.get(copy).v4.is_token);
        assert_eq!(engine::checked_effective_pt(&state, copy).unwrap(), (2, 2));
        cast(&mut state, actor, "Think Twice", None, 2);
        assert_eq!(
            trigger_rows(&state).len(),
            2,
            "two original battlefield abilities, no copied-spell cast"
        );
        finish(&mut state, false);
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (4, 4)
        );
        assert_eq!(engine::checked_effective_pt(&state, copy).unwrap(), (4, 4));
        let aura = cast(
            &mut state,
            actor,
            "Witness Protection",
            Some(Target::Object(source)),
            1,
        );
        finish(&mut state, false);
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (1, 1)
        );
        assert!(!crate::continuous_characteristics_v1::printed_abilities_active(&state, source));
        cast(
            &mut state,
            actor,
            "Giant Growth",
            Some(Target::Object(source)),
            1,
        );
        assert_eq!(
            trigger_rows(&state).len(),
            1,
            "suppressed original grants no cast trigger"
        );
        let mut replay = restore(&state);
        finish(&mut state, false);
        finish(&mut replay, false);
        assert_eq!(state, replay);
        assert_eq!(engine::effective_base_power(&state, source), Some(1));
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (4, 4)
        );
        engine::step(&mut state, Action::Pass).unwrap();
        cast(
            &mut state,
            actor.opponent(),
            "Broken Wings",
            Some(Target::Object(aura)),
            3,
        );
        let mut replay = restore(&state);
        finish(&mut state, false);
        finish(&mut replay, false);
        assert_eq!(state, replay);
        assert_eq!(state.objects.get(aura).zone, Zone::Graveyard);
        assert!(crate::continuous_characteristics_v1::printed_abilities_active(&state, source));
        let base = state.players[actor.opponent().index()].graveyard.len() as i32;
        assert_eq!(engine::effective_base_power(&state, source), Some(base));
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (base + 3, base + 3)
        );
    }
}

#[test]
fn checked_full_native_cardinality_and_modifier_overflow_refuse_readonly() {
    assert_eq!(engine::consuming_aberration_card_count_v1(0).unwrap(), 0);
    assert_eq!(
        engine::consuming_aberration_card_count_v1(40_000).unwrap(),
        40_000
    );
    assert_eq!(
        engine::consuming_aberration_card_count_v1(i32::MAX as usize).unwrap(),
        i32::MAX
    );
    assert!(engine::consuming_aberration_card_count_v1(i32::MAX as usize + 1).is_err());
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest"; 4]);
        let source = paid_aberration(&mut state, actor);
        // Numeric native counter boundary, not a claim that this counter amount was paid.
        state.objects.get_mut(source).counters.plus1_plus1 = i32::MAX - 1;
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (i32::MAX, i32::MAX)
        );
        let original = state.clone();
        assert!(engine::checked_effective_pt_with_boost(&state, source, (3, 3)).is_err());
        assert_eq!(state, original);
    }
}

#[test]
fn paid_zero_graveyard_dies_and_paid_counter_keeps_costs_without_self_trigger_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest"; 8]);
        let original = state.players[actor.opponent().index()].graveyard[0];
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(original, Zone::Exile),
        );
        let source = cast(&mut state, actor, "Consuming Aberration", None, 5);
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (0, 0)
        );
        finish(&mut state, false);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert!(trigger_rows(&state).is_empty());

        let mut state = ready(actor, &["Forest"; 8]);
        let source = cast(&mut state, actor, "Consuming Aberration", None, 5);
        engine::step(&mut state, Action::Pass).unwrap();
        cast(
            &mut state,
            actor.opponent(),
            "Counterspell",
            Some(Target::Object(source)),
            2,
        );
        let mut replay = restore(&state);
        finish(&mut state, false);
        finish(&mut replay, false);
        assert_eq!(state, replay);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(taps(&state, actor), 5);
        assert_eq!(taps(&state, actor.opponent()), 2);
        assert!(trigger_rows(&state).is_empty());
    }
}

#[test]
fn genuine_paid_original_mill_public_api_and_erased_resume_refuse_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Think Twice", "Elvish Mystic", "Forest", "Swamp"]);
        paid_aberration(&mut state, actor);
        cast(&mut state, actor, "Think Twice", None, 2);
        let item = trigger_rows(&state)[0].clone();
        let ctx = crystal_prevention_v1::context(&item);
        let root = item.inline_effect.clone().unwrap();
        for op in [
            root.clone(),
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ] {
            let mut bad = restore(&state);
            let original = bad.clone();
            assert!(begin_resumable_resolution(&op, &ctx, item.clone(), &mut bad).is_err());
            assert_eq!(bad, original);
        }
        let foreign = EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        };
        // Naming a Ward operation is insufficient: this actual paid source
        // has no Ward grant and the actual Think Twice does not target it.
        let forged_ward = EffectOp::CounterUnlessPaysGeneric {
            ward_target: StackTargetContractV4::capture(&state, Target::Object(item.source)),
            targeting_stack_item: state
                .stack
                .iter()
                .find(|row| row.kind == crate::state::StackItemKind::Spell)
                .unwrap()
                .v4
                .stack_item_id,
            generic: 1,
        };
        for substituted in [foreign.clone(), forged_ward.clone()] {
            let mut changed_item = item.clone();
            changed_item.inline_effect = Some(substituted.clone());
            let mut bad = restore(&state);
            let original = bad.clone();
            assert!(
                begin_resumable_resolution(&substituted, &ctx, changed_item, &mut bad).is_err()
            );
            assert_eq!(bad, original);
        }
        to_order(&mut state);
        for substituted in [foreign, forged_ward] {
            // The operation, sole frame and erased choice coherently agree,
            // but the captured printed family still selects the refusal.
            let mut changed = restore(&state);
            let p = changed.engine.pending_effect.as_mut().unwrap();
            p.resolving_item.inline_effect = Some(substituted.clone());
            p.choice = None;
            p.frames = vec![EffectFrame::Program {
                op: substituted.clone(),
                path: vec![],
            }];
            p.answered_choice_guard = None;
            let original = changed.clone();
            assert!(resume_resumable_resolution(&mut changed).is_err());
            assert_eq!(changed, original);
            // Synchronous execution with the actual captured context cannot
            // replace the native mill by a foreign operation either.
            execute(&substituted, &ctx, &mut changed);
            assert!(changed.engine.halted.is_some());
            changed.engine.halted = None;
            assert_eq!(changed, original);
        }
        // Keep the real public stack row and original source contract:
        // editable pending metadata and context IDs cannot hide its printed family.
        for changed_kind in [false, true] {
            for erase_context_id in [false, true] {
                let mut changed = restore(&state);
                let p = changed.engine.pending_effect.as_mut().unwrap();
                p.resolving_item.inline_effect = Some(EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                });
                if changed_kind {
                    p.resolving_item.kind = crate::state::StackItemKind::Spell;
                } else {
                    p.resolving_item.v4.granted_by = p.ctx.ability_source_contract;
                }
                p.choice = None;
                p.frames = vec![EffectFrame::Program {
                    op: p.resolving_item.inline_effect.clone().unwrap(),
                    path: vec![],
                }];
                p.answered_choice_guard = None;
                if erase_context_id {
                    p.ctx.stack_item_id = None;
                }
                let ctx = p.ctx.clone();
                let item = p.resolving_item.clone();
                let foreign = item.inline_effect.clone().unwrap();
                let before = changed.clone();
                assert!(resume_resumable_resolution(&mut changed).is_err());
                assert_eq!(changed, before);
                assert!(acquire_tutelage_resume_v1(&changed, &item).is_err());
                assert_eq!(changed, before);
                execute(&foreign, &ctx, &mut changed);
                assert_eq!(
                    changed.engine.halted,
                    Some((
                        engine::UnsupportedMechanic::InvalidEffectContinuation,
                        ctx.source
                    ))
                );
                changed.engine.halted = None;
                assert_eq!(changed, before);
                changed.engine.pending_effect = None;
                let before_begin = changed.clone();
                assert!(
                    begin_resumable_resolution(&foreign, &ctx, item.clone(), &mut changed).is_err()
                );
                assert_eq!(changed, before_begin);
                assert!(acquire_tutelage_begin_v1(&changed, &item).is_err());
                assert_eq!(changed, before_begin);
            }
        }
        for erase in [false, true] {
            let mut bad = restore(&state);
            if erase {
                let p = bad.engine.pending_effect.as_mut().unwrap();
                p.choice = None;
                p.frames.clear();
                p.answered_choice_guard = None;
            }
            let original = bad.clone();
            assert!(resume_resumable_resolution(&mut bad).is_err());
            assert_eq!(bad, original);
        }
        let mut bad = restore(&state);
        let original = bad.clone();
        execute(&root, &ctx, &mut bad);
        assert_eq!(
            bad.engine.halted,
            Some((
                engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source
            ))
        );
        bad.engine.halted = None;
        assert_eq!(bad, original);
        // An unknown Some id, changed kind/root and erased source contract
        // cannot hide the actual still-published source in either checkpoint.
        let waiting = restore(&state);
        let mut answered = restore(&state);
        for _ in 0..8 {
            if answered
                .engine
                .pending_effect
                .as_ref()
                .unwrap()
                .choice
                .is_none()
            {
                break;
            }
            let decision = engine::advance_until_decision(&mut answered);
            respond(&mut answered, decision, false);
        }
        assert!(answered
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .is_none());
        for checkpoint in [waiting, answered] {
            for erase_contract in [false, true] {
                let mut forged = restore(&checkpoint);
                let pending = forged.engine.pending_effect.as_mut().unwrap();
                pending.ctx.stack_item_id = Some(crate::ids::StackItemId(u64::MAX));
                pending.resolving_item.v4.stack_item_id = pending.ctx.stack_item_id.unwrap();
                pending.resolving_item.kind = crate::state::StackItemKind::Spell;
                let draw = EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                };
                pending.resolving_item.inline_effect = Some(draw.clone());
                if erase_contract {
                    pending.ctx.ability_source_contract = None;
                    pending.resolving_item.v4.ability_source_contract = None;
                }
                pending.choice = None;
                pending.frames = vec![EffectFrame::Program {
                    op: draw.clone(),
                    path: vec![],
                }];
                pending.answered_choice_guard = None;
                let ctx = pending.ctx.clone();
                let item = pending.resolving_item.clone();
                let before = forged.clone();
                assert!(validate_pending_effect_choice(&forged).is_err());
                assert!(resume_resumable_resolution(&mut forged).is_err());
                assert_eq!(forged, before);
                execute(&draw, &ctx, &mut forged);
                assert_eq!(
                    forged.engine.halted,
                    Some((
                        engine::UnsupportedMechanic::InvalidEffectContinuation,
                        ctx.source
                    ))
                );
                forged.engine.halted = None;
                assert_eq!(forged, before);
                forged.engine.pending_effect = None;
                let before = forged.clone();
                assert!(
                    begin_resumable_resolution(&draw, &ctx, item.clone(), &mut forged).is_err()
                );
                assert_eq!(forged, before);
                assert!(acquire_native_execution_owner_v1(&forged, &item).is_err());
                assert_eq!(forged, before);
            }
        }
        // The genuine native engine, using its consumed private owner, still
        // completes both the waiting and immediately answered restore paths.
        let mut replay = restore(&state);
        finish(&mut state, true);
        finish(&mut replay, true);
        assert_eq!(state, replay);
    }
}

fn advance_to_expected_pt_refusal(state: &mut GameState, source: ObjectId, reverse: bool) {
    for _ in 0..128 {
        let decision = engine::advance_until_decision(state);
        if state.engine.halted.is_some() {
            break;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseEffectTargets { legal_targets, .. } => Action::ChooseEffectTarget(
                *if reverse {
                    legal_targets.last()
                } else {
                    legal_targets.first()
                }
                .unwrap(),
            ),
            other => panic!("genuine overflow continuation changed decision: {other:?}"),
        };
        let result = engine::step(state, action);
        if state.engine.halted.is_some() {
            break;
        }
        result.unwrap();
    }
    assert_eq!(
        state.engine.halted,
        Some((
            engine::UnsupportedMechanic::InvalidEffectContinuation,
            source
        ))
    );
}
fn assert_unmoved_revealed_batch(original: &GameState, refused: &GameState, prefix: &[ObjectId]) {
    for player in [PlayerId::P0, PlayerId::P1] {
        assert_eq!(
            refused.players[player.index()].library,
            original.players[player.index()].library
        );
        assert_eq!(
            refused.players[player.index()].graveyard,
            original.players[player.index()].graveyard
        );
    }
    for &object in prefix {
        assert_eq!(refused.objects.get(object), original.objects.get(object));
    }
    assert!(!refused.engine.event_history[original.engine.event_history.len()..].iter().any(|event|
        matches!(event, event::CommittedEvent::ZoneChange { object, .. } if prefix.contains(object))));
    assert_eq!(
        refused.engine.active_replacements,
        original.engine.active_replacements
    );
}

#[test]
fn genuine_paid_think_twice_first_land_pt_overflow_refuses_before_zone_commit_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest", "Swamp"]);
        let source = paid_aberration(&mut state, actor);
        // A labelled numeric boundary, not a claim of paying for these counters.
        state.objects.get_mut(source).counters.plus1_plus1 = i32::MAX - 1;
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (i32::MAX, i32::MAX)
        );
        cast(&mut state, actor, "Think Twice", None, 2);
        assert_eq!(trigger_rows(&state).len(), 1);
        let prefix = vec![state.players[actor.opponent().index()].library[0]];
        let original = state.clone();
        let mut restored = restore(&state);
        advance_to_expected_pt_refusal(&mut state, source, false);
        advance_to_expected_pt_refusal(&mut restored, source, false);
        assert_eq!(state, restored);
        assert_unmoved_revealed_batch(&original, &state, &prefix);
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (i32::MAX, i32::MAX)
        );
    }
}

#[test]
fn genuine_paid_waiting_answered_mill_overflow_restore_keeps_original_zones_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reverse in [false, true] {
            let mut state = ready(actor, &["Think Twice", "Forest", "Swamp"]);
            let source = paid_aberration(&mut state, actor);
            state.objects.get_mut(source).counters.plus1_plus1 = i32::MAX - 1;
            cast(&mut state, actor, "Think Twice", None, 2);
            to_order(&mut state);
            let prefix = state.players[actor.opponent().index()].library[..2].to_vec();
            let waiting = restore(&state);
            let decision = engine::advance_until_decision(&mut state);
            let Decision::ChooseEffectTargets { legal_targets, .. } = decision else {
                panic!("original revealed order missing");
            };
            engine::step(
                &mut state,
                Action::ChooseEffectTarget(
                    *if reverse {
                        legal_targets.last()
                    } else {
                        legal_targets.first()
                    }
                    .unwrap(),
                ),
            )
            .unwrap();
            assert!(state.engine.halted.is_none());
            validate_pending_effect_choice(&state).unwrap();
            let answered = restore(&state);
            let mut from_waiting = waiting.clone();
            let mut from_answered = answered.clone();
            advance_to_expected_pt_refusal(&mut state, source, reverse);
            advance_to_expected_pt_refusal(&mut from_waiting, source, reverse);
            advance_to_expected_pt_refusal(&mut from_answered, source, reverse);
            assert_eq!(state, from_waiting);
            assert_eq!(state, from_answered);
            assert_unmoved_revealed_batch(&waiting, &state, &prefix);
        }
    }
}

fn paid_suppressed_boundary(actor: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready(actor, &["Forest"; 4]);
    let source = paid_aberration(&mut state, actor);
    // The Aura has normal sorcery timing, so its real caster needs their main.
    crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, actor.opponent());
    let aura = cast(
        &mut state,
        actor.opponent(),
        "Witness Protection",
        Some(Target::Object(source)),
        1,
    );
    finish(&mut state, false);
    assert_eq!(
        state.objects.get(aura).v4.attached_to,
        Some(crate::state::ObjectLinkV4 {
            object: source,
            zone_change_count: state.objects.get(source).zone_change_count,
        })
    );
    assert!(!crate::continuous_characteristics_v1::printed_abilities_active(&state, source));
    state.objects.get_mut(source).counters.plus1_plus1 = i32::MAX - 1;
    assert_eq!(
        engine::checked_effective_pt(&state, source).unwrap(),
        (i32::MAX, i32::MAX)
    );
    (state, source, aura)
}

#[test]
fn paid_suppressed_simultaneous_departures_keep_original_lki_in_both_orders_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for aura_first in [false, true] {
            let (mut state, source, aura) = paid_suppressed_boundary(actor);
            let generation = state.objects.get(source).zone_change_count;
            // Native simultaneous-zone primitive over genuinely paid objects;
            // this is not a claim of a printed two-permanent destruction spell.
            let objects = if aura_first {
                vec![aura, source]
            } else {
                vec![source, aura]
            };
            let events = objects
                .iter()
                .map(|id| ProposedEvent::zone_change(*id, Zone::Graveyard))
                .collect::<Vec<_>>();
            let mut restored = restore(&state);
            event::propose_and_commit_batch(&mut state, events.clone());
            event::propose_and_commit_batch(&mut restored, events);
            assert_eq!(state, restored);
            assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(aura).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(source).zone_change_count, generation + 1);
            let lki = state
                .creature_stats_lki_v1
                .as_ref()
                .unwrap()
                .iter()
                .filter(|entry| {
                    entry.source.object == source && entry.source.zone_change_count == generation
                })
                .collect::<Vec<_>>();
            assert_eq!(lki.len(), 1);
            assert_eq!((lki[0].power, lki[0].toughness), (i32::MAX, i32::MAX));
            assert!(engine::validate_battlefield_pt(&state).is_ok());
        }
    }
}

#[test]
fn paid_suppressed_survivor_exposure_and_mixed_prevention_refuse_before_mutation_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source, aura) = paid_suppressed_boundary(actor);
        for route in 0..4 {
            let mut current = restore(&state);
            if route == 3 {
                // Native prevention primitive, separately labelled from payment.
                // Its exact shield and history must not be consumed by the
                // mixed batch whose other member has unrepresentable final PT.
                current
                    .engine
                    .active_replacements
                    .push(event::ActiveReplacement {
                        id: 110_467,
                        source: aura,
                        kind: event::ReplacementEffectKind::PreventNextDamage {
                            target: Target::Player(actor),
                            remaining: 2,
                        },
                    });
            }
            let original = current.clone();
            let zone = ProposedEvent::zone_change(aura, Zone::Graveyard);
            match route {
                0 => event::commit(&mut current, zone),
                1 => event::propose_and_commit(&mut current, zone),
                2 => event::propose_and_commit_batch(&mut current, vec![zone]),
                3 => event::propose_and_commit_batch(
                    &mut current,
                    vec![
                        ProposedEvent::Damage(event::DamageProposed {
                            source: aura,
                            target: Target::Player(actor),
                            amount: 3,
                            is_combat: false,
                            touched_by: vec![],
                            historical_source: None,
                            niv_damage_origin_v1: None,
                        }),
                        zone,
                    ],
                ),
                _ => unreachable!(),
            }
            assert_eq!(
                current.engine.halted,
                Some((
                    engine::UnsupportedMechanic::InvalidEffectContinuation,
                    source
                ))
            );
            current.engine.halted = None;
            assert_eq!(current, original);
        }
    }
}

#[test]
fn genuine_paid_looting_madness_abort_overflow_keeps_placeholder_and_graveyard_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest"; 8]);
        let source = paid_aberration(&mut state, actor);
        let caster = actor.opponent();
        // Looting is a sorcery. Reach the actual caster's main before the
        // original one-red-source setup, without untapping after that setup.
        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, caster);
        let temper = put(&mut state, caster, "Fiery Temper", Zone::Hand);
        let other_discard = put(&mut state, caster, "Island", Zone::Hand);
        // Fixture mana availability: exactly one untapped red source. The
        // genuine Looting payment spends it, leaving Madness {R} unavailable.
        let mountains = state.players[caster.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|&object| state.objects.get(object).card_def == id("Mountain"))
            .collect::<Vec<_>>();
        for &object in mountains.iter().skip(1) {
            state.objects.get_mut(object).tapped = true;
        }
        let looting = cast(&mut state, caster, "Faithless Looting", None, 1);
        let mut reached_madness = false;
        for _ in 0..128 {
            let decision = engine::advance_until_decision(&mut state);
            assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
            let action = match decision {
                Decision::ChooseMadnessCast { player, card } => {
                    assert_eq!((player, card), (caster, temper));
                    reached_madness = true;
                    break;
                }
                Decision::Discard {
                    player,
                    count,
                    choices,
                } => {
                    assert_eq!((player, count), (caster, 2));
                    assert!(choices.contains(&temper) && choices.contains(&other_discard));
                    Action::Discard(vec![temper, other_discard])
                }
                Decision::CastSpellOrPass { .. } => Action::Pass,
                Decision::OrderTriggers { pending, .. } => {
                    Action::OrderTriggers((0..pending.len()).collect())
                }
                other => panic!("genuine paid Looting/Madness stage: {other:?}"),
            };
            engine::step(&mut state, action).unwrap();
        }
        assert!(reached_madness);
        assert_eq!(state.objects.get(looting).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(temper).zone, Zone::Exile);
        assert!(mountains
            .iter()
            .all(|&object| state.objects.get(object).tapped));
        let base = engine::effective_base_power(&state, source).unwrap();
        state.objects.get_mut(source).counters.plus1_plus1 = i32::MAX - base;
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (i32::MAX, i32::MAX)
        );
        engine::step(&mut state, Action::ChooseMadnessCast(true)).unwrap();
        let Decision::ChooseTargets {
            player,
            spell,
            legal_targets,
            ..
        } = engine::advance_until_decision(&mut state)
        else {
            panic!("actual Madness attempt must reach its real target choice");
        };
        assert_eq!((player, spell), (caster, temper));
        assert!(legal_targets.contains(&Target::Player(actor)));
        engine::step(&mut state, Action::ChooseTarget(Target::Player(actor))).unwrap();
        let original = state.clone();
        let mut restored = restore(&state);
        engine::advance_until_decision(&mut state);
        engine::advance_until_decision(&mut restored);
        assert_eq!(state, restored);
        assert_eq!(
            state.engine.halted,
            Some((
                engine::UnsupportedMechanic::InvalidEffectContinuation,
                source
            ))
        );
        assert_eq!(state.stack, original.stack);
        assert_eq!(state.objects.get(temper), original.objects.get(temper));
        assert_eq!(state.engine.event_history, original.engine.event_history);
        for player in [actor, caster] {
            assert_eq!(
                state.players[player.index()].graveyard,
                original.players[player.index()].graveyard
            );
            assert_eq!(
                state.players[player.index()].mana_pool,
                original.players[player.index()].mana_pool
            );
        }
        assert_eq!(taps(&state, caster), taps(&original, caster));
    }
}

#[test]
fn native_checked_negative_power_flashback_announcement_refuses_before_stack_move_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest"; 4]);
        let source = paid_aberration(&mut state, actor);
        let flashback = put(&mut state, actor.opponent(), "Think Twice", Zone::Graveyard);
        assert_eq!(engine::effective_base_power(&state, source), Some(2));
        let binding = EffectObjectBinding {
            object: source,
            expected_zone: Zone::Battlefield,
            expected_zone_change_count: state.objects.get(source).zone_change_count,
        };
        // Full-width native boost primitives, not printed spell/cost claims.
        // Toughness remains positive, so the real cast menu exists at P=i32::MIN.
        install_temporary_boost(&mut state, binding, i32::MIN, 0, card_def::Keywords::NONE);
        install_temporary_boost(&mut state, binding, -2, 0, card_def::Keywords::NONE);
        assert!(state.engine.halted.is_none());
        assert_eq!(
            engine::checked_effective_pt(&state, source).unwrap(),
            (i32::MIN, 2)
        );
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player, ref castable_spells, .. }
                if player == actor.opponent() && castable_spells.contains(&flashback)));
        let original = state.clone();
        let mut restored = restore(&state);
        engine::step(&mut state, Action::CastSpell(flashback)).unwrap();
        engine::step(&mut restored, Action::CastSpell(flashback)).unwrap();
        assert_eq!(state, restored);
        assert_eq!(
            state.engine.halted,
            Some((
                engine::UnsupportedMechanic::InvalidEffectContinuation,
                source
            ))
        );
        state.engine.halted = None;
        assert_eq!(state, original);
    }
}

#[test]
fn paid_longbow_granted_activation_above_original_mill_keeps_native_owner_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, &["Forest"; 10]);
        let source = paid_aberration(&mut state, actor);
        let equipment = cast(&mut state, actor, "Viridian Longbow", None, 1);
        finish(&mut state, false);
        let before = taps(&state, actor);
        engine::step(&mut state, Action::ActivateAbility(equipment, 0)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { ref legal_targets, .. } if legal_targets.contains(&Target::Object(source)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        assert_eq!(taps(&state, actor) - before, 3);
        finish(&mut state, false);
        assert_eq!(
            state.objects.get(equipment).v4.attached_to.unwrap().object,
            source
        );
        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, actor);
        assert!(!state.objects.get(source).summoning_sick);
        cast(&mut state, actor, "Think Twice", None, 2);
        let waiting = trigger_rows(&state)[0].clone();
        assert!(aberration_item_v1(&waiting));
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseTargets { ref legal_targets, .. } if legal_targets.contains(&Target::Player(actor.opponent())))
        );
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Player(actor.opponent())),
        )
        .unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        let ping = state.stack.last().unwrap();
        assert_eq!(ping.kind, crate::state::StackItemKind::ActivatedAbility);
        assert_eq!((ping.source, ping.controller), (source, actor));
        assert_ne!(ping.v4.stack_item_id, waiting.v4.stack_item_id);
        assert!(ping.v4.granted_by.is_some());
        assert!(state.stack.iter().any(|row| row == &waiting));
        assert!(state.objects.get(source).tapped);
        // The actual granted damage row needs its native owner, while the
        // catalog's unrelated as-enters probes need no enclosing Runner.
        assert!(crate::linked_aura_operation_v1::needs_event_owner(&state));
        assert!(!crate::linked_aura_operation_v1::needs_surrounding_operation_owner(&state));
        let life = state.players[actor.opponent().index()].life;
        let mut replay = restore(&state);
        finish(&mut state, false);
        finish(&mut replay, false);
        assert_eq!(state, replay);
        assert_eq!(state.players[actor.opponent().index()].life, life - 1);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        assert_eq!(
            state.objects.get(equipment).v4.attached_to.unwrap().object,
            source
        );
    }
}
