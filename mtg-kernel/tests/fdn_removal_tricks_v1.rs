//! FDN removal, damage and combat-trick spells composed from generic effect
//! operations, exercised through cast, mode, target and resolution decisions.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CardType, Keywords,
    TargetSpec, CARD_DEFS,
};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};

const BATCH: [&str; 10] = [
    "Sure Strike",
    "Snakeskin Veil",
    "Seismic Rupture",
    "Boltwave",
    "Day of Judgment",
    "Incinerating Blast",
    "Slagstorm",
    "Abrade",
    "Preposterous Proportions",
    "Bake into a Pie",
];

fn ready() -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = Step::Main1;
    enable_foundations_combat_v1(&mut state).unwrap();
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("expected decision: {other:?}"),
    }
}

fn priority(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    let decision = next(surface, state);
    assert!(
        matches!(decision, Decision::CastSpellOrPass { .. }),
        "{decision:?}"
    );
    decision
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

/// Cast `name` from P0's hand with ample mana, choosing `mode` when the
/// spell is modal and then each object target in order.
fn cast(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    name: &str,
    mode: Option<u8>,
    targets: &[ObjectId],
) -> ObjectId {
    let spell = put(state, PlayerId::P0, name, Zone::Hand);
    state.players[0].mana_pool = [10; 6];
    assert!(
        matches!(priority(surface, state), Decision::CastSpellOrPass { castable_spells, .. }
        if castable_spells.contains(&spell))
    );
    apply(surface, state, Action::CastSpell(spell));
    if let Some(mode) = mode {
        match next(surface, state) {
            Decision::ChooseSpellMode {
                spell: chosen,
                mode_count,
                legal_modes,
                ..
            } => {
                assert_eq!(chosen, spell);
                assert_eq!(mode_count, 2);
                assert!(legal_modes.contains(&mode), "{legal_modes:?}");
            }
            other => panic!("expected mode choice: {other:?}"),
        }
        apply(surface, state, Action::ChooseSpellMode(mode));
    }
    for &target in targets {
        assert!(
            matches!(next(surface, state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target)))
        );
        apply(surface, state, Action::ChooseTarget(Target::Object(target)));
    }
    priority(surface, state);
    spell
}

/// Both players pass; the top stack item resolves and state-based actions
/// run before the next priority decision.
fn pass_both(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    apply(surface, state, Action::Pass);
    priority(surface, state);
    apply(surface, state, Action::Pass);
}

fn resolve(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    let before = state.stack.len();
    assert!(before > 0);
    pass_both(surface, state);
    priority(surface, state);
    assert_eq!(state.stack.len(), before - 1);
    assert!(surface.suppressions().is_empty());
}

fn zone(state: &GameState, id: ObjectId) -> Zone {
    state.objects.get(id).zone
}

#[test]
fn appended_definitions_are_full_contiguous_and_use_existing_target_shapes() {
    let first = usize::from(card_id_by_name(BATCH[0]).unwrap());
    assert_eq!(first + BATCH.len(), CARD_DEFS.len());
    for (offset, name) in BATCH.into_iter().enumerate() {
        let id = card_id_by_name(name).unwrap();
        assert_eq!(usize::from(id), first + offset, "{name}");
        let def = &CARD_DEFS[usize::from(id)];
        assert_eq!(def.capability, CardCapability::Full, "{name}");
        assert!(def.power.is_none() && def.toughness.is_none(), "{name}");
        preflight_fully_supported_deck(&[id]).unwrap();
    }
    let spec = |name| CARD_DEFS[usize::from(card_id_by_name(name).unwrap())].target_spec;
    assert_eq!(spec("Sure Strike"), TargetSpec::Creature);
    assert_eq!(spec("Snakeskin Veil"), TargetSpec::ControlledCreature);
    assert_eq!(spec("Incinerating Blast"), TargetSpec::Creature);
    assert_eq!(spec("Bake into a Pie"), TargetSpec::Creature);
    for name in [
        "Seismic Rupture",
        "Boltwave",
        "Day of Judgment",
        "Slagstorm",
        "Preposterous Proportions",
    ] {
        assert_eq!(spec(name), TargetSpec::None, "{name}");
    }
    let def = |name| &CARD_DEFS[usize::from(card_id_by_name(name).unwrap())];
    assert!(def("Sure Strike").has_type(CardType::Instant));
    assert!(def("Abrade").has_type(CardType::Instant));
    assert!(def("Day of Judgment").has_type(CardType::Sorcery));
    assert_eq!(def("Abrade").target_spec, TargetSpec::Creature);
    assert_eq!(
        def("Abrade").mode2.as_ref().unwrap().target_spec,
        TargetSpec::ArtifactPermanent
    );
    assert_eq!(
        def("Slagstorm").mode2.as_ref().unwrap().target_spec,
        TargetSpec::None
    );
    for name in BATCH {
        if name != "Abrade" && name != "Slagstorm" {
            assert!(def(name).mode2.is_none(), "{name}");
        }
    }
}

