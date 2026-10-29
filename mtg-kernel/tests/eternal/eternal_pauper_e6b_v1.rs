//! Eternal wave E6b: native rules tests for the cards that complete Pauper
//! Bogles, White Aggro, Rakdos Madness, Boros Moxite, Mono-White Aggro,
//! Snacker Gates and Boros Synthesizer.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, AltCostCondition, CardCapability, CardDef, Keywords, Subtype, Supertype,
    TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E6B_CARDS: [&str; 48] = [
    "Abundant Growth",
    "Alms of the Vein",
    "Ancestral Mask",
    "Angelic Gift",
    "Armadillo Cloak",
    "Asgardian Citadel",
    "Ash Barrens",
    "Battle Screech",
    "Benevolent Blessing",
    "Boros Garrison",
    "Bushwhack",
    "Cartouche of Solidarity",
    "Cliffgate",
    "Dawnbringer Cleric",
    "Demand Answers",
    "Eagles of the North",
    "Ethereal Armor",
    "Fortress Kin-Guard",
    "Gladecover Scout",
    "Greatsword of Tyr",
    "Guardians' Pledge",
    "Heliod's Pilgrim",
    "Idyllic Grange",
    "Jagged Barrens",
    "Kitchen Imp",
    "Leonardo, Big Brother",
    "Lunarch Veteran",
    "Malevolent Rumble",
    "Mana Tithe",
    "Manor Gate",
    "Martyr's Soul",
    "Pursue the Past",
    "Raffine's Informant",
    "Ragged Short Spear",
    "Rakdos Carnarium",
    "Ramosian Rally",
    "Rancor",
    "Salt Road Packbeast",
    "Search Party Captain",
    "Sheltering Landscape",
    "Silhana Ledgewalker",
    "Slippery Bogle",
    "Spider-Man, Web-Slinger",
    "Spirit Link",
    "Thraben Inspector",
    "Utopia Sprawl",
    "Vampire's Kiss",
    "Wind-Scarred Crag",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4536_4221_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn plains() -> GameState {
    game(&["Plains"; 20])
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
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
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(object),
        Zone::Battlefield => seat.battlefield.push(object),
        Zone::Graveyard => seat.graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Library => seat.library.insert(0, object),
        _ => panic!("helper zone"),
    }
    object
}

fn pool(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[ManaColor::C.pool_index()] += colorless;
}

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
}

fn named(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    [P0, P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            let live = state.objects.get(object);
            live.controller == player && CARD_DEFS[live.card_def as usize].name == name
        })
        .collect()
}

fn power_toughness(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
}

fn keyword(state: &GameState, object: ObjectId, keyword: Keywords) -> bool {
    engine::has_effective_keyword(state, object, keyword)
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(
        !matches!(decision, Decision::Halted { .. }),
        "{decision:?}\nstack: {:#?}\npending triggers: {:#?}",
        state.stack,
        state.engine.pending_triggers
    );
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => return Some(other),
        }
    }
    panic!("did not settle");
}

fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice: {other:?}");
    }
}

/// The first legal answer to any choice: the first target or option,
/// finishing a selection that may finish, declining a Boolean or kicker.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(legal_targets[0]),
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(false),
        Decision::ChooseCostTargets { candidates, .. } => Action::ChooseCostTarget(candidates[0]),
        Decision::ChooseCastMode { options, .. } => Action::ChooseCastMode(options[0]),
        Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
        Decision::ChooseSpellMode { legal_modes, .. } => Action::ChooseSpellMode(legal_modes[0]),
        Decision::ChooseTriggerMode { legal_modes, .. } => {
            Action::ChooseTriggerMode(legal_modes[0])
        }
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Decline)
        }
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => panic!("unexpected choice: {other:?}"),
    }
}

/// Settles, answering every choice with `answer`.
fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
    }
}

fn settle_first(state: &mut GameState) {
    settle_with(state, |_, decision| first_answer(decision));
}

/// Settles, choosing `object` whenever a selection or target offers it.
fn settle_choosing(state: &mut GameState, object: ObjectId) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(object)) =>
        {
            Action::ChooseTarget(Target::Object(object))
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(object)) =>
        {
            Action::ChooseEffectTarget(Target::Object(object))
        }
        other => first_answer(other),
    });
}

/// Settles, answering effect options with `option` and otherwise with the
/// first legal answer.
fn settle_option(state: &mut GameState, option: u16) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
        other => first_answer(other),
    });
}

fn priority(state: &mut GameState) -> (Vec<ObjectId>, Vec<(ObjectId, u8)>) {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells,
            activatable_abilities,
            ..
        } => (castable_spells, activatable_abilities),
        other => panic!("expected priority, got {other:?}"),
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    priority(state).0.contains(&spell)
}

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).1.contains(&(source, index))
}

/// Casts `spell`, answering each target prompt from `targets` in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    for target in targets {
        choose_target(state, *target);
    }
}

fn choose_target(state: &mut GameState, target: Target) {
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseTarget(target));
        }
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseEffectTarget(target));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

/// The legal targets the first target prompt of casting `spell` offers;
/// the cast is then abandoned with the state restored.
fn cast_targets(state: &GameState, spell: ObjectId) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(castable(&mut probe, spell));
    act(&mut probe, Action::CastSpell(spell));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Activates `source`'s ability `index`, answering each target prompt.
