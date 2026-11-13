//! Legacy sideboard wave S1g: native rules tests for Damping Sphere, which
//! completes Sewer-Cam, Painter and Mono-Black Midrange, Damping Sphere and
//! Drop of Honey, which complete Lands, and Alpine Moon and Ghost Vacuum,
//! which complete Sneak and Show.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::eternal_sideboard_s1g_v1::{
    alpine_chosen_name, alpine_name_options, alpine_option_name, read_state,
};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const SPHERE: &str = "Damping Sphere";
const HONEY: &str = "Drop of Honey";
const ALPINE: &str = "Alpine Moon";
const VACUUM: &str = "Ghost Vacuum";

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority. Each library
/// holds twenty Islands, and both hands are empty.
fn islands() -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5331_4700_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    assert!(state.players.iter().all(|seat| seat.hand.is_empty()));
    state
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

/// A permanent that entered from `player`'s hand, so it has a timestamp.
fn entered(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Hand);
    enter(state, object);
    settled(state);
    assert_eq!(zone(state, object), Zone::Battlefield);
    object
}

/// Puts `object` onto the battlefield under its owner's control, as if an
/// effect did.
fn enter(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Battlefield));
}

/// Moves `object` to its owner's graveyard, as if destroyed.
fn to_graveyard(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
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

fn has_type(state: &GameState, object: ObjectId, card_type: CardType) -> bool {
    engine::object_has_type(state, object, card_type)
}

fn has_subtype(state: &GameState, object: ObjectId, subtype: Subtype) -> bool {
    engine::has_effective_subtype(state, object, subtype)
}

fn keyword(state: &GameState, object: ObjectId, keyword: Keywords) -> bool {
    engine::has_effective_keyword(state, object, keyword)
}

fn power_toughness(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
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

/// The first legal answer to a choice.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets {
            can_finish: true,
            ref legal_targets,
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
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => panic!("unexpected choice: {other:?}"),
    }
}

/// The priority holder's castable spells and activatable abilities.
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

fn choose_target(state: &mut GameState, target: Target) {
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseTarget(target));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Casts `spell` for its mana cost, answering each target prompt from
/// `targets` in order.
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

/// Casts the modal `spell` choosing `mode`.
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

/// A serialized round trip of `state`.
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

