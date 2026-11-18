//! Vintage Cube wave C2b: native rules and public-session tests for the
//! utility lands.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, EntersTappedUnlessControllerDef, Keywords,
    Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C2B_CARDS: [&str; 9] = [
    "Sunbaked Canyon",
    "Waterlogged Grove",
    "Mana Confluence",
    "Library of Alexandria",
    "Celestial Colonnade",
    "Creeping Tar Pit",
    "Fabled Passage",
    "Abandoned Air Temple",
    "Shelldock Isle",
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
        0x4355_4245_0000 + 0x3262,
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

/// Advances until the ability just activated is on the stack with its costs
/// paid (the engine finishes a pending activation as it next advances).
fn on_the_stack(state: &mut GameState) {
    for _ in 0..20 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => return,
            other => {
                let action = first_answer(other);
                act(state, action);
            }
        }
    }
    panic!("the activation did not finish");
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

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

/// A custom deck for a public session.
fn deck(rows: &[(&str, u32)]) -> CustomDeckV1 {
    CustomDeckV1 {
        cards: rows
            .iter()
            .map(|&(name, count)| CustomCardCountV1 {
                name: name.to_owned(),
                count,
            })
            .collect(),
        companion: None,
    }
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn c2b_cards_are_full() {
    for name in C2B_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c2b_cards_have_their_printed_characteristics() {
    for name in C2B_CARDS {
        let card = def(name);
        assert!(card.is_land, "{name}");
        assert_eq!(card.types, &[CardType::Land], "{name}");
        assert!(card.subtypes.is_empty(), "{name} has no land type");
        assert!(
            card.supertypes.is_empty(),
            "{name} is not basic or legendary"
        );
        assert!(
            card.cost.pips.is_empty() && card.cost.generic == 0,
            "{name} has no mana cost"
        );
        let enters_tapped = matches!(
            name,
            "Celestial Colonnade" | "Creeping Tar Pit" | "Shelldock Isle"
        );
        assert_eq!(card.enters_battlefield_tapped, enters_tapped, "{name}");
        assert_eq!(
            card.enters_tapped_unless_controller,
            (name == "Abandoned Air Temple")
                .then_some(EntersTappedUnlessControllerDef::ControlsBasicLandV1),
            "{name}"
        );
    }
    assert_eq!(
        def("Sunbaked Canyon").produces_mana,
        &[ManaColor::R, ManaColor::W]
    );
    assert_eq!(
        def("Waterlogged Grove").produces_mana,
        &[ManaColor::G, ManaColor::U]
    );
    assert_eq!(def("Mana Confluence").produces_mana.len(), 5);
    assert_eq!(def("Library of Alexandria").produces_mana, &[ManaColor::C]);
    assert!(def("Fabled Passage").produces_mana.is_empty());
    let colonnade = def("Celestial Colonnade").animation.expect("animation");
    assert!(colonnade.until_end_of_turn);
    assert_eq!((colonnade.power, colonnade.toughness), (4, 4));
    assert_eq!(colonnade.colors, &[ManaColor::W, ManaColor::U]);
    assert_eq!(colonnade.subtypes, &[Subtype::Elemental]);
    assert!(
        colonnade.keywords.has(Keywords::FLYING) && colonnade.keywords.has(Keywords::VIGILANCE)
    );
    let pit = def("Creeping Tar Pit").animation.expect("animation");
    assert!(pit.until_end_of_turn);
    assert_eq!((pit.power, pit.toughness), (3, 2));
    assert_eq!(pit.colors, &[ManaColor::U, ManaColor::B]);
    assert_eq!(pit.subtypes, &[Subtype::Elemental]);
    assert!(pit.keywords.has(Keywords::CANT_BE_BLOCKED));
}

// ---- Helpers of this wave ------------------------------------------------------------

/// Settles, choosing `wanted` as the one card a search finds and otherwise
/// answering with the first legal answer (a search that has found its card
/// finishes).
fn settle_finding(state: &mut GameState, wanted: ObjectId) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if legal_targets.contains(&Target::Object(wanted)) => {
            Action::ChooseEffectTarget(Target::Object(wanted))
        }
        other => first_answer(other),
    });
}

