//! Focused rules coverage for Adventure (Fang Dragon // Forktail Sweep) and
//! the monarch designation (Azure Fleet Admiral).
//!
//! The bounded rules oracle is the Mage fork at `72a08a3b`:
//! `FangDragon.java` blob `ba21bbd892be96961332238a56a90d7ba1a44201`
//! (an `AdventureCard`: creature side Fang Dragon 6/3 flying Dragon for
//! `{5}{R}{R}`; adventure side Forktail Sweep, `{1}{R}` sorcery, "deals 1
//! damage to each creature you don't control") and `AzureFleetAdmiral.java`
//! blob `50de91ed814a3373392873f03083084424e22606` (ETB: "you become the
//! monarch"; static: "can't be blocked by creatures the monarch controls").
//! The monarch designation itself is `mage/designations/Monarch.java` blob
//! `20cbc62b599a196a35009741ab7c6f1d8e54d7ca`: "At the beginning of the
//! monarch's end step, that player draws a card" and "Whenever a creature
//! deals combat damage to the monarch, its controller becomes the monarch."
//! In XMage both of those are genuine `TriggeredAbilityImpl`s (stack-based);
//! this kernel keeps the end-step draw stack-based
//! (`MonarchTriggerBindingV1`, mirroring `InitiativeTriggerBindingV1`) but
//! applies the combat-damage transfer as a direct designation change with no
//! stack window -- a documented simplification (see the task report), not
//! exercised by anything in this pool that could observe the difference
//! (no card responds to "about to lose the crown").

use mtg_kernel::card_def::{card_id_by_name, card_id_by_visible_name, CardType, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{
    Counters, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Zone,
};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

// Copied verbatim from `tests/pauper_meta_w1_delve_and_longbow.rs`.
fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not construct stack objects"),
    }
    id
}

/// Mirrors `pauper_meta_w1_delve_and_longbow.rs`'s `ready_main1`: Main1, P0
/// active with priority, both libraries as given.
fn ready_main1(p0_library: &[&str], p1_library: &[&str]) -> GameState {
    let p0_defs = p0_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let p1_defs = p1_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let mut state =
        GameState::new_from_libraries(&p0_defs, &p1_defs, card_name, 0x4144_5645_4e54_5552);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

/// Copied from `tests/pauper_meta_w1_delve_and_longbow.rs`: resolves the
/// stack (and any pending cast/activation/trigger) to idle, passing every
/// `CastSpellOrPass` window along the way. Panics if the engine halts.
fn resolve_until_idle(state: &mut GameState) {
    for _ in 0..96 {
        let decision = engine::advance_until_decision(state);
        if let Decision::Halted { mechanic, source } = decision {
            panic!("engine halted while resolving {source:?}: {mechanic:?}");
        }
        if state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_activation.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while resolving: {other:?}"),
        }
    }
    panic!("stack did not settle within the focused bound");
}

