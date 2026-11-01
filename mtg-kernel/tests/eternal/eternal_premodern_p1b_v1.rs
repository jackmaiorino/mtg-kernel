//! Premodern wave P1b: native rules tests for the cards that complete
//! Enchantress, Replenish, Oath of Druids, Landstill, Psychatog and Grow.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_premodern_p1b_v1::statics as p1b;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P1B_CARDS: [&str; 35] = [
    "Accumulated Knowledge",
    "Argothian Enchantress",
    "Attunement",
    "Aura of Silence",
    "City of Brass",
    "Coastal Tower",
    "Decree of Justice",
    "Decree of Silence",
    "Dust Bowl",
    "Enchantress's Presence",
    "Fact or Fiction",
    "Frantic Search",
    "Intuition",
    "Legacy's Allure",
    "Mana Leak",
    "Mirri's Guile",
    "Mishra's Factory",
    "Opalescence",
    "Parallax Wave",
    "Phyrexian Furnace",
    "Psychatog",
    "Quirion Dryad",
    "Replenish",
    "Rewind",
    "Seal of Cleansing",
    "Serra's Sanctum",
    "Sickening Dreams",
    "Skycloud Expanse",
    "Solitary Confinement",
    "Standstill",
    "Sterling Grove",
    "Treetop Village",
    "Tsabo's Web",
    "Winter's Grasp",
    "Wrath of God",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn name_of(state: &GameState, object: ObjectId) -> &'static str {
    CARD_DEFS[state.objects.get(object).card_def as usize].name
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5031_4200_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn islands() -> GameState {
    game(&["Island"; 20])
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

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
}

fn controls(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| name_of(state, object) == name)
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

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    engine::object_has_type(state, object, CardType::Creature)
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
        Decision::ChooseTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.is_empty() => Action::FinishEffectSelection,
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