/// Settles, answering a "may play the exiled card" question with `play`.
fn settle_playing(state: &mut GameState, play: bool) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(play),
        other => first_answer(other),
    });
}

/// Moves cards from the bottom of `player`'s library to their graveyard until
/// `size` remain.
fn shrink_library(state: &mut GameState, player: PlayerId, size: usize) {
    while state.players[player.index()].library.len() > size {
        let card = state.players[player.index()]
            .library
            .pop()
            .expect("a library card");
        state.objects.get_mut(card).zone = Zone::Graveyard;
        state.players[player.index()].graveyard.push(card);
    }
}

fn fill_hand(state: &mut GameState, player: PlayerId, count: usize) {
    while state.players[player.index()].hand.len() < count {
        put(state, player, "Plains", Zone::Hand);
    }
}

/// Pays for and resolves a creature land's animation from exactly the mana
/// put in the pool (the land is tapped while it resolves, so it cannot pay for
/// itself, and keeps its tapped state afterwards).
fn animate(state: &mut GameState, land: ObjectId) {
    let was_tapped = state.objects.get(land).tapped;
    state.objects.get_mut(land).tapped = true;
    match CARD_DEFS[state.objects.get(land).card_def as usize].name {
        "Celestial Colonnade" => pool(state, P0, &[ManaColor::W, ManaColor::U], 3),
        "Creeping Tar Pit" => pool(state, P0, &[ManaColor::U, ManaColor::B], 1),
        other => panic!("{other} is not a creature land"),
    }
    activate(state, land, 0, &[]);
    settled(state);
    state.objects.get_mut(land).tapped = was_tapped;
}

// ---- Sunbaked Canyon, Waterlogged Grove ----------------------------------------------

#[test]
fn canyon_and_grove_pay_a_life_for_either_color_and_sacrifice_to_draw() {
    for (name, colors) in [
        ("Sunbaked Canyon", [ManaColor::R, ManaColor::W]),
        ("Waterlogged Grove", [ManaColor::G, ManaColor::U]),
    ] {
        for color in colors {
            let mut state = plains();
            let land = put(&mut state, P0, name, Zone::Battlefield);
            next(&mut state);
            act(&mut state, Action::ActivateManaAbilityChoice(land, color));
            assert_eq!(mana(&state, P0, color), 1, "{name}");
            assert_eq!(life(&state, P0), 19, "{name} pays 1 life");
            assert!(state.objects.get(land).tapped, "{name}");
            assert!(
                engine::step(&mut state, Action::ActivateManaAbilityChoice(land, color)).is_err(),
                "{name} is tapped"
            );
        }

        // "{1}, {T}, Sacrifice this land: Draw a card." The {1} must be paid.
        let mut state = plains();
        let land = put(&mut state, P0, name, Zone::Battlefield);
        assert!(!activatable(&mut state, land, 0), "{name} without {{1}}");
        pool(&mut state, P0, &[], 1);
        let hand = state.players[0].hand.len();
        activate(&mut state, land, 0, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, land), Zone::Graveyard, "{name}");
        assert_eq!(state.players[0].hand.len(), hand + 1, "{name} draws");
        assert_eq!(life(&state, P0), 20, "{name}: no life for the draw");

        // A tapped land cannot pay the {T} of the draw ability.
        let mut state = plains();
        let land = put(&mut state, P0, name, Zone::Battlefield);
        state.objects.get_mut(land).tapped = true;
        pool(&mut state, P0, &[], 1);
        assert!(!activatable(&mut state, land, 0), "{name} tapped");
    }
}

// ---- Mana Confluence -----------------------------------------------------------------

#[test]
fn mana_confluence_pays_a_life_for_one_mana_of_any_color() {
    for color in [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ] {
        let mut state = plains();
        let land = put(&mut state, P0, "Mana Confluence", Zone::Battlefield);
        assert!(
            !activatable(&mut state, land, 0),
            "Mana Confluence has no other ability"
        );
        act(&mut state, Action::ActivateManaAbilityChoice(land, color));
        assert_eq!(mana(&state, P0, color), 1);
        assert_eq!(life(&state, P0), 19, "the life is a cost of the ability");
        assert!(state.objects.get(land).tapped);
        assert!(engine::step(&mut state, Action::ActivateManaAbilityChoice(land, color)).is_err());
    }
}

