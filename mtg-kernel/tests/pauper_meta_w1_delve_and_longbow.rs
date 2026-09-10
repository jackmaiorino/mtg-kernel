//! Focused rules coverage for Delve (Gurmag Angler) and an equipment-granted
//! activated ability (Viridian Longbow).
//!
//! The bounded rules oracle is the Mage fork at `72a08a3b`:
//! `GurmagAngler.java` (delve; 5/5 for `{6}{B}`) and `ViridianLongbow.java`
//! ("Equipped creature has \"{T}: This creature deals 1 damage to any
//! target.\""; equip `{3}`), plus `DelveAbility.java`'s rules-text doc:
//! "Each card you exile from your graveyard while casting this spell pays
//! for {1}." (702.65a-b) -- delve is not an alternative/additional cost, it
//! composes with the total cost already determined, and never exiles more
//! than the printed generic amount even when the graveyard holds more.
//!
//! This kernel resolves the "how many cards to delve" and "which cards"
//! choices deterministically inside the mana-payment planner
//! (`mana::delve_payment_plan`) rather than through a new RL decision/action
//! kind: mana payment is already an opaque, engine-chosen detail (no
//! existing decision lets a player pick which lands tap for a spell
//! either), and `legal_action_candidates_v1` offers exactly one "cast Gurmag
//! Angler" candidate regardless of how it ends up paid. The planner tries
//! ascending delve counts (0, 1, 2, ...) and takes the first that's
//! payable -- i.e. it prefers paying with mana over the graveyard -- and
//! exiles the *oldest* graveyard cards first. Several tests below prove two
//! different delve counts can be independently legal in the same state
//! (both "plan families") even though only one is ever actually chosen.

use mtg_kernel::card_def::{card_id_by_name, CostComponent, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::{EffectOp, TargetRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{self, Cost, ManaColor, Pip};
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectLinkV4, ObjectStateV4, Step, Target, Zone};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

// Copied verbatim from `tests/pauper_meta_w1_snuff_out_and_duals.rs` /
// `tests/artifact_equipment_completion.rs`.
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

/// Mirrors `pauper_meta_w1_spells.rs`'s `ready_main1`: Main1, P0 active with
/// priority, both libraries as given.
fn ready_main1(p0_library: &[&str], p1_library: &[&str]) -> GameState {
    let p0_defs = p0_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let p1_defs = p1_library.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let mut state =
        GameState::new_from_libraries(&p0_defs, &p1_defs, card_name, 0x44_45_4c_56_45_57_31);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

/// Copied from `tests/artifact_equipment_completion.rs`: resolves the stack
/// (and any pending cast/activation/trigger) to idle, passing every
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

/// Directly wires an attachment relation, skipping the equip cost/targeting
/// flow -- copied from `tests/artifact_equipment_completion.rs`. Used only
/// where the equip flow itself is not the thing under test.
fn attach_exact_for_test(state: &mut GameState, equipment: ObjectId, host: ObjectId) {
    let host_generation = state.objects.get(host).zone_change_count;
    state.objects.get_mut(equipment).v4.attached_to = Some(ObjectLinkV4 {
        object: host,
        zone_change_count: host_generation,
    });
    state.objects.get_mut(host).attachments.push(equipment);
    state.objects.get_mut(host).attachments.sort_unstable();
    state.objects.get_mut(host).attachments.dedup();
    state.validate_attachment_relations().unwrap();
}

// ---------------------------------------------------------------------
// Registry metadata
// ---------------------------------------------------------------------

#[test]
fn gurmag_angler_registry_metadata() {
    let id = card_id("Gurmag Angler");
    let def = &CARD_DEFS[id as usize];
    assert!(def.delve, "Gurmag Angler has Delve");
    assert_eq!(def.power, Some(5));
    assert_eq!(def.toughness, Some(5));
    assert_eq!(def.cost.generic, 6, "printed cost is {{6}}{{B}}");
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::B)]);
}

