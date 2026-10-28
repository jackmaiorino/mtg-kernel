//! Eternal wave E9b: native rules tests for the cards that complete Pauper
//! Turbo Fog, Gruul Ponza, Gruul Ramp, Food Gardens, 4c Pestilence and Esper
//! Faeries, and Vintage Scam, Grixis Lurrus and Underworld Breach (with the
//! cards Vintage Red Prison, Vintage Eldrazi and Pauper 4c Ephemerate need).
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E9B_CARDS: [&str; 44] = [
    "Annoyed Altisaur",
    "Arbor Elf",
    "Arcane Denial",
    "Bannerhide Krushok",
    "Barrowgoyf",
    "Black Dragon Gate",
    "Boarding Party",
    "Boggart Trawler",
    "Boomerang Basics",
    "Bridgeworks Battle",
    "Castigate",
    "Cauldron Familiar",
    "Dauthi Voidwalker",
    "Echoing Truth",
    "Eldrazi Repurposer",
    "Elvish Spirit Guide",
    "Fog",
    "Grapple with Death",
    "Growth Spiral",
    "Heritage Reclamation",
    "Highland Forest",
    "Jewel Thief",
    "Mai, Scornful Striker",
    "Momentum Breaker",
    "Mwonvuli Acid-Moss",
    "Mystical Tutor",
    "Nutrient Block",
    "Oswald Fiddlebender",
    "Pestilence",
    "Pieces of the Puzzle",
    "Radiant Fountain",
    "Ram Through",
    "Respite",
    "Secluded Courtyard",
    "Simic Growth Chamber",
    "Sowing Mycospawn",
    "Step Through",
    "Thermokarst",
    "Thief of Existence",
    "Thornwood Falls",
    "Vampire Sovereign",
    "Vraska's Final Mercy",
    "Wild Growth",
    "You Meet in a Tavern",
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
        0x4539_4221_0001,
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

fn token(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Battlefield);
    state.objects.get_mut(object).v4.is_token = true;
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
/// finishing a selection that may finish, declining a Boolean or cost.
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
        Decision::ChooseCostTargets { candidates, .. } if candidates.contains(&object) => {
            Action::ChooseCostTarget(object)
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

/// Casts the modal `spell` choosing `mode` (chosen silently when it is the
/// only legal one), then its targets.
fn cast_mode(state: &mut GameState, spell: ObjectId, mode: u8, targets: &[Target]) {
    assert!(castable(state, spell));
    act(state, Action::CastSpell(spell));
    if let Decision::ChooseSpellMode { legal_modes, .. } = next(state) {
        assert!(legal_modes.contains(&mode), "{mode} not in {legal_modes:?}");
        act(state, Action::ChooseSpellMode(mode));
    }
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

/// The targets `spell` may choose first.
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

/// Plays `land` from P0's hand and settles its triggers with first answers.
fn play_land(state: &mut GameState, land: ObjectId) {
    next(state);
    act(state, Action::PlayLand(land));
    settle_first(state);
}

/// P1 casts a Lightning Bolt at `target` and passes, leaving it on the
/// stack with P0 holding priority.
fn opponent_bolts(state: &mut GameState, target: Target) -> ObjectId {
    state.priority_player = P1;
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    cast(state, bolt, &[target]);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(state, Action::Pass);
    bolt
}

/// A tapped dual: enters tapped and taps for either color.
fn tapped_dual(name: &str, colors: [ManaColor; 2]) {
    let mut state = plains();
    let land = put(&mut state, P0, name, Zone::Hand);
    play_land(&mut state, land);
    assert!(state.objects.get(land).tapped, "{name}");
    for color in colors {
        let mut state = plains();
        let land = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(land, color));
        assert_eq!(mana(&state, P0, color), 1, "{name} {color:?}");
    }
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e9b_cards_are_full_with_their_printed_characteristics() {
    for name in E9B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    for (name, keywords) in [
        ("Annoyed Altisaur", Keywords::REACH),
        ("Annoyed Altisaur", Keywords::TRAMPLE),
        ("Bannerhide Krushok", Keywords::TRAMPLE),
        ("Barrowgoyf", Keywords::DEATHTOUCH),
        ("Barrowgoyf", Keywords::LIFELINK),
        ("Boarding Party", Keywords::HASTE),
        ("Jewel Thief", Keywords::VIGILANCE),
        ("Jewel Thief", Keywords::TRAMPLE),
        ("Mai, Scornful Striker", Keywords::FIRST_STRIKE),
        ("Vampire Sovereign", Keywords::FLYING),
        ("Nutrient Block", Keywords::INDESTRUCTIBLE),
    ] {
        assert!(def(name).keywords.has(keywords), "{name}");
    }
    for name in [
        "Eldrazi Repurposer",
        "Sowing Mycospawn",
        "Thief of Existence",
    ] {
        assert!(def(name).colors.is_empty(), "{name} has devoid");
    }
    assert_eq!(
        def("Dauthi Voidwalker").subtypes,
        &[Subtype::Dauthi, Subtype::Rogue]
    );
    assert_eq!(
        def("Mai, Scornful Striker").subtypes,
        &[Subtype::Human, Subtype::Noble, Subtype::Ally]
    );
    assert_eq!(
        def("Sowing Mycospawn").subtypes,
        &[Subtype::Eldrazi, Subtype::Fungus]
    );
    for subtype in [Subtype::Dauthi, Subtype::Ally, Subtype::Fungus] {
        assert!(subtype.is_creature_type(), "{subtype:?}");
    }
    assert!(def("Sowing Mycospawn").kicker_cost.is_some());
    for name in [
        "Heritage Reclamation",
        "Vraska's Final Mercy",
        "You Meet in a Tavern",
    ] {
        assert!(def(name).mode2.is_some(), "{name}");
    }
    assert!(def("Heritage Reclamation").mode3.is_some());
    assert_eq!(def("Boomerang Basics").subtypes, &[Subtype::Lesson]);
    assert_eq!(def("Nutrient Block").subtypes, &[Subtype::Food]);
    assert_eq!(def("Wild Growth").subtypes, &[Subtype::Aura]);
    for (name, face) in [
        ("Boggart Trawler", "Boggart Bog"),
        ("Bridgeworks Battle", "Tanglespan Bridgeworks"),
    ] {
        let face_def = def(name).transform_face.as_ref().expect(name);
        assert_eq!(face_def.name, face);
        assert_eq!(face_def.types, &[CardType::Land]);
    }
    assert_eq!(
        def("Highland Forest").supertypes,
        &[Supertype::Snow],
        "a snow dual"
    );
    for name in [
        "Highland Forest",
        "Thornwood Falls",
        "Simic Growth Chamber",
        "Black Dragon Gate",
    ] {
        assert!(def(name).enters_battlefield_tapped, "{name}");
    }
    assert!(!def("Radiant Fountain").enters_battlefield_tapped);
    assert_eq!(
        def("Black Dragon Gate").as_enters_choose_color_other_than,
        Some(ManaColor::B)
    );
    assert_eq!(def("Elvish Spirit Guide").activated_abilities.len(), 1);
    assert_eq!(def("Bannerhide Krushok").activated_abilities.len(), 2);
    assert_eq!(def("Step Through").activated_abilities.len(), 1);
    assert_eq!(
        def("Step Through").target_spec,
        TargetSpec::StandardV1(
            mtg_kernel::standard_cards_v1::StandardTargetV1::TwoDistinctCreaturesV1
        )
    );
    assert_eq!(
        (def("Fog").spell_effect)(),
        Some(mtg_kernel::eternal_pauper_e6a_v1::moments_peace()),
        "Fog is Moment's Peace without flashback"
    );
}

// ---- Lands ---------------------------------------------------------------------

#[test]
fn highland_forest_is_a_tapped_snow_mountain_forest() {
    tapped_dual("Highland Forest", [ManaColor::R, ManaColor::G]);
    let mut state = plains();
    let forest = put(&mut state, P0, "Highland Forest", Zone::Battlefield);
    assert!(engine::effective_supertypes(&state, forest).contains(&Supertype::Snow));
    assert!(engine::has_effective_subtype(
        &state,
        forest,
        Subtype::Mountain
    ));
    assert!(engine::has_effective_subtype(
        &state,
        forest,
        Subtype::Forest
    ));
}

#[test]
fn thornwood_falls_enters_tapped_and_gains_a_life() {
    tapped_dual("Thornwood Falls", [ManaColor::G, ManaColor::U]);
    let mut state = plains();
    let falls = put(&mut state, P0, "Thornwood Falls", Zone::Hand);
    play_land(&mut state, falls);
    assert_eq!(state.players[0].life, 21);
}

#[test]
fn radiant_fountain_gains_two_life_and_taps_for_colorless() {
    let mut state = plains();
    let fountain = put(&mut state, P0, "Radiant Fountain", Zone::Hand);
    play_land(&mut state, fountain);
    assert!(!state.objects.get(fountain).tapped);
    assert_eq!(state.players[0].life, 22);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(fountain));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
}

#[test]
fn simic_growth_chamber_returns_a_land_and_taps_for_green_and_blue() {
    let mut state = plains();
    let plain = put(&mut state, P0, "Plains", Zone::Battlefield);
    let chamber = put(&mut state, P0, "Simic Growth Chamber", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(chamber));
    settle_choosing(&mut state, plain);
    assert!(state.objects.get(chamber).tapped);
    assert!(hand_contains(&state, P0, plain));

    let mut state = plains();
    let chamber = put(&mut state, P0, "Simic Growth Chamber", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(chamber));
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
}

#[test]
fn black_dragon_gate_chooses_a_color_other_than_black() {
    let mut state = plains();
    let gate = put(&mut state, P0, "Black Dragon Gate", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(gate));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 4,
            ..
        }
    ));
    // The first color other than black is white.
    act(&mut state, Action::ChooseEffectOption(0));
    assert!(state.objects.get(gate).tapped);
    assert_eq!(state.objects.get(gate).v4.chosen_color, Some(ManaColor::W));
    state.objects.get_mut(gate).tapped = false;
    next(&mut state);
    for color in [ManaColor::B, ManaColor::W] {
        let mut tapped = state.clone();
        act(&mut tapped, Action::ActivateManaAbilityChoice(gate, color));
        assert_eq!(mana(&tapped, P0, color), 1, "{color:?}");
    }
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(gate, ManaColor::G)
    )
    .is_err());
}

