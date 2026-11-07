//! Premodern wave P2a: native rules tests for the cards that complete The
//! Rock, Mono-Black Control, BW Control, Stasis and Other Control.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P2A_CARDS: [&str; 31] = [
    "Academy Rector",
    "Bane of the Living",
    "Black Vise",
    "Contamination",
    "Diabolic Tutor",
    "Dromar's Charm",
    "Eternal Dragon",
    "Forsaken City",
    "Future Sight",
    "Gaea's Blessing",
    "Genesis",
    "Gerrard's Verdict",
    "High Market",
    "Innocent Blood",
    "Krosan Tusker",
    "Mana Short",
    "Nether Spirit",
    "Nevinyrral's Disk",
    "Pernicious Deed",
    "Phyrexian Tower",
    "Ravenous Baloth",
    "Ravenous Rats",
    "Root Maze",
    "Stasis",
    "Tainted Field",
    "Tainted Wood",
    "Upheaval",
    "Veteran Explorer",
    "Vindicate",
    "Wall of Blossoms",
    "Zombie Infestation",
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
        0x5032_4100_0001,
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

/// Answers a card selection by choosing `wanted` in order, then finishing.
fn selection(wanted: &[ObjectId], decision: &Decision) -> Option<Action> {
    match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count,
            ..
        } => match wanted.get(*selected_count as usize) {
            Some(card) if legal_targets.contains(&Target::Object(*card)) => {
                Some(Action::ChooseEffectTarget(Target::Object(*card)))
            }
            _ => Some(Action::FinishEffectSelection),
        },
        _ => None,
    }
}

/// Settles, choosing `wanted` in every card selection.
fn choose_cards(wanted: Vec<ObjectId>) -> impl FnMut(&GameState, Decision) -> Action {
    move |_, decision| selection(&wanted, &decision).unwrap_or_else(|| first_answer(decision))
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

/// Answers the X announcement of a pending choice with `x`.
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

/// Puts `object` onto the battlefield under its owner's control, as if an
/// effect did.
fn enter(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Battlefield));
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p2a_cards_are_full_with_their_printed_characteristics() {
    for name in P2A_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    assert!(def("Wall of Blossoms").keywords.has(Keywords::DEFENDER));
    assert!(def("Eternal Dragon").keywords.has(Keywords::FLYING));
    assert_eq!(
        (
            def("Bane of the Living").power,
            def("Bane of the Living").toughness
        ),
        (Some(4), Some(3))
    );
    assert!(def("Bane of the Living")
        .subtypes
        .contains(&Subtype::Insect));
    assert!(def("Genesis").subtypes.contains(&Subtype::Incarnation));
    assert!(def("Krosan Tusker").subtypes.contains(&Subtype::Beast));
    assert!(def("Nevinyrral's Disk").enters_battlefield_tapped);
    assert!(def("Phyrexian Tower")
        .supertypes
        .contains(&mtg_kernel::card_def::Supertype::Legendary));
    assert_eq!(def("Pernicious Deed").activated_abilities.len(), 9);
    assert_eq!(def("Eternal Dragon").activated_abilities.len(), 2);
    assert_eq!(def("Krosan Tusker").mana_value, 7);
}

// ---- Creatures with plain triggers -----------------------------------------------

#[test]
fn wall_of_blossoms_draws_a_card_as_it_enters() {
    let mut state = game(&["Forest"; 10]);
    let wall = put(&mut state, P0, "Wall of Blossoms", Zone::Hand);
    let top = state.players[0].library[0];
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, wall, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, wall), Zone::Battlefield);
    assert_eq!(zone(&state, top), Zone::Hand);
}

#[test]
fn ravenous_rats_makes_the_opponent_discard() {
    let mut state = islands();
    let rats = put(&mut state, P0, "Ravenous Rats", Zone::Hand);
    let card = put(&mut state, P1, "Forest", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, rats, &[]);
    settle_picking(&mut state, 0, None);
    assert_eq!(zone(&state, rats), Zone::Battlefield);
    assert_eq!(zone(&state, card), Zone::Graveyard);
}