/// Settles, answering effect options with `option` and choosing `object`
/// whenever a selection or target offers it; otherwise the first answer.
fn settle_picking(state: &mut GameState, option: u16, object: Option<ObjectId>) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
        Decision::ChooseTargets { legal_targets, .. }
            if object.is_some_and(|object| legal_targets.contains(&Target::Object(object))) =>
        {
            Action::ChooseTarget(Target::Object(object.unwrap()))
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if object.is_some_and(|object| legal_targets.contains(&Target::Object(object))) => {
            Action::ChooseEffectTarget(Target::Object(object.unwrap()))
        }
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

/// Answers the X announcement of a pending cast with `x`.
fn announce_x(state: &mut GameState, x: u16) {
    match next(state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert!(x < option_count, "X {x} of {option_count}");
            act(state, Action::ChooseEffectOption(x));
        }
        other => panic!("expected the X announcement, got {other:?}"),
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
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

/// Plays to P0's next turn's first main phase, answering upkeep choices with
/// `answer`.
fn to_next_turn(
    state: &mut GameState,
    answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    let turn = state.turn;
    run_until(
        state,
        |state| state.turn > turn && state.active_player == P0 && state.step == Step::Main1,
        answer,
    );
}

/// Moves `object` to its owner's graveyard, as if destroyed.
fn to_graveyard(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// Deals `amount` damage from `source` to `target`.
fn damage(state: &mut GameState, source: ObjectId, target: Target, amount: i32) {
    event::propose_and_commit(state, ProposedEvent::damage(source, target, amount));
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p1b_cards_are_full_with_their_printed_characteristics() {
    for name in P1B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let angel = def("Angel Token");
    assert!(angel.is_token && angel.capability == CardCapability::Full);
    assert!(angel.keywords.has(Keywords::FLYING));
    assert_eq!((angel.power, angel.toughness), (Some(4), Some(4)));
    assert_eq!(angel.subtypes, &[Subtype::Angel]);
    assert!(def("Argothian Enchantress").keywords.has(Keywords::SHROUD));
    assert_eq!(def("Psychatog").subtypes, &[Subtype::Atog]);
    assert!(Subtype::Atog.is_creature_type());
    assert!(def("Coastal Tower").enters_battlefield_tapped);
    assert!(def("Treetop Village").enters_battlefield_tapped);
    assert_eq!(def("Decree of Justice").cost.x_count, 2);
    for name in ["Decree of Justice", "Decree of Silence"] {
        assert_eq!(def(name).activated_abilities.len(), 1, "{name} cycles");
    }
}

// ---- Lands ---------------------------------------------------------------------

#[test]
fn coastal_tower_enters_tapped_and_taps_for_white_or_blue() {
    let mut state = islands();
    let tower = put(&mut state, P0, "Coastal Tower", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(tower));
    assert!(state.objects.get(tower).tapped);
    for color in [ManaColor::W, ManaColor::U] {
        let mut state = islands();
        let tower = put(&mut state, P0, "Coastal Tower", Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(tower, color));
        assert_eq!(mana(&state, P0, color), 1);
    }
}

#[test]
fn city_of_brass_deals_one_damage_whenever_it_becomes_tapped() {
    let mut state = islands();
    let city = put(&mut state, P0, "City of Brass", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(city, ManaColor::B),
    );
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    settled(&mut state);
    assert_eq!(state.players[0].life, 19);
    // Tapped by anything, not only for mana.
    let mut state = islands();
    let city = put(&mut state, P0, "City of Brass", Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::tap(city));
    settled(&mut state);
    assert_eq!(state.players[0].life, 19);
}

#[test]
fn serras_sanctum_adds_white_for_each_enchantment() {
    let mut state = islands();
    let sanctum = put(&mut state, P0, "Serra's Sanctum", Zone::Battlefield);
    put(&mut state, P0, "Mirri's Guile", Zone::Battlefield);
    put(&mut state, P0, "Seal of Cleansing", Zone::Battlefield);
    put(&mut state, P1, "Standstill", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(sanctum));
    assert_eq!(mana(&state, P0, ManaColor::W), 2);
    assert!(def("Serra's Sanctum")
        .supertypes
        .contains(&mtg_kernel::card_def::Supertype::Legendary));
}

#[test]
fn skycloud_expanse_filters_one_mana_into_white_and_blue() {
    let mut state = islands();
    let expanse = put(&mut state, P0, "Skycloud Expanse", Zone::Battlefield);
    next(&mut state);
    assert!(
        engine::step(&mut state.clone(), Action::ActivateManaAbility(expanse)).is_err(),
        "no free mana ability"
    );
    pool(&mut state, P0, &[], 1);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(expanse));
    assert!(state.objects.get(expanse).tapped);
    assert_eq!(mana(&state, P0, ManaColor::W), 1);
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn mishras_factory_animates_and_pumps_assembly_workers() {
    let mut state = islands();
    let factory = put(&mut state, P0, "Mishra's Factory", Zone::Battlefield);
    let other = put(&mut state, P0, "Mishra's Factory", Zone::Battlefield);
    assert!(!is_creature(&state, factory));
    pool(&mut state, P0, &[], 2);
    activate(&mut state, factory, 0, &[]);
    settled(&mut state);
    assert!(is_creature(&state, factory));
    assert!(engine::object_has_type(&state, factory, CardType::Artifact));
    assert!(engine::object_has_type(&state, factory, CardType::Land));
    assert!(engine::has_effective_subtype(
        &state,
        factory,
        Subtype::AssemblyWorker
    ));
    assert_eq!(power_toughness(&state, factory), (2, 2));
    // The other Factory taps to pump it; an unanimated Factory isn't a
    // legal target.
    act(&mut state, Action::ActivateAbility(other, 1));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(factory)]);
            act(&mut state, Action::ChooseTarget(Target::Object(factory)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(power_toughness(&state, factory), (3, 3));
    assert!(state.objects.get(other).tapped);
}

#[test]
fn treetop_village_enters_tapped_and_becomes_a_trampling_ape() {
    let mut state = islands();
    let village = put(&mut state, P0, "Treetop Village", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(village));
    assert!(state.objects.get(village).tapped);

    let mut state = islands();
    let village = put(&mut state, P0, "Treetop Village", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 1);
    activate(&mut state, village, 0, &[]);
    settled(&mut state);
    assert!(is_creature(&state, village));
    assert_eq!(power_toughness(&state, village), (3, 3));
    assert!(keyword(&state, village, Keywords::TRAMPLE));
    assert!(engine::has_effective_subtype(&state, village, Subtype::Ape));
    assert_eq!(
        engine::object_color_mask(&state, village),
        mtg_kernel::card_def::mana_color_mask(ManaColor::G)
    );
}

#[test]
fn dust_bowl_sacrifices_a_land_to_destroy_a_nonbasic_land() {
    let mut state = islands();
    let bowl = put(&mut state, P0, "Dust Bowl", Zone::Battlefield);
    let fodder = put(&mut state, P0, "Island", Zone::Battlefield);
    let tower = put(&mut state, P1, "Coastal Tower", Zone::Battlefield);
    let basic = put(&mut state, P1, "Island", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    assert!(activatable(&mut state, bowl, 0));
    act(&mut state, Action::ActivateAbility(bowl, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(tower)));
            assert!(!legal_targets.contains(&Target::Object(basic)));
            act(&mut state, Action::ChooseTarget(Target::Object(tower)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&fodder));
            Action::ChooseCostTarget(fodder)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, fodder), Zone::Graveyard);
    assert_eq!(zone(&state, tower), Zone::Graveyard);
    assert!(state.objects.get(bowl).tapped);
}

// ---- Spells --------------------------------------------------------------------

#[test]
fn accumulated_knowledge_counts_its_name_in_all_graveyards() {
    let mut state = islands();
    put(&mut state, P1, "Accumulated Knowledge", Zone::Graveyard);
    put(&mut state, P0, "Accumulated Knowledge", Zone::Graveyard);
    let knowledge = put(&mut state, P0, "Accumulated Knowledge", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, knowledge, &[]);
    settled(&mut state);
    // One, then one for each of the two in graveyards.
    assert_eq!(state.players[0].hand.len(), 3);
    assert_eq!(zone(&state, knowledge), Zone::Graveyard);
}

#[test]
fn mana_leak_counters_unless_its_controller_pays_three() {
    let mut state = islands();
    let seal = put(&mut state, P1, "Seal of Cleansing", Zone::Hand);
    let leak = put(&mut state, P0, "Mana Leak", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::W], 1);
    cast(&mut state, seal, &[]);
    pool(&mut state, P0, &[ManaColor::U], 1);
    match next(&mut state) {
        Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
        other => panic!("{other:?}"),
    }
    cast(&mut state, leak, &[Target::Object(seal)]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(zone(&state, seal), Zone::Graveyard, "P1 could not pay");
}

#[test]
fn rewind_counters_and_untaps_up_to_four_lands() {
    let mut state = islands();
    let lands: Vec<ObjectId> = (0..4)
        .map(|_| put(&mut state, P0, "Island", Zone::Battlefield))
        .collect();
    let seal = put(&mut state, P1, "Seal of Cleansing", Zone::Hand);
    let rewind = put(&mut state, P0, "Rewind", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::W], 1);
    cast(&mut state, seal, &[]);
    match next(&mut state) {
        Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
        other => panic!("{other:?}"),
    }
    for &land in &lands {
        act(&mut state, Action::ActivateManaAbility(land));
    }
    assert!(lands.iter().all(|&land| state.objects.get(land).tapped));
    cast(&mut state, rewind, &[Target::Object(seal)]);
    let mut untapped = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if untapped.len() < 4
                && legal_targets
                    .iter()
                    .any(|target| !untapped.contains(target)) =>
        {
            let target = *legal_targets
                .iter()
                .find(|target| !untapped.contains(*target))
                .unwrap();
            untapped.push(target);
            Action::ChooseEffectTarget(target)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, seal), Zone::Graveyard);
    assert!(lands.iter().all(|&land| !state.objects.get(land).tapped));
}

#[test]
fn frantic_search_draws_two_discards_two_and_untaps_three_lands() {
    let mut state = islands();
    let lands: Vec<ObjectId> = (0..3)
        .map(|_| put(&mut state, P0, "Island", Zone::Battlefield))
        .collect();
    let search = put(&mut state, P0, "Frantic Search", Zone::Hand);
    next(&mut state);
    for &land in &lands {
        act(&mut state, Action::ActivateManaAbility(land));
    }
    cast(&mut state, search, &[]);
    let mut untapped = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets
                .iter()
                .any(|target| !untapped.contains(target)) =>
        {
            let target = *legal_targets
                .iter()
                .find(|target| !untapped.contains(*target))
                .unwrap();
            untapped.push(target);
            Action::ChooseEffectTarget(target)
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[0].hand.len(), 0, "drew two, discarded two");
    assert_eq!(state.players[0].graveyard.len(), 3);
    assert!(lands.iter().all(|&land| !state.objects.get(land).tapped));
}

#[test]
fn fact_or_fiction_lets_the_opponent_split_and_the_caster_choose() {
    let mut state = game(&["Island", "Forest", "Plains", "Swamp", "Mountain", "Island"]);
    let top: Vec<ObjectId> = state.players[0].library.iter().copied().take(5).collect();
    let fact = put(&mut state, P0, "Fact or Fiction", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, fact, &[]);
    // The opponent puts the top card alone in a pile.
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        }) => {
            assert_eq!(player, P1);
            assert_eq!(legal_targets.len(), 5);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(top[0])),
            );
            act(&mut state, Action::FinishEffectSelection);
        }
        other => panic!("expected the split, got {other:?}"),
    }
    // The caster points at a card of the four-card pile.
    match next(&mut state) {
        Decision::ChooseEffectTargets { player, .. } => {
            assert_eq!(player, P0);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(top[1])),
            );
        }
        other => panic!("expected the pile choice, got {other:?}"),
    }
    settle_with(&mut state, |_, decision| first_answer(decision));
    for &card in &top[1..] {
        assert_eq!(zone(&state, card), Zone::Hand);
    }
    assert_eq!(zone(&state, top[0]), Zone::Graveyard);
}