/// Plays P0's Secluded Courtyard naming `subtype`.
fn courtyard_naming(state: &mut GameState, subtype: Subtype) -> ObjectId {
    let courtyard = put(state, P0, "Secluded Courtyard", Zone::Hand);
    let options = mtg_kernel::eternal_cards_v1::e3::cavern_type_options(state, P0);
    let index = options
        .iter()
        .position(|option| *option == subtype)
        .expect("a creature type of an owned creature card");
    next(state);
    act(state, Action::PlayLand(courtyard));
    match next(state) {
        Decision::ChooseEffectOption { source, .. } => {
            assert_eq!(source, courtyard);
            act(
                state,
                Action::ChooseEffectOption(u16::try_from(index).unwrap()),
            );
        }
        other => panic!("expected the creature type, got {other:?}"),
    }
    assert!(!state.objects.get(courtyard).tapped);
    courtyard
}

#[test]
fn secluded_courtyard_pays_for_creature_spells_and_abilities_of_its_type() {
    let mut state = plains();
    let repurposer = put(&mut state, P0, "Eldrazi Repurposer", Zone::Hand);
    let thief = put(&mut state, P0, "Jewel Thief", Zone::Hand);
    let courtyard = courtyard_naming(&mut state, Subtype::Eldrazi);
    let mut colorless = state.clone();
    next(&mut colorless);
    act(&mut colorless, Action::ActivateManaAbility(courtyard));
    assert_eq!(mana(&colorless, P0, ManaColor::C), 1);
    pool(&mut state, P0, &[], 2);
    assert!(
        castable(&mut state, repurposer),
        "an Eldrazi creature spell"
    );
    assert!(!castable(&mut state, thief), "a Cat creature spell");
    cast(&mut state, repurposer, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, repurposer), Zone::Battlefield);
    assert!(state.objects.get(courtyard).tapped);

    // An ability of a creature source of the chosen type.
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let krushok = put(&mut state, P0, "Bannerhide Krushok", Zone::Hand);
    courtyard_naming(&mut state, Subtype::Beast);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, krushok, 0, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(state.objects.get(inspector).counters.plus1_plus1, 2);
}