#[test]
fn mana_confluence_can_pay_the_last_life_point() {
    let mut state = plains();
    state.players[0].life = 1;
    let land = put(&mut state, P0, "Mana Confluence", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(land, ManaColor::G),
    );
    assert_eq!(life(&state, P0), 0);
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert!(matches!(
        next(&mut state),
        Decision::GameOver { winner: Some(P1) }
    ));
}

// ---- Library of Alexandria -----------------------------------------------------------

#[test]
fn library_of_alexandria_taps_for_colorless_and_draws_only_with_exactly_seven_cards() {
    let mut state = plains();
    let library = put(&mut state, P0, "Library of Alexandria", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(library));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
    assert!(state.objects.get(library).tapped);

    for hand in [0, 1, 6, 7, 8, 9] {
        let mut state = plains();
        let library = put(&mut state, P0, "Library of Alexandria", Zone::Battlefield);
        fill_hand(&mut state, P0, hand);
        assert_eq!(
            activatable(&mut state, library, 0),
            hand == 7,
            "hand of {hand}"
        );
    }

    // The opponent's hand does not count.
    let mut state = plains();
    let library = put(&mut state, P0, "Library of Alexandria", Zone::Battlefield);
    fill_hand(&mut state, P0, 3);
    fill_hand(&mut state, P1, 7);
    assert!(!activatable(&mut state, library, 0));

    // Seven cards: the ability draws one, tapping the Library.
    let mut state = plains();
    let library = put(&mut state, P0, "Library of Alexandria", Zone::Battlefield);
    fill_hand(&mut state, P0, 7);
    let cards = state.players[0].library.len();
    activate(&mut state, library, 0, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 8);
    assert_eq!(state.players[0].library.len(), cards - 1);
    assert!(state.objects.get(library).tapped);
    assert_eq!(zone(&state, library), Zone::Battlefield);
    assert!(
        !activatable(&mut state, library, 0),
        "tapped and eight cards"
    );

    // The condition is checked on activation only: cards added while the
    // ability is on the stack do not stop it.
    let mut state = plains();
    let library = put(&mut state, P0, "Library of Alexandria", Zone::Battlefield);
    fill_hand(&mut state, P0, 7);
    activate(&mut state, library, 0, &[]);
    fill_hand(&mut state, P0, 9);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 10);
}

// ---- Celestial Colonnade, Creeping Tar Pit -------------------------------------------

#[test]
fn creature_lands_enter_tapped_and_tap_for_either_color() {
    for (name, colors) in [
        ("Celestial Colonnade", [ManaColor::W, ManaColor::U]),
        ("Creeping Tar Pit", [ManaColor::U, ManaColor::B]),
    ] {
        let mut state = plains();
        let land = put(&mut state, P0, name, Zone::Hand);
        play_land(&mut state, land);
        assert_eq!(zone(&state, land), Zone::Battlefield);
        assert!(state.objects.get(land).tapped, "{name} enters tapped");
        assert!(!engine::object_has_type(&state, land, CardType::Creature));
        for color in colors {
            let mut state = plains();
            let land = put(&mut state, P0, name, Zone::Battlefield);
            next(&mut state);
            act(&mut state, Action::ActivateManaAbilityChoice(land, color));
            assert_eq!(mana(&state, P0, color), 1, "{name}");
            assert_eq!(life(&state, P0), 20, "{name} costs no life");
        }
    }
}