fn activate(state: &mut GameState, source: ObjectId, index: u8, targets: &[Target]) {
    assert!(
        activatable(state, source, index),
        "{} ability {index} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, index));
    for target in targets {
        choose_target(state, *target);
    }
}

/// Plays on until `stop` holds before a decision: passing priority, not
/// attacking or blocking, ordering triggers as given and answering any
/// other choice with `answer` (or the first legal answer).
fn run_until(
    state: &mut GameState,
    stop: impl Fn(&GameState) -> bool,
    mut answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    for _ in 0..2000 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        if let Some(action) = answer(state, &decision) {
            act(state, action);
            continue;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseCombatDamageRange { .. } => {
                Action::ChooseCombatDamageRange { upper_half: true }
            }
            Decision::ChooseAttackTarget {
                player, candidates, ..
            } => Action::ChooseAttackTarget(
                if candidates.contains(&Target::Player(player.opponent())) {
                    Target::Player(player.opponent())
                } else {
                    candidates[0]
                },
            ),
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

/// Plays on with first answers until `stop` holds.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    run_until(state, stop, |_, _| None);
}

/// P0 attacks with `attackers` in this turn's combat, then the game plays
/// on to P0's second main phase with an empty stack. `answer` answers other
/// choices.
fn attack_with(
    state: &mut GameState,
    attackers: Vec<ObjectId>,
    answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(attackers));
    run_until(
        state,
        |state| {
            state.step == Step::Main2
                && state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && state.engine.pending_effect.is_none()
        },
        answer,
    );
}

/// P0 attacks with `attacker`; the creatures P1 may block it with.
fn legal_blockers(state: &mut GameState, attacker: ObjectId) -> Vec<ObjectId> {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(vec![attacker]));
    for _ in 0..50 {
        match next(state) {
            Decision::DeclareBlockers { legal_blockers, .. } => {
                return legal_blockers
                    .into_iter()
                    .find(|(candidate, _)| *candidate == attacker)
                    .map(|(_, blockers)| blockers)
                    .unwrap_or_default();
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("expected blockers, got {other:?}"),
        }
    }
    panic!("no declare blockers decision");
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

/// Puts `aura` from P0's hand onto `host` by casting it.
fn enchant(state: &mut GameState, aura: ObjectId, host: ObjectId) {
    cast(state, aura, &[Target::Object(host)]);
    settle_first(state);
    assert_eq!(zone(state, aura), Zone::Battlefield);
    assert_eq!(
        state
            .objects
            .get(aura)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(host)
    );
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e6b_cards_are_full_with_their_printed_characteristics() {
    for name in E6B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    for (token, subtype, keyword) in [
        ("Bird Token", Subtype::Bird, Keywords::FLYING),
        ("White Warrior Token", Subtype::Warrior, Keywords::VIGILANCE),
        ("White Spirit Token", Subtype::Spirit, Keywords::NONE),
    ] {
        let token_def = def(token);
        assert_eq!(token_def.capability, CardCapability::Full, "{token}");
        assert!(token_def.is_token, "{token}");
        assert_eq!(token_def.subtypes, &[subtype], "{token}");
        assert_eq!(token_def.colors, &[ManaColor::W], "{token}");
        assert_eq!((token_def.power, token_def.toughness), (Some(1), Some(1)));
        assert_eq!(token_def.keywords, keyword, "{token}");
    }
    assert!(Subtype::Imp.is_creature_type());
    assert!(!Subtype::Cartouche.is_creature_type());
    assert_eq!(def("Kitchen Imp").subtypes, &[Subtype::Imp]);
    assert_eq!(
        def("Cartouche of Solidarity").subtypes,
        &[Subtype::Aura, Subtype::Cartouche]
    );
    for hexproof in ["Gladecover Scout", "Slippery Bogle", "Silhana Ledgewalker"] {
        assert!(def(hexproof).keywords.has(Keywords::HEXPROOF), "{hexproof}");
    }
    let imp = def("Kitchen Imp");
    assert!(imp.keywords.has(Keywords::FLYING) && imp.keywords.has(Keywords::HASTE));
    assert!(imp.madness_cost.is_some());
    assert!(def("Alms of the Vein").madness_cost.is_some());
    assert!(def("Eagles of the North").keywords.has(Keywords::FLYING));
    assert!(def("Benevolent Blessing").keywords.has(Keywords::FLASH));
    for aura in [
        "Angelic Gift",
        "Armadillo Cloak",
        "Ancestral Mask",
        "Ethereal Armor",
        "Rancor",
        "Spirit Link",
        "Benevolent Blessing",
    ] {
        assert!(def(aura).attachment.is_some(), "{aura}");
    }
    for equipment in ["Greatsword of Tyr", "Ragged Short Spear"] {
        assert!(def(equipment).equipment.is_some(), "{equipment}");
        assert_eq!(def(equipment).activated_abilities.len(), 1, "equip");
    }
    for (name, condition) in [
        ("Leonardo, Big Brother", AltCostCondition::SneakV1),
        ("Spider-Man, Web-Slinger", AltCostCondition::WebSlingingV1),
        ("Lunarch Veteran", AltCostCondition::DisturbV1),
    ] {
        assert_eq!(def(name).alt_cost.map(|alt| alt.condition), Some(condition));
    }
    for legendary in ["Leonardo, Big Brother", "Spider-Man, Web-Slinger"] {
        assert_eq!(def(legendary).supertypes, &[Supertype::Legendary]);
    }
    let veteran = def("Lunarch Veteran");
    let phantom = veteran.transform_face.as_ref().expect("back face");
    assert_eq!(phantom.name, "Luminous Phantom");
    assert_eq!(phantom.subtypes, &[Subtype::Spirit, Subtype::Cleric]);
    assert!(phantom.keywords.has(Keywords::FLYING));
    assert!(def("Battle Screech").flashback.is_some());
    assert!(def("Pursue the Past").flashback.is_some());
    assert!(def("Demand Answers").additional_cost.is_some());
    assert!(def("Demand Answers").alt_cost.is_some());
    assert!(def("Martyr's Soul").alt_cost.is_some(), "convoke");
    assert!(def("Ramosian Rally").alt_cost.is_some());
    assert!(def("Salt Road Packbeast").generic_cost_reduction.is_some());
    assert!(def("Search Party Captain").generic_cost_reduction.is_some());
    assert_eq!(def("Jagged Barrens").subtypes, &[Subtype::Desert]);
    assert_eq!(def("Idyllic Grange").subtypes, &[Subtype::Plains]);
    for gate in ["Cliffgate", "Manor Gate"] {
        assert_eq!(def(gate).subtypes, &[Subtype::Gate], "{gate}");
        assert!(def(gate).enters_battlefield_tapped, "{gate}");
        assert!(def(gate).mana_ability_includes_chosen_color, "{gate}");
    }
    assert_eq!(
        def("Cliffgate").as_enters_choose_color_other_than,
        Some(ManaColor::R)
    );
    assert_eq!(
        def("Manor Gate").as_enters_choose_color_other_than,
        Some(ManaColor::G)
    );
    assert_eq!(
        def("Utopia Sprawl").target_spec,
        TargetSpec::StandardV1(mtg_kernel::standard_cards_v1::StandardTargetV1::ForestV1)
    );
    assert_eq!(def("Abundant Growth").target_spec, TargetSpec::Land);
    assert_eq!(
        def("Cartouche of Solidarity").target_spec,
        TargetSpec::ControlledCreature
    );
}

// ---- Lands ---------------------------------------------------------------------

#[test]
fn gainlands_enter_tapped_gain_one_and_tap_for_two_colors() {
    for name in ["Asgardian Citadel", "Wind-Scarred Crag"] {
        let mut state = plains();
        let land = put(&mut state, P0, name, Zone::Hand);
        next(&mut state);
        act(&mut state, Action::PlayLand(land));
        settled(&mut state);
        assert!(state.objects.get(land).tapped, "{name}");
        assert_eq!(state.players[0].life, 21, "{name}");
        for color in [ManaColor::R, ManaColor::W] {
            let mut state = plains();
            let land = put(&mut state, P0, name, Zone::Battlefield);
            next(&mut state);
            act(&mut state, Action::ActivateManaAbilityChoice(land, color));
            assert_eq!(mana(&state, P0, color), 1, "{name} {color:?}");
        }
    }
}

#[test]
fn jagged_barrens_deals_one_damage_to_target_opponent() {
    let mut state = plains();
    let barrens = put(&mut state, P0, "Jagged Barrens", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(barrens));
    settle_first(&mut state);
    assert!(state.objects.get(barrens).tapped);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(state.players[0].life, 20);
    for color in [ManaColor::B, ManaColor::R] {
        let mut state = plains();
        let land = put(&mut state, P0, "Jagged Barrens", Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(land, color));
        assert_eq!(mana(&state, P0, color), 1);
    }
}

#[test]
fn karoos_return_a_land_and_tap_for_both_colors() {
    for (name, colors) in [
        ("Boros Garrison", [ManaColor::R, ManaColor::W]),
        ("Rakdos Carnarium", [ManaColor::B, ManaColor::R]),
    ] {
        let mut state = plains();
        let plain = put(&mut state, P0, "Plains", Zone::Battlefield);
        let karoo = put(&mut state, P0, name, Zone::Hand);
        next(&mut state);
        act(&mut state, Action::PlayLand(karoo));
        settle_choosing(&mut state, plain);
        assert!(state.objects.get(karoo).tapped, "{name}");
        assert!(hand_contains(&state, P0, plain), "{name}");

        let mut state = plains();
        let karoo = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(karoo));
        for color in colors {
            assert_eq!(mana(&state, P0, color), 1, "{name} {color:?}");
        }
    }
}

#[test]
fn cliffgate_and_manor_gate_choose_a_color_as_they_enter() {
    for (name, fixed, excluded) in [
        ("Cliffgate", ManaColor::R, ManaColor::R),
        ("Manor Gate", ManaColor::G, ManaColor::G),
    ] {
        let mut state = plains();
        let gate = put(&mut state, P0, name, Zone::Hand);
        next(&mut state);
        act(&mut state, Action::PlayLand(gate));
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption {
                option_count: 4,
                ..
            }
        ));
        // The first color other than the excluded one is white.
        act(&mut state, Action::ChooseEffectOption(0));
        assert_eq!(zone(&state, gate), Zone::Battlefield);
        assert!(state.objects.get(gate).tapped);
        assert_eq!(state.objects.get(gate).v4.chosen_color, Some(ManaColor::W));
        assert_ne!(excluded, ManaColor::W);
        state.objects.get_mut(gate).tapped = false;
        next(&mut state);
        for color in [fixed, ManaColor::W] {
            let mut tapped = state.clone();
            act(&mut tapped, Action::ActivateManaAbilityChoice(gate, color));
            assert_eq!(mana(&tapped, P0, color), 1, "{name} {color:?}");
        }
        let other = if fixed == ManaColor::R {
            ManaColor::G
        } else {
            ManaColor::R
        };
        assert!(engine::step(&mut state, Action::ActivateManaAbilityChoice(gate, other)).is_err());
    }
}