/// Plays the land face of the modal double-faced card `name`, paying 3 life
/// or not.
fn play_land_face(name: &str, pay: bool, color: ManaColor) {
    let mut state = plains();
    let card = put(&mut state, P0, name, Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(card));
    match next(&mut state) {
        Decision::ChooseEffectOption {
            source,
            option_count,
            ..
        } => {
            assert_eq!((source, option_count), (card, 2), "{name}");
            act(&mut state, Action::ChooseEffectOption(u16::from(pay)));
        }
        other => panic!("{name}: expected the life payment, got {other:?}"),
    }
    let land = state.objects.get(card);
    assert_eq!(land.zone, Zone::Battlefield, "{name}");
    assert!(engine::object_has_type(&state, card, CardType::Land));
    assert!(!engine::object_has_type(&state, card, CardType::Creature));
    assert_eq!(engine::object_mana_value(&state, card), 0);
    assert_eq!(land.tapped, !pay, "{name}");
    assert_eq!(state.players[0].life, if pay { 17 } else { 20 });
    if pay {
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(card));
        assert_eq!(mana(&state, P0, color), 1, "{name}");
    }
}

#[test]
fn boggart_bog_and_tanglespan_bridgeworks_are_played_paying_three_life_or_tapped() {
    for pay in [true, false] {
        play_land_face("Boggart Trawler", pay, ManaColor::B);
        play_land_face("Bridgeworks Battle", pay, ManaColor::G);
    }
}

// ---- Creatures ------------------------------------------------------------------

#[test]
fn boggart_trawler_exiles_target_players_graveyard_and_has_no_mana_as_a_creature() {
    let mut state = plains();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let mine = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let trawler = put(&mut state, P0, "Boggart Trawler", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, trawler, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, trawler), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(zone(&state, mine), Zone::Graveyard);
    assert_eq!(power_toughness(&state, trawler), (3, 1));
    next(&mut state);
    assert!(
        engine::step(&mut state, Action::ActivateManaAbility(trawler)).is_err(),
        "its {{B}} belongs to its land face"
    );
}

#[test]
fn arbor_elf_untaps_target_forest() {
    let mut state = plains();
    let elf = put(&mut state, P0, "Arbor Elf", Zone::Battlefield);
    let forest = put(&mut state, P0, "Highland Forest", Zone::Battlefield);
    let plain = put(&mut state, P0, "Plains", Zone::Battlefield);
    state.objects.get_mut(forest).tapped = true;
    state.objects.get_mut(plain).tapped = true;
    let mut probe = state.clone();
    act(&mut probe, Action::ActivateAbility(elf, 0));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(forest)));
            assert!(!legal_targets.contains(&Target::Object(plain)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    activate(&mut state, elf, 0, &[Target::Object(forest)]);
    settled(&mut state);
    assert!(!state.objects.get(forest).tapped);
    assert!(state.objects.get(elf).tapped);
}

#[test]
fn bannerhide_krushok_reinforces_from_hand_and_scavenges_from_the_graveyard() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let krushok = put(&mut state, P0, "Bannerhide Krushok", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    activate(&mut state, krushok, 0, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, krushok), Zone::Graveyard);
    assert_eq!(state.objects.get(inspector).counters.plus1_plus1, 2);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 5);
    activate(&mut state, krushok, 1, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, krushok), Zone::Exile);
    assert_eq!(state.objects.get(inspector).counters.plus1_plus1, 6);
    assert_eq!(power_toughness(&state, inspector), (7, 8));
}