#[test]
fn celestial_colonnade_becomes_a_flying_vigilant_four_four_until_end_of_turn() {
    let mut state = plains();
    let land = put(&mut state, P0, "Celestial Colonnade", Zone::Battlefield);
    // The land is tapped, so it cannot pay for its own animation.
    state.objects.get_mut(land).tapped = true;
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 2);
    assert!(
        !activatable(&mut state, land, 0),
        "{{3}}{{W}}{{U}} is five mana"
    );
    pool(&mut state, P0, &[], 1);
    activate(&mut state, land, 0, &[]);
    settled(&mut state);
    state.objects.get_mut(land).tapped = false;
    assert!(engine::object_has_type(&state, land, CardType::Creature));
    assert!(engine::object_has_type(&state, land, CardType::Land));
    assert_eq!(power_toughness(&state, land), (4, 4));
    assert!(keyword(&state, land, Keywords::FLYING));
    assert!(keyword(&state, land, Keywords::VIGILANCE));
    assert_eq!(
        engine::object_color_mask(&state, land),
        0b11,
        "white and blue"
    );
    assert!(engine::has_effective_subtype(
        &state,
        land,
        Subtype::Elemental
    ));
    // It is still a land with its mana ability.
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(land, ManaColor::U),
    );
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
    // Until end of turn.
    pass_until(&mut state, |state| state.active_player == P1);
    assert!(!engine::object_has_type(&state, land, CardType::Creature));
    assert!(engine::object_has_type(&state, land, CardType::Land));
    assert!(!keyword(&state, land, Keywords::FLYING));
}