#[test]
fn idyllic_grange_enters_untapped_with_three_other_plains_and_grows_a_creature() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let grange = put(&mut state, P0, "Idyllic Grange", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(grange));
    settled(&mut state);
    assert!(state.objects.get(grange).tapped, "too few Plains");
    assert_eq!(power_toughness(&state, inspector), (1, 2));

    let mut state = plains();
    for _ in 0..3 {
        put(&mut state, P0, "Plains", Zone::Battlefield);
    }
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let grange = put(&mut state, P0, "Idyllic Grange", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(grange));
    settle_choosing(&mut state, inspector);
    assert!(!state.objects.get(grange).tapped);
    assert_eq!(power_toughness(&state, inspector), (2, 3));
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(grange));
    assert_eq!(mana(&state, P0, ManaColor::W), 1);
}

#[test]
fn ash_barrens_taps_for_colorless_and_basic_landcycles() {
    let mut state = plains();
    let barrens = put(&mut state, P0, "Ash Barrens", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(barrens));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);

    let mut state = plains();
    let barrens = put(&mut state, P0, "Ash Barrens", Zone::Hand);
    let forest = put(&mut state, P0, "Forest", Zone::Library);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, barrens, 0, &[]);
    settle_choosing(&mut state, forest);
    assert_eq!(zone(&state, barrens), Zone::Graveyard);
    assert!(hand_contains(&state, P0, forest));
}