#[test]
fn intuition_lets_the_target_opponent_choose_the_card_to_hand() {
    let mut state = game(&["Island"; 10]);
    let wanted = put(&mut state, P0, "Opalescence", Zone::Library);
    let second = put(&mut state, P0, "Replenish", Zone::Library);
    let third = put(&mut state, P0, "Parallax Wave", Zone::Library);
    let intuition = put(&mut state, P0, "Intuition", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, intuition, &[Target::Player(P1)]);
    let mut found = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } if player == P0 => {
            let target = [third, second, wanted][found];
            found += 1;
            assert!(legal_targets.contains(&Target::Object(target)));
            Action::ChooseEffectTarget(Target::Object(target))
        }
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } => {
            assert_eq!(player, P1);
            assert_eq!(legal_targets.len(), 3);
            Action::ChooseEffectTarget(Target::Object(second))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, second), Zone::Hand);
    assert_eq!(zone(&state, wanted), Zone::Graveyard);
    assert_eq!(zone(&state, third), Zone::Graveyard);
}

#[test]
fn sickening_dreams_discards_x_cards_for_x_damage_to_everything() {
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let tog = put(&mut state, P0, "Psychatog", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Hand);
    let dreams = put(&mut state, P0, "Sickening Dreams", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    assert!(castable(&mut state, dreams));
    act(&mut state, Action::CastSpell(dreams));
    // X from 0 to the three cards in hand.
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => assert_eq!(option_count, 4),
        other => panic!("expected X, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(2));
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(state.players[0].hand.len(), 1, "discarded two");
    assert_eq!(state.players[0].life, 18);
    assert_eq!(state.players[1].life, 18);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(state.objects.get(tog).damage, 2);
}

#[test]
fn decree_of_justice_makes_angels_and_cycles_for_soldiers() {
    let mut state = islands();
    let decree = put(&mut state, P0, "Decree of Justice", Zone::Hand);
    // X = 1: {1}{1}{2}{W}{W}.
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 4);
    act(&mut state, Action::CastSpell(decree));
    announce_x(&mut state, 1);
    settled(&mut state);
    let angels = controls(&state, P0, "Angel Token");
    assert_eq!(angels.len(), 1);
    assert_eq!(power_toughness(&state, angels[0]), (4, 4));
    assert!(keyword(&state, angels[0], Keywords::FLYING));

    // Cycling for {2}{W}, then paying {2} for two Soldiers.
    let mut state = islands();
    let decree = put(&mut state, P0, "Decree of Justice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 4);
    activate(&mut state, decree, 0, &[]);
    // Options: decline, X = 1, X = 2.
    settle_picking(&mut state, 2, None);
    assert_eq!(controls(&state, P0, "Soldier Token").len(), 2);
    assert_eq!(state.players[0].hand.len(), 1, "cycling drew a card");
    assert_eq!(zone(&state, decree), Zone::Graveyard);
}

#[test]
fn winters_grasp_destroys_a_land() {
    let mut state = islands();
    let tower = put(&mut state, P1, "Coastal Tower", Zone::Battlefield);
    let grasp = put(&mut state, P0, "Winter's Grasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 1);
    cast(&mut state, grasp, &[Target::Object(tower)]);
    settled(&mut state);
    assert_eq!(zone(&state, tower), Zone::Graveyard);
}

#[test]
fn wrath_of_god_destroys_every_creature_without_regeneration() {
    let mut state = islands();
    let troll = put(&mut state, P0, "Golgari Grave-Troll", Zone::Battlefield);
    state.objects.get_mut(troll).counters.plus1_plus1 = 2;
    // A regeneration shield first.
    pool(&mut state, P0, &[], 1);
    activate(&mut state, troll, 0, &[]);
    settled(&mut state);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let wrath = put(&mut state, P0, "Wrath of God", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, wrath, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, troll), Zone::Graveyard, "can't be regenerated");
    assert_eq!(zone(&state, elves), Zone::Graveyard);
}

// ---- Enchantments --------------------------------------------------------------

#[test]
fn enchantresses_draw_when_their_controller_casts_an_enchantment() {
    let mut state = islands();
    put(&mut state, P0, "Argothian Enchantress", Zone::Battlefield);
    put(&mut state, P0, "Enchantress's Presence", Zone::Battlefield);
    let guile = put(&mut state, P0, "Mirri's Guile", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, guile, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, guile), Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 2);
    // A creature spell draws nothing.
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 2);
}