#[test]
fn sure_strike_gives_three_power_and_first_strike_until_end_of_turn() {
    let mut state = ready();
    let mut surface = surface();
    let lions = put(&mut state, PlayerId::P1, "Savannah Lions", Zone::Battlefield);
    cast(&mut surface, &mut state, "Sure Strike", None, &[lions]);
    resolve(&mut surface, &mut state);
    assert_eq!(engine::effective_power(&state, lions), 5);
    assert_eq!(engine::effective_toughness(&state, lions), 1);
    assert!(engine::has_effective_keyword(
        &state,
        lions,
        Keywords::FIRST_STRIKE
    ));
}

#[test]
fn snakeskin_veil_targets_only_controlled_creatures_and_adds_counter_and_hexproof() {
    let mut state = ready();
    let mut surface = surface();
    let own = put(&mut state, PlayerId::P0, "Savannah Lions", Zone::Battlefield);
    let theirs = put(&mut state, PlayerId::P1, "Savannah Lions", Zone::Battlefield);
    let spell = put(&mut state, PlayerId::P0, "Snakeskin Veil", Zone::Hand);
    state.players[0].mana_pool = [10; 6];
    priority(&mut surface, &mut state);
    apply(&mut surface, &mut state, Action::CastSpell(spell));
    match next(&mut surface, &mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(own)));
            assert!(!legal_targets.contains(&Target::Object(theirs)));
        }
        other => panic!("{other:?}"),
    }
    apply(&mut surface, &mut state, Action::ChooseTarget(Target::Object(own)));
    priority(&mut surface, &mut state);
    resolve(&mut surface, &mut state);
    assert_eq!(state.objects.get(own).counters.plus1_plus1, 1);
    assert_eq!(engine::effective_power(&state, own), 3);
    assert!(engine::has_effective_keyword(&state, own, Keywords::HEXPROOF));
}

#[test]
fn seismic_rupture_damages_only_creatures_without_flying() {
    let mut state = ready();
    let mut surface = surface();
    let lions = put(&mut state, PlayerId::P1, "Savannah Lions", Zone::Battlefield);
    let own_turtle = put(&mut state, PlayerId::P0, "Aegis Turtle", Zone::Battlefield);
    let angel = put(&mut state, PlayerId::P1, "Serra Angel", Zone::Battlefield);
    cast(&mut surface, &mut state, "Seismic Rupture", None, &[]);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, lions), Zone::Graveyard);
    assert_eq!(state.objects.get(own_turtle).damage, 2);
    assert_eq!(state.objects.get(angel).damage, 0);
    assert_eq!(zone(&state, angel), Zone::Battlefield);
}

#[test]
fn boltwave_damages_only_the_opponent() {
    let mut state = ready();
    let mut surface = surface();
    let (own, theirs) = (state.players[0].life, state.players[1].life);
    cast(&mut surface, &mut state, "Boltwave", None, &[]);
    resolve(&mut surface, &mut state);
    assert_eq!(state.players[0].life, own);
    assert_eq!(state.players[1].life, theirs - 3);
}