#[test]
fn sheltering_landscape_fetches_a_basic_mountain_forest_or_plains_and_cycles() {
    let mut state = plains();
    let landscape = put(&mut state, P0, "Sheltering Landscape", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Library);
    activate(&mut state, landscape, 0, &[]);
    settle_choosing(&mut state, forest);
    assert_eq!(zone(&state, landscape), Zone::Graveyard);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert!(state.objects.get(forest).tapped);

    let mut state = plains();
    let landscape = put(&mut state, P0, "Sheltering Landscape", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::G, ManaColor::W],
        0,
    );
    let hand = state.players[0].hand.len();
    activate(&mut state, landscape, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, landscape), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), hand, "cycled for a card");
}

// ---- Hexproof creatures --------------------------------------------------------

#[test]
fn hexproof_creatures_are_targeted_only_by_their_controller() {
    for name in ["Gladecover Scout", "Slippery Bogle", "Silhana Ledgewalker"] {
        let mut state = plains();
        let creature = put(&mut state, P1, name, Zone::Battlefield);
        let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], 0);
        assert!(
            !cast_targets(&state, bolt).contains(&Target::Object(creature)),
            "{name}"
        );
        let own = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
        state.active_player = P1;
        state.priority_player = P1;
        pool(&mut state, P1, &[ManaColor::R], 0);
        assert!(
            cast_targets(&state, own).contains(&Target::Object(creature)),
            "{name}"
        );
    }
}

#[test]
fn slippery_bogle_is_cast_with_green_or_blue() {
    for color in [ManaColor::G, ManaColor::U] {
        let mut state = plains();
        let bogle = put(&mut state, P0, "Slippery Bogle", Zone::Hand);
        pool(&mut state, P0, &[color], 0);
        cast(&mut state, bogle, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, bogle), Zone::Battlefield, "{color:?}");
    }
}

#[test]
fn silhana_ledgewalker_is_blocked_only_by_creatures_with_flying() {
    let mut state = plains();
    let silhana = put(&mut state, P0, "Silhana Ledgewalker", Zone::Battlefield);
    let scout = put(&mut state, P1, "Gladecover Scout", Zone::Battlefield);
    let bird = put(&mut state, P1, "Bird Token", Zone::Battlefield);
    let blockers = legal_blockers(&mut state, silhana);
    assert!(blockers.contains(&bird));
    assert!(!blockers.contains(&scout));
}

// ---- Creatures ------------------------------------------------------------------

#[test]
fn thraben_inspector_investigates() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Battlefield);
    assert_eq!(named(&state, P0, "Clue Token").len(), 1);
}

/// Casts Dawnbringer Cleric and answers its entry trigger with `mode`,
/// targeting `target` when the mode takes one.
fn dawnbringer(state: &mut GameState, mode: u8, target: Option<ObjectId>) {
    let cleric = put(state, P0, "Dawnbringer Cleric", Zone::Hand);
    pool(state, P0, &[ManaColor::W], 1);
    cast(state, cleric, &[]);
    settle_with(state, |_, decision| match decision {
        Decision::ChooseTriggerMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&mode), "{legal_modes:?}");
            Action::ChooseTriggerMode(mode)
        }
        Decision::ChooseTargets { legal_targets, .. } => {
            let target = Target::Object(target.expect("a targeted mode"));
            assert!(legal_targets.contains(&target), "{legal_targets:?}");
            Action::ChooseTarget(target)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(state, cleric), Zone::Battlefield);
}

#[test]
fn dawnbringer_cleric_gains_two_destroys_an_enchantment_or_exiles_a_graveyard_card() {
    let mut state = plains();
    dawnbringer(&mut state, 0, None);
    assert_eq!(state.players[0].life, 22);

    let mut state = plains();
    let journey = put(&mut state, P1, "Journey to Nowhere", Zone::Battlefield);
    dawnbringer(&mut state, 1, Some(journey));
    assert_eq!(zone(&state, journey), Zone::Graveyard);

    let mut state = plains();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    dawnbringer(&mut state, 2, Some(bolt));
    assert_eq!(zone(&state, bolt), Zone::Exile);
}

#[test]
fn eagles_of_the_north_pumps_the_team_and_plainscycles() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let eagles = put(&mut state, P0, "Eagles of the North", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 5);
    cast(&mut state, eagles, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, inspector), (2, 2));
    assert!(keyword(&state, inspector, Keywords::FIRST_STRIKE));
    assert_eq!(power_toughness(&state, eagles), (4, 3));
    assert!(keyword(&state, eagles, Keywords::FLYING));

    let mut state = game(&["Island"; 20]);
    let eagles = put(&mut state, P0, "Eagles of the North", Zone::Hand);
    let plain = put(&mut state, P0, "Plains", Zone::Library);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, eagles, 0, &[]);
    settle_choosing(&mut state, plain);
    assert_eq!(zone(&state, eagles), Zone::Graveyard);
    assert!(hand_contains(&state, P0, plain));
}