#[test]
fn bannerhide_krushok_scavenges_only_as_a_sorcery() {
    let mut state = plains();
    put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let krushok = put(&mut state, P0, "Bannerhide Krushok", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 5);
    assert!(activatable(&mut state, krushok, 1));
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    assert!(state.stack.iter().any(|item| item.source == bolt));
    assert!(
        !activatable(&mut state, krushok, 1),
        "the stack is not empty"
    );
}

#[test]
fn barrowgoyf_counts_card_types_in_all_graveyards_and_mills_on_combat_damage() {
    let mut state = game(&["Thraben Inspector", "Plains", "Plains", "Plains"]);
    let inspector = state.players[0].library[0];
    let goyf = put(&mut state, P0, "Barrowgoyf", Zone::Battlefield);
    assert_eq!(power_toughness(&state, goyf), (0, 1));
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P1, "Plains", Zone::Graveyard);
    assert_eq!(power_toughness(&state, goyf), (2, 3), "instant and land");
    attack_with(&mut state, vec![goyf], |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(0)),
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(inspector)) =>
        {
            Some(Action::ChooseEffectTarget(Target::Object(inspector)))
        }
        _ => None,
    });
    assert_eq!(state.players[1].life, 18);
    assert_eq!(state.players[0].life, 22, "lifelink");
    assert!(hand_contains(&state, P0, inspector), "the milled creature");
    // The other milled card stays in the graveyard.
    assert_eq!(state.players[0].graveyard.len(), 2);
    assert!(keyword(&state, goyf, Keywords::DEATHTOUCH));
}

#[test]
fn cauldron_familiar_drains_and_returns_by_sacrificing_a_food() {
    let mut state = plains();
    let familiar = put(&mut state, P0, "Cauldron Familiar", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, familiar, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 21);
    assert_eq!(state.players[1].life, 19);

    let mut state = plains();
    let familiar = put(&mut state, P0, "Cauldron Familiar", Zone::Graveyard);
    assert!(!activatable(&mut state, familiar, 0), "no Food");
    let block = put(&mut state, P0, "Nutrient Block", Zone::Battlefield);
    let hand = state.players[0].hand.len();
    activate(&mut state, familiar, 0, &[]);
    settle_choosing(&mut state, block);
    assert_eq!(zone(&state, familiar), Zone::Battlefield);
    assert_eq!(zone(&state, block), Zone::Graveyard);
    assert_eq!(
        state.players[0].hand.len(),
        hand + 1,
        "Nutrient Block's draw"
    );
    assert_eq!(state.players[1].life, 19, "entering drains again");
}

#[test]
fn dauthi_voidwalker_has_shadow() {
    let mut state = plains();
    let dauthi = put(&mut state, P0, "Dauthi Voidwalker", Zone::Battlefield);
    put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    assert!(legal_blockers(&mut state, dauthi).is_empty());

    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    put(&mut state, P1, "Dauthi Voidwalker", Zone::Battlefield);
    assert!(
        legal_blockers(&mut state, inspector).is_empty(),
        "a shadow creature blocks only shadow"
    );
}

#[test]
fn dauthi_voidwalker_exiles_opponents_cards_with_void_counters_and_plays_one_free() {
    let mut state = plains();
    let dauthi = put(&mut state, P0, "Dauthi Voidwalker", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    settled(&mut state);
    assert_eq!(state.players[0].life, 17);
    assert_eq!(zone(&state, bolt), Zone::Exile, "exiled instead");
    assert!(mtg_kernel::eternal_e9b_v1::has_void_counter(&state, bolt));
    // P0's own cards still go to P0's graveyard.
    let mine = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, mine, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, mine), Zone::Graveyard);
    assert_eq!(state.players[1].life, 17);

    activate(&mut state, dauthi, 0, &[]);
    settle_choosing(&mut state, bolt);
    assert_eq!(zone(&state, dauthi), Zone::Graveyard);
    // P0 may cast the void Bolt without paying its mana cost.
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settle_first(&mut state);
    assert_eq!(state.players[1].life, 14);
}