#[test]
fn celestial_colonnade_attacks_in_the_air_without_tapping() {
    let mut state = plains();
    let land = put(&mut state, P0, "Celestial Colonnade", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    animate(&mut state, land);
    assert_eq!(
        legal_blockers(&mut state, land),
        vec![specter],
        "only the flier may block"
    );

    let mut state = plains();
    let land = put(&mut state, P0, "Celestial Colonnade", Zone::Battlefield);
    animate(&mut state, land);
    attack_with(&mut state, vec![land], |_, _| None);
    assert!(!state.objects.get(land).tapped, "vigilance");
    assert_eq!(life(&state, P1), 16);
}

#[test]
fn creeping_tar_pit_becomes_an_unblockable_three_two_until_end_of_turn() {
    let mut state = plains();
    let land = put(&mut state, P0, "Creeping Tar Pit", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    // The land is tapped, so it cannot pay for its own animation.
    state.objects.get_mut(land).tapped = true;
    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(
        !activatable(&mut state, land, 0),
        "{{1}}{{U}}{{B}} needs black"
    );
    pool(&mut state, P0, &[ManaColor::B], 0);
    activate(&mut state, land, 0, &[]);
    settled(&mut state);
    state.objects.get_mut(land).tapped = false;
    assert!(engine::object_has_type(&state, land, CardType::Creature));
    assert!(engine::object_has_type(&state, land, CardType::Land));
    assert_eq!(power_toughness(&state, land), (3, 2));
    assert!(!keyword(&state, land, Keywords::FLYING));
    assert_eq!(
        engine::object_color_mask(&state, land),
        0b110,
        "blue and black"
    );
    assert!(engine::has_effective_subtype(
        &state,
        land,
        Subtype::Elemental
    ));
    // Animating it a second time keeps it unblockable for the turn.
    animate(&mut state, land);
    assert_eq!(power_toughness(&state, land), (3, 2));
    assert!(
        legal_blockers(&mut state, land).is_empty(),
        "nothing can block it"
    );

    let mut state = plains();
    let land = put(&mut state, P0, "Creeping Tar Pit", Zone::Battlefield);
    animate(&mut state, land);
    attack_with(&mut state, vec![land], |_, _| None);
    assert!(state.objects.get(land).tapped, "no vigilance");
    assert_eq!(life(&state, P1), 17);
    pass_until(&mut state, |state| state.active_player == P1);
    assert!(!engine::object_has_type(&state, land, CardType::Creature));
    assert!(!keyword(&state, land, Keywords::CANT_BE_BLOCKED));
}

#[test]
fn creeping_tar_pit_stays_unblockable_after_ability_removal_or_a_creature_override() {
    // "It can't be blocked this turn" is a rule effect of the resolved
    // ability, not an ability of the land: Unable to Scream's later removal
    // keeps it...
    let mut state = plains();
    let land = put(&mut state, P0, "Creeping Tar Pit", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    animate(&mut state, land);
    let scream = put(&mut state, P0, "Unable to Scream", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, scream, &[Target::Object(land)]);
    settled(&mut state);
    assert_eq!(zone(&state, scream), Zone::Battlefield);
    assert_eq!(power_toughness(&state, land), (0, 2));
    assert!(keyword(&state, land, Keywords::CANT_BE_BLOCKED));
    assert!(legal_blockers(&mut state, land).is_empty());

    // ...and so does Witness Protection, whose characteristics replace the
    // animation's.
    let mut state = plains();
    let land = put(&mut state, P0, "Creeping Tar Pit", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    animate(&mut state, land);
    let protection = put(&mut state, P0, "Witness Protection", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, protection, &[Target::Object(land)]);
    settled(&mut state);
    assert_eq!(zone(&state, protection), Zone::Battlefield);
    assert!(engine::object_has_type(&state, land, CardType::Creature));
    assert_eq!(power_toughness(&state, land), (1, 1));
    assert!(keyword(&state, land, Keywords::CANT_BE_BLOCKED));
    assert!(
        legal_blockers(&mut state, land).is_empty(),
        "nothing can block it"
    );
}

#[test]
fn dress_down_removes_an_animated_colonnades_keywords_only_when_it_is_later() {
    // Animated first, Dress Down later: the animation's creature type
    // doesn't depend on abilities, so Dress Down applies to the land, and
    // its removal is later than the animation's flying and vigilance
    // (613.7).
    let mut state = plains();
    let land = put(&mut state, P0, "Celestial Colonnade", Zone::Battlefield);
    animate(&mut state, land);
    assert!(keyword(&state, land, Keywords::FLYING));
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, dress), Zone::Battlefield);
    assert!(!keyword(&state, land, Keywords::FLYING));
    assert!(!keyword(&state, land, Keywords::VIGILANCE));

    // Dress Down first, animated later: the animation's keywords apply after
    // the removal.
    let mut state = plains();
    let land = put(&mut state, P0, "Celestial Colonnade", Zone::Battlefield);
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress, &[]);
    settled(&mut state);
    animate(&mut state, land);
    assert!(engine::object_has_type(&state, land, CardType::Creature));
    assert!(keyword(&state, land, Keywords::FLYING));
    assert!(keyword(&state, land, Keywords::VIGILANCE));
}

#[test]
fn creature_lands_cannot_attack_or_tap_the_turn_they_arrive() {
    for name in ["Celestial Colonnade", "Creeping Tar Pit"] {
        for sick in [true, false] {
            let mut state = plains();
            let land = put(&mut state, P0, name, Zone::Battlefield);
            state.objects.get_mut(land).summoning_sick = sick;
            animate(&mut state, land);
            if sick {
                assert!(
                    engine::step(
                        &mut state,
                        Action::ActivateManaAbilityChoice(land, ManaColor::U)
                    )
                    .is_err(),
                    "{name}: a creature that just arrived cannot use its {{T}} ability"
                );
            }
            let mut offered: Option<Vec<ObjectId>> = None;
            run_until(
                &mut state,
                |state| state.step == Step::Main2,
                |_, decision| {
                    if let Decision::DeclareAttackers { eligible, .. } = decision {
                        offered = Some(eligible.clone());
                    }
                    None
                },
            );
            let can_attack = offered.is_some_and(|eligible| eligible.contains(&land));
            assert_eq!(can_attack, !sick, "{name} sick={sick}");
        }
    }
}

// ---- Fabled Passage ------------------------------------------------------------------

#[test]
fn fabled_passage_fetches_a_basic_tapped_and_untaps_it_with_four_or_more_lands() {
    for others in 0..=4_usize {
        let mut state = plains();
        let passage = put(&mut state, P0, "Fabled Passage", Zone::Battlefield);
        for _ in 0..others {
            put(&mut state, P0, "Island", Zone::Battlefield);
        }
        let wanted = put(&mut state, P0, "Forest", Zone::Library);
        let confluence = put(&mut state, P0, "Mana Confluence", Zone::Library);
        activate(&mut state, passage, 0, &[]);
        let mut offered = Vec::new();
        settle_with(&mut state, |state, decision| match decision {
            Decision::ChooseEffectTargets {
                legal_targets,
                selected_count: 0,
                ..
            } => {
                for target in &legal_targets {
                    let Target::Object(card) = *target else {
                        panic!("{target:?}")
                    };
                    let card_def = &CARD_DEFS[state.objects.get(card).card_def as usize];
                    assert!(
                        card_def.is_land && card_def.supertypes.contains(&Supertype::Basic),
                        "only basic land cards are offered, not {}",
                        card_def.name
                    );
                }
                offered = legal_targets;
                Action::ChooseEffectTarget(Target::Object(wanted))
            }
            other => first_answer(other),
        });
        assert!(offered.contains(&Target::Object(wanted)));
        assert!(!offered.contains(&Target::Object(confluence)));
        assert_eq!(zone(&state, passage), Zone::Graveyard, "sacrificed");
        assert_eq!(zone(&state, wanted), Zone::Battlefield);
        assert_eq!(zone(&state, confluence), Zone::Library);
        // The found land counts, the sacrificed Fabled Passage does not.
        assert_eq!(
            state.objects.get(wanted).tapped,
            others + 1 < 4,
            "{others} other lands"
        );
    }
}

#[test]
fn fabled_passage_counts_only_its_controllers_lands_and_may_find_nothing() {
    let mut state = plains();
    let passage = put(&mut state, P0, "Fabled Passage", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Battlefield);
    for _ in 0..5 {
        put(&mut state, P1, "Mountain", Zone::Battlefield);
    }
    let wanted = put(&mut state, P0, "Forest", Zone::Library);
    activate(&mut state, passage, 0, &[]);
    settle_finding(&mut state, wanted);
    assert_eq!(zone(&state, wanted), Zone::Battlefield);
    assert!(
        state.objects.get(wanted).tapped,
        "the opponent's lands do not count"
    );

    // The search may find nothing: the land is sacrificed and nothing enters.
    let mut state = plains();
    let passage = put(&mut state, P0, "Fabled Passage", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Library);
    activate(&mut state, passage, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, passage), Zone::Graveyard);
    assert_eq!(zone(&state, forest), Zone::Library);
    assert!(state.players[0].battlefield.is_empty());
}