#[test]
fn viridian_longbow_registry_metadata() {
    let id = card_id("Viridian Longbow");
    let def = &CARD_DEFS[id as usize];
    let equipment = def.equipment.expect("Viridian Longbow is Equipment");
    assert_eq!(equipment.power_delta, 0);
    assert_eq!(equipment.toughness_delta, 0);
    let granted = equipment
        .granted_activated_ability
        .expect("Viridian Longbow grants a tap-ping ability");
    assert_eq!(granted.cost, &[CostComponent::Tap]);
    assert_eq!(granted.target_spec, TargetSpec::AnyTarget);
    match (granted.effect)() {
        EffectOp::DealDamage {
            target: TargetRef::Target(0),
            amount: 1,
        } => {}
        other => panic!("expected DealDamage to Target(0) amount 1, got {other:?}"),
    }
}

#[test]
fn every_other_equipment_still_has_no_granted_ability() {
    for name in ["Black Mage's Rod", "Hunter's Blowgun"] {
        let id = card_id(name);
        let equipment = CARD_DEFS[id as usize]
            .equipment
            .unwrap_or_else(|| panic!("{name} is Equipment"));
        assert!(
            equipment.granted_activated_ability.is_none(),
            "{name} grants no activated ability"
        );
    }
}

// ---------------------------------------------------------------------
// Delve
// ---------------------------------------------------------------------

#[test]
fn gurmag_angler_castable_by_delving_all_six_graveyard_cards_for_one_swamp() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let gurmag = put_object(&mut state, PlayerId::P0, "Gurmag Angler", Zone::Hand);
    let swamp = put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    let graveyard_cards: Vec<ObjectId> = (0..6)
        .map(|_| put_object(&mut state, PlayerId::P0, "Mountain", Zone::Graveyard))
        .collect();

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => {
            assert!(
                castable_spells.contains(&gurmag),
                "delving all 6 graveyard cards pays the {{6}} generic, leaving only {{B}} for the one Swamp"
            );
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }

    engine::step(&mut state, Action::CastSpell(gurmag)).unwrap();
    resolve_until_idle(&mut state);

    assert!(
        state.players[0].graveyard.is_empty(),
        "all six graveyard cards were exiled to pay delve"
    );
    for id in &graveyard_cards {
        assert_eq!(
            state.objects.get(*id).zone,
            Zone::Exile,
            "delved card moved to exile"
        );
    }
    assert!(
        state.objects.get(swamp).tapped,
        "the one Swamp paid the remaining {{B}} pip"
    );
    assert!(state.players[0].battlefield.contains(&gurmag));
}

#[test]
fn gurmag_angler_not_castable_with_two_graveyard_cards_and_one_swamp() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let gurmag = put_object(&mut state, PlayerId::P0, "Gurmag Angler", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Graveyard);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Graveyard);

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => {
            assert!(
                !castable_spells.contains(&gurmag),
                "delving both graveyard cards only pays {{2}}; {{4}}{{B}} remains unpayable with one Swamp"
            );
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }
}

#[test]
fn gurmag_angler_castable_with_two_graveyard_cards_and_five_swamps_exiling_both() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let gurmag = put_object(&mut state, PlayerId::P0, "Gurmag Angler", Zone::Hand);
    let swamps: Vec<ObjectId> = (0..5)
        .map(|_| put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield))
        .collect();
    let graveyard_cards: Vec<ObjectId> = (0..2)
        .map(|_| put_object(&mut state, PlayerId::P0, "Mountain", Zone::Graveyard))
        .collect();

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => {
            assert!(castable_spells.contains(&gurmag));
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }

    engine::step(&mut state, Action::CastSpell(gurmag)).unwrap();
    resolve_until_idle(&mut state);

    assert!(
        state.players[0].graveyard.is_empty(),
        "both graveyard cards were exiled: only 5 mana is available, and {{6}}{{B}} needs 7"
    );
    for id in &graveyard_cards {
        assert_eq!(state.objects.get(*id).zone, Zone::Exile);
    }
    for id in &swamps {
        assert!(
            state.objects.get(*id).tapped,
            "all five Swamps tapped -- {{4}}{{B}} (after delving 2) needs exactly 5 mana"
        );
    }
    assert!(state.players[0].battlefield.contains(&gurmag));
}