/// Plays on until `stop` holds before a decision, answering it with
/// `answer` when that gives an action and otherwise passing priority, not
/// attacking or blocking, keeping triggers in order and taking first
/// answers.
fn run_until(
    state: &mut GameState,
    stop: impl Fn(&GameState) -> bool,
    mut answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    for _ in 0..3000 {
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

/// Plays from P0's turn to P1's, past P1's upkeep, answering choices with
/// `answer`.
fn to_opponents_turn(
    state: &mut GameState,
    answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    run_until(
        state,
        |state| state.active_player == P1 && matches!(state.step, Step::Draw | Step::Main1),
        answer,
    );
}

/// The triggered ability on top of the stack, as a target.
fn top_trigger(state: &GameState) -> Option<Target> {
    state
        .stack
        .last()
        .filter(|item| item.kind == StackItemKind::TriggeredAbility)
        .map(|item| Target::StackItem(item.v4.stack_item_id))
}

fn triggers_on_stack(state: &GameState) -> usize {
    state
        .stack
        .iter()
        .filter(|item| item.kind == StackItemKind::TriggeredAbility)
        .count()
}

/// The Alpine Moon option that names `land`.
fn alpine_option(land: &str) -> u16 {
    (0..alpine_name_options().len())
        .map(|option| u16::try_from(option).unwrap())
        .find(|&option| alpine_option_name(option) == Some(land))
        .unwrap_or_else(|| panic!("{land} is not a nonbasic land name"))
}

fn alpine_names() -> Vec<&'static str> {
    (0..alpine_name_options().len())
        .map(|option| alpine_option_name(u16::try_from(option).unwrap()).unwrap())
        .collect()
}

/// An Alpine Moon that entered from `player`'s hand, naming `land`.
fn moon_naming(state: &mut GameState, player: PlayerId, land: &str) -> ObjectId {
    let moon = put(state, player, ALPINE, Zone::Hand);
    enter(state, moon);
    match settle(state) {
        Some(Decision::ChooseEffectOption {
            player: chooser,
            source,
            option_count,
        }) => {
            assert_eq!((chooser, source), (player, moon));
            assert_eq!(usize::from(option_count), alpine_name_options().len());
            act(state, Action::ChooseEffectOption(alpine_option(land)));
        }
        other => panic!("expected the name choice, got {other:?}"),
    }
    settled(state);
    assert_eq!(alpine_chosen_name(state, moon), Some(land));
    moon
}

/// Gives `vacuum` its untapped state back and six colorless mana to its
/// controller, then activates its return ability.
fn vacuum_return(state: &mut GameState, vacuum: ObjectId) {
    state.objects.get_mut(vacuum).tapped = false;
    let controller = state.objects.get(vacuum).controller;
    pool(state, controller, &[], 6);
    activate(state, vacuum, 1, &[]);
    settled(state);
}

/// Exiles `card` from a graveyard with `vacuum`'s first ability.
fn vacuum_exile(state: &mut GameState, vacuum: ObjectId, card: ObjectId) {
    state.objects.get_mut(vacuum).tapped = false;
    activate(state, vacuum, 0, &[Target::Object(card)]);
    settled(state);
    assert_eq!(zone(state, card), Zone::Exile);
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn s1g_cards_are_full_with_their_printed_characteristics() {
    for name in [SPHERE, HONEY, ALPINE, VACUUM] {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let sphere = def(SPHERE);
    assert_eq!(sphere.types, &[CardType::Artifact][..]);
    assert_eq!((sphere.mana_value, sphere.colors), (2, &[][..]));
    let honey = def(HONEY);
    assert_eq!(honey.types, &[CardType::Enchantment][..]);
    assert_eq!((honey.mana_value, honey.colors), (1, &[ManaColor::G][..]));
    let moon = def(ALPINE);
    assert_eq!(moon.types, &[CardType::Enchantment][..]);
    assert_eq!((moon.mana_value, moon.colors), (1, &[ManaColor::R][..]));
    let vacuum = def(VACUUM);
    assert_eq!(vacuum.types, &[CardType::Artifact][..]);
    assert_eq!((vacuum.mana_value, vacuum.colors), (1, &[][..]));
    assert_eq!(vacuum.activated_abilities.len(), 2);
    assert!(!vacuum.activated_abilities[0].sorcery_speed_only);
    assert!(vacuum.activated_abilities[1].sorcery_speed_only);
}

// ---- Damping Sphere: mana ----------------------------------------------------------

#[test]
fn damping_sphere_makes_a_land_tapped_for_two_or_more_produce_one_colorless() {
    let mut state = islands();
    // An opponent's Sphere applies to every land.
    let sphere = put(&mut state, P1, SPHERE, Zone::Battlefield);
    let tomb = put(&mut state, P0, "Ancient Tomb", Zone::Battlefield);
    let city = put(&mut state, P0, "City of Traitors", Zone::Battlefield);
    let chancery = put(&mut state, P0, "Azorius Chancery", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(tomb));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
    assert_eq!(state.players[0].life, 18, "its damage stays");
    assert!(state.objects.get(tomb).tapped);
    act(&mut state, Action::ActivateManaAbility(city));
    assert_eq!(mana(&state, P0, ManaColor::C), 2);
    // "{T}: Add {W}{U}" is two mana.
    act(&mut state, Action::ActivateManaAbility(chancery));
    assert_eq!(mana(&state, P0, ManaColor::C), 3);
    assert_eq!(
        mana(&state, P0, ManaColor::W) + mana(&state, P0, ManaColor::U),
        0
    );
    // A land tapped for one mana and a nonland source are unchanged.
    act(&mut state, Action::ActivateManaAbility(island));
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
    act(&mut state, Action::ActivateManaAbility(ring));
    assert_eq!(mana(&state, P0, ManaColor::C), 5);

    // Without the Sphere the Tomb adds {C}{C} again.
    to_graveyard(&mut state, sphere);
    state.objects.get_mut(tomb).tapped = false;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(tomb));
    assert_eq!(mana(&state, P0, ManaColor::C), 7);
    assert_eq!(state.players[0].life, 16);
}

#[test]
fn damping_sphere_reads_what_a_land_would_add_as_it_is_tapped() {
    // Gaea's Cradle with one creature adds {G}; with two, one {C}.
    let mut state = islands();
    put(&mut state, P0, SPHERE, Zone::Battlefield);
    let cradle = put(&mut state, P0, "Gaea's Cradle", Zone::Battlefield);
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(cradle));
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    state.objects.get_mut(cradle).tapped = false;
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(cradle));
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 1);

    // An assembled Urza's Tower and Tolarian Academy with two artifacts.
    let mut state = islands();
    put(&mut state, P1, SPHERE, Zone::Battlefield);
    let tower = put(&mut state, P0, "Urza's Tower", Zone::Battlefield);
    put(&mut state, P0, "Urza's Mine", Zone::Battlefield);
    put(&mut state, P0, "Urza's Power Plant", Zone::Battlefield);
    let academy = put(&mut state, P0, "Tolarian Academy", Zone::Battlefield);
    put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(tower));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
    act(&mut state, Action::ActivateManaAbility(academy));
    assert_eq!(mana(&state, P0, ManaColor::C), 2);
    assert_eq!(mana(&state, P0, ManaColor::U), 0);
}