#[test]
fn fortress_kin_guard_endures_with_a_counter_or_a_spirit() {
    for option in [0, 1] {
        let mut state = plains();
        let guard = put(&mut state, P0, "Fortress Kin-Guard", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::W], 1);
        cast(&mut state, guard, &[]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 2);
                Action::ChooseEffectOption(option)
            }
            other => first_answer(other),
        });
        if option == 0 {
            assert_eq!(power_toughness(&state, guard), (2, 3));
            assert!(named(&state, P0, "White Spirit Token").is_empty());
        } else {
            assert_eq!(power_toughness(&state, guard), (1, 2));
            let spirits = named(&state, P0, "White Spirit Token");
            assert_eq!(spirits.len(), 1);
            assert_eq!(power_toughness(&state, spirits[0]), (1, 1));
        }
    }
}

#[test]
fn heliods_pilgrim_finds_an_aura() {
    let mut state = plains();
    let rancor = put(&mut state, P0, "Rancor", Zone::Library);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    let pilgrim = put(&mut state, P0, "Heliod's Pilgrim", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, pilgrim, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(rancor)) =>
        {
            assert!(!legal_targets.contains(&Target::Object(bolt)));
            Action::ChooseEffectTarget(Target::Object(rancor))
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, rancor));
}

#[test]
fn kitchen_imp_is_cast_for_its_madness_cost_after_demand_answers_discards_it() {
    let mut state = plains();
    let imp = put(&mut state, P0, "Kitchen Imp", Zone::Hand);
    let demand = put(&mut state, P0, "Demand Answers", Zone::Hand);
    // Demand Answers spends the pool; the Swamp pays the madness cost.
    put(&mut state, P0, "Swamp", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, demand, &[]);
    let mut madness = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::Discard { choices, .. } => {
            assert!(choices.contains(&imp));
            Action::Discard(vec![imp])
        }
        Decision::ChooseMadnessCast { card, .. } => {
            assert_eq!(card, imp);
            madness = true;
            Action::ChooseMadnessCast(true)
        }
        other => first_answer(other),
    });
    assert!(madness);
    assert_eq!(zone(&state, imp), Zone::Battlefield);
    assert!(keyword(&state, imp, Keywords::FLYING));
    assert!(keyword(&state, imp, Keywords::HASTE));
    assert_eq!(zone(&state, demand), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 2, "Demand Answers drew two");
}

#[test]
fn demand_answers_may_sacrifice_an_artifact_instead_of_discarding() {
    let mut state = plains();
    let blood = put(&mut state, P0, "Blood Token", Zone::Battlefield);
    let keep = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let demand = put(&mut state, P0, "Demand Answers", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, demand, &[]);
    let mut modes = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            modes = options;
            Action::ChooseCastMode(CastMode::Alternative)
        }
        Decision::Discard { .. } => panic!("the sacrifice replaces the discard"),
        other => first_answer(other),
    });
    assert!(modes.contains(&CastMode::Normal) && modes.contains(&CastMode::Alternative));
    assert_eq!(zone(&state, blood), Zone::Graveyard);
    assert!(hand_contains(&state, P0, keep));
    assert_eq!(state.players[0].hand.len(), 3);

    // With an empty hand, only the sacrifice is payable.
    let mut state = plains();
    let clue = put(&mut state, P0, "Clue Token", Zone::Battlefield);
    let demand = put(&mut state, P0, "Demand Answers", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, demand, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, clue), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 2);

    // With neither an artifact nor another card, it can't be cast.
    let mut state = plains();
    let demand = put(&mut state, P0, "Demand Answers", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(!castable(&mut state, demand));
}

#[test]
fn leonardo_gets_plus_one_power_for_each_other_creature() {
    let mut state = plains();
    let leonardo = put(&mut state, P0, "Leonardo, Big Brother", Zone::Battlefield);
    assert_eq!(power_toughness(&state, leonardo), (1, 3));
    put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    put(&mut state, P0, "Bird Token", Zone::Battlefield);
    put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    assert_eq!(power_toughness(&state, leonardo), (3, 3));
}

#[test]
fn leonardo_is_sneaked_in_tapped_and_attacking_during_the_declare_blockers_step() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let partner = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let leonardo = put(&mut state, P0, "Leonardo, Big Brother", Zone::Hand);
    put(&mut state, P0, "Plains", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 2);
    assert!(
        castable(&mut state, leonardo),
        "an ordinary main-phase cast"
    );
    let mut sneaked = false;
    attack_with(
        &mut state,
        vec![inspector, partner],
        |state, decision| match decision {
            Decision::CastSpellOrPass {
                player: P0,
                castable_spells,
                ..
            } if state.step == Step::DeclareBlockers && !sneaked => {
                assert!(castable_spells.contains(&leonardo));
                sneaked = true;
                Some(Action::CastSpell(leonardo))
            }
            Decision::CastSpellOrPass {
                player: P0,
                castable_spells,
                ..
            } if state.step == Step::DeclareAttackers => {
                assert!(!castable_spells.contains(&leonardo), "not before blockers");
                None
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                assert!(candidates.contains(&inspector));
                Some(Action::ChooseCostTarget(inspector))
            }
            _ => None,
        },
    );
    assert!(sneaked);
    assert!(hand_contains(&state, P0, inspector), "returned to hand");
    assert_eq!(zone(&state, leonardo), Zone::Battlefield);
    assert!(state.objects.get(leonardo).tapped);
    // Leonardo (1 + 1 for the other Inspector) and the Inspector dealt combat
    // damage.
    assert_eq!(state.players[1].life, 20 - 2 - 1);
}

#[test]
fn lunarch_veteran_gains_life_as_creatures_enter_and_is_disturbed_into_luminous_phantom() {
    let mut state = plains();
    put(&mut state, P0, "Lunarch Veteran", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 21);

    // Disturb {1}{W}: cast from the graveyard transformed.
    let mut state = plains();
    let veteran = put(&mut state, P0, "Lunarch Veteran", Zone::Graveyard);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(
        !castable(&mut state, veteran),
        "only the disturb cost casts it from the graveyard"
    );
    pool(&mut state, P0, &[], 1);
    cast(&mut state, veteran, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, veteran), Zone::Battlefield);
    assert_eq!(state.objects.get(veteran).v4.face_index, 1);
    assert!(keyword(&state, veteran, Keywords::FLYING));
    assert_eq!(power_toughness(&state, veteran), (1, 1));

    // Another creature leaving gains 1; the Phantom itself is exiled
    // instead of going to the graveyard.
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(state.players[0].life, 21);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(veteran)]);
    settled(&mut state);
    assert_eq!(zone(&state, veteran), Zone::Exile);
}