// ---- Abandoned Air Temple ------------------------------------------------------------

#[test]
fn abandoned_air_temple_enters_tapped_unless_its_controller_controls_a_basic_land() {
    // (lands P0 controls, lands P1 controls, enters tapped)
    let cases: [(&[&str], &[&str], bool); 6] = [
        (&[], &[], true),
        (&["Plains"], &[], false),
        (&["Island", "Mountain"], &[], false),
        (&["Mana Confluence"], &[], true),
        (&[], &["Plains", "Island"], true),
        (&["Abandoned Air Temple"], &[], true),
    ];
    for (own, theirs, tapped) in cases {
        let mut state = plains();
        for name in own {
            put(&mut state, P0, name, Zone::Battlefield);
        }
        for name in theirs {
            put(&mut state, P1, name, Zone::Battlefield);
        }
        let temple = put(&mut state, P0, "Abandoned Air Temple", Zone::Hand);
        play_land(&mut state, temple);
        assert_eq!(zone(&state, temple), Zone::Battlefield);
        assert_eq!(
            state.objects.get(temple).tapped,
            tapped,
            "controls {own:?}, opponent controls {theirs:?}"
        );
    }
}

#[test]
fn abandoned_air_temple_taps_for_white_and_puts_a_counter_on_each_creature() {
    let mut state = plains();
    let temple = put(&mut state, P0, "Abandoned Air Temple", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(temple));
    assert_eq!(mana(&state, P0, ManaColor::W), 1);

    let mut state = plains();
    let temple = put(&mut state, P0, "Abandoned Air Temple", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 3);
    activate(&mut state, temple, 0, &[]);
    on_the_stack(&mut state);
    assert!(state.objects.get(temple).tapped, "{{T}} is a cost");
    // "Each creature you control" is read as the ability resolves.
    let late = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    settled(&mut state);
    for creature in [elves, specter, late] {
        assert_eq!(state.objects.get(creature).counters.plus1_plus1, 1);
    }
    assert_eq!(state.objects.get(theirs).counters.plus1_plus1, 0);
    assert_eq!(state.objects.get(island).counters.plus1_plus1, 0);
    assert_eq!(state.objects.get(temple).counters.plus1_plus1, 0);
    assert_eq!(power_toughness(&state, elves), (2, 2));
    assert!(
        !activatable(&mut state, temple, 0),
        "tapped (and no mana left)"
    );

    // {3}{W} is four mana, and the Temple cannot pay for its own {T} ability.
    let mut state = plains();
    let temple = put(&mut state, P0, "Abandoned Air Temple", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 2);
    assert!(!activatable(&mut state, temple, 0), "three mana");
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, temple, 0), "four mana");
    // With no creature the ability does nothing.
    activate(&mut state, temple, 0, &[]);
    settled(&mut state);
    assert!(state.objects.get(temple).tapped);
}