#[test]
fn argothian_enchantress_has_shroud() {
    let mut state = islands();
    let enchantress = put(&mut state, P1, "Argothian Enchantress", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let wave = put(&mut state, P0, "Parallax Wave", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, wave, &[]);
    settled(&mut state);
    act(&mut state, Action::ActivateAbility(wave, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(elves)));
            assert!(!legal_targets.contains(&Target::Object(enchantress)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

#[test]
fn attunement_returns_itself_to_draw_three_and_discard_four() {
    let mut state = islands();
    let attunement = put(&mut state, P0, "Attunement", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Hand);
    activate(&mut state, attunement, 0, &[]);
    next(&mut state);
    assert_eq!(zone(&state, attunement), Zone::Hand, "returned as its cost");
    settle_with(&mut state, |_, decision| first_answer(decision));
    // Two, the Attunement and three drawn, then four discarded.
    assert_eq!(state.players[0].hand.len(), 2);
    assert_eq!(state.players[0].graveyard.len(), 4);
}

#[test]
fn aura_of_silence_taxes_opponents_and_destroys_an_artifact_or_enchantment() {
    let mut state = islands();
    let aura = put(&mut state, P1, "Aura of Silence", Zone::Battlefield);
    let web = put(&mut state, P0, "Tsabo's Web", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    assert!(!castable(&mut state, web), "costs {{4}} now");
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, web));
    // Its controller's own spells cost no more.
    let mut state = islands();
    put(&mut state, P0, "Aura of Silence", Zone::Battlefield);
    let web = put(&mut state, P0, "Tsabo's Web", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    assert!(castable(&mut state, web));

    let _ = aura;
    let mut state = islands();
    let aura = put(&mut state, P0, "Aura of Silence", Zone::Battlefield);
    let furnace = put(&mut state, P1, "Phyrexian Furnace", Zone::Battlefield);
    activate(&mut state, aura, 0, &[Target::Object(furnace)]);
    settled(&mut state);
    assert_eq!(zone(&state, aura), Zone::Graveyard);
    assert_eq!(zone(&state, furnace), Zone::Graveyard);
}

#[test]
fn seal_of_cleansing_destroys_an_enchantment() {
    let mut state = islands();
    let seal = put(&mut state, P0, "Seal of Cleansing", Zone::Battlefield);
    let standstill = put(&mut state, P1, "Standstill", Zone::Battlefield);
    activate(&mut state, seal, 0, &[Target::Object(standstill)]);
    settled(&mut state);
    assert_eq!(zone(&state, seal), Zone::Graveyard);
    assert_eq!(zone(&state, standstill), Zone::Graveyard);
}

#[test]
fn decree_of_silence_counters_opponents_spells_until_depleted() {
    let mut state = islands();
    let decree = put(&mut state, P0, "Decree of Silence", Zone::Battlefield);
    state.active_player = P1;
    state.priority_player = P1;
    for round in 1..=3 {
        let seal = put(&mut state, P1, "Seal of Cleansing", Zone::Hand);
        pool(&mut state, P1, &[ManaColor::W], 1);
        cast(&mut state, seal, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, seal), Zone::Graveyard, "round {round}");
        if round < 3 {
            assert_eq!(p1b::depletion_counters(&state, decree), round);
        }
    }
    assert_eq!(zone(&state, decree), Zone::Graveyard, "sacrificed at three");
    // Its controller's own spells are not countered.
    let mut state = islands();
    put(&mut state, P0, "Decree of Silence", Zone::Battlefield);
    let seal = put(&mut state, P0, "Seal of Cleansing", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, seal, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, seal), Zone::Battlefield);
}

#[test]
fn decree_of_silence_cycles_to_counter_target_spell() {
    let mut state = islands();
    let decree = put(&mut state, P0, "Decree of Silence", Zone::Hand);
    let seal = put(&mut state, P1, "Seal of Cleansing", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::W], 1);
    cast(&mut state, seal, &[]);
    match next(&mut state) {
        Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
        other => panic!("{other:?}"),
    }
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 4);
    activate(&mut state, decree, 0, &[]);
    // The trigger targets the spell, then counters it.
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(seal)));
            Action::ChooseTarget(Target::Object(seal))
        }
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, seal), Zone::Graveyard);
    assert_eq!(zone(&state, decree), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1, "cycling drew a card");
}