#[test]
fn eldrazi_repurposer_makes_a_spawn_when_cast_and_when_it_dies() {
    let mut state = plains();
    let repurposer = put(&mut state, P0, "Eldrazi Repurposer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, repurposer, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, repurposer), Zone::Battlefield);
    assert_eq!(named(&state, P0, "Eldrazi Spawn Token").len(), 1);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(repurposer, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(named(&state, P0, "Eldrazi Spawn Token").len(), 2);
}

#[test]
fn elvish_spirit_guide_exiles_itself_from_hand_for_green_without_the_stack() {
    let mut state = plains();
    let guide = put(&mut state, P0, "Elvish Spirit Guide", Zone::Hand);
    assert!(activatable(&mut state, guide, 0));
    act(&mut state, Action::ActivateAbility(guide, 0));
    assert_eq!(zone(&state, guide), Zone::Exile);
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert!(state.stack.is_empty());
}

#[test]
fn jewel_thief_makes_a_treasure() {
    let mut state = plains();
    let thief = put(&mut state, P0, "Jewel Thief", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, thief, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Treasure Token").len(), 1);
    assert!(keyword(&state, thief, Keywords::VIGILANCE));
    assert!(keyword(&state, thief, Keywords::TRAMPLE));
}

#[test]
fn mai_makes_each_player_casting_a_noncreature_spell_lose_two_life() {
    let mut state = plains();
    put(&mut state, P0, "Mai, Scornful Striker", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[1].life, 17);
    opponent_bolts(&mut state, Target::Player(P0));
    settled(&mut state);
    assert_eq!(state.players[1].life, 15, "the opponent loses 2 too");
    assert_eq!(state.players[0].life, 15);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 15, "a creature spell costs nothing");
}

#[test]
fn oswald_fiddlebender_finds_an_artifact_one_mana_value_greater() {
    let mut state = game(&["Nutrient Block", "Plains", "Plains"]);
    let block = state.players[0].library[0];
    let oswald = put(&mut state, P0, "Oswald Fiddlebender", Zone::Battlefield);
    let treasure = token(&mut state, P0, "Treasure Token");
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, oswald, 0, &[]);
    settle_choosing(&mut state, block);
    assert!(named(&state, P0, "Treasure Token").is_empty(), "sacrificed");
    assert_ne!(zone(&state, treasure), Zone::Battlefield);
    assert_eq!(zone(&state, block), Zone::Battlefield, "mana value 0 + 1");
    assert!(state.objects.get(oswald).tapped);
}

#[test]
fn sowing_mycospawn_fetches_a_land_and_exiles_a_land_when_kicked() {
    let mut state = game(&["Forest", "Plains", "Plains"]);
    let forest = state.players[0].library[0];
    let theirs = put(&mut state, P1, "Plains", Zone::Battlefield);
    let spawn = put(&mut state, P0, "Sowing Mycospawn", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 5);
    assert!(castable(&mut state, spawn));
    act(&mut state, Action::CastSpell(spawn));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => Action::ChooseKicker(true),
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(theirs)) =>
        {
            Action::ChooseTarget(Target::Object(theirs))
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(forest)) =>
        {
            Action::ChooseEffectTarget(Target::Object(forest))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, spawn), Zone::Battlefield);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert!(!state.objects.get(forest).tapped);
    assert_eq!(zone(&state, theirs), Zone::Exile);

    let mut state = game(&["Forest", "Plains", "Plains"]);
    let forest = state.players[0].library[0];
    let theirs = put(&mut state, P1, "Plains", Zone::Battlefield);
    let spawn = put(&mut state, P0, "Sowing Mycospawn", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 3);
    act(&mut state, Action::CastSpell(spawn));
    settle_choosing(&mut state, forest);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert_eq!(zone(&state, theirs), Zone::Battlefield, "not kicked");
}

#[test]
fn thief_of_existence_exiles_a_cheap_noncreature_permanent_and_gains_a_draw() {
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let creature = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let thief = put(&mut state, P0, "Thief of Existence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    act(&mut state, Action::CastSpell(thief));
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(ring)) =>
        {
            offered = legal_targets;
            Action::ChooseTarget(Target::Object(ring))
        }
        other => first_answer(other),
    });
    assert!(!offered.contains(&Target::Object(creature)));
    assert_eq!(zone(&state, ring), Zone::Exile);
    assert_eq!(zone(&state, thief), Zone::Battlefield);
    assert_eq!(
        state
            .eternal_v1
            .as_ref()
            .map(|eternal| eternal.e9b.thief_grants.len()),
        Some(1),
        "the Thief gained its leaves trigger"
    );
    let hand = state.players[1].hand.len();
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(thief, Zone::Graveyard),
    );
    settle_first(&mut state);
    assert_eq!(
        state.players[1].hand.len(),
        hand + 1,
        "target opponent draws"
    );
}

#[test]
fn vampire_sovereign_drains_three() {
    let mut state = plains();
    let sovereign = put(&mut state, P0, "Vampire Sovereign", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 3);
    cast(&mut state, sovereign, &[]);
    settle_first(&mut state);
    assert_eq!(state.players[0].life, 23);
    assert_eq!(state.players[1].life, 17);
    assert!(keyword(&state, sovereign, Keywords::FLYING));
}

/// Casts the cascade creature `name` with the library holding a land, then
/// a Lightning Bolt; the Bolt is cast at P1.
fn cascade_into_bolt(name: &str, colored: &[ManaColor], generic: u8) {
    let mut state = game(&["Island", "Lightning Bolt", "Plains", "Plains"]);
    let island = state.players[0].library[0];
    let bolt = state.players[0].library[1];
    let creature = put(&mut state, P0, name, Zone::Hand);
    pool(&mut state, P0, colored, generic);
    cast(&mut state, creature, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2, "cast or decline");
            Action::ChooseEffectOption(0)
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets == vec![Target::Object(bolt)] =>
        {
            Action::ChooseEffectTarget(Target::Object(bolt))
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 17, "{name}: the Bolt was cast");
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, creature), Zone::Battlefield);
    assert_eq!(
        state.players[0].library.last(),
        Some(&island),
        "{name}: the land went to the bottom"
    );
    assert!(state.exile.is_empty());
}