/// Passes through every `CastSpellOrPass` window (and declares zero
/// attackers whenever offered) until the very first decision returned once
/// `state.step == step && state.active_player == active_player`. Returns
/// that decision *unapplied*, so the caller can inspect `state.stack`
/// (e.g. a step-entry trigger sitting on the stack, not yet resolved)
/// before deciding how to proceed.
fn advance_to_step_decision(state: &mut GameState, step: Step, active_player: PlayerId) -> Decision {
    for _ in 0..2000 {
        let decision = engine::advance_until_decision(state);
        if state.step == step && state.active_player == active_player {
            return decision;
        }
        match decision {
            Decision::Halted { mechanic, source } => {
                panic!("engine halted at {source:?}: {mechanic:?}")
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareAttackers { .. } => {
                engine::step(state, Action::DeclareAttackers(Vec::new())).unwrap()
            }
            other => panic!("unexpected decision while advancing to {step:?}: {other:?}"),
        }
    }
    panic!("did not reach {step:?} (active {active_player:?}) within the focused bound");
}

// ---------------------------------------------------------------------
// Adventure
// ---------------------------------------------------------------------

#[test]
fn forktail_sweep_is_cast_from_hand_then_the_dragon_is_cast_from_exile() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let fang_dragon = put_object(&mut state, PlayerId::P0, "Fang Dragon", Zone::Hand);
    let p0_elf = put_object(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Battlefield);
    let p1_elf = put_object(&mut state, PlayerId::P1, "Llanowar Elves", Zone::Battlefield);

    // Exactly {1}{R}: enough for Forktail Sweep, nowhere near Fang Dragon's
    // own {5}{R}{R}, so the Adventure form is the only viable one and gets
    // silently auto-selected (no `ChooseSpellMode` decision).
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => assert!(
            castable_spells.contains(&fang_dragon),
            "Forktail Sweep is castable at sorcery speed for {{1}}{{R}}"
        ),
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }

    engine::step(&mut state, Action::CastSpell(fang_dragon)).unwrap();
    assert!(
        state.stack.iter().any(|item| item.source == fang_dragon),
        "Forktail Sweep spell on stack"
    );
    resolve_until_idle(&mut state);

    assert_eq!(
        state.objects.get(p1_elf).zone,
        Zone::Graveyard,
        "P1's 1/1 takes 1 damage and dies"
    );
    assert_eq!(
        state.objects.get(p0_elf).zone,
        Zone::Battlefield,
        "P0 doesn't damage its own creature"
    );
    assert_eq!(
        state.objects.get(fang_dragon).zone,
        Zone::Exile,
        "Forktail Sweep resolves by exiling the physical card, not the graveyard"
    );
    assert!(
        state.objects.get(fang_dragon).v4.on_adventure,
        "the exiled card carries adventure permission to cast the creature later"
    );

    // A later turn, with enough mana for the creature's own {5}{R}{R}.
    state.turn += 1;
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 2;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 5;

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => assert!(
            castable_spells.contains(&fang_dragon),
            "the creature face is castable from exile via on_adventure"
        ),
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }
    engine::step(&mut state, Action::CastSpell(fang_dragon)).unwrap();
    assert!(
        state.stack.iter().any(|item| item.source == fang_dragon),
        "Fang Dragon spell on stack"
    );
    resolve_until_idle(&mut state);

    assert_eq!(state.objects.get(fang_dragon).zone, Zone::Battlefield);
    assert_eq!(engine::effective_power(&state, fang_dragon), 6);
    assert_eq!(engine::effective_toughness(&state, fang_dragon), 3);
    assert!(
        engine::has_effective_keyword(&state, fang_dragon, mtg_kernel::card_def::Keywords::FLYING),
        "Fang Dragon enters as a 6/3 flier"
    );
    assert!(
        !state.objects.get(fang_dragon).v4.on_adventure,
        "the permission clears with the zone change onto the battlefield"
    );

    // A card exiled by any other means never carries the permission.
    let mut other = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let stray_dragon = put_object(&mut other, PlayerId::P0, "Fang Dragon", Zone::Exile);
    assert!(
        !other.objects.get(stray_dragon).v4.on_adventure,
        "a plain exile placement never sets on_adventure"
    );
    other.players[0].mana_pool[ManaColor::R.pool_index()] = 2;
    other.players[0].mana_pool[ManaColor::C.pool_index()] = 5;
    match engine::advance_until_decision(&mut other) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => assert!(
            !castable_spells.contains(&stray_dragon),
            "no adventure permission means no exile-cast eligibility"
        ),
        other_decision => panic!("expected CastSpellOrPass, got {other_decision:?}"),
    }
}