#[test]
fn legacys_allure_steals_a_creature_no_stronger_than_its_counters() {
    let mut state = islands();
    let allure = put(&mut state, P0, "Legacy's Allure", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let tog = put(&mut state, P1, "Psychatog", Zone::Battlefield);
    // Two upkeeps, each adding a treasure counter.
    for _ in 0..2 {
        to_next_turn(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
            _ => None,
        });
    }
    assert_eq!(p1b::treasure_counters(&state, allure), 2);
    // Psychatog has power 1, so both are legal; take the Elves.
    act(&mut state, Action::ActivateAbility(allure, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(elves)));
            assert!(legal_targets.contains(&Target::Object(tog)));
            act(&mut state, Action::ChooseTarget(Target::Object(elves)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, allure), Zone::Graveyard);
    assert_eq!(state.objects.get(elves).controller, P0);

    // No counters: nothing with power 1 or more.
    let mut state = islands();
    let allure = put(&mut state, P0, "Legacy's Allure", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(!activatable(&mut state, allure, 0));
}

#[test]
fn mirris_guile_may_reorder_the_top_three_at_upkeep() {
    let mut state = game(&["Island"; 10]);
    put(&mut state, P0, "Mirri's Guile", Zone::Battlefield);
    let third = put(&mut state, P0, "Forest", Zone::Library);
    let second = put(&mut state, P0, "Plains", Zone::Library);
    let first = put(&mut state, P0, "Swamp", Zone::Library);
    let mut looked = false;
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            looked = true;
            // Each pick is the next deepest card: Swamp, then Plains.
            let pick = [first, second, third]
                .into_iter()
                .find(|card| legal_targets.contains(&Target::Object(*card)))
                .unwrap();
            Some(Action::ChooseEffectTarget(Target::Object(pick)))
        }
        _ => None,
    });
    assert!(looked);
    // The draw step drew the new top card: Forest.
    assert_eq!(zone(&state, third), Zone::Hand);
    assert_eq!(state.players[0].library[0], second);
}