#[test]
fn damping_sphere_keeps_restricted_mana_restricted() {
    let mut state = islands();
    put(&mut state, P1, SPHERE, Zone::Battlefield);
    let workshop = put(&mut state, P0, "Mishra's Workshop", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(workshop));
    assert_eq!(state.players[0].restricted_mana_pool.0.len(), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
    assert!(state.players[0].restricted_mana_pool.0.is_empty());
}

#[test]
fn damping_sphere_shrinks_what_the_payment_planner_counts() {
    // Assembled Tron pays seven; under a Sphere, three.
    let mut state = islands();
    for land in ["Urza's Tower", "Urza's Mine", "Urza's Power Plant"] {
        put(&mut state, P0, land, Zone::Battlefield);
    }
    let seer = put(&mut state, P0, "Thought-Knot Seer", Zone::Hand);
    assert!(castable(&mut state, seer));
    put(&mut state, P1, SPHERE, Zone::Battlefield);
    assert!(!castable(&mut state, seer), "three {{C}}");
    put(&mut state, P0, "Island", Zone::Battlefield);
    assert!(castable(&mut state, seer));

    // Eldrazi Temple's {C}{C} for a colorless Eldrazi spell is one {C}.
    let mut state = islands();
    let temple = put(&mut state, P0, "Eldrazi Temple", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Battlefield);
    let seer = put(&mut state, P0, "Thought-Knot Seer", Zone::Hand);
    assert!(castable(&mut state, seer));
    put(&mut state, P1, SPHERE, Zone::Battlefield);
    assert!(!castable(&mut state, seer));
    // Its "{T}: Add {C}." is unchanged.
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(temple));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
}

#[test]
fn damping_sphere_and_contamination_make_a_land_produce_one_black() {
    let mut state = islands();
    put(&mut state, P1, SPHERE, Zone::Battlefield);
    put(&mut state, P1, "Contamination", Zone::Battlefield);
    let tomb = put(&mut state, P0, "Ancient Tomb", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(tomb));
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(state.players[0].life, 18);
}

// ---- Damping Sphere: spell tax ---------------------------------------------------------