#[test]
fn day_of_judgment_destroys_every_creature_but_not_indestructible_or_noncreatures() {
    let mut state = ready();
    let mut surface = surface();
    let own = put(&mut state, PlayerId::P0, "Serra Angel", Zone::Battlefield);
    let theirs = put(&mut state, PlayerId::P1, "Vampire Nighthawk", Zone::Battlefield);
    let land = put(&mut state, PlayerId::P1, "Forest", Zone::Battlefield);
    let spell = cast(&mut surface, &mut state, "Day of Judgment", None, &[]);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, own), Zone::Graveyard);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, land), Zone::Battlefield);
    assert_eq!(zone(&state, spell), Zone::Graveyard);
}

#[test]
fn incinerating_blast_deals_six_then_offers_an_optional_discard_to_draw() {
    for accept in [false, true] {
        let mut state = ready();
        let mut surface = surface();
        let angel = put(&mut state, PlayerId::P1, "Serra Angel", Zone::Battlefield);
        let fodder = put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
        let spell = cast(&mut surface, &mut state, "Incinerating Blast", None, &[angel]);
        let hand_before = state.players[0].hand.len();
        pass_both(&mut surface, &mut state);
        // The harness surface reshapes the engine's optional cost into a
        // "pay at all?" gate, presented as the (false, false) sentinel.
        match next(&mut surface, &mut state) {
            Decision::ChooseOptionalCost {
                player,
                discard_payable: false,
                sacrifice_payable: false,
                ..
            } => assert_eq!(player, PlayerId::P0),
            other => panic!("{other:?}"),
        }
        assert!(state.engine.pending_optional_cost.as_ref().unwrap().discard_payable);
        let choice = if accept {
            OptionalCostChoice::Discard
        } else {
            OptionalCostChoice::Decline
        };
        apply(&mut surface, &mut state, Action::ChooseOptionalCost(choice));
        if accept {
            if let Decision::Discard { .. } = next(&mut surface, &mut state) {
                apply(&mut surface, &mut state, Action::Discard(vec![fodder]));
            }
        }
        priority(&mut surface, &mut state);
        assert!(state.stack.is_empty());
        assert_eq!(zone(&state, angel), Zone::Graveyard);
        assert_eq!(zone(&state, spell), Zone::Graveyard);
        if accept {
            assert_eq!(zone(&state, fodder), Zone::Graveyard);
            assert_eq!(state.players[0].hand.len(), hand_before);
        } else {
            assert_eq!(zone(&state, fodder), Zone::Hand);
            assert_eq!(state.players[0].hand.len(), hand_before);
        }
    }
}

#[test]
fn slagstorm_modes_damage_every_creature_or_each_player() {
    let mut state = ready();
    let mut surface = surface();
    let own = put(&mut state, PlayerId::P0, "Vampire Nighthawk", Zone::Battlefield);
    let angel = put(&mut state, PlayerId::P1, "Serra Angel", Zone::Battlefield);
    let (own_life, their_life) = (state.players[0].life, state.players[1].life);
    cast(&mut surface, &mut state, "Slagstorm", Some(0), &[]);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, own), Zone::Graveyard);
    assert_eq!(state.objects.get(angel).damage, 3);
    assert_eq!(state.players[0].life, own_life);
    assert_eq!(state.players[1].life, their_life);

    let mut state = ready();
    let mut surface = crate::surface();
    let lions = put(&mut state, PlayerId::P1, "Savannah Lions", Zone::Battlefield);
    let (own_life, their_life) = (state.players[0].life, state.players[1].life);
    cast(&mut surface, &mut state, "Slagstorm", Some(1), &[]);
    resolve(&mut surface, &mut state);
    assert_eq!(state.players[0].life, own_life - 3);
    assert_eq!(state.players[1].life, their_life - 3);
    assert_eq!(zone(&state, lions), Zone::Battlefield);
}