// ---- Shelldock Isle ------------------------------------------------------------------

/// A game in which P0's top four cards are Llanowar Elves, a Forest, an
/// Island and a Swamp, with Shelldock Isle just played and its hideaway
/// resolved on the Elves. Both libraries start well above twenty cards.
fn shelldock_game() -> (GameState, ObjectId, ObjectId) {
    let mut state = game(&["Plains"; 30]);
    put(&mut state, P0, "Swamp", Zone::Library);
    put(&mut state, P0, "Island", Zone::Library);
    put(&mut state, P0, "Forest", Zone::Library);
    let hidden = put(&mut state, P0, "Llanowar Elves", Zone::Library);
    let isle = put(&mut state, P0, "Shelldock Isle", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(isle));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert_eq!(legal_targets.len(), 4, "the top four cards");
            Action::ChooseEffectTarget(Target::Object(hidden))
        }
        other => first_answer(other),
    });
    (state, isle, hidden)
}

#[test]
fn shelldock_isle_enters_tapped_and_hides_one_of_the_top_four_cards() {
    let (state, isle, hidden) = shelldock_game();
    assert_eq!(zone(&state, isle), Zone::Battlefield);
    assert!(state.objects.get(isle).tapped, "enters tapped");
    assert_eq!(zone(&state, hidden), Zone::Exile);
    assert!(state.objects.get(hidden).v4.face_down_v1.is_some());
    // The other three go to the bottom of the library.
    let library = &state.players[0].library;
    assert_eq!(library.len(), 33);
    let bottom: Vec<&str> = library[30..]
        .iter()
        .map(|&card| CARD_DEFS[state.objects.get(card).card_def as usize].name)
        .collect();
    for name in ["Forest", "Island", "Swamp"] {
        assert!(bottom.contains(&name), "{name} on the bottom: {bottom:?}");
    }
    // The opponent learns nothing about the hidden card.
    let surface = mtg_kernel::surface_v2::HarnessSurfaceV2::new();
    let view = mtg_kernel::rl::observe_v2(&state, &surface, P1, 0).unwrap();
    assert!(view.known_face_down_cards.is_empty());
    let mut alternative = state.clone();
    alternative.objects.get_mut(hidden).card_def = id("Hypnotic Specter");
    alternative.objects.get_mut(hidden).name = "Hypnotic Specter".into();
    assert_eq!(
        view,
        mtg_kernel::rl::observe_v2(&alternative, &surface, P1, 0).unwrap()
    );
}

#[test]
fn shelldock_isle_plays_the_exiled_card_free_only_if_a_library_has_twenty_or_fewer() {
    // Both libraries above twenty: the ability does nothing.
    let (mut state, isle, hidden) = shelldock_game();
    shrink_library(&mut state, P1, 21);
    state.objects.get_mut(isle).tapped = false;
    pool(&mut state, P0, &[ManaColor::U], 0);
    activate(&mut state, isle, 0, &[]);
    settled(&mut state);
    assert!(state.objects.get(isle).tapped, "{{T}} is a cost");
    assert_eq!(zone(&state, hidden), Zone::Exile);
    assert!(state.objects.get(hidden).v4.face_down_v1.is_some());

    // Exactly twenty cards in the opponent's library: the question is asked.
    // Declining leaves the card hidden.
    shrink_library(&mut state, P1, 20);
    state.objects.get_mut(isle).tapped = false;
    pool(&mut state, P0, &[ManaColor::U], 0);
    activate(&mut state, isle, 0, &[]);
    settle_playing(&mut state, false);
    assert_eq!(zone(&state, hidden), Zone::Exile);
    assert!(state.objects.get(hidden).v4.face_down_v1.is_some());

    // Accepting casts it without paying its mana cost.
    state.objects.get_mut(isle).tapped = false;
    pool(&mut state, P0, &[ManaColor::U], 0);
    activate(&mut state, isle, 0, &[]);
    settle_playing(&mut state, true);
    assert_eq!(zone(&state, hidden), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::G), 0, "no mana was paid");
}