#[test]
fn ravenous_baloth_sacrifices_a_beast_for_four_life() {
    let mut state = islands();
    let baloth = put(&mut state, P0, "Ravenous Baloth", Zone::Battlefield);
    let tusker = put(&mut state, P0, "Krosan Tusker", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    activate(&mut state, baloth, 0, &[]);
    // Sacrifice a Beast, not the Elves.
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&baloth) && candidates.contains(&tusker));
            assert!(!candidates.contains(&elves));
            Action::ChooseCostTarget(tusker)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, tusker), Zone::Graveyard);
    assert_eq!(zone(&state, baloth), Zone::Battlefield);
    assert_eq!(state.players[0].life, 24);
}

#[test]
fn veteran_explorer_lets_each_player_fetch_two_basic_lands() {
    let mut state = game(&["Forest"; 10]);
    let explorer = put(&mut state, P0, "Veteran Explorer", Zone::Battlefield);
    to_graveyard(&mut state, explorer);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count,
            ..
        } if selected_count < 2 => Action::ChooseEffectTarget(legal_targets[0]),
        Decision::ChooseEffectTargets { .. } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(controls(&state, P0, "Forest").len(), 2);
    assert_eq!(controls(&state, P1, "Forest").len(), 2);
    assert_eq!(state.players[0].library.len(), 8);
    assert_eq!(state.players[1].library.len(), 8);
}

#[test]
fn academy_rector_exiles_itself_to_fetch_an_enchantment() {
    let mut state = islands();
    let rector = put(&mut state, P0, "Academy Rector", Zone::Battlefield);
    let enchantment = put(&mut state, P0, "Mirri's Guile", Zone::Library);
    to_graveyard(&mut state, rector);
    settle_picking(&mut state, 1, Some(enchantment));
    assert_eq!(zone(&state, rector), Zone::Exile);
    assert_eq!(zone(&state, enchantment), Zone::Battlefield);
    // Declining leaves it in the graveyard.
    let mut state = islands();
    let rector = put(&mut state, P0, "Academy Rector", Zone::Battlefield);
    let enchantment = put(&mut state, P0, "Mirri's Guile", Zone::Library);
    to_graveyard(&mut state, rector);
    settle_picking(&mut state, 0, None);
    assert_eq!(zone(&state, rector), Zone::Graveyard);
    assert_eq!(zone(&state, enchantment), Zone::Library);
}

#[test]
fn krosan_tusker_cycles_and_may_fetch_a_basic_land() {
    let mut state = game(&["Island"; 10]);
    let tusker = put(&mut state, P0, "Krosan Tusker", Zone::Hand);
    let forest = put(&mut state, P0, "Forest", Zone::Library);
    pool(&mut state, P0, &[ManaColor::G], 2);
    activate(&mut state, tusker, 0, &[]);
    settle_picking(&mut state, 1, Some(forest));
    assert_eq!(zone(&state, tusker), Zone::Graveyard);
    assert_eq!(zone(&state, forest), Zone::Hand);
    // The cycling drew a card too.
    assert_eq!(state.players[0].hand.len(), 2);
}

// ---- Graveyard abilities -------------------------------------------------------------

#[test]
fn genesis_returns_a_creature_card_at_upkeep_for_two_green() {
    let mut state = game(&["Island"; 10]);
    let genesis = put(&mut state, P0, "Genesis", Zone::Graveyard);
    let rats = put(&mut state, P0, "Ravenous Rats", Zone::Graveyard);
    for _ in 0..3 {
        put(&mut state, P0, "Forest", Zone::Battlefield);
    }
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(genesis)));
            Some(Action::ChooseTarget(Target::Object(rats)))
        }
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        _ => None,
    });
    assert_eq!(zone(&state, rats), Zone::Hand);
    assert_eq!(zone(&state, genesis), Zone::Graveyard);
    assert!(controls(&state, P0, "Forest")
        .iter()
        .all(|&forest| state.objects.get(forest).tapped));
}