#[test]
fn opalescence_animates_the_other_non_aura_enchantments() {
    let mut state = islands();
    let opalescence = put(&mut state, P0, "Opalescence", Zone::Hand);
    let seal = put(&mut state, P0, "Seal of Cleansing", Zone::Battlefield);
    let decree = put(&mut state, P1, "Decree of Silence", Zone::Battlefield);
    let land = put(&mut state, P0, "Island", Zone::Battlefield);
    let rancor = put(&mut state, P0, "Rancor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, opalescence, &[]);
    settled(&mut state);
    assert!(is_creature(&state, seal));
    assert_eq!(power_toughness(&state, seal), (2, 2));
    assert!(is_creature(&state, decree));
    assert_eq!(power_toughness(&state, decree), (8, 8));
    assert!(!is_creature(&state, opalescence), "only the others");
    assert!(!is_creature(&state, land));
    // An Aura stays an Aura.
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, rancor, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, rancor), Zone::Battlefield);
    assert!(!is_creature(&state, rancor));
    to_graveyard(&mut state, opalescence);
    assert!(!is_creature(&state, seal), "ends as Opalescence leaves");
}

#[test]
fn parallax_wave_fades_exiles_and_returns_its_cards() {
    let mut state = islands();
    let wave = put(&mut state, P0, "Parallax Wave", Zone::Hand);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let tog = put(&mut state, P1, "Psychatog", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, wave, &[]);
    settled(&mut state);
    assert_eq!(p1b::fade_counters(&state, wave), 5);
    activate(&mut state, wave, 0, &[Target::Object(elves)]);
    next(&mut state);
    assert_eq!(p1b::fade_counters(&state, wave), 4, "paid as activated");
    settled(&mut state);
    activate(&mut state, wave, 0, &[Target::Object(tog)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Exile);
    assert_eq!(zone(&state, tog), Zone::Exile);
    assert_eq!(p1b::fade_counters(&state, wave), 3);
    // Fading: one counter at its controller's upkeep.
    to_next_turn(&mut state, |_, _| None);
    assert_eq!(p1b::fade_counters(&state, wave), 2);
    // Leaving the battlefield returns them under their owner's control.
    to_graveyard(&mut state, wave);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, tog), Zone::Battlefield);
    assert_eq!(state.objects.get(elves).controller, P1);
}