#[test]
fn gurmag_angler_prefers_paying_mana_over_delving_when_both_plans_are_legal() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let gurmag = put_object(&mut state, PlayerId::P0, "Gurmag Angler", Zone::Hand);
    let swamps: Vec<ObjectId> = (0..7)
        .map(|_| put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield))
        .collect();
    let graveyard_cards: Vec<ObjectId> = (0..2)
        .map(|_| put_object(&mut state, PlayerId::P0, "Mountain", Zone::Graveyard))
        .collect();

    // Both plan families are independently legal in this state: paying the
    // full printed generic with mana alone (k=0: {6}{B} against 7 Swamps),
    // and delving both graveyard cards (k=2: {4}{B} against 7 Swamps).
    // `legal_action_candidates_v1`/`castable_spells` only ever offer one
    // "cast Gurmag Angler" candidate (mana payment -- including delve -- is
    // an opaque engine-chosen detail with no dedicated RL decision), so
    // "both plans are legal" is verified directly against the mana solver
    // that `mana::delve_payment_plan` enumerates over.
    let full_price = Cost {
        pips: &[Pip::Colored(ManaColor::B)],
        generic: 6,
        x_count: 0,
    };
    let delved_price = Cost {
        pips: &[Pip::Colored(ManaColor::B)],
        generic: 4,
        x_count: 0,
    };
    assert!(
        mana::can_pay(&full_price, 0, PlayerId::P0, &state).is_some(),
        "k=0 (no delve) is a legal plan"
    );
    assert!(
        mana::can_pay(&delved_price, 0, PlayerId::P0, &state).is_some(),
        "k=2 (delve both) is also a legal plan"
    );

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => assert!(castable_spells.contains(&gurmag)),
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }

    engine::step(&mut state, Action::CastSpell(gurmag)).unwrap();
    resolve_until_idle(&mut state);

    assert_eq!(
        state.players[0].graveyard.len(),
        2,
        "no delve: the deterministic planner tries k=0 first and prefers paying with mana"
    );
    for id in &graveyard_cards {
        assert_eq!(state.objects.get(*id).zone, Zone::Graveyard);
    }
    let tapped_count = swamps
        .iter()
        .filter(|&&id| state.objects.get(id).tapped)
        .count();
    assert_eq!(
        tapped_count, 7,
        "all seven Swamps tapped to pay the full {{6}}{{B}} without delving"
    );
}

#[test]
fn delve_with_seven_graveyard_cards_and_seven_lands_never_exceeds_the_generic_cap() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let gurmag = put_object(&mut state, PlayerId::P0, "Gurmag Angler", Zone::Hand);
    for _ in 0..7 {
        put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    }
    let graveyard_cards: Vec<ObjectId> = (0..7)
        .map(|_| put_object(&mut state, PlayerId::P0, "Mountain", Zone::Graveyard))
        .collect();

    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::CastSpell(gurmag)).unwrap();
    resolve_until_idle(&mut state);

    let exiled = graveyard_cards
        .iter()
        .filter(|&&id| state.objects.get(id).zone == Zone::Exile)
        .count();
    assert!(
        exiled <= 6,
        "delve can never exile more than the printed generic amount (6), even with 7 in the graveyard; got {exiled}"
    );
}