#[test]
fn genesis_offers_nothing_without_the_mana() {
    let mut state = game(&["Island"; 10]);
    put(&mut state, P0, "Genesis", Zone::Graveyard);
    let rats = put(&mut state, P0, "Ravenous Rats", Zone::Graveyard);
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => panic!("nothing to pay with"),
        Decision::ChooseTargets { legal_targets, .. } => {
            Some(Action::ChooseTarget(legal_targets[0]))
        }
        _ => None,
    });
    assert_eq!(zone(&state, rats), Zone::Graveyard);
}

#[test]
fn nether_spirit_returns_only_as_the_lone_creature_card() {
    let mut state = game(&["Island"; 10]);
    let spirit = put(&mut state, P0, "Nether Spirit", Zone::Graveyard);
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        _ => None,
    });
    assert_eq!(zone(&state, spirit), Zone::Battlefield);

    let mut state = game(&["Island"; 10]);
    let spirit = put(&mut state, P0, "Nether Spirit", Zone::Graveyard);
    put(&mut state, P0, "Ravenous Rats", Zone::Graveyard);
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => panic!("another creature card is there"),
        _ => None,
    });
    assert_eq!(zone(&state, spirit), Zone::Graveyard);

    // Declining leaves it there.
    let mut state = game(&["Island"; 10]);
    let spirit = put(&mut state, P0, "Nether Spirit", Zone::Graveyard);
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(0)),
        _ => None,
    });
    assert_eq!(zone(&state, spirit), Zone::Graveyard);
}

#[test]
fn eternal_dragon_returns_to_hand_only_during_its_owners_upkeep() {
    let mut state = islands();
    let dragon = put(&mut state, P0, "Eternal Dragon", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 3);
    assert!(
        !activatable(&mut state, dragon, 0),
        "not in the first main phase"
    );
    state.step = Step::Upkeep;
    assert!(activatable(&mut state, dragon, 0));
    activate(&mut state, dragon, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, dragon), Zone::Hand);
    // The opponent's upkeep is not its owner's.
    let mut state = islands();
    let dragon = put(&mut state, P0, "Eternal Dragon", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 3);
    state.step = Step::Upkeep;
    state.active_player = P1;
    assert!(!activatable(&mut state, dragon, 0));
}

#[test]
fn eternal_dragon_plainscycles_for_two() {
    let mut state = game(&["Island"; 10]);
    let dragon = put(&mut state, P0, "Eternal Dragon", Zone::Hand);
    let plains = put(&mut state, P0, "Plains", Zone::Library);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, dragon, 1, &[]);
    settle_picking(&mut state, 0, Some(plains));
    assert_eq!(zone(&state, dragon), Zone::Graveyard);
    assert_eq!(zone(&state, plains), Zone::Hand);
}

// ---- Spells ---------------------------------------------------------------------------

#[test]
fn diabolic_tutor_searches_for_any_card() {
    let mut state = game(&["Island"; 10]);
    let tutor = put(&mut state, P0, "Diabolic Tutor", Zone::Hand);
    let target = put(&mut state, P0, "Vindicate", Zone::Library);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    cast(&mut state, tutor, &[]);
    settle_picking(&mut state, 0, Some(target));
    assert_eq!(zone(&state, target), Zone::Hand);
}

#[test]
fn innocent_blood_makes_each_player_sacrifice_a_creature() {
    let mut state = islands();
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Wall of Blossoms", Zone::Battlefield);
    let other = put(&mut state, P1, "Ravenous Rats", Zone::Battlefield);
    let blood = put(&mut state, P0, "Innocent Blood", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, blood, &[]);
    settle_picking(&mut state, 0, Some(theirs));
    assert_eq!(zone(&state, mine), Zone::Graveyard);
    let survivors = [theirs, other]
        .into_iter()
        .filter(|&creature| zone(&state, creature) == Zone::Battlefield)
        .count();
    assert_eq!(survivors, 1, "the opponent sacrificed one of two");
}