#[test]
fn damping_sphere_taxes_each_spell_by_the_other_spells_its_caster_cast_this_turn() {
    let mut state = islands();
    let bolts = [(); 3].map(|()| put(&mut state, P0, "Lightning Bolt", Zone::Hand));
    // A spell cast before the Sphere arrived still counts (its ruling).
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolts[0], &[Target::Player(P1)]);
    settled(&mut state);
    put(&mut state, P1, SPHERE, Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, bolts[1]), "{{1}}{{R}}");
    pool(&mut state, P0, &[], 1);
    cast(&mut state, bolts[1], &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(
        mana(&state, P0, ManaColor::R) + mana(&state, P0, ManaColor::C),
        0
    );
    // The third costs {2}{R}; under a second Sphere, {4}{R}.
    pool(&mut state, P0, &[ManaColor::R], 2);
    assert!(castable(&mut state, bolts[2]));
    put(&mut state, P0, SPHERE, Zone::Battlefield);
    assert!(!castable(&mut state, bolts[2]));
    pool(&mut state, P0, &[], 2);
    cast(&mut state, bolts[2], &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 11);
    assert_eq!(
        def("Lightning Bolt").mana_value,
        1,
        "mana value is unchanged"
    );

    // The other player's first spell this turn costs no more.
    let theirs = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(castable(&mut state, theirs));
}

// ---- Drop of Honey ----------------------------------------------------------------------

#[test]
fn drop_of_honey_destroys_the_creature_with_the_least_power_at_its_controllers_upkeep() {
    let mut state = islands();
    let drop = put(&mut state, P1, HONEY, Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    // It can't be regenerated.
    mtg_kernel::eternal_vintage_e7b_v1::statics::add_regeneration_shield(&mut state, elves);
    to_opponents_turn(&mut state, |_, _| None);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(zone(&state, drop), Zone::Battlefield, "a creature is left");
}

#[test]
fn drop_of_honey_lets_its_controller_choose_among_creatures_tied_for_least_power() {
    let mut state = islands();
    put(&mut state, P1, HONEY, Zone::Battlefield);
    let ours = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let mut offered = Vec::new();
    to_opponents_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            selected_count: 0,
            ..
        } => {
            offered.push((*player, legal_targets.clone()));
            Some(Action::ChooseEffectTarget(Target::Object(theirs)))
        }
        _ => None,
    });
    assert_eq!(
        offered,
        vec![(P1, vec![Target::Object(ours), Target::Object(theirs)])]
    );
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, ours), Zone::Battlefield);
}

#[test]
fn drop_of_honey_passes_over_indestructible_creatures() {
    // The only creature is indestructible: nothing is destroyed.
    let mut state = islands();
    let drop = put(&mut state, P1, HONEY, Zone::Battlefield);
    let lage = token(&mut state, P1, "Marit Lage Token");
    to_opponents_turn(&mut state, |_, _| None);
    assert_eq!(zone(&state, lage), Zone::Battlefield);
    assert_eq!(zone(&state, drop), Zone::Battlefield);

    // Tied for least power with an indestructible creature: the other one
    // is destroyed, with no choice.
    let mut state = islands();
    put(&mut state, P1, HONEY, Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let mage = put(&mut state, P0, "Snapcaster Mage", Zone::Battlefield);
    let origin = put(&mut state, P1, "Origin of Metalbending", Zone::Hand);
    run_until(
        &mut state,
        |state| state.active_player == P1 && top_trigger(state).is_some(),
        |_, _| None,
    );
    pool(&mut state, P1, &[ManaColor::G], 1);
    cast_mode(&mut state, origin, 1, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, elves), (2, 2));
    assert!(keyword(&state, elves, Keywords::INDESTRUCTIBLE));
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, mage), Zone::Graveyard, "tied at 2 power");
}

#[test]
fn drop_of_honey_is_sacrificed_when_no_creatures_are_on_the_battlefield() {
    let mut state = islands();
    let drop = put(&mut state, P0, HONEY, Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, drop), Zone::Graveyard);
    assert!(read_state(&state).is_none_or(|s1g| s1g.honey_triggered.is_empty()));
}