#[test]
fn annoyed_altisaur_and_boarding_party_cascade() {
    cascade_into_bolt("Annoyed Altisaur", &[ManaColor::G, ManaColor::G], 5);
    cascade_into_bolt("Boarding Party", &[ManaColor::R], 5);
}

#[test]
fn cascade_may_decline_and_skips_cards_that_cost_too_much() {
    // Annoyed Altisaur (7) skips nothing cheaper; a declined Bolt goes to
    // the bottom with the land.
    let mut state = game(&["Island", "Lightning Bolt", "Plains", "Plains"]);
    let bolt = state.players[0].library[1];
    let altisaur = put(&mut state, P0, "Annoyed Altisaur", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 5);
    cast(&mut state, altisaur, &[]);
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, bolt), Zone::Library);
    assert!(state.exile.is_empty());
    assert_eq!(state.players[1].life, 20);
    let library = &state.players[0].library;
    assert!(library[library.len() - 2..].contains(&bolt));
}

// ---- Noncreature permanents ------------------------------------------------------

#[test]
fn momentum_breaker_edicts_starts_engines_and_gains_life_equal_to_speed() {
    let mut state = plains();
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let breaker = put(&mut state, P0, "Momentum Breaker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, breaker, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard, "sacrificed");
    assert_eq!(zone(&state, breaker), Zone::Battlefield);
    assert_eq!(state.speed_v1.map(|speed| speed.speeds[0]), Some(1));
    pool(&mut state, P0, &[], 2);
    activate(&mut state, breaker, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, breaker), Zone::Graveyard);
    assert_eq!(state.players[0].life, 21, "speed 1");

    // An opponent without a creature or Vehicle discards instead.
    let mut state = plains();
    let card = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let breaker = put(&mut state, P0, "Momentum Breaker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, breaker, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, card), Zone::Graveyard, "discarded");
}

#[test]
fn nutrient_block_is_an_indestructible_food_that_draws_when_it_goes_to_the_graveyard() {
    let mut state = plains();
    let block = put(&mut state, P0, "Nutrient Block", Zone::Battlefield);
    assert!(keyword(&state, block, Keywords::INDESTRUCTIBLE));
    let hand = state.players[0].hand.len();
    pool(&mut state, P0, &[], 2);
    activate(&mut state, block, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, block), Zone::Graveyard);
    assert_eq!(state.players[0].life, 23);
    assert_eq!(state.players[0].hand.len(), hand + 1);
}

#[test]
fn pestilence_damages_everything_and_dies_with_the_last_creature() {
    let mut state = plains();
    let pestilence = put(&mut state, P0, "Pestilence", Zone::Battlefield);
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    activate(&mut state, pestilence, 0, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].life, 19);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(state.objects.get(inspector).damage, 1);
    activate(&mut state, pestilence, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    pass_until(&mut state, |state| {
        state.step == Step::Cleanup || state.active_player == P1
    });
    assert_eq!(zone(&state, pestilence), Zone::Graveyard, "no creatures");

    // With a creature on the battlefield it stays.
    let mut state = plains();
    let pestilence = put(&mut state, P0, "Pestilence", Zone::Battlefield);
    put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(zone(&state, pestilence), Zone::Battlefield);
}

#[test]
fn wild_growth_adds_an_extra_green_to_its_land() {
    let mut state = plains();
    let plain = put(&mut state, P0, "Plains", Zone::Battlefield);
    let growth = put(&mut state, P0, "Wild Growth", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, growth, &[Target::Object(plain)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, growth), Zone::Battlefield);
    assert_eq!(
        state
            .objects
            .get(growth)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(plain)
    );
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(plain));
    assert_eq!(mana(&state, P0, ManaColor::W), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
}

// ---- Instants and sorceries --------------------------------------------------------

#[test]
fn arcane_denial_counters_and_both_players_draw_at_the_next_upkeep() {
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let denial = put(&mut state, P0, "Arcane Denial", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, denial, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.players[0].life, 20);
    let hands = [state.players[0].hand.len(), state.players[1].hand.len()];
    run_until(
        &mut state,
        |state| state.active_player == P1 && state.step == Step::Draw,
        |_, decision| match decision {
            Decision::ChooseEffectOption {
                player: P1,
                option_count: 3,
                ..
            } => Some(Action::ChooseEffectOption(2)),
            _ => None,
        },
    );
    assert_eq!(state.players[0].hand.len(), hands[0] + 1, "you draw a card");
    // Two from Arcane Denial and one from the draw step.
    assert_eq!(state.players[1].hand.len(), hands[1] + 3);
}