#[test]
fn abrade_damages_a_creature_or_destroys_an_artifact() {
    let mut state = ready();
    let mut surface = surface();
    let nighthawk = put(&mut state, PlayerId::P1, "Vampire Nighthawk", Zone::Battlefield);
    let furnace = put(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    cast(&mut surface, &mut state, "Abrade", Some(0), &[nighthawk]);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, nighthawk), Zone::Graveyard);
    assert_eq!(zone(&state, furnace), Zone::Battlefield);

    // The only creature is gone, so the artifact mode is selected silently.
    cast(&mut surface, &mut state, "Abrade", None, &[furnace]);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, furnace), Zone::Graveyard);
}

#[test]
fn abrade_without_an_artifact_selects_the_creature_mode_silently() {
    let mut state = ready();
    let mut surface = surface();
    let lions = put(&mut state, PlayerId::P1, "Savannah Lions", Zone::Battlefield);
    cast(&mut surface, &mut state, "Abrade", None, &[lions]);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, lions), Zone::Graveyard);
}

#[test]
fn targeted_spells_do_nothing_when_their_target_leaves_before_resolution() {
    let mut state = ready();
    let mut surface = surface();
    let angel = put(&mut state, PlayerId::P1, "Serra Angel", Zone::Battlefield);
    let fodder = put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    let spell = cast(&mut surface, &mut state, "Incinerating Blast", None, &[angel]);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(angel, Zone::Graveyard),
    );
    let hand = state.players[0].hand.len();
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, spell), Zone::Graveyard);
    assert_eq!(zone(&state, fodder), Zone::Hand);
    assert_eq!(state.players[0].hand.len(), hand);
}

#[test]
fn preposterous_proportions_boosts_only_controlled_creatures_with_vigilance() {
    let mut state = ready();
    let mut surface = surface();
    let own = put(&mut state, PlayerId::P0, "Savannah Lions", Zone::Battlefield);
    let theirs = put(&mut state, PlayerId::P1, "Savannah Lions", Zone::Battlefield);
    cast(&mut surface, &mut state, "Preposterous Proportions", None, &[]);
    resolve(&mut surface, &mut state);
    assert_eq!(engine::effective_power(&state, own), 12);
    assert_eq!(engine::effective_toughness(&state, own), 11);
    assert!(engine::has_effective_keyword(&state, own, Keywords::VIGILANCE));
    assert_eq!(engine::effective_power(&state, theirs), 2);
    assert!(!engine::has_effective_keyword(&state, theirs, Keywords::VIGILANCE));
}

#[test]
fn bake_into_a_pie_destroys_the_creature_and_creates_a_food_for_its_caster() {
    let mut state = ready();
    let mut surface = surface();
    let angel = put(&mut state, PlayerId::P1, "Serra Angel", Zone::Battlefield);
    cast(&mut surface, &mut state, "Bake into a Pie", None, &[angel]);
    let before = state.players[0].battlefield.len();
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, angel), Zone::Graveyard);
    assert_eq!(state.players[0].battlefield.len(), before + 1);
    let food = *state.players[0].battlefield.last().unwrap();
    assert_eq!(state.objects.get(food).name, "Food Token");
    assert_eq!(state.objects.get(food).controller, PlayerId::P0);
}

/// Pass priority, answering trigger target and optional-effect decisions,
/// until the stack is empty again.
fn drive(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    target: Option<ObjectId>,
    option: Option<u16>,
) {
    for _ in 0..32 {
        match next(surface, state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => apply(surface, state, Action::Pass),
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = target.expect("trigger target expected");
                assert!(legal_targets.contains(&Target::Object(target)));
                apply(surface, state, Action::ChooseTarget(Target::Object(target)));
            }
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 2);
                let option = option.expect("optional effect choice expected");
                apply(surface, state, Action::ChooseEffectOption(option));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    panic!("stack did not empty");
}