#[test]
fn drop_of_honey_triggers_once_and_is_sacrificed_even_if_a_creature_returns() {
    let mut state = islands();
    let drop = put(&mut state, P0, HONEY, Zone::Hand);
    let first = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    let second = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    enter(&mut state, drop);
    next(&mut state);
    assert_eq!(triggers_on_stack(&state), 1);
    // Not again while that instance is on the stack (603.8), even after a
    // creature came and went.
    enter(&mut state, first);
    next(&mut state);
    to_graveyard(&mut state, first);
    next(&mut state);
    assert_eq!(triggers_on_stack(&state), 1);
    let snapshot = restored(&state);
    assert_eq!(read_state(&snapshot), read_state(&state));
    // A creature is back by resolution, and the Drop is sacrificed anyway
    // (its ruling).
    enter(&mut state, second);
    settled(&mut state);
    assert_eq!(zone(&state, drop), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Battlefield);
}

#[test]
fn drop_of_honey_triggers_again_after_its_trigger_is_countered() {
    let mut state = islands();
    let drop = put(&mut state, P0, HONEY, Zone::Hand);
    let stifle = put(&mut state, P0, "Stifle", Zone::Hand);
    enter(&mut state, drop);
    next(&mut state);
    let trigger = top_trigger(&state).expect("the state trigger");
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, stifle, &[trigger]);
    settled(&mut state);
    assert_eq!(zone(&state, stifle), Zone::Graveyard);
    assert_eq!(
        zone(&state, drop),
        Zone::Graveyard,
        "the second instance sacrificed it"
    );
}

// ---- Alpine Moon ------------------------------------------------------------------------

#[test]
fn alpine_moon_may_name_any_nonbasic_land_card() {
    let names = alpine_names();
    for land in [
        "Ancient Tomb",
        "Karakas",
        "Volcanic Island",
        "Mishra's Factory",
        "Dryad Arbor",
        // A modal double-faced card's land face.
        "Shatterskull, the Hammer Pass",
    ] {
        assert!(names.contains(&land), "{land}");
    }
    for other in [
        "Island",
        "Forest",
        "Lightning Bolt",
        "Shatterskull Smashing",
    ] {
        assert!(!names.contains(&other), "{other}");
    }
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "one option per name");

    let mut state = islands();
    let moon = put(&mut state, P0, ALPINE, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, moon, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption {
            player,
            source,
            option_count,
        }) => {
            assert_eq!((player, source), (P0, moon));
            assert_eq!(usize::from(option_count), names.len());
            assert!(
                engine::step(&mut state.clone(), Action::ChooseEffectOption(option_count)).is_err()
            );
            act(
                &mut state,
                Action::ChooseEffectOption(alpine_option("Karakas")),
            );
        }
        other => panic!("expected the name choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(alpine_chosen_name(&state, moon), Some("Karakas"));
    let snapshot = restored(&state);
    assert_eq!(alpine_chosen_name(&snapshot, moon), Some("Karakas"));
    assert_eq!(read_state(&snapshot), read_state(&state));

    // The name belongs to that incarnation, and is dropped as a turn begins.
    to_graveyard(&mut state, moon);
    assert_eq!(alpine_chosen_name(&state, moon), None);
    to_opponents_turn(&mut state, |_, _| None);
    assert!(read_state(&state).is_none_or(|s1g| s1g.alpine_names.is_empty()));
}