#[test]
fn boomerang_basics_draws_only_for_your_own_permanent() {
    let mut state = plains();
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let basics = put(&mut state, P0, "Boomerang Basics", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    let hand = state.players[0].hand.len();
    cast(&mut state, basics, &[Target::Object(theirs)]);
    settled(&mut state);
    assert!(hand_contains(&state, P1, theirs));
    assert_eq!(state.players[0].hand.len(), hand - 1);

    let mut state = plains();
    let mine = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let basics = put(&mut state, P0, "Boomerang Basics", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, basics, &[Target::Object(mine)]);
    settled(&mut state);
    assert!(hand_contains(&state, P0, mine));
    assert_eq!(state.players[0].hand.len(), 2, "the creature and a draw");
}

#[test]
fn bridgeworks_battle_pumps_then_fights_up_to_one_creature() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Jewel Thief", Zone::Battlefield);
    let battle = put(&mut state, P0, "Bridgeworks Battle", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(
        &mut state,
        battle,
        &[Target::Object(mine), Target::Object(theirs)],
    );
    settled(&mut state);
    assert_eq!(zone(&state, theirs), Zone::Graveyard, "a 3/4 Inspector");
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(state.objects.get(mine).damage, 3);
    assert_eq!(power_toughness(&state, mine), (3, 4));
}

#[test]
fn castigate_exiles_a_nonland_card_from_the_revealed_hand() {
    let mut state = plains();
    let land = put(&mut state, P1, "Plains", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let castigate = put(&mut state, P0, "Castigate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::B], 0);
    cast(&mut state, castigate, &[Target::Player(P1)]);
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            offered = legal_targets;
            Action::ChooseEffectTarget(Target::Object(bolt))
        }
        other => first_answer(other),
    });
    assert_eq!(offered, vec![Target::Object(bolt)], "nonland cards only");
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(zone(&state, land), Zone::Hand);
}

#[test]
fn echoing_truth_returns_every_permanent_with_the_same_name() {
    let mut state = plains();
    let first = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let second = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let mine = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let other = put(&mut state, P1, "Jewel Thief", Zone::Battlefield);
    let truth = put(&mut state, P0, "Echoing Truth", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, truth, &[Target::Object(first)]);
    settled(&mut state);
    for inspector in [first, second] {
        assert!(hand_contains(&state, P1, inspector));
    }
    assert!(hand_contains(&state, P0, mine));
    assert_eq!(zone(&state, other), Zone::Battlefield);
}

#[test]
fn fog_and_respite_prevent_combat_damage() {
    for (name, life) in [("Fog", 20), ("Respite", 21)] {
        let mut state = plains();
        let attacker = put(&mut state, P0, "Jewel Thief", Zone::Battlefield);
        for _ in 0..2 {
            put(&mut state, P1, "Forest", Zone::Battlefield);
        }
        let fog = put(&mut state, P1, name, Zone::Hand);
        let mut cast_it = false;
        attack_with(
            &mut state,
            vec![attacker],
            |state, decision| match decision {
                Decision::CastSpellOrPass {
                    player: P1,
                    castable_spells,
                    ..
                } if state.step == Step::DeclareBlockers && !cast_it => {
                    assert!(castable_spells.contains(&fog), "{name}");
                    cast_it = true;
                    Some(Action::CastSpell(fog))
                }
                _ => None,
            },
        );
        assert!(cast_it, "{name}");
        assert_eq!(state.players[1].life, life, "{name}");
    }
}

