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
            assert!(crate::drake_public_v53::capture(&state).is_err());
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