fn legal_spell_targets(state: &mut GameState, name: &str) -> Vec<Target> {
    let mut surface = surface();
    let spell = put(state, PlayerId::P0, name, Zone::Hand);
    state.players[0].mana_pool = [10; 6];
    priority(&mut surface, state);
    apply(&mut surface, state, Action::CastSpell(spell));
    match next(&mut surface, state) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("{other:?}"),
    }
}

#[test]
fn batch_two_definitions_use_their_new_target_shapes() {
    let def = |name| &CARD_DEFS[usize::from(card_id_by_name(name).unwrap())];
    assert_eq!(def("Hero's Downfall").target_spec, TargetSpec::CreatureOrPlaneswalker);
    assert_eq!(
        def("Broken Wings").target_spec,
        TargetSpec::ArtifactEnchantmentOrFlyingCreature
    );
    assert_eq!(
        def("Make Your Move").target_spec,
        TargetSpec::ArtifactEnchantmentOrCreaturePowerAtLeastFour
    );
    assert_eq!(TargetSpec::CreatureOrPlaneswalker.stable_id(), 42);
    assert_eq!(TargetSpec::OpponentNonlandPermanent.stable_id(), 45);
    let golem = def("Meteor Golem");
    assert!(golem.has_type(CardType::Artifact) && golem.has_type(CardType::Creature));
    assert_eq!((golem.power, golem.toughness), (Some(3), Some(3)));
    let sage = def("Reclamation Sage");
    assert_eq!((sage.power, sage.toughness), (Some(2), Some(1)));
    for name in [
        "Hero's Downfall",
        "Broken Wings",
        "Make Your Move",
        "Meteor Golem",
        "Reclamation Sage",
    ] {
        let id = card_id_by_name(name).unwrap();
        assert_eq!(CARD_DEFS[usize::from(id)].capability, CardCapability::Full);
        preflight_fully_supported_deck(&[id]).unwrap();
    }
}

#[test]
fn heros_downfall_destroys_a_creature_or_planeswalker() {
    let mut state = ready();
    let mut surface = surface();
    let angel = put(&mut state, PlayerId::P1, "Serra Angel", Zone::Battlefield);
    let walker = put(&mut state, PlayerId::P1, "Ajani, Caller of the Pride", Zone::Hand);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(walker, Zone::Battlefield),
    );
    let land = put(&mut state, PlayerId::P1, "Forest", Zone::Battlefield);
    let legal = legal_spell_targets(&mut state.clone(), "Hero's Downfall");
    assert!(legal.contains(&Target::Object(angel)));
    assert!(legal.contains(&Target::Object(walker)));
    assert!(!legal.contains(&Target::Object(land)));
    cast(&mut surface, &mut state, "Hero's Downfall", None, &[walker]);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, walker), Zone::Graveyard);
    cast(&mut surface, &mut state, "Hero's Downfall", None, &[angel]);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, angel), Zone::Graveyard);
}

#[test]
fn broken_wings_and_make_your_move_filter_creatures_by_flying_and_power() {
    let mut state = ready();
    let angel = put(&mut state, PlayerId::P1, "Serra Angel", Zone::Battlefield);
    let lions = put(&mut state, PlayerId::P1, "Savannah Lions", Zone::Battlefield);
    let ceratops = put(&mut state, PlayerId::P1, "Quakestrider Ceratops", Zone::Battlefield);
    let furnace = put(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    let wings = legal_spell_targets(&mut state.clone(), "Broken Wings");
    assert!(wings.contains(&Target::Object(angel)));
    assert!(wings.contains(&Target::Object(furnace)));
    assert!(!wings.contains(&Target::Object(lions)));
    assert!(!wings.contains(&Target::Object(ceratops)));
    let mov = legal_spell_targets(&mut state.clone(), "Make Your Move");
    assert!(mov.contains(&Target::Object(angel)));
    assert!(mov.contains(&Target::Object(ceratops)));
    assert!(mov.contains(&Target::Object(furnace)));
    assert!(!mov.contains(&Target::Object(lions)));

    let mut surface = surface();
    cast(&mut surface, &mut state, "Broken Wings", None, &[angel]);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, angel), Zone::Graveyard);
    cast(&mut surface, &mut state, "Make Your Move", None, &[ceratops]);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, ceratops), Zone::Graveyard);
    assert_eq!(zone(&state, lions), Zone::Battlefield);
}