#[test]
fn fang_dragon_can_be_cast_directly_from_hand() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let fang_dragon = put_object(&mut state, PlayerId::P0, "Fang Dragon", Zone::Hand);
    // Enough for the printed {5}{R}{R} (and, incidentally, also enough for
    // the cheaper {1}{R} Adventure form -- both forms are viable here, so
    // an explicit `ChooseSpellMode` selects the creature).
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 2;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 5;

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => assert!(castable_spells.contains(&fang_dragon)),
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }
    engine::step(&mut state, Action::CastSpell(fang_dragon)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseSpellMode {
            spell,
            mode_count: 2,
            ref legal_modes,
            ..
        } => {
            assert_eq!(spell, fang_dragon);
            assert!(legal_modes.contains(&0), "the creature form is legal");
            assert!(legal_modes.contains(&1), "the Adventure form is also legal");
        }
        other => panic!("expected ChooseSpellMode, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseSpellMode(0)).unwrap();
    resolve_until_idle(&mut state);

    assert_eq!(state.objects.get(fang_dragon).zone, Zone::Battlefield);
    assert_eq!(engine::effective_power(&state, fang_dragon), 6);
    assert_eq!(engine::effective_toughness(&state, fang_dragon), 3);
    assert!(engine::has_effective_keyword(
        &state,
        fang_dragon,
        mtg_kernel::card_def::Keywords::FLYING
    ));
}

#[test]
fn visible_name_resolves_adventure_face() {
    let fang_dragon_id = card_id("Fang Dragon");
    assert_eq!(
        card_id_by_visible_name("Forktail Sweep"),
        Some((fang_dragon_id, 2))
    );
    assert_eq!(
        card_id_by_visible_name("Fang Dragon"),
        Some((fang_dragon_id, 0))
    );
    let def = &CARD_DEFS[fang_dragon_id as usize];
    let adventure = def.adventure.as_ref().expect("Fang Dragon has an Adventure");
    assert_eq!(adventure.name, "Forktail Sweep");
    assert_eq!(adventure.types, &[CardType::Sorcery]);
}

// ---------------------------------------------------------------------
// Monarch
// ---------------------------------------------------------------------

#[test]
fn azure_fleet_admiral_makes_its_controller_the_monarch_who_draws_at_end_step() {
    let mut state = ready_main1(&["Mountain"; 10], &["Mountain"; 10]);
    let admiral = put_object(&mut state, PlayerId::P0, "Azure Fleet Admiral", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 3;

    engine::step(&mut state, Action::CastSpell(admiral)).unwrap();
    resolve_until_idle(&mut state);

    assert_eq!(state.objects.get(admiral).zone, Zone::Battlefield);
    assert_eq!(
        state.monarch,
        Some(PlayerId::P0),
        "Azure Fleet Admiral's ETB makes its controller the monarch"
    );

    let p0_library_before = state.players[0].library.len();
    let p0_hand_before = state.players[0].hand.len();
    let decision = advance_to_step_decision(&mut state, Step::End, PlayerId::P0);
    assert!(
        matches!(decision, Decision::CastSpellOrPass { .. }),
        "expected a priority window at P0's End step, got {decision:?}"
    );
    assert_eq!(
        state.stack.len(),
        1,
        "the monarch's end-step draw trigger is on the stack"
    );
    assert_eq!(state.stack[0].kind, StackItemKind::TriggeredAbility);
    resolve_until_idle(&mut state);
    assert_eq!(
        state.players[0].library.len(),
        p0_library_before - 1,
        "the monarch drew a card at their own end step"
    );
    assert_eq!(state.players[0].hand.len(), p0_hand_before + 1);

    // Continue on to P1's own End step: P1 is not the monarch, so nothing
    // should be sitting on the stack there.
    let decision = advance_to_step_decision(&mut state, Step::End, PlayerId::P1);
    assert!(
        matches!(decision, Decision::CastSpellOrPass { .. }),
        "expected a priority window at P1's End step, got {decision:?}"
    );
    assert!(
        state.stack.is_empty(),
        "nobody draws at the non-monarch's end step"
    );
    assert_eq!(
        state.monarch,
        Some(PlayerId::P0),
        "the crown does not move on its own"
    );
}

#[test]
fn combat_damage_to_the_monarch_moves_the_crown() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    state.monarch = Some(PlayerId::P0);
    let attacker = put_object(&mut state, PlayerId::P1, "Guttersnipe", Zone::Battlefield);
    state.step = Step::DeclareAttackers;
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;

    match engine::advance_until_decision(&mut state) {
        Decision::DeclareAttackers { player, eligible } => {
            assert_eq!(player, PlayerId::P1);
            assert!(eligible.contains(&attacker));
        }
        other => panic!("expected DeclareAttackers, got {other:?}"),
    }
    engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();

    loop {
        match engine::advance_until_decision(&mut state) {
            Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
            Decision::DeclareBlockers { player, .. } => {
                assert_eq!(player, PlayerId::P0);
                engine::step(&mut state, Action::DeclareBlockers(Vec::new())).unwrap();
                break;
            }
            other => panic!("unexpected path to blockers: {other:?}"),
        }
    }
    // Keep passing priority (through Combat Damage and End of Combat) all
    // the way to Main2 -- an already-idle stack at DeclareBlockers itself
    // does not mean combat damage has actually been dealt yet.
    advance_to_step_decision(&mut state, Step::Main2, PlayerId::P1);

    assert_eq!(state.players[0].life, 18, "2 unblocked combat damage to P0");
    assert_eq!(
        state.monarch,
        Some(PlayerId::P1),
        "the attacker's controller becomes the monarch"
    );
    let source_contract = state
        .engine
        .monarch_source
        .expect("combat-damage transfer rebinds monarch_source");
    assert_eq!(
        source_contract.source, attacker,
        "monarch_source rebinds to the attacking creature, not any earlier ETB grant"
    );
}

#[test]
fn admiral_cannot_be_blocked_by_the_monarchs_creatures() {
    // P1 is the monarch: its untapped 3/3 may not block the attacking
    // Admiral.
    let mut blocked_out = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    blocked_out.monarch = Some(PlayerId::P1);
    let admiral = put_object(
        &mut blocked_out,
        PlayerId::P0,
        "Azure Fleet Admiral",
        Zone::Battlefield,
    );
    let blocker = put_object(
        &mut blocked_out,
        PlayerId::P1,
        "Azure Fleet Admiral",
        Zone::Battlefield,
    );
    blocked_out.step = Step::DeclareAttackers;
    blocked_out.active_player = PlayerId::P0;
    blocked_out.priority_player = PlayerId::P0;

    engine::step(&mut blocked_out, Action::DeclareAttackers(vec![admiral])).unwrap();
    loop {
        match engine::advance_until_decision(&mut blocked_out) {
            Decision::CastSpellOrPass { .. } => {
                engine::step(&mut blocked_out, Action::Pass).unwrap()
            }
            Decision::DeclareBlockers { legal_blockers, .. } => {
                let (_, blockers) = legal_blockers
                    .iter()
                    .find(|(attacker, _)| *attacker == admiral)
                    .expect("the Admiral is an attacker with a legal_blockers row");
                assert!(
                    blockers.is_empty(),
                    "the monarch's own creature may not block the Admiral"
                );
                assert!(!blockers.contains(&blocker));
                break;
            }
            other => panic!("unexpected path to blockers: {other:?}"),
        }
    }

    // Once P0 (the Admiral's own controller) is the monarch instead, P1's
    // 3/3 is no longer "a creature the monarch controls" and may block.
    let mut blockable = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    blockable.monarch = Some(PlayerId::P0);
    let admiral = put_object(
        &mut blockable,
        PlayerId::P0,
        "Azure Fleet Admiral",
        Zone::Battlefield,
    );
    let blocker = put_object(
        &mut blockable,
        PlayerId::P1,
        "Azure Fleet Admiral",
        Zone::Battlefield,
    );
    blockable.step = Step::DeclareAttackers;
    blockable.active_player = PlayerId::P0;
    blockable.priority_player = PlayerId::P0;

    engine::step(&mut blockable, Action::DeclareAttackers(vec![admiral])).unwrap();
    loop {
        match engine::advance_until_decision(&mut blockable) {
            Decision::CastSpellOrPass { .. } => {
                engine::step(&mut blockable, Action::Pass).unwrap()
            }
            Decision::DeclareBlockers { legal_blockers, .. } => {
                let (_, blockers) = legal_blockers
                    .iter()
                    .find(|(attacker, _)| *attacker == admiral)
                    .expect("the Admiral is an attacker with a legal_blockers row");
                assert!(
                    blockers.contains(&blocker),
                    "P1's 3/3 may block once P0 (not P1) is the monarch"
                );
                break;
            }
            other => panic!("unexpected path to blockers: {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------
// Review fix round 1: `trigger::selected_spell_types` classified an Omen-
// tagged cast by `CardDef::omen` only, so an Adventure spell (Forktail
// Sweep, `CardDef::omen` is `None`) was misclassified as typeless and never
// matched `TriggerCondition::CastInstantOrSorcery` (Guttersnipe, Murmuring
// Mystic).
// ---------------------------------------------------------------------

#[test]
fn forktail_sweep_still_triggers_cast_instant_or_sorcery_abilities() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let fang_dragon = put_object(&mut state, PlayerId::P0, "Fang Dragon", Zone::Hand);
    let guttersnipe = put_object(&mut state, PlayerId::P0, "Guttersnipe", Zone::Battlefield);
    let p0_elf = put_object(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Battlefield);
    let p1_elf = put_object(&mut state, PlayerId::P1, "Llanowar Elves", Zone::Battlefield);
    // Exactly {1}{R}: only Forktail Sweep is payable, so it auto-selects.
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;

    engine::step(&mut state, Action::CastSpell(fang_dragon)).unwrap();
    resolve_until_idle(&mut state);

    assert_eq!(
        state.objects.get(p1_elf).zone,
        Zone::Graveyard,
        "Forktail Sweep still deals 1 to P1's 1/1"
    );
    assert_eq!(
        state.objects.get(p0_elf).zone,
        Zone::Battlefield,
        "Forktail Sweep still doesn't hit P0's own creature"
    );
    assert_eq!(
        state.players[1].life, 18,
        "Guttersnipe's cast-instant-or-sorcery trigger also deals 2 to the opponent, \
         in addition to (not instead of) the sweep's own creature damage"
    );
    assert!(state.players[0].battlefield.contains(&guttersnipe));
}

#[test]
fn fang_dragon_cast_as_a_creature_does_not_trigger_cast_instant_or_sorcery_abilities() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let fang_dragon = put_object(&mut state, PlayerId::P0, "Fang Dragon", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Guttersnipe", Zone::Battlefield);
    // {5}{R}{R}: both forms are payable, so the creature form must be
    // chosen explicitly.
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 2;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 5;

    engine::step(&mut state, Action::CastSpell(fang_dragon)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseSpellMode { mode_count: 2, .. } => {}
        other => panic!("expected ChooseSpellMode, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseSpellMode(0)).unwrap();
    resolve_until_idle(&mut state);

    assert_eq!(state.objects.get(fang_dragon).zone, Zone::Battlefield);
    assert_eq!(
        state.players[1].life, 20,
        "Fang Dragon's creature side is not an instant or sorcery; Guttersnipe does not trigger"
    );
}