#[test]
fn delve_caps_at_six_cards_even_when_mana_is_tight_enough_to_force_it() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let gurmag = put_object(&mut state, PlayerId::P0, "Gurmag Angler", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Swamp", Zone::Battlefield);
    // Seven graveyard cards, but only one Swamp: the smallest payable delve
    // count is exactly 6 (0 generic + {B} = 1 mana), not 7, proving the cap
    // is the printed generic amount, not the graveyard's size.
    let graveyard_cards: Vec<ObjectId> = (0..7)
        .map(|_| put_object(&mut state, PlayerId::P0, "Mountain", Zone::Graveyard))
        .collect();

    engine::advance_until_decision(&mut state);
    engine::step(&mut state, Action::CastSpell(gurmag)).unwrap();
    resolve_until_idle(&mut state);

    let exiled = graveyard_cards
        .iter()
        .filter(|&&id| state.objects.get(id).zone == Zone::Exile)
        .count();
    assert_eq!(
        exiled, 6,
        "the printed generic amount ({{6}}) caps delve even though 7 cards sit in the graveyard"
    );
    assert_eq!(
        state.players[0].graveyard.len(),
        1,
        "one card must remain: delve cannot pay the colored {{B}} pip"
    );
    // Deterministic exile order: oldest graveyard cards first (index 0..6
    // of insertion order), documented on `mana::delve_payment_plan`.
    for id in &graveyard_cards[..6] {
        assert_eq!(
            state.objects.get(*id).zone,
            Zone::Exile,
            "oldest cards delved first"
        );
    }
    assert_eq!(
        state.objects.get(graveyard_cards[6]).zone,
        Zone::Graveyard,
        "newest card stays behind"
    );
}

// ---------------------------------------------------------------------
// Viridian Longbow
// ---------------------------------------------------------------------

#[test]
fn viridian_longbow_equips_for_three_and_grants_a_tap_ping() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let longbow = put_object(&mut state, PlayerId::P0, "Viridian Longbow", Zone::Battlefield);
    let elf = put_object(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Battlefield);
    let other_elf = put_object(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Battlefield);

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            activatable_abilities,
            ..
        } => {
            assert!(
                !activatable_abilities.contains(&(elf, 0)),
                "unequipped creature has no granted ability"
            );
            assert!(!activatable_abilities.contains(&(other_elf, 0)));
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }

    state.players[0].mana_pool[ManaColor::C.pool_index()] = 3;
    engine::step(&mut state, Action::ActivateAbility(longbow, 0)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(elf)));
        }
        other => panic!("expected ChooseTargets, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(elf))).unwrap();
    resolve_until_idle(&mut state);

    assert_eq!(state.objects.get(longbow).v4.attached_to.unwrap().object, elf);

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            activatable_abilities,
            ..
        } => {
            assert!(
                activatable_abilities.contains(&(elf, 0)),
                "the equipped creature now offers the granted tap ability at index 0 \
                 (Llanowar Elves' own printed activated_abilities is empty)"
            );
            assert!(
                !activatable_abilities.contains(&(other_elf, 0)),
                "the other Elf is still unequipped"
            );
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }

    engine::step(&mut state, Action::ActivateAbility(elf, 0)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(PlayerId::P1)));
        }
        other => panic!("expected ChooseTargets, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Player(PlayerId::P1))).unwrap();
    resolve_until_idle(&mut state);

    assert_eq!(state.players[1].life, 19, "1 damage to P1's face");
    assert!(state.objects.get(elf).tapped, "paid its tap cost");
}

#[test]
fn summoning_sick_equipped_creature_cannot_activate_the_granted_tap_ability() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let longbow = put_object(&mut state, PlayerId::P0, "Viridian Longbow", Zone::Battlefield);
    let elf = put_object(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(elf).summoning_sick = true;
    attach_exact_for_test(&mut state, longbow, elf);

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            activatable_abilities,
            ..
        } => {
            assert!(
                !activatable_abilities.contains(&(elf, 0)),
                "a summoning-sick creature cannot pay a {{T}} cost"
            );
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }
    assert!(
        engine::step(&mut state, Action::ActivateAbility(elf, 0)).is_err(),
        "activating an unoffered ability is rejected"
    );
}

// ---------------------------------------------------------------------
// CR 113.7a: an activated ability on the stack resolves independently of
// its source. A response that removes the granting Equipment or the
// equipped creature after the ping is activated must not halt the engine
// (review finding, fix round 1).
// ---------------------------------------------------------------------