#[test]
fn vindicate_destroys_any_permanent() {
    for target_name in ["Forest", "Wall of Blossoms", "Mirri's Guile"] {
        let mut state = islands();
        let target = put(&mut state, P1, target_name, Zone::Battlefield);
        let vindicate = put(&mut state, P0, "Vindicate", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::W, ManaColor::B], 1);
        cast(&mut state, vindicate, &[Target::Object(target)]);
        settled(&mut state);
        assert_eq!(zone(&state, target), Zone::Graveyard, "{target_name}");
    }
}

#[test]
fn gerrards_verdict_discards_two_and_gains_three_per_land() {
    let mut state = islands();
    state.players[1].hand.clear();
    let forest = put(&mut state, P1, "Forest", Zone::Hand);
    let wall = put(&mut state, P1, "Wall of Blossoms", Zone::Hand);
    let kept = put(&mut state, P1, "Ravenous Rats", Zone::Hand);
    let verdict = put(&mut state, P0, "Gerrard's Verdict", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::B], 0);
    cast(&mut state, verdict, &[Target::Player(P1)]);
    settle_with(&mut state, choose_cards(vec![forest, wall]));
    assert_eq!(zone(&state, forest), Zone::Graveyard);
    assert_eq!(zone(&state, wall), Zone::Graveyard);
    assert_eq!(zone(&state, kept), Zone::Hand);
    assert_eq!(state.players[0].life, 23, "one land");

    // A hand of two or fewer is discarded without a choice.
    let mut state = islands();
    state.players[1].hand.clear();
    let first = put(&mut state, P1, "Forest", Zone::Hand);
    let second = put(&mut state, P1, "Swamp", Zone::Hand);
    let verdict = put(&mut state, P0, "Gerrard's Verdict", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::B], 0);
    cast(&mut state, verdict, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
    assert_eq!(state.players[0].life, 26, "two lands");
}

#[test]
fn mana_short_taps_the_lands_and_empties_the_pool() {
    let mut state = islands();
    let lands = [
        put(&mut state, P1, "Island", Zone::Battlefield),
        put(&mut state, P1, "Forest", Zone::Battlefield),
    ];
    pool(&mut state, P1, &[ManaColor::G], 2);
    let short = put(&mut state, P0, "Mana Short", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, short, &[Target::Player(P1)]);
    settled(&mut state);
    assert!(lands.iter().all(|&land| state.objects.get(land).tapped));
    assert_eq!(state.players[1].mana_pool, [0; 6]);
    // Its caster's pool is theirs to keep.
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn upheaval_returns_every_permanent_and_tokens_vanish() {
    let mut state = islands();
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let land = put(&mut state, P0, "Forest", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Wall of Blossoms", Zone::Battlefield);
    let token = put(&mut state, P1, "Zombie Token", Zone::Battlefield);
    state.objects.get_mut(token).v4.is_token = true;
    let upheaval = put(&mut state, P0, "Upheaval", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 4);
    cast(&mut state, upheaval, &[]);
    settled(&mut state);
    for card in [mine, land] {
        assert_eq!(zone(&state, card), Zone::Hand);
    }
    assert_eq!(zone(&state, theirs), Zone::Hand);
    assert_ne!(zone(&state, token), Zone::Battlefield);
    assert!(state.players[1].battlefield.is_empty());
    assert!(state.players[0].battlefield.is_empty());
}

#[test]
fn dromars_charm_gains_life_counters_or_shrinks() {
    let mut state = islands();
    let charm = put(&mut state, P0, "Dromar's Charm", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::U, ManaColor::B],
        0,
    );
    act(&mut state, Action::CastSpell(charm));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(0),
        other => first_answer(other),
    });
    assert_eq!(state.players[0].life, 25);

    // Counter target spell.
    let mut state = islands();
    let victim = put(&mut state, P1, "Wall of Blossoms", Zone::Hand);
    let charm = put(&mut state, P0, "Dromar's Charm", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::G], 1);
    cast(&mut state, victim, &[]);
    next(&mut state);
    act(&mut state, Action::Pass);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::U, ManaColor::B],
        0,
    );
    assert!(castable(&mut state, charm));
    act(&mut state, Action::CastSpell(charm));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(1),
        Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(legal_targets[0]),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, victim), Zone::Graveyard);

    // -2/-2 on a creature.
    let mut state = islands();
    let creature = put(&mut state, P1, "Ravenous Baloth", Zone::Battlefield);
    let charm = put(&mut state, P0, "Dromar's Charm", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::U, ManaColor::B],
        0,
    );
    act(&mut state, Action::CastSpell(charm));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(2),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(creature)),
        other => first_answer(other),
    });
    assert_eq!(power_toughness(&state, creature), (2, 2));
}