#[test]
fn alpine_moon_strips_opponents_lands_with_the_chosen_name() {
    let mut state = islands();
    let theirs = put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    let ours = put(&mut state, P0, "Volcanic Island", Zone::Battlefield);
    let tomb = put(&mut state, P1, "Ancient Tomb", Zone::Battlefield);
    let moon = moon_naming(&mut state, P0, "Volcanic Island");
    assert!(!has_subtype(&state, theirs, Subtype::Island));
    assert!(!has_subtype(&state, theirs, Subtype::Mountain));
    assert!(has_type(&state, theirs, CardType::Land), "still a land");
    // Lands its controller controls are unaffected.
    assert!(has_subtype(&state, ours, Subtype::Island));
    assert!(has_subtype(&state, ours, Subtype::Mountain));
    // The named land taps for any color; another land keeps its ability.
    state.priority_player = P1;
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(theirs, ManaColor::G),
    );
    assert_eq!(mana(&state, P1, ManaColor::G), 1);
    act(&mut state, Action::ActivateManaAbility(tomb));
    assert_eq!(mana(&state, P1, ManaColor::C), 2);
    // Once the Moon leaves, the land is itself again.
    to_graveyard(&mut state, moon);
    assert!(has_subtype(&state, theirs, Subtype::Island));
    assert!(has_subtype(&state, theirs, Subtype::Mountain));
}

#[test]
fn alpine_moon_removes_the_named_lands_abilities() {
    let mut state = islands();
    let tomb = put(&mut state, P1, "Ancient Tomb", Zone::Battlefield);
    moon_naming(&mut state, P0, "Ancient Tomb");
    state.priority_player = P1;
    next(&mut state);
    assert!(
        engine::step(
            &mut state.clone(),
            Action::ActivateManaAbilityChoice(tomb, ManaColor::C)
        )
        .is_err(),
        "no {{C}}{{C}}"
    );
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(tomb, ManaColor::R),
    );
    assert_eq!(mana(&state, P1, ManaColor::R), 1);
    assert_eq!(mana(&state, P1, ManaColor::C), 0);
    assert_eq!(state.players[1].life, 20, "and no damage");
}

#[test]
fn alpine_moon_and_blood_moon_apply_in_timestamp_order() {
    // Blood Moon after the Alpine Moon: the land is a Mountain, and it
    // still taps for any color.
    let mut state = islands();
    let volcanic = put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    moon_naming(&mut state, P0, "Volcanic Island");
    entered(&mut state, P0, "Blood Moon");
    assert!(has_subtype(&state, volcanic, Subtype::Mountain));
    assert!(!has_subtype(&state, volcanic, Subtype::Island));
    state.priority_player = P1;
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(volcanic, ManaColor::G),
    );
    assert_eq!(mana(&state, P1, ManaColor::G), 1);

    // The Alpine Moon after Blood Moon: no land types (613.7).
    let mut state = islands();
    let volcanic = put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    entered(&mut state, P0, "Blood Moon");
    assert!(has_subtype(&state, volcanic, Subtype::Mountain));
    moon_naming(&mut state, P0, "Volcanic Island");
    assert!(!has_subtype(&state, volcanic, Subtype::Mountain));
    assert!(!has_subtype(&state, volcanic, Subtype::Island));
    state.priority_player = P1;
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(volcanic, ManaColor::G),
    );
    assert_eq!(mana(&state, P1, ManaColor::G), 1);
}

// ---- Ghost Vacuum -----------------------------------------------------------------------

#[test]
fn ghost_vacuum_returns_the_creature_cards_it_exiled_as_flying_1_1_spirits() {
    let mut state = islands();
    let vacuum = put(&mut state, P0, VACUUM, Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    vacuum_exile(&mut state, vacuum, angler);
    vacuum_exile(&mut state, vacuum, bolt);
    assert_eq!(
        read_state(&state).map(|s1g| s1g.vacuum_exiles.len()),
        Some(2)
    );
    vacuum_return(&mut state, vacuum);
    assert_eq!(zone(&state, vacuum), Zone::Graveyard);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).controller, P0);
    assert_eq!(state.objects.get(angler).owner, P1);
    assert_eq!(zone(&state, bolt), Zone::Exile, "not a creature card");
    assert_eq!(power_toughness(&state, angler), (1, 1));
    assert!(keyword(&state, angler, Keywords::FLYING));
    assert!(has_subtype(&state, angler, Subtype::Spirit));
    assert!(has_subtype(&state, angler, Subtype::Zombie), "in addition");
    let s1g = read_state(&state).unwrap();
    assert!(s1g.vacuum_exiles.is_empty());
    assert_eq!(s1g.spirits.len(), 1);
    let snapshot = restored(&state);
    assert_eq!(read_state(&snapshot), read_state(&state));
    assert!(keyword(&snapshot, angler, Keywords::FLYING));

    // Counters apply on top of the 1/1 base.
    state.objects.get_mut(angler).counters.plus1_plus1 = 2;
    assert_eq!(power_toughness(&state, angler), (3, 3));

    // Once it leaves, it is the card again, and its record is dropped as a
    // turn begins.
    to_graveyard(&mut state, angler);
    assert!(!has_subtype(&state, angler, Subtype::Spirit));
    to_opponents_turn(&mut state, |_, _| None);
    assert!(read_state(&state).is_none_or(|s1g| s1g.spirits.is_empty()));
}