#[test]
fn grapple_with_death_destroys_an_artifact_or_creature_and_gains_a_life() {
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let grapple = put(&mut state, P0, "Grapple with Death", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 1);
    cast(&mut state, grapple, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(state.players[0].life, 21);
}

#[test]
fn growth_spiral_draws_and_may_put_a_land_onto_the_battlefield() {
    let mut state = game(&["Forest", "Plains", "Plains"]);
    let forest = state.players[0].library[0];
    let spiral = put(&mut state, P0, "Growth Spiral", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 0);
    cast(&mut state, spiral, &[]);
    settle_choosing(&mut state, forest);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert_eq!(state.players[0].lands_played_this_turn, 0);
}

#[test]
fn heritage_reclamation_has_three_modes() {
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let reclamation = put(&mut state, P0, "Heritage Reclamation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast_mode(&mut state, reclamation, 0, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);

    let mut state = plains();
    let card = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let reclamation = put(&mut state, P0, "Heritage Reclamation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    let hand = state.players[0].hand.len();
    cast_mode(&mut state, reclamation, 2, &[Target::Object(card)]);
    settled(&mut state);
    assert_eq!(zone(&state, card), Zone::Exile);
    assert_eq!(state.players[0].hand.len(), hand, "cast one, drew one");
}

#[test]
fn mwonvuli_acid_moss_destroys_a_land_and_fetches_a_forest_tapped() {
    let mut state = game(&["Highland Forest", "Plains", "Plains"]);
    let forest = state.players[0].library[0];
    let theirs = put(&mut state, P1, "Plains", Zone::Battlefield);
    let moss = put(&mut state, P0, "Mwonvuli Acid-Moss", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    cast(&mut state, moss, &[Target::Object(theirs)]);
    settle_choosing(&mut state, forest);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert!(state.objects.get(forest).tapped);
}

#[test]
fn mystical_tutor_puts_an_instant_or_sorcery_on_top() {
    let mut state = game(&["Plains", "Plains", "Lightning Bolt", "Plains"]);
    let bolt = state.players[0].library[2];
    let tutor = put(&mut state, P0, "Mystical Tutor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, tutor, &[]);
    settle_choosing(&mut state, bolt);
    assert_eq!(state.players[0].library[0], bolt);
}

#[test]
fn pieces_of_the_puzzle_takes_up_to_two_instants_and_sorceries() {
    let mut state = game(&[
        "Lightning Bolt",
        "Plains",
        "Lightning Bolt",
        "Lightning Bolt",
        "Plains",
        "Plains",
    ]);
    let top = state.players[0].library[..5].to_vec();
    let pieces = put(&mut state, P0, "Pieces of the Puzzle", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, pieces, &[]);
    let mut taken: Vec<ObjectId> = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            max_targets,
            selected_count,
            ..
        } if selected_count < 2 => {
            assert_eq!(max_targets, 2);
            let pick = legal_targets
                .into_iter()
                .find(|target| !matches!(target, Target::Object(card) if taken.contains(card)))
                .expect("another instant or sorcery");
            if let Target::Object(card) = pick {
                taken.push(card);
            }
            Action::ChooseEffectTarget(pick)
        }
        other => first_answer(other),
    });
    assert_eq!(taken.len(), 2);
    for card in &taken {
        assert!(hand_contains(&state, P0, *card));
    }
    for card in top.iter().filter(|card| !taken.contains(card)) {
        assert_eq!(zone(&state, *card), Zone::Graveyard);
    }
}

#[test]
fn ram_through_tramples_excess_damage_over_to_the_controller() {
    let mut state = plains();
    let thief = put(&mut state, P0, "Jewel Thief", Zone::Battlefield);
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let ram = put(&mut state, P0, "Ram Through", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(
        &mut state,
        ram,
        &[Target::Object(thief), Target::Object(inspector)],
    );
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(state.players[1].life, 19, "3 power, 2 lethal, 1 excess");

    // Without trample all of it goes to the creature.
    let mut state = plains();
    let mine = put(&mut state, P0, "Vampire Sovereign", Zone::Battlefield);
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let ram = put(&mut state, P0, "Ram Through", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(
        &mut state,
        ram,
        &[Target::Object(mine), Target::Object(inspector)],
    );
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(state.players[1].life, 20);
}

#[test]
fn step_through_returns_two_creatures_and_wizardcycles() {
    let mut state = plains();
    let first = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let second = put(&mut state, P0, "Jewel Thief", Zone::Battlefield);
    let step = put(&mut state, P0, "Step Through", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    let targets = cast_targets(&state, step);
    assert!(targets.contains(&Target::Object(first)));
    cast(
        &mut state,
        step,
        &[Target::Object(first), Target::Object(second)],
    );
    settled(&mut state);
    assert!(hand_contains(&state, P1, first));
    assert!(hand_contains(&state, P0, second));

    let mut state = game(&["Plains", "Spellstutter Sprite", "Plains"]);
    let sprite = state.players[0].library[1];
    let step = put(&mut state, P0, "Step Through", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, step, 0, &[]);
    settle_choosing(&mut state, sprite);
    assert_eq!(zone(&state, step), Zone::Graveyard);
    assert!(hand_contains(&state, P0, sprite), "a Wizard card");
}

#[test]
fn thermokarst_gains_a_life_only_for_a_snow_land() {
    for (name, life) in [("Highland Forest", 21), ("Plains", 20)] {
        let mut state = plains();
        let land = put(&mut state, P1, name, Zone::Battlefield);
        let karst = put(&mut state, P0, "Thermokarst", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 1);
        cast(&mut state, karst, &[Target::Object(land)]);
        settled(&mut state);
        assert_eq!(zone(&state, land), Zone::Graveyard, "{name}");
        assert_eq!(state.players[0].life, life, "{name}");
    }
}

#[test]
fn vraskas_final_mercy_destroys_or_empowers_jace() {
    let mut state = plains();
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let mercy = put(&mut state, P0, "Vraska's Final Mercy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    cast_mode(&mut state, mercy, 0, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(state.players[0].life, 18);

    let mut state = plains();
    let mercy = put(&mut state, P0, "Vraska's Final Mercy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    cast_mode(&mut state, mercy, 1, &[]);
    settle_first(&mut state);
    assert_eq!(state.players[0].life, 18);
    let jace = named(&state, P0, "Jace Token");
    assert_eq!(jace.len(), 1);
    assert_eq!(
        mtg_kernel::planeswalker_v1::loyalty(&state, jace[0]),
        Some(6)
    );
}

#[test]
fn you_meet_in_a_tavern_forms_a_party_or_starts_a_brawl() {
    let mut state = game(&[
        "Thraben Inspector",
        "Plains",
        "Jewel Thief",
        "Plains",
        "Plains",
        "Plains",
    ]);
    let inspector = state.players[0].library[0];
    let thief = state.players[0].library[2];
    let tavern = put(&mut state, P0, "You Meet in a Tavern", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    cast_mode(&mut state, tavern, 0, &[]);
    let mut taken: Vec<Target> = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count,
            ..
        } if selected_count < 2 => {
            let pick = legal_targets
                .into_iter()
                .find(|target| !taken.contains(target))
                .expect("another creature card");
            taken.push(pick);
            Action::ChooseEffectTarget(pick)
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, inspector));
    assert!(hand_contains(&state, P0, thief));
    // The three Plains went to the bottom; the sixth card is now on top.
    assert_eq!(state.players[0].library.len(), 4);

    let mut state = plains();
    let mine = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let tavern = put(&mut state, P0, "You Meet in a Tavern", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    cast_mode(&mut state, tavern, 1, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, mine), (3, 4));
    assert_eq!(power_toughness(&state, theirs), (1, 2));
}