#[test]
fn gaeas_blessing_shuffles_three_cards_back_and_draws() {
    let mut state = game(&["Island"; 10]);
    let cards = [
        put(&mut state, P0, "Forest", Zone::Graveyard),
        put(&mut state, P0, "Swamp", Zone::Graveyard),
        put(&mut state, P0, "Plains", Zone::Graveyard),
    ];
    let left = put(&mut state, P0, "Mountain", Zone::Graveyard);
    let blessing = put(&mut state, P0, "Gaea's Blessing", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    let library_before = state.players[0].library.len();
    cast(
        &mut state,
        blessing,
        &[
            Target::Player(P0),
            Target::Object(cards[0]),
            Target::Object(cards[1]),
            Target::Object(cards[2]),
        ],
    );
    settled(&mut state);
    for card in cards {
        assert_ne!(zone(&state, card), Zone::Graveyard);
    }
    assert_eq!(zone(&state, left), Zone::Graveyard);
    // Three cards shuffled in, one drawn.
    assert_eq!(state.players[0].library.len(), library_before + 3 - 1);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn gaeas_blessing_shuffles_the_graveyard_in_when_milled() {
    let mut state = game(&["Island"; 10]);
    let old = put(&mut state, P0, "Forest", Zone::Graveyard);
    let blessing = put(&mut state, P0, "Gaea's Blessing", Zone::Library);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(blessing, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, blessing), Zone::Library);
    assert_eq!(zone(&state, old), Zone::Library);
    assert!(state.players[0].graveyard.is_empty());
    // Put into the graveyard from anywhere else, it does nothing.
    let mut state = game(&["Island"; 10]);
    let old = put(&mut state, P0, "Forest", Zone::Graveyard);
    let blessing = put(&mut state, P0, "Gaea's Blessing", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(blessing, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, blessing), Zone::Graveyard);
    assert_eq!(zone(&state, old), Zone::Graveyard);
}

// ---- Artifacts and enchantments --------------------------------------------------------

#[test]
fn pernicious_deed_destroys_by_mana_value_up_to_x() {
    let mut state = islands();
    let deed = put(&mut state, P0, "Pernicious Deed", Zone::Battlefield);
    let cheap = put(&mut state, P1, "Wall of Blossoms", Zone::Battlefield);
    let enchantment = put(&mut state, P1, "Mirri's Guile", Zone::Battlefield);
    let tusker = put(&mut state, P1, "Krosan Tusker", Zone::Battlefield);
    let land = put(&mut state, P1, "Forest", Zone::Battlefield);
    pool(&mut state, P0, &[], 8);
    for x in 0..=8 {
        assert!(activatable(&mut state, deed, x), "X = {x}");
    }
    activate(&mut state, deed, 2, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, deed), Zone::Graveyard, "sacrificed");
    assert_eq!(zone(&state, cheap), Zone::Graveyard);
    assert_eq!(zone(&state, enchantment), Zone::Graveyard);
    assert_eq!(zone(&state, tusker), Zone::Battlefield, "mana value 7");
    assert_eq!(zone(&state, land), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::C), 6, "paid X = 2");
}