#[test]
fn parallax_wave_sacrifices_itself_when_it_cannot_fade() {
    let mut state = islands();
    let wave = put(&mut state, P0, "Parallax Wave", Zone::Hand);
    let elves: Vec<ObjectId> = (0..5)
        .map(|_| put(&mut state, P1, "Llanowar Elves", Zone::Battlefield))
        .collect();
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, wave, &[]);
    settled(&mut state);
    for &elf in &elves {
        activate(&mut state, wave, 0, &[Target::Object(elf)]);
        settled(&mut state);
    }
    assert_eq!(p1b::fade_counters(&state, wave), 0);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(!activatable(&mut state, wave, 0), "no fade counter left");
    to_next_turn(&mut state, |_, _| None);
    assert_eq!(zone(&state, wave), Zone::Graveyard);
}

#[test]
fn phyrexian_furnace_exiles_graveyard_cards() {
    let mut state = islands();
    let furnace = put(&mut state, P0, "Phyrexian Furnace", Zone::Battlefield);
    let bottom = put(&mut state, P1, "Replenish", Zone::Graveyard);
    let top = put(&mut state, P1, "Opalescence", Zone::Graveyard);
    activate(&mut state, furnace, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, bottom), Zone::Exile);
    assert_eq!(zone(&state, top), Zone::Graveyard);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, furnace, 1, &[Target::Object(top)]);
    settled(&mut state);
    assert_eq!(zone(&state, top), Zone::Exile);
    assert_eq!(zone(&state, furnace), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn psychatog_grows_by_discarding_and_by_exiling_two_graveyard_cards() {
    let mut state = islands();
    let tog = put(&mut state, P0, "Psychatog", Zone::Battlefield);
    let card = put(&mut state, P0, "Island", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Graveyard);
    activate(&mut state, tog, 0, &[]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(zone(&state, card), Zone::Graveyard);
    assert_eq!(power_toughness(&state, tog), (2, 3));
    activate(&mut state, tog, 1, &[]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(state.players[0].graveyard.len(), 0);
    assert_eq!(power_toughness(&state, tog), (3, 4));
    assert!(!activatable(&mut state, tog, 1), "no graveyard left");
}

#[test]
fn quirion_dryad_grows_from_white_blue_black_and_red_spells() {
    let mut state = islands();
    let dryad = put(&mut state, P0, "Quirion Dryad", Zone::Battlefield);
    let leak = put(&mut state, P0, "Accumulated Knowledge", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, leak, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, dryad), (2, 2));
    let guile = put(&mut state, P0, "Mirri's Guile", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, guile, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, dryad), (2, 2), "green adds nothing");
}

#[test]
fn replenish_returns_enchantments_and_attaches_auras() {
    let mut state = islands();
    let land = put(&mut state, P0, "Forest", Zone::Battlefield);
    let seal = put(&mut state, P0, "Seal of Cleansing", Zone::Graveyard);
    let wave = put(&mut state, P0, "Parallax Wave", Zone::Graveyard);
    let growth = put(&mut state, P0, "Wild Growth", Zone::Graveyard);
    let rancor = put(&mut state, P0, "Rancor", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Standstill", Zone::Graveyard);
    let replenish = put(&mut state, P0, "Replenish", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 3);
    cast(&mut state, replenish, &[]);
    settle_picking(&mut state, 0, Some(land));
    assert_eq!(zone(&state, seal), Zone::Battlefield);
    assert_eq!(zone(&state, wave), Zone::Battlefield);
    assert_eq!(p1b::fade_counters(&state, wave), 5);
    assert_eq!(zone(&state, growth), Zone::Battlefield);
    assert_eq!(
        state
            .objects
            .get(growth)
            .v4
            .attached_to
            .map(|host| host.object),
        Some(land)
    );
    // No creature to enchant: Rancor stays.
    assert_eq!(zone(&state, rancor), Zone::Graveyard);
    assert_eq!(
        zone(&state, theirs),
        Zone::Graveyard,
        "only its controller's"
    );
}

#[test]
fn solitary_confinement_skips_draws_shrouds_and_prevents_damage() {
    let mut state = game(&["Island"; 10]);
    let confinement = put(&mut state, P0, "Solitary Confinement", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Hand);
    let city = put(&mut state, P0, "City of Brass", Zone::Battlefield);
    damage(&mut state, city, Target::Player(P0), 3);
    assert_eq!(
        state.players[0].life, 20,
        "damage to its controller prevented"
    );
    // Shroud: the controller can't be targeted.
    let furnace = put(&mut state, P1, "Phyrexian Furnace", Zone::Battlefield);
    state.active_player = P1;
    state.priority_player = P1;
    act(&mut state, Action::ActivateAbility(furnace, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Player(P0)));
            assert!(legal_targets.contains(&Target::Player(P1)));
        }
        other => panic!("expected targets, got {other:?}"),
    }

    // At upkeep: discard to keep it, and no draw step.
    let mut state = game(&["Island"; 10]);
    let confinement2 = put(&mut state, P0, "Solitary Confinement", Zone::Battlefield);
    let _ = confinement;
    let card = put(&mut state, P0, "Forest", Zone::Hand);
    to_next_turn(&mut state, |_, decision| match decision {
        // Discard rather than sacrifice.
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        _ => None,
    });
    assert_eq!(zone(&state, card), Zone::Graveyard);
    assert_eq!(zone(&state, confinement2), Zone::Battlefield);
    assert!(state.players[0].hand.is_empty(), "skipped the draw step");

    // With no card in hand, it is sacrificed.
    let mut state = game(&["Island"; 10]);
    let confinement3 = put(&mut state, P0, "Solitary Confinement", Zone::Battlefield);
    to_next_turn(&mut state, |_, _| None);
    assert_eq!(zone(&state, confinement3), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1, "drew once it was gone");
}