#[test]
fn longbow_ping_resolves_after_the_longbow_is_destroyed_in_response() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let longbow = put_object(&mut state, PlayerId::P0, "Viridian Longbow", Zone::Battlefield);
    let elf = put_object(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Battlefield);
    attach_exact_for_test(&mut state, longbow, elf);
    let ancient_grudge = put_object(&mut state, PlayerId::P0, "Ancient Grudge", Zone::Hand);
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;

    engine::step(&mut state, Action::ActivateAbility(elf, 0)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(PlayerId::P1)));
        }
        other => panic!("expected ChooseTargets, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Player(PlayerId::P1))).unwrap();

    // In response, destroy the Longbow while the ping still sits on the
    // stack beneath the new spell.
    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            player,
            castable_spells,
            ..
        } => {
            assert_eq!(player, PlayerId::P0);
            assert!(castable_spells.contains(&ancient_grudge));
            assert_eq!(state.stack.len(), 1, "the ping is on the stack");
            assert!(state.objects.get(elf).tapped, "paid its tap cost");
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }
    engine::step(&mut state, Action::CastSpell(ancient_grudge)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(longbow)));
        }
        other => panic!("expected ChooseTargets, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(longbow))).unwrap();

    resolve_until_idle(&mut state);

    assert!(
        state.engine.halted.is_none(),
        "the engine must not halt: {:?}",
        state.engine.halted
    );
    assert_eq!(
        state.objects.get(longbow).zone,
        Zone::Graveyard,
        "the Longbow was destroyed before the ping resolved"
    );
    assert_eq!(
        state.players[1].life, 19,
        "the ping still resolves for 1 damage using the frozen ability, \
         even though the Equipment that granted it is gone"
    );
}

#[test]
fn longbow_ping_resolves_after_the_equipped_creature_is_destroyed_in_response() {
    let mut state = ready_main1(&["Mountain"; 8], &["Mountain"; 8]);
    let longbow = put_object(&mut state, PlayerId::P0, "Viridian Longbow", Zone::Battlefield);
    let elf = put_object(&mut state, PlayerId::P0, "Llanowar Elves", Zone::Battlefield);
    attach_exact_for_test(&mut state, longbow, elf);
    let snuff_out = put_object(&mut state, PlayerId::P0, "Snuff Out", Zone::Hand);
    // Cast for the printed {3}{B} cost (floating mana, same shape as
    // `pauper_meta_w1_snuff_out_and_duals.rs`'s own test) so no Swamp is
    // needed.
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 3;

    engine::step(&mut state, Action::ActivateAbility(elf, 0)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(PlayerId::P1)));
        }
        other => panic!("expected ChooseTargets, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Player(PlayerId::P1))).unwrap();

    // In response, destroy the equipped creature (the ping's own source)
    // while it still sits on the stack.
    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass {
            player,
            castable_spells,
            ..
        } => {
            assert_eq!(player, PlayerId::P0);
            assert!(castable_spells.contains(&snuff_out));
            assert_eq!(state.stack.len(), 1, "the ping is on the stack");
        }
        other => panic!("expected CastSpellOrPass, got {other:?}"),
    }
    engine::step(&mut state, Action::CastSpell(snuff_out)).unwrap();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(elf)));
        }
        other => panic!("expected ChooseTargets, got {other:?}"),
    }
    engine::step(&mut state, Action::ChooseTarget(Target::Object(elf))).unwrap();

    resolve_until_idle(&mut state);

    assert!(
        state.engine.halted.is_none(),
        "the engine must not halt: {:?}",
        state.engine.halted
    );
    assert_eq!(
        state.objects.get(elf).zone,
        Zone::Graveyard,
        "the equipped creature was destroyed before the ping resolved"
    );
    assert_eq!(
        state.players[1].life, 19,
        "the ping still resolves for 1 damage via last-known information \
         even though its own source creature already died"
    );
}