#[test]
fn nevinyrrals_disk_enters_tapped_and_destroys_everything_but_lands() {
    let mut state = islands();
    let disk = put(&mut state, P0, "Nevinyrral's Disk", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, disk, &[]);
    settled(&mut state);
    assert!(state.objects.get(disk).tapped);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let enchantment = put(&mut state, P1, "Mirri's Guile", Zone::Battlefield);
    let land = put(&mut state, P1, "Forest", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, disk, 0), "tapped");
    state.objects.get_mut(disk).tapped = false;
    activate(&mut state, disk, 0, &[]);
    settled(&mut state);
    for card in [disk, elves, enchantment] {
        assert_eq!(zone(&state, card), Zone::Graveyard);
    }
    assert_eq!(zone(&state, land), Zone::Battlefield);
}

#[test]
fn zombie_infestation_discards_two_for_a_zombie() {
    let mut state = islands();
    let infestation = put(&mut state, P0, "Zombie Infestation", Zone::Battlefield);
    assert!(
        !activatable(&mut state, infestation, 0),
        "no cards to discard"
    );
    put(&mut state, P0, "Forest", Zone::Hand);
    put(&mut state, P0, "Swamp", Zone::Hand);
    activate(&mut state, infestation, 0, &[]);
    settle_picking(&mut state, 0, None);
    assert_eq!(state.players[0].hand.len(), 0);
    let zombies = controls(&state, P0, "Zombie Token");
    assert_eq!(zombies.len(), 1);
    assert_eq!(power_toughness(&state, zombies[0]), (2, 2));
}

#[test]
fn black_vise_damages_the_chosen_player_for_cards_beyond_four() {
    let mut state = game(&["Island"; 20]);
    let vise = put(&mut state, P0, "Black Vise", Zone::Hand);
    enter(&mut state, vise);
    for _ in 0..6 {
        put(&mut state, P1, "Forest", Zone::Hand);
    }
    to_next_turn(&mut state, |_, _| None);
    // The upkeep count was six cards: two damage; the draw came after.
    assert_eq!(state.players[1].life, 18);
    assert_eq!(state.players[0].life, 20);
}

#[test]
fn root_maze_makes_artifacts_and_lands_enter_tapped() {
    let mut state = islands();
    let maze = put(&mut state, P0, "Root Maze", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, maze, &[]);
    settled(&mut state);
    let land = put(&mut state, P0, "Forest", Zone::Hand);
    act(&mut state, Action::PlayLand(land));
    assert!(state.objects.get(land).tapped);
    let vise = put(&mut state, P0, "Black Vise", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, vise, &[]);
    settled(&mut state);
    assert!(state.objects.get(vise).tapped);
    // A creature is not affected, and neither is an opponent's land.
    let wall = put(&mut state, P0, "Wall of Blossoms", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, wall, &[]);
    settled(&mut state);
    assert!(!state.objects.get(wall).tapped);
    let theirs = put(&mut state, P1, "Forest", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    act(&mut state, Action::PlayLand(theirs));
    assert!(state.objects.get(theirs).tapped);
}

#[test]
fn stasis_skips_untap_steps_and_costs_u_each_upkeep() {
    let mut state = game(&["Island"; 10]);
    put(&mut state, P0, "Stasis", Zone::Battlefield);
    let tapped = put(&mut state, P1, "Forest", Zone::Battlefield);
    state.objects.get_mut(tapped).tapped = true;
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(0)),
        _ => None,
    });
    assert!(
        state.objects.get(tapped).tapped,
        "no untap for the opponent"
    );
    assert!(
        state.objects.get(island).tapped,
        "paid {{U}} with the Island"
    );

    // Declining the payment sacrifices it.
    let mut state = game(&["Island"; 10]);
    let stasis = put(&mut state, P0, "Stasis", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Battlefield);
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        _ => None,
    });
    assert_eq!(zone(&state, stasis), Zone::Graveyard);

    // Without blue mana it is sacrificed outright.
    let mut state = game(&["Island"; 10]);
    let stasis = put(&mut state, P0, "Stasis", Zone::Battlefield);
    to_next_turn(&mut state, |_, _| None);
    assert_eq!(zone(&state, stasis), Zone::Graveyard);
}