#[test]
fn standstill_is_sacrificed_and_the_casters_opponent_draws_three() {
    let mut state = islands();
    let standstill = put(&mut state, P0, "Standstill", Zone::Battlefield);
    let seal = put(&mut state, P1, "Seal of Cleansing", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::W], 1);
    cast(&mut state, seal, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, standstill), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 3);
    assert_eq!(state.players[1].hand.len(), 0);
    // Its own controller casting makes the opponent draw.
    let mut state = islands();
    put(&mut state, P0, "Standstill", Zone::Battlefield);
    let seal = put(&mut state, P0, "Seal of Cleansing", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, seal, &[]);
    settled(&mut state);
    assert_eq!(state.players[1].hand.len(), 3);
}

#[test]
fn sterling_grove_shrouds_enchantments_and_tutors_one_to_the_top() {
    let mut state = game(&["Island"; 10]);
    let grove = put(&mut state, P0, "Sterling Grove", Zone::Battlefield);
    let seal = put(&mut state, P0, "Seal of Cleansing", Zone::Battlefield);
    assert!(keyword(&state, seal, Keywords::SHROUD));
    assert!(
        !keyword(&state, grove, Keywords::SHROUD),
        "other enchantments"
    );
    let wanted = put(&mut state, P0, "Opalescence", Zone::Library);
    // Bury it under an Island.
    let library = &mut state.players[0].library;
    let position = library.iter().position(|&card| card == wanted).unwrap();
    library.remove(position);
    library.insert(3, wanted);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, grove, 0, &[]);
    settle_picking(&mut state, 0, Some(wanted));
    assert_eq!(zone(&state, grove), Zone::Graveyard);
    assert_eq!(state.players[0].library[0], wanted);
    assert!(!keyword(&state, seal, Keywords::SHROUD));
}

#[test]
fn tsabos_web_draws_and_keeps_utility_lands_tapped() {
    let mut state = game(&["Island"; 10]);
    let web = put(&mut state, P0, "Tsabo's Web", Zone::Hand);
    let bowl = put(&mut state, P0, "Dust Bowl", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, web, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, web), Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 1);
    event::propose_and_commit(&mut state, ProposedEvent::tap(bowl));
    event::propose_and_commit(&mut state, ProposedEvent::tap(island));
    to_next_turn(&mut state, |_, _| None);
    assert!(state.objects.get(bowl).tapped, "Dust Bowl stays tapped");
    assert!(!state.objects.get(island).tapped);
}