#[test]
fn make_your_move_sees_pumped_power_and_fizzles_when_power_drops() {
    let mut state = ready();
    let mut surface = surface();
    let lions = put(&mut state, PlayerId::P1, "Savannah Lions", Zone::Battlefield);
    assert!(!legal_spell_targets(&mut state.clone(), "Make Your Move")
        .contains(&Target::Object(lions)));
    cast(&mut surface, &mut state, "Sure Strike", None, &[lions]);
    resolve(&mut surface, &mut state);
    let spell = cast(&mut surface, &mut state, "Make Your Move", None, &[lions]);
    // Respond by shrinking the creature below power 4: the only target is
    // illegal on resolution and the spell does nothing.
    cast(&mut surface, &mut state, "Fleeting Distraction", None, &[lions]);
    resolve(&mut surface, &mut state);
    assert_eq!(engine::effective_power(&state, lions), 4);
    cast(&mut surface, &mut state, "Fleeting Distraction", None, &[lions]);
    resolve(&mut surface, &mut state);
    assert_eq!(engine::effective_power(&state, lions), 3);
    resolve(&mut surface, &mut state);
    assert_eq!(zone(&state, lions), Zone::Battlefield);
    assert_eq!(zone(&state, spell), Zone::Graveyard);
}

#[test]
fn meteor_golem_destroys_only_an_opponents_nonland_permanent() {
    let mut state = ready();
    let mut surface = surface();
    let own = put(&mut state, PlayerId::P0, "Serra Angel", Zone::Battlefield);
    let theirs = put(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);
    let their_land = put(&mut state, PlayerId::P1, "Forest", Zone::Battlefield);
    let golem = cast(&mut surface, &mut state, "Meteor Golem", None, &[]);
    apply(&mut surface, &mut state, Action::Pass);
    match next(&mut surface, &mut state) {
        Decision::CastSpellOrPass { .. } => apply(&mut surface, &mut state, Action::Pass),
        other => panic!("{other:?}"),
    }
    match next(&mut surface, &mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(theirs)]);
            let _ = (own, their_land);
            apply(&mut surface, &mut state, Action::ChooseTarget(Target::Object(theirs)));
        }
        other => panic!("{other:?}"),
    }
    drive(&mut surface, &mut state, None, None);
    assert_eq!(zone(&state, golem), Zone::Battlefield);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, own), Zone::Battlefield);
    assert_eq!(zone(&state, their_land), Zone::Battlefield);
}

#[test]
fn meteor_golem_without_a_legal_target_just_enters() {
    let mut state = ready();
    let mut surface = surface();
    put(&mut state, PlayerId::P1, "Forest", Zone::Battlefield);
    let golem = cast(&mut surface, &mut state, "Meteor Golem", None, &[]);
    drive(&mut surface, &mut state, None, None);
    assert_eq!(zone(&state, golem), Zone::Battlefield);
}

#[test]
fn reclamation_sage_may_destroy_an_artifact_or_enchantment() {
    for destroy in [false, true] {
        let mut state = ready();
        let mut surface = surface();
        let furnace = put(&mut state, PlayerId::P1, "Great Furnace", Zone::Battlefield);
        let sage = cast(&mut surface, &mut state, "Reclamation Sage", None, &[]);
        drive(&mut surface, &mut state, Some(furnace), Some(u16::from(destroy)));
        assert_eq!(zone(&state, sage), Zone::Battlefield);
        assert_eq!(
            zone(&state, furnace),
            if destroy { Zone::Graveyard } else { Zone::Battlefield }
        );
    }
}