#[test]
fn a_disturbed_spell_put_into_a_graveyard_is_exiled_instead() {
    let mut state = plains();
    let veteran = put(&mut state, P0, "Lunarch Veteran", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, veteran, &[]);
    // Collect the cast so the disturb is recorded, then counter it.
    next(&mut state);
    assert_eq!(zone(&state, veteran), Zone::Stack);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(veteran, Zone::Graveyard),
    );
    assert_eq!(zone(&state, veteran), Zone::Exile);
}

#[test]
fn martyrs_soul_gets_two_counters_only_without_tapped_lands_and_convokes() {
    let mut state = plains();
    let soul = put(&mut state, P0, "Martyr's Soul", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, soul, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, soul), (5, 4));

    let mut state = plains();
    let land = put(&mut state, P0, "Plains", Zone::Battlefield);
    state.objects.get_mut(land).tapped = true;
    let soul = put(&mut state, P0, "Martyr's Soul", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, soul, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, soul), (3, 2));

    // Convoke: two creatures pay the generic part.
    let mut state = plains();
    let first = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let second = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let soul = put(&mut state, P0, "Martyr's Soul", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, soul, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, soul), Zone::Battlefield);
    assert!(state.objects.get(first).tapped && state.objects.get(second).tapped);
    assert_eq!(power_toughness(&state, soul), (5, 4), "no tapped lands");
}

#[test]
fn raffines_informant_connives() {
    for discard_land in [false, true] {
        let mut state = plains();
        let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        let informant = put(&mut state, P0, "Raffine's Informant", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::W], 1);
        cast(&mut state, informant, &[]);
        let drawn = state.players[0].library[0];
        let discard = if discard_land { drawn } else { bolt };
        settle_choosing(&mut state, discard);
        assert_eq!(zone(&state, discard), Zone::Graveyard);
        assert_eq!(
            power_toughness(&state, informant),
            if discard_land { (2, 1) } else { (3, 2) }
        );
    }
}

#[test]
fn salt_road_packbeast_costs_less_per_creature_and_draws() {
    let mut state = plains();
    for _ in 0..3 {
        put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    }
    let beast = put(&mut state, P0, "Salt Road Packbeast", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(!castable(&mut state, beast));
    pool(&mut state, P0, &[], 1);
    cast(&mut state, beast, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, beast), Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn search_party_captain_costs_less_per_creature_that_attacked_and_draws() {
    let mut state = plains();
    let first = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let second = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let captain = put(&mut state, P0, "Search Party Captain", Zone::Hand);
    attack_with(&mut state, vec![first, second], |_, _| None);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, captain, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, captain), Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn spider_man_is_cast_for_web_slinging_by_returning_a_tapped_creature() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let spider = put(&mut state, P0, "Spider-Man, Web-Slinger", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(!castable(&mut state, spider), "no tapped creature");
    state.objects.get_mut(inspector).tapped = true;
    cast(&mut state, spider, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, spider), Zone::Battlefield);
    assert!(hand_contains(&state, P0, inspector));
    assert_eq!(power_toughness(&state, spider), (3, 3));
}

// ---- Auras --------------------------------------------------------------------

#[test]
fn angelic_gift_draws_and_grants_flying() {
    let mut state = plains();
    let bogle = put(&mut state, P0, "Slippery Bogle", Zone::Battlefield);
    let gift = put(&mut state, P0, "Angelic Gift", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    enchant(&mut state, gift, bogle);
    assert!(keyword(&state, bogle, Keywords::FLYING));
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn armadillo_cloak_pumps_grants_trample_and_gains_life_for_its_damage() {
    let mut state = plains();
    let bogle = put(&mut state, P0, "Slippery Bogle", Zone::Battlefield);
    let cloak = put(&mut state, P0, "Armadillo Cloak", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::W], 1);
    enchant(&mut state, cloak, bogle);
    assert_eq!(power_toughness(&state, bogle), (3, 3));
    assert!(keyword(&state, bogle, Keywords::TRAMPLE));
    attack_with(&mut state, vec![bogle], |_, _| None);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].life, 23);
}

#[test]
fn spirit_link_gains_its_controller_life_for_the_damage_dealt() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let link = put(&mut state, P0, "Spirit Link", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    enchant(&mut state, link, inspector);
    attack_with(&mut state, vec![inspector], |_, _| None);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(state.players[0].life, 21);

    // On an opponent's creature, the Aura's controller gains the life.
    let mut state = plains();
    let opposing = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let link = put(&mut state, P0, "Spirit Link", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    enchant(&mut state, link, opposing);
    let bushwhack = put(&mut state, P0, "Bushwhack", Zone::Hand);
    let mine = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    act(&mut state, Action::CastSpell(bushwhack));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            Action::ChooseSpellMode(*legal_modes.last().unwrap())
        }
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(mine)) =>
        {
            Action::ChooseTarget(Target::Object(mine))
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[0].life, 21, "the fight damage it dealt");
}