#[test]
fn forsaken_city_stays_tapped_unless_a_card_is_exiled_at_upkeep() {
    let mut state = game(&["Island"; 10]);
    let city = put(&mut state, P0, "Forsaken City", Zone::Battlefield);
    state.objects.get_mut(city).tapped = true;
    let card = put(&mut state, P0, "Forest", Zone::Hand);
    let wanted = vec![card];
    to_next_turn(&mut state, |_, decision| selection(&wanted, decision));
    assert_eq!(zone(&state, card), Zone::Exile);
    assert!(!state.objects.get(city).tapped);

    // Declining keeps it tapped.
    let mut state = game(&["Island"; 10]);
    let city = put(&mut state, P0, "Forsaken City", Zone::Battlefield);
    state.objects.get_mut(city).tapped = true;
    let card = put(&mut state, P0, "Forest", Zone::Hand);
    to_next_turn(&mut state, |_, decision| selection(&[], decision));
    assert_eq!(zone(&state, card), Zone::Hand);
    assert!(state.objects.get(city).tapped);
}

// ---- Lands --------------------------------------------------------------------------

#[test]
fn the_tainted_lands_need_a_swamp_for_their_colors() {
    for (name, colors) in [
        ("Tainted Wood", [ManaColor::B, ManaColor::G]),
        ("Tainted Field", [ManaColor::W, ManaColor::B]),
    ] {
        let mut state = islands();
        let land = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        for color in colors {
            assert!(
                engine::step(
                    &mut state.clone(),
                    Action::ActivateManaAbilityChoice(land, color)
                )
                .is_err(),
                "{name} {color:?} without a Swamp"
            );
        }
        act(&mut state, Action::ActivateManaAbility(land));
        assert_eq!(mana(&state, P0, ManaColor::C), 1);

        for color in colors {
            let mut state = islands();
            let land = put(&mut state, P0, name, Zone::Battlefield);
            put(&mut state, P0, "Swamp", Zone::Battlefield);
            next(&mut state);
            act(&mut state, Action::ActivateManaAbilityChoice(land, color));
            assert_eq!(mana(&state, P0, color), 1, "{name}");
        }
    }
}

#[test]
fn high_market_and_phyrexian_tower_sacrifice_creatures() {
    let mut state = islands();
    let market = put(&mut state, P0, "High Market", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    activate(&mut state, market, 0, &[]);
    settle_picking(&mut state, 0, None);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(state.players[0].life, 21);
    assert!(state.objects.get(market).tapped);

    // The free ability adds {C}; sacrificing a creature adds {B}{B}.
    let mut state = islands();
    let tower = put(&mut state, P0, "Phyrexian Tower", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(tower, ManaColor::C),
    );
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    let mut state = islands();
    let tower = put(&mut state, P0, "Phyrexian Tower", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityWithCostTarget(tower, ManaColor::B, elves),
    );
    assert_eq!(mana(&state, P0, ManaColor::B), 2);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
}

// ---- Contamination ----------------------------------------------------------------------

#[test]
fn contamination_makes_every_land_produce_one_black() {
    let mut state = islands();
    put(&mut state, P0, "Contamination", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let city = put(&mut state, P1, "City of Brass", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(forest));
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 0);
    assert!(state.objects.get(forest).tapped);
    // Another player's land too, and still a tap: City of Brass's damage.
    state.priority_player = P1;
    act(&mut state, Action::ActivateManaAbility(city));
    assert_eq!(mana(&state, P1, ManaColor::B), 1);
    settled(&mut state);
    assert_eq!(state.players[1].life, 19);
}

#[test]
fn contamination_pays_for_spells_with_black() {
    let mut state = islands();
    for _ in 0..4 {
        put(&mut state, P0, "Forest", Zone::Battlefield);
    }
    let tutor = put(&mut state, P0, "Diabolic Tutor", Zone::Hand);
    assert!(!castable(&mut state, tutor), "no black mana");
    put(&mut state, P0, "Contamination", Zone::Battlefield);
    assert!(castable(&mut state, tutor));
    cast(&mut state, tutor, &[]);
    settle_picking(&mut state, 0, None);
    assert_eq!(zone(&state, tutor), Zone::Graveyard);
}

#[test]
fn contamination_costs_a_creature_each_upkeep() {
    let mut state = game(&["Island"; 10]);
    let contamination = put(&mut state, P0, "Contamination", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        _ => None,
    });
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, contamination), Zone::Battlefield);

    let mut state = game(&["Island"; 10]);
    let contamination = put(&mut state, P0, "Contamination", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(0)),
        _ => None,
    });
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, contamination), Zone::Graveyard);

    // Without a creature it is sacrificed outright.
    let mut state = game(&["Island"; 10]);
    let contamination = put(&mut state, P0, "Contamination", Zone::Battlefield);
    to_next_turn(&mut state, |_, _| None);
    assert_eq!(zone(&state, contamination), Zone::Graveyard);
}