#[test]
fn shelldock_isle_counts_its_controllers_own_library_too() {
    let (mut state, isle, hidden) = shelldock_game();
    shrink_library(&mut state, P0, 20);
    state.objects.get_mut(isle).tapped = false;
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(activatable(&mut state, isle, 0));
    activate(&mut state, isle, 0, &[]);
    settle_playing(&mut state, true);
    assert_eq!(zone(&state, hidden), Zone::Battlefield);
}

#[test]
fn shelldock_isle_plays_an_exiled_land_only_with_a_land_play_available() {
    for played in [0, 1] {
        let mut state = game(&["Plains"; 30]);
        // The Forest is the deepest of the top four cards.
        let forest = put(&mut state, P0, "Forest", Zone::Library);
        for _ in 0..3 {
            put(&mut state, P0, "Island", Zone::Library);
        }
        let isle = put(&mut state, P0, "Shelldock Isle", Zone::Hand);
        next(&mut state);
        act(&mut state, Action::PlayLand(isle));
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets { .. } => {
                Action::ChooseEffectTarget(Target::Object(forest))
            }
            other => first_answer(other),
        });
        assert_eq!(zone(&state, forest), Zone::Exile);
        shrink_library(&mut state, P1, 20);
        state.objects.get_mut(isle).tapped = false;
        state.players[0].lands_played_this_turn = played;
        pool(&mut state, P0, &[ManaColor::U], 0);
        activate(&mut state, isle, 0, &[]);
        settle_playing(&mut state, true);
        assert_eq!(
            zone(&state, forest),
            if played == 0 {
                Zone::Battlefield
            } else {
                Zone::Exile
            },
            "lands played this turn: {played}"
        );
    }
}

// ---- Public sessions -----------------------------------------------------------------

#[derive(Default)]
struct C2bEvents {
    plays: std::collections::BTreeMap<&'static str, usize>,
    activations: usize,
}

impl C2bEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let Some(card_id) = semantic["source"]["card_db_id"].as_u64() else {
            return;
        };
        let name = CARD_DEFS[card_id as usize].name;
        let Some(&card) = C2B_CARDS.iter().find(|&&candidate| candidate == name) else {
            return;
        };
        match semantic["action_kind"].as_str() {
            Some("play_land") => *self.plays.entry(card).or_default() += 1,
            Some("activate_ability" | "activate_mana_ability") => self.activations += 1,
            _ => {}
        }
    }
}

#[test]
fn c2b_decks_finish_and_replay_through_public_sessions_both_seats() {
    let red_white = deck(&[
        ("Mountain", 6),
        ("Plains", 5),
        ("Sunbaked Canyon", 4),
        ("Mana Confluence", 3),
        ("Fabled Passage", 3),
        ("Library of Alexandria", 3),
        ("Celestial Colonnade", 3),
        ("Abandoned Air Temple", 3),
        ("Jackal Pup", 8),
        ("Lightning Bolt", 6),
        ("Seal of Fire", 4),
    ]);
    let green_blue_black = deck(&[
        ("Forest", 5),
        ("Island", 5),
        ("Swamp", 4),
        ("Waterlogged Grove", 4),
        ("Creeping Tar Pit", 4),
        ("Shelldock Isle", 4),
        ("Mana Confluence", 3),
        ("Fabled Passage", 3),
        ("Llanowar Elves", 8),
        ("Hypnotic Specter", 6),
        ("Sol Ring", 4),
    ]);

    let mut totals = C2bEvents::default();
    for (pair, first_seed) in [
        (
            [red_white.clone(), green_blue_black.clone()],
            0x4332_6200_u64,
        ),
        ([green_blue_black, red_white], 0x4332_6210),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let mut events = C2bEvents::default();
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action)
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
                for (card, count) in events.plays {
                    *totals.plays.entry(card).or_default() += count;
                }
                totals.activations += events.activations;
            }
        }
    }
    for card in C2B_CARDS {
        assert!(
            totals.plays.get(card).is_some_and(|&count| count > 0),
            "{card} never played"
        );
    }
}