#[test]
fn ancestral_mask_counts_every_other_enchantment() {
    let mut state = plains();
    let bogle = put(&mut state, P0, "Slippery Bogle", Zone::Battlefield);
    let mask = put(&mut state, P0, "Ancestral Mask", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    enchant(&mut state, mask, bogle);
    assert_eq!(
        power_toughness(&state, bogle),
        (1, 1),
        "no other enchantment"
    );
    put(&mut state, P1, "Journey to Nowhere", Zone::Battlefield);
    let rancor = put(&mut state, P0, "Rancor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    enchant(&mut state, rancor, bogle);
    // +2/+2 for each of two other enchantments, and Rancor's +2/+0.
    assert_eq!(power_toughness(&state, bogle), (7, 5));
}

#[test]
fn ethereal_armor_counts_enchantments_you_control_and_grants_first_strike() {
    let mut state = plains();
    let bogle = put(&mut state, P0, "Slippery Bogle", Zone::Battlefield);
    put(&mut state, P1, "Journey to Nowhere", Zone::Battlefield);
    let armor = put(&mut state, P0, "Ethereal Armor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    enchant(&mut state, armor, bogle);
    assert_eq!(power_toughness(&state, bogle), (2, 2), "itself only");
    assert!(keyword(&state, bogle, Keywords::FIRST_STRIKE));
    let link = put(&mut state, P0, "Spirit Link", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    enchant(&mut state, link, bogle);
    assert_eq!(power_toughness(&state, bogle), (3, 3));
}

#[test]
fn rancor_grants_trample_and_returns_to_hand_from_the_graveyard() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let rancor = put(&mut state, P0, "Rancor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    enchant(&mut state, rancor, inspector);
    assert_eq!(power_toughness(&state, inspector), (3, 2));
    assert!(keyword(&state, inspector, Keywords::TRAMPLE));
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert!(
        hand_contains(&state, P0, rancor),
        "returned to its owner's hand"
    );
}

#[test]
fn benevolent_blessing_gives_protection_from_the_chosen_color() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let blessing = put(&mut state, P0, "Benevolent Blessing", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, blessing, &[Target::Object(inspector)]);
    // Colors are offered in W, U, B, R, G order; choose red.
    settle_option(&mut state, 3);
    assert_eq!(zone(&state, blessing), Zone::Battlefield);
    assert_eq!(
        state.objects.get(blessing).v4.chosen_color,
        Some(ManaColor::R)
    );
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!cast_targets(&state, bolt).contains(&Target::Object(inspector)));
    assert!(cast_targets(&state, bolt).contains(&Target::Player(P1)));

    // A red creature can't block it.
    let goblin = put(&mut state, P1, "Kessig Flamebreather", Zone::Battlefield);
    let inspector_two = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let blockers = legal_blockers(&mut state, inspector);
    assert!(!blockers.contains(&goblin));
    assert!(blockers.contains(&inspector_two));
}

#[test]
fn benevolent_blessing_has_flash() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let blessing = put(&mut state, P0, "Benevolent Blessing", Zone::Hand);
    put(&mut state, P0, "Plains", Zone::Battlefield);
    put(&mut state, P0, "Plains", Zone::Battlefield);
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.priority_player == P0
    });
    cast(&mut state, blessing, &[Target::Object(inspector)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, blessing), Zone::Battlefield);
    assert_eq!(
        state.objects.get(blessing).v4.chosen_color,
        Some(ManaColor::W)
    );
}

#[test]
fn cartouche_of_solidarity_enchants_only_your_creatures_and_makes_a_warrior() {
    let mut state = plains();
    let opposing = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let cartouche = put(&mut state, P0, "Cartouche of Solidarity", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    let targets = cast_targets(&state, cartouche);
    assert!(!targets.contains(&Target::Object(opposing)));
    assert!(targets.contains(&Target::Object(inspector)));
    enchant(&mut state, cartouche, inspector);
    assert_eq!(power_toughness(&state, inspector), (2, 3));
    assert!(keyword(&state, inspector, Keywords::FIRST_STRIKE));
    let warriors = named(&state, P0, "White Warrior Token");
    assert_eq!(warriors.len(), 1);
    assert!(keyword(&state, warriors[0], Keywords::VIGILANCE));
}

#[test]
fn utopia_sprawl_adds_an_extra_mana_of_its_chosen_color() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let plain = put(&mut state, P0, "Plains", Zone::Battlefield);
    let sprawl = put(&mut state, P0, "Utopia Sprawl", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    let targets = cast_targets(&state, sprawl);
    assert!(targets.contains(&Target::Object(forest)));
    assert!(!targets.contains(&Target::Object(plain)));
    cast(&mut state, sprawl, &[Target::Object(forest)]);
    // Choose blue.
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, sprawl), Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(forest));
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
}

#[test]
fn abundant_growth_draws_and_lets_its_land_tap_for_any_color() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let growth = put(&mut state, P0, "Abundant Growth", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    enchant(&mut state, growth, forest);
    assert_eq!(state.players[0].hand.len(), 1);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(forest, ManaColor::B),
    );
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert!(state.objects.get(forest).tapped);
}

// ---- Equipment ----------------------------------------------------------------

#[test]
fn greatsword_of_tyr_grows_its_attacker_and_taps_a_defender() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let sword = put(&mut state, P0, "Greatsword of Tyr", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, sword, 0, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(
        state
            .objects
            .get(sword)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(inspector)
    );
    attack_with(&mut state, vec![inspector], |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(blocker)) =>
        {
            Some(Action::ChooseTarget(Target::Object(blocker)))
        }
        _ => None,
    });
    assert_eq!(power_toughness(&state, inspector), (2, 3));
    assert!(state.objects.get(blocker).tapped);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn ragged_short_spear_loots_on_entry_and_equips_for_three() {
    let mut state = plains();
    let fodder = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let spear = put(&mut state, P0, "Ragged Short Spear", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, spear, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Discard)
        }
        Decision::Discard { .. } => Action::Discard(vec![fodder]),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, fodder), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 2);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    assert!(!activatable(&mut state, spear, 0));
    pool(&mut state, P0, &[], 1);
    activate(&mut state, spear, 0, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, inspector), (3, 2));
}