// ---- Future Sight -------------------------------------------------------------------------

#[test]
fn future_sight_reveals_the_top_card_and_lets_its_controller_play_it() {
    let mut state = game(&["Island"; 10]);
    put(&mut state, P0, "Future Sight", Zone::Battlefield);
    let top = put(&mut state, P0, "Wall of Blossoms", Zone::Library);
    pool(&mut state, P0, &[ManaColor::G], 1);
    assert!(castable(&mut state, top));
    for observer in [P0, P1] {
        assert!(
            state
                .known_library_cards(observer, P0)
                .iter()
                .any(|entry| entry.position == 0 && entry.object == top),
            "revealed to {observer:?}"
        );
    }
    cast(&mut state, top, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, top), Zone::Battlefield);

    // The new top card is revealed in turn, and a land on top is played.
    let land = put(&mut state, P0, "Forest", Zone::Library);
    next(&mut state);
    assert!(state
        .known_library_cards(P1, P0)
        .iter()
        .any(|entry| entry.position == 0 && entry.object == land));
    act(&mut state, Action::PlayLand(land));
    assert_eq!(zone(&state, land), Zone::Battlefield);

    // Only its controller may play from the top.
    let mut state = game(&["Island"; 10]);
    put(&mut state, P0, "Future Sight", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Wall of Blossoms", Zone::Library);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::G], 1);
    assert!(!castable(&mut state, theirs));
}

// ---- Bane of the Living ----------------------------------------------------------------------

#[test]
fn bane_of_the_living_is_a_morph_whose_x_shrinks_every_creature() {
    let mut state = islands();
    let bane = put(&mut state, P0, "Bane of the Living", Zone::Hand);
    let rats = put(&mut state, P1, "Ravenous Rats", Zone::Battlefield);
    let wall = put(&mut state, P1, "Wall of Blossoms", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    act(&mut state, Action::CastSpell(bane));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bane), Zone::Battlefield);
    assert!(state.objects.get(bane).v4.face_down_v1.is_some());
    assert_eq!(power_toughness(&state, bane), (2, 2));
    assert_eq!(
        state.objects.get(bane).v4.ward_generic,
        0,
        "morph has no ward"
    );

    // {X}{B}{B} with X = 2.
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    assert!(activatable(&mut state, bane, 255));
    act(&mut state, Action::ActivateAbility(bane, 255));
    assert!(state.objects.get(bane).v4.face_down_v1.is_none());
    announce_x(&mut state, 2);
    settled(&mut state);
    assert_eq!(zone(&state, rats), Zone::Graveyard);
    assert_eq!(power_toughness(&state, wall), (-2, 2));
    assert_eq!(power_toughness(&state, bane), (2, 1));
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn bane_of_the_living_with_no_spare_mana_owes_no_x() {
    let mut state = islands();
    let bane = put(&mut state, P0, "Bane of the Living", Zone::Hand);
    let rats = put(&mut state, P1, "Ravenous Rats", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    act(&mut state, Action::CastSpell(bane));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        other => first_answer(other),
    });
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    act(&mut state, Action::ActivateAbility(bane, 255));
    settled(&mut state);
    assert_eq!(zone(&state, rats), Zone::Battlefield);
    assert_eq!(power_toughness(&state, bane), (4, 3));
}