#[test]
fn ghost_vacuum_returns_only_the_cards_that_incarnation_exiled() {
    let mut state = islands();
    let first = put(&mut state, P0, VACUUM, Zone::Battlefield);
    let second = put(&mut state, P0, VACUUM, Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    vacuum_exile(&mut state, first, angler);
    vacuum_exile(&mut state, second, elves);
    vacuum_return(&mut state, second);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, angler), Zone::Exile);

    // A card that left exile is a new object, and stays exiled (607.2a).
    let mut state = islands();
    let vacuum = put(&mut state, P0, VACUUM, Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    vacuum_exile(&mut state, vacuum, angler);
    to_graveyard(&mut state, angler);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(angler, Zone::Exile));
    vacuum_return(&mut state, vacuum);
    assert_eq!(zone(&state, angler), Zone::Exile);

    // A Vacuum that left the battlefield keeps no exiles past the turn.
    let mut state = islands();
    let vacuum = put(&mut state, P0, VACUUM, Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    vacuum_exile(&mut state, vacuum, angler);
    to_graveyard(&mut state, vacuum);
    to_opponents_turn(&mut state, |_, _| None);
    assert!(read_state(&state).is_none_or(|s1g| s1g.vacuum_exiles.is_empty()));
}

#[test]
fn ghost_vacuum_exiles_at_instant_speed_and_returns_at_sorcery_speed() {
    let mut state = islands();
    let vacuum = put(&mut state, P0, VACUUM, Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    pool(&mut state, P0, &[], 6);
    assert!(activatable(&mut state, vacuum, 1));
    // With a spell on the stack.
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    let (_, abilities) = priority(&mut state);
    assert!(abilities.contains(&(vacuum, 0)));
    assert!(!abilities.contains(&(vacuum, 1)));
    settled(&mut state);
    // In the opponent's turn.
    state.active_player = P1;
    state.priority_player = P0;
    let (_, abilities) = priority(&mut state);
    assert!(abilities.contains(&(vacuum, 0)));
    assert!(!abilities.contains(&(vacuum, 1)));
}

#[test]
fn a_later_ability_and_type_change_overrides_a_ghost_vacuum_spirit() {
    let mut state = islands();
    let vacuum = put(&mut state, P0, VACUUM, Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    vacuum_exile(&mut state, vacuum, angler);
    vacuum_return(&mut state, vacuum);
    assert!(keyword(&state, angler, Keywords::FLYING));
    // Oko's +1: "loses all abilities and becomes a green Elk creature with
    // base power and toughness 3/3".
    let oko = put(&mut state, P0, "Oko, Thief of Crowns", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, oko, &[]);
    settled(&mut state);
    activate(&mut state, oko, 1, &[Target::Object(angler)]);
    settled(&mut state);
    assert!(!keyword(&state, angler, Keywords::FLYING));
    assert_eq!(power_toughness(&state, angler), (3, 3));
    assert!(has_subtype(&state, angler, Subtype::Elk));
    assert!(!has_subtype(&state, angler, Subtype::Spirit));
}