// ---- Instants and sorceries ------------------------------------------------------

#[test]
fn alms_of_the_vein_drains_three() {
    let mut state = plains();
    let alms = put(&mut state, P0, "Alms of the Vein", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, alms, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].life, 23);
}

#[test]
fn vampires_kiss_drains_two_and_makes_two_blood_tokens() {
    let mut state = plains();
    let kiss = put(&mut state, P0, "Vampire's Kiss", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, kiss, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 18);
    assert_eq!(state.players[0].life, 22);
    assert_eq!(named(&state, P0, "Blood Token").len(), 2);
}

#[test]
fn battle_screech_makes_two_birds_and_flashes_back_by_tapping_three_white_creatures() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let screech = put(&mut state, P0, "Battle Screech", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, screech, &[]);
    settled(&mut state);
    let birds = named(&state, P0, "Bird Token");
    assert_eq!(birds.len(), 2);
    assert!(keyword(&state, birds[0], Keywords::FLYING));
    assert_eq!(zone(&state, screech), Zone::Graveyard);

    // Flashback: tap the two Birds and the Inspector.
    assert!(castable(&mut state, screech));
    act(&mut state, Action::CastSpell(screech));
    settle_first(&mut state);
    assert_eq!(zone(&state, screech), Zone::Exile);
    assert_eq!(named(&state, P0, "Bird Token").len(), 4);
    assert!(state.objects.get(inspector).tapped);
    assert!(birds.iter().all(|&bird| state.objects.get(bird).tapped));
}

#[test]
fn battle_screech_flashback_needs_three_untapped_white_creatures() {
    let mut state = plains();
    put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    put(&mut state, P0, "Slippery Bogle", Zone::Battlefield);
    let screech = put(&mut state, P0, "Battle Screech", Zone::Graveyard);
    assert!(!castable(&mut state, screech));
}

#[test]
fn guardians_pledge_pumps_only_white_creatures() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let bogle = put(&mut state, P0, "Slippery Bogle", Zone::Battlefield);
    let opposing = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let pledge = put(&mut state, P0, "Guardians' Pledge", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 1);
    cast(&mut state, pledge, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, inspector), (3, 4));
    assert_eq!(power_toughness(&state, bogle), (1, 1));
    assert_eq!(power_toughness(&state, opposing), (1, 2));
}

#[test]
fn ramosian_rally_taps_a_creature_instead_of_its_mana_cost_with_a_plains() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let rally = put(&mut state, P0, "Ramosian Rally", Zone::Hand);
    assert!(!castable(&mut state, rally), "no Plains");
    let plain = put(&mut state, P0, "Plains", Zone::Battlefield);
    state.objects.get_mut(plain).tapped = true;
    cast(&mut state, rally, &[]);
    settle_first(&mut state);
    assert!(state.objects.get(inspector).tapped);
    assert_eq!(power_toughness(&state, inspector), (2, 3));
}

#[test]
fn mana_tithe_counters_unless_its_controller_pays_one() {
    for payable in [false, true] {
        let mut state = plains();
        if payable {
            put(&mut state, P0, "Plains", Zone::Battlefield);
        }
        let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        let tithe = put(&mut state, P0, "Mana Tithe", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 0);
        cast(&mut state, bolt, &[Target::Player(P1)]);
        cast(&mut state, tithe, &[Target::Object(bolt)]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(true),
            other => first_answer(other),
        });
        assert_eq!(
            state.players[1].life,
            if payable { 17 } else { 20 },
            "payable {payable}"
        );
    }
}

#[test]
fn pursue_the_past_gains_two_may_loot_and_has_flashback() {
    let mut state = plains();
    let fodder = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let pursue = put(&mut state, P0, "Pursue the Past", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 0);
    cast(&mut state, pursue, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Discard)
        }
        Decision::Discard { .. } => Action::Discard(vec![fodder]),
        other => first_answer(other),
    });
    assert_eq!(state.players[0].life, 22);
    assert_eq!(zone(&state, fodder), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 2);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 2);
    cast(&mut state, pursue, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, pursue), Zone::Exile, "flashback");
    assert_eq!(state.players[0].life, 24);
}

#[test]
fn malevolent_rumble_keeps_a_permanent_card_mills_the_rest_and_makes_a_spawn() {
    let mut state = plains();
    let rancor = put(&mut state, P0, "Rancor", Zone::Library);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    let rumble = put(&mut state, P0, "Malevolent Rumble", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, rumble, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(rancor)) =>
        {
            assert!(
                !legal_targets.contains(&Target::Object(bolt)),
                "not a permanent card"
            );
            Action::ChooseEffectTarget(Target::Object(rancor))
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, rancor));
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(
        state.players[0].graveyard.len(),
        4,
        "two Plains, the Bolt, Rumble"
    );
    assert_eq!(state.players[0].library.len(), 18);
    assert_eq!(named(&state, P0, "Eldrazi Spawn Token").len(), 1);
}

#[test]
fn bushwhack_finds_a_basic_land_or_fights() {
    let mut state = game(&["Island"; 20]);
    let forest = put(&mut state, P0, "Forest", Zone::Library);
    let bushwhack = put(&mut state, P0, "Bushwhack", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    act(&mut state, Action::CastSpell(bushwhack));
    settle_choosing(&mut state, forest);
    assert!(hand_contains(&state, P0, forest));

    let mut state = plains();
    let mine = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let bushwhack = put(&mut state, P0, "Bushwhack", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    act(&mut state, Action::CastSpell(bushwhack));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            Action::ChooseSpellMode(*legal_modes.last().unwrap())
        }
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(mine)) =>
        {
            Action::ChooseTarget(Target::Object(mine))
        }
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(theirs)) =>
        {
            Action::ChooseTarget(Target::Object(theirs))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, mine), Zone::Battlefield);
}
