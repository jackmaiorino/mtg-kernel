//! Vintage Cube wave C2a: native rules and public-session tests for the
//! lands with an entry choice or a typed tap.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::collections::BTreeMap;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C2A_CARDS: [&str; 22] = [
    "Blood Crypt",
    "Breeding Pool",
    "Godless Shrine",
    "Hallowed Fountain",
    "Overgrown Tomb",
    "Sacred Foundry",
    "Steam Vents",
    "Stomping Ground",
    "Temple Garden",
    "Watery Grave",
    "Multiversal Passage",
    "Indatha Triome",
    "Ketria Triome",
    "Raugrin Triome",
    "Savai Triome",
    "Zagoth Triome",
    "Raffine's Tower",
    "Xander's Lounge",
    "Blazemire Verge",
    "Bleachbone Verge",
    "Botanical Sanctum",
    "Commercial District",
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
        0x4355_4245_0000 + 0x3261,
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

// ---- Tables ------------------------------------------------------------------------

type Dual = (&'static str, [ManaColor; 2], [Subtype; 2]);
type Triome = (&'static str, [ManaColor; 3], [Subtype; 3]);

/// The shocklands: name, the two colors of its mana, its two basic land types.
const SHOCKS: [Dual; 10] = [
    (
        "Blood Crypt",
        [ManaColor::B, ManaColor::R],
        [Subtype::Swamp, Subtype::Mountain],
    ),
    (
        "Breeding Pool",
        [ManaColor::G, ManaColor::U],
        [Subtype::Forest, Subtype::Island],
    ),
    (
        "Godless Shrine",
        [ManaColor::W, ManaColor::B],
        [Subtype::Plains, Subtype::Swamp],
    ),
    (
        "Hallowed Fountain",
        [ManaColor::W, ManaColor::U],
        [Subtype::Plains, Subtype::Island],
    ),
    (
        "Overgrown Tomb",
        [ManaColor::B, ManaColor::G],
        [Subtype::Swamp, Subtype::Forest],
    ),
    (
        "Sacred Foundry",
        [ManaColor::R, ManaColor::W],
        [Subtype::Mountain, Subtype::Plains],
    ),
    (
        "Steam Vents",
        [ManaColor::U, ManaColor::R],
        [Subtype::Island, Subtype::Mountain],
    ),
    (
        "Stomping Ground",
        [ManaColor::R, ManaColor::G],
        [Subtype::Mountain, Subtype::Forest],
    ),
    (
        "Temple Garden",
        [ManaColor::G, ManaColor::W],
        [Subtype::Forest, Subtype::Plains],
    ),
    (
        "Watery Grave",
        [ManaColor::U, ManaColor::B],
        [Subtype::Island, Subtype::Swamp],
    ),
];

const TRIOMES: [Triome; 7] = [
    (
        "Indatha Triome",
        [ManaColor::W, ManaColor::B, ManaColor::G],
        [Subtype::Plains, Subtype::Swamp, Subtype::Forest],
    ),
    (
        "Ketria Triome",
        [ManaColor::G, ManaColor::U, ManaColor::R],
        [Subtype::Forest, Subtype::Island, Subtype::Mountain],
    ),
    (
        "Raugrin Triome",
        [ManaColor::U, ManaColor::R, ManaColor::W],
        [Subtype::Island, Subtype::Mountain, Subtype::Plains],
    ),
    (
        "Savai Triome",
        [ManaColor::R, ManaColor::W, ManaColor::B],
        [Subtype::Mountain, Subtype::Plains, Subtype::Swamp],
    ),
    (
        "Zagoth Triome",
        [ManaColor::B, ManaColor::G, ManaColor::U],
        [Subtype::Swamp, Subtype::Forest, Subtype::Island],
    ),
    (
        "Raffine's Tower",
        [ManaColor::W, ManaColor::U, ManaColor::B],
        [Subtype::Plains, Subtype::Island, Subtype::Swamp],
    ),
    (
        "Xander's Lounge",
        [ManaColor::U, ManaColor::B, ManaColor::R],
        [Subtype::Island, Subtype::Swamp, Subtype::Mountain],
    ),
];

/// Verges: name, unconditional color, conditional color, the two basic land
/// types either of which enables the conditional color.
const VERGES: [(&str, ManaColor, ManaColor, [&str; 2]); 2] = [
    (
        "Blazemire Verge",
        ManaColor::B,
        ManaColor::R,
        ["Swamp", "Mountain"],
    ),
    (
        "Bleachbone Verge",
        ManaColor::B,
        ManaColor::W,
        ["Plains", "Swamp"],
    ),
];

/// Multiversal Passage's five choices, in the order the engine offers them.
const BASICS: [(Subtype, ManaColor); 5] = [
    (Subtype::Plains, ManaColor::W),
    (Subtype::Island, ManaColor::U),
    (Subtype::Swamp, ManaColor::B),
    (Subtype::Mountain, ManaColor::R),
    (Subtype::Forest, ManaColor::G),
];

// ---- Helpers -----------------------------------------------------------------------

/// Plays `land` from P0's hand (the choice it asks, if any, is left pending).
fn play(state: &mut GameState, land: ObjectId) {
    next(state);
    act(state, Action::PlayLand(land));
}

/// The next decision must be the land's own choice with `count` options.
fn expect_choice(state: &mut GameState, land: ObjectId, count: u16) {
    match next(state) {
        Decision::ChooseEffectOption {
            source,
            option_count,
            ..
        } => assert_eq!(
            (source, option_count),
            (land, count),
            "{}",
            state.objects.get(land).name
        ),
        other => panic!("expected a {count}-way choice of the land, got {other:?}"),
    }
}

/// No choice is pending: the game offers priority.
fn expect_priority(state: &mut GameState) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { .. }),
        "no choice is offered"
    );
}

fn total_mana(state: &GameState, player: PlayerId) -> u8 {
    state.players[player.index()].mana_pool.iter().sum()
}

fn is_tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
}

/// Taps `land` on a copy of the game for `color`: whether it worked and
/// added exactly that one mana.
fn taps_for(state: &GameState, land: ObjectId, color: ManaColor) -> bool {
    let mut probe = state.clone();
    next(&mut probe);
    engine::step(&mut probe, Action::ActivateManaAbilityChoice(land, color)).is_ok()
        && mana(&probe, P0, color) == 1
        && total_mana(&probe, P0) == 1
}

/// Taps a source that can add only one color on a copy of the game, with the
/// plain activation: whether it added exactly one mana of `color`.
fn taps_single(state: &GameState, land: ObjectId, color: ManaColor) -> bool {
    let mut probe = state.clone();
    next(&mut probe);
    engine::step(&mut probe, Action::ActivateManaAbility(land)).is_ok()
        && mana(&probe, P0, color) == 1
        && total_mana(&probe, P0) == 1
}

/// Puts `land` onto the battlefield as an effect would.
fn enter(state: &mut GameState, land: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(land, Zone::Battlefield));
}

fn has_subtype(state: &GameState, land: ObjectId, subtype: Subtype) -> bool {
    engine::has_effective_subtype(state, land, subtype)
}

// ---- Characteristics -----------------------------------------------------------------

#[test]
fn c2a_cards_are_full() {
    for name in C2A_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c2a_lands_have_their_printed_types_and_entry_rules() {
    for (name, _, subtypes) in SHOCKS {
        let card = def(name);
        assert!(card.has_type(CardType::Land), "{name}");
        assert_eq!(card.subtypes, &subtypes[..], "{name}");
        // The shocklands never enter tapped by rule: they ask.
        assert!(!card.enters_battlefield_tapped, "{name}");
    }
    for (name, _, subtypes) in TRIOMES {
        let card = def(name);
        assert!(card.has_type(CardType::Land), "{name}");
        assert_eq!(card.subtypes, &subtypes[..], "{name}");
        assert!(card.enters_battlefield_tapped, "{name}");
    }
    for name in [
        "Multiversal Passage",
        "Blazemire Verge",
        "Bleachbone Verge",
        "Botanical Sanctum",
    ] {
        let card = def(name);
        assert!(card.has_type(CardType::Land), "{name}");
        assert!(card.subtypes.is_empty(), "{name}: no printed land type");
        assert!(!card.enters_battlefield_tapped, "{name}");
    }
    let district = def("Commercial District");
    assert!(district.has_type(CardType::Land));
    assert_eq!(district.subtypes, &[Subtype::Mountain, Subtype::Forest][..]);
    assert!(district.enters_battlefield_tapped);

    let mut state = plains();
    for name in C2A_CARDS {
        let card = put(&mut state, P0, name, Zone::Hand);
        assert_eq!(engine::object_mana_value(&state, card), 0, "{name}");
        assert!(engine::object_has_type(&state, card, CardType::Land));
    }
}

// ---- Shocklands --------------------------------------------------------------------------

#[test]
fn shocklands_may_pay_two_life_or_enter_tapped() {
    for (name, colors, _) in SHOCKS {
        for pay in [true, false] {
            let mut state = plains();
            let land = put(&mut state, P0, name, Zone::Hand);
            play(&mut state, land);
            expect_choice(&mut state, land, 2);
            act(&mut state, Action::ChooseEffectOption(u16::from(pay)));
            settled(&mut state);
            assert_eq!(zone(&state, land), Zone::Battlefield, "{name}");
            assert_eq!(is_tapped(&state, land), !pay, "{name} pay={pay}");
            assert_eq!(life(&state, P0), if pay { 18 } else { 20 }, "{name}");
            assert_eq!(state.players[0].lands_played_this_turn, 1, "{name}");
            // The land is a normal dual: each color of its two taps once it
            // is untapped, and nothing while it is tapped.
            for color in colors {
                assert_eq!(taps_for(&state, land, color), pay, "{name} {color:?}");
            }
        }
    }
}

#[test]
fn shocklands_are_not_offered_the_payment_without_two_life() {
    for (name, _, _) in SHOCKS {
        let mut state = plains();
        state.players[0].life = 1;
        let land = put(&mut state, P0, name, Zone::Hand);
        play(&mut state, land);
        expect_priority(&mut state);
        assert!(is_tapped(&state, land), "{name}");
        assert_eq!(life(&state, P0), 1, "{name}");

        // With exactly two life the payment is offered (and may be refused).
        let mut state = plains();
        state.players[0].life = 2;
        let land = put(&mut state, P0, name, Zone::Hand);
        play(&mut state, land);
        expect_choice(&mut state, land, 2);
        act(&mut state, Action::ChooseEffectOption(0));
        settled(&mut state);
        assert!(is_tapped(&state, land), "{name}");
        assert_eq!(life(&state, P0), 2, "{name}");
    }
}

#[test]
fn a_shockland_or_passage_entering_under_blood_moon_has_no_entry_choice() {
    // Blood Moon makes the entering nonbasic land a Mountain with no
    // abilities, so nothing is asked and it enters untapped.
    for name in ["Steam Vents", "Watery Grave", "Multiversal Passage"] {
        let mut state = plains();
        // The Moon really enters: its effect is recorded as it does.
        let moon = put(&mut state, P1, "Blood Moon", Zone::Hand);
        enter(&mut state, moon);
        settled(&mut state);
        assert_eq!(zone(&state, moon), Zone::Battlefield);
        let land = put(&mut state, P0, name, Zone::Hand);
        play(&mut state, land);
        expect_priority(&mut state);
        assert_eq!(zone(&state, land), Zone::Battlefield, "{name}");
        assert!(!is_tapped(&state, land), "{name}");
        assert_eq!(life(&state, P0), 20, "{name}");
        assert!(
            taps_single(&state, land, ManaColor::R),
            "{name} is a Mountain"
        );
    }
}

#[test]
fn a_shockland_that_enters_tapped_for_another_reason_still_asks() {
    for pay in [true, false] {
        let mut state = plains();
        // Archon of Emeria: nonbasic lands its opponents control enter tapped.
        put(&mut state, P1, "Archon of Emeria", Zone::Battlefield);
        let land = put(&mut state, P0, "Steam Vents", Zone::Hand);
        play(&mut state, land);
        expect_choice(&mut state, land, 2);
        act(&mut state, Action::ChooseEffectOption(u16::from(pay)));
        settled(&mut state);
        assert!(is_tapped(&state, land), "the Archon's tapped entry stands");
        assert_eq!(life(&state, P0), if pay { 18 } else { 20 });
    }
}

#[test]
fn a_shockland_asks_however_it_enters_the_battlefield() {
    // Put onto the battlefield by an effect.
    for pay in [true, false] {
        let mut state = plains();
        let land = put(&mut state, P0, "Overgrown Tomb", Zone::Hand);
        enter(&mut state, land);
        expect_choice(&mut state, land, 2);
        act(&mut state, Action::ChooseEffectOption(u16::from(pay)));
        settled(&mut state);
        assert_eq!(is_tapped(&state, land), !pay);
        assert_eq!(life(&state, P0), if pay { 18 } else { 20 });
    }
    // Found by a fetchland that names its land types: a shockland has both.
    for pay in [true, false] {
        let mut state = game(&["Plains", "Hallowed Fountain", "Island"]);
        let fountain = state.players[0].library[1];
        let strand = put(&mut state, P0, "Flooded Strand", Zone::Battlefield);
        activate(&mut state, strand, 0, &[]);
        let (mut picked, mut asked) = (false, 0);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets { legal_targets, .. }
                if !picked && legal_targets.contains(&Target::Object(fountain)) =>
            {
                picked = true;
                Action::ChooseEffectTarget(Target::Object(fountain))
            }
            Decision::ChooseEffectOption {
                source,
                option_count,
                ..
            } if source == fountain => {
                assert_eq!(option_count, 2);
                asked += 1;
                Action::ChooseEffectOption(u16::from(pay))
            }
            other => first_answer(other),
        });
        assert!(picked, "the fetchland finds Hallowed Fountain");
        assert_eq!(asked, 1, "and Hallowed Fountain asks once");
        assert_eq!(zone(&state, fountain), Zone::Battlefield);
        assert_eq!(is_tapped(&state, fountain), !pay);
        assert_eq!(life(&state, P0), if pay { 17 } else { 19 });
    }
}

// ---- Botanical Sanctum --------------------------------------------------------------------

#[test]
fn botanical_sanctum_enters_untapped_only_with_two_or_fewer_other_lands() {
    for (others, tapped) in [(0, false), (2, false), (3, true)] {
        let mut state = plains();
        for _ in 0..others {
            put(&mut state, P0, "Plains", Zone::Battlefield);
        }
        // An opponent's lands never count.
        for _ in 0..3 {
            put(&mut state, P1, "Plains", Zone::Battlefield);
        }
        let land = put(&mut state, P0, "Botanical Sanctum", Zone::Hand);
        play(&mut state, land);
        expect_priority(&mut state);
        assert_eq!(is_tapped(&state, land), tapped, "{others} other lands");
        if !tapped {
            assert!(taps_for(&state, land, ManaColor::G));
            assert!(taps_for(&state, land, ManaColor::U));
            assert!(!taps_for(&state, land, ManaColor::R));
        }
    }
    // Tapped lands count as lands too.
    let mut state = plains();
    for _ in 0..3 {
        let land = put(&mut state, P0, "Plains", Zone::Battlefield);
        state.objects.get_mut(land).tapped = true;
    }
    let land = put(&mut state, P0, "Botanical Sanctum", Zone::Hand);
    play(&mut state, land);
    assert!(is_tapped(&state, land));
}

// ---- Triomes ------------------------------------------------------------------------------

#[test]
fn triomes_enter_tapped_tap_for_three_colors_and_cycle_for_three() {
    for (name, colors, _) in TRIOMES {
        let mut state = plains();
        let triome = put(&mut state, P0, name, Zone::Hand);
        play(&mut state, triome);
        expect_priority(&mut state);
        assert!(is_tapped(&state, triome), "{name} enters tapped");
        assert!(!taps_for(&state, triome, colors[0]), "{name} while tapped");

        // On the battlefield and untapped it taps for each of its colors.
        let mut state = plains();
        let triome = put(&mut state, P0, name, Zone::Battlefield);
        for color in colors {
            assert!(taps_for(&state, triome, color), "{name} {color:?}");
        }
        let outside = [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
        .into_iter()
        .find(|color| !colors.contains(color))
        .unwrap();
        assert!(!taps_for(&state, triome, outside), "{name} {outside:?}");

        // Cycling {3} from the hand: not with two mana, once with three.
        let mut state = plains();
        let triome = put(&mut state, P0, name, Zone::Hand);
        pool(&mut state, P0, &[], 2);
        assert!(!activatable(&mut state, triome, 0), "{name} with two mana");
        pool(&mut state, P0, &[], 1);
        let hand = state.players[0].hand.len();
        activate(&mut state, triome, 0, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, triome), Zone::Graveyard, "{name}");
        assert_eq!(
            state.players[0].hand.len(),
            hand,
            "{name}: discarded itself, drew one card"
        );
        assert_eq!(total_mana(&state, P0), 0, "{name}");
    }
}

// ---- Verges --------------------------------------------------------------------------------

#[test]
fn verges_add_their_second_color_only_while_you_control_a_matching_basic_type() {
    for (name, primary, conditional, enablers) in VERGES {
        // Without an enabler only the first color.
        let mut state = plains();
        let verge = put(&mut state, P0, name, Zone::Battlefield);
        // A single-color source uses the plain activation.
        assert!(taps_single(&state, verge, primary), "{name}");
        assert!(!taps_for(&state, verge, conditional), "{name} bare");
        // An opponent's matching land does not enable it.
        for enabler in enablers {
            put(&mut state, P1, enabler, Zone::Battlefield);
        }
        assert!(!taps_for(&state, verge, conditional), "{name} opponent's");

        for enabler in enablers {
            let mut state = plains();
            let verge = put(&mut state, P0, name, Zone::Battlefield);
            put(&mut state, P0, enabler, Zone::Battlefield);
            assert!(taps_for(&state, verge, primary), "{name} with {enabler}");
            assert!(
                taps_for(&state, verge, conditional),
                "{name} with {enabler}"
            );
        }
    }
    // A land that merely has the basic land type counts: Steam Vents is a
    // Mountain, Godless Shrine a Plains and a Swamp.
    for (name, dual) in [
        ("Blazemire Verge", "Steam Vents"),
        ("Bleachbone Verge", "Godless Shrine"),
    ] {
        let (_, _, conditional, _) = VERGES.iter().copied().find(|v| v.0 == name).unwrap();
        let mut state = plains();
        let verge = put(&mut state, P0, name, Zone::Battlefield);
        put(&mut state, P0, dual, Zone::Battlefield);
        assert!(taps_for(&state, verge, conditional), "{name} with {dual}");
    }
}

// ---- Commercial District ---------------------------------------------------------------------

#[test]
fn commercial_district_enters_tapped_taps_for_two_colors_and_surveils_one() {
    for (put_in_graveyard, expected) in [(true, Zone::Graveyard), (false, Zone::Library)] {
        let mut state = game(&["Lightning Bolt", "Island", "Island", "Island"]);
        let top = state.players[0].library[0];
        let land = put(&mut state, P0, "Commercial District", Zone::Hand);
        play(&mut state, land);
        assert!(is_tapped(&state, land));
        match settle(&mut state) {
            // Option 1 puts the top card into the graveyard.
            Some(Decision::ChooseEffectOption {
                source,
                option_count,
                ..
            }) => {
                assert_eq!((source, option_count), (land, 2));
                act(
                    &mut state,
                    Action::ChooseEffectOption(u16::from(put_in_graveyard)),
                );
            }
            other => panic!("expected the surveil, got {other:?}"),
        }
        settled(&mut state);
        assert_eq!(zone(&state, top), expected);
        if !put_in_graveyard {
            assert_eq!(state.players[0].library[0], top, "kept on top");
        }
    }
    let mut state = plains();
    let land = put(&mut state, P0, "Commercial District", Zone::Battlefield);
    assert!(taps_for(&state, land, ManaColor::R));
    assert!(taps_for(&state, land, ManaColor::G));
    assert!(!taps_for(&state, land, ManaColor::U));
    assert!(has_subtype(&state, land, Subtype::Mountain));
    assert!(has_subtype(&state, land, Subtype::Forest));
}

// ---- Multiversal Passage ----------------------------------------------------------------------

#[test]
fn multiversal_passage_is_the_chosen_basic_type_and_may_pay_two_life() {
    for (index, (chosen, color)) in BASICS.iter().copied().enumerate() {
        for pay in [true, false] {
            let mut state = plains();
            let land = put(&mut state, P0, "Multiversal Passage", Zone::Hand);
            play(&mut state, land);
            // First the basic land type, then the payment.
            expect_choice(&mut state, land, 5);
            act(
                &mut state,
                Action::ChooseEffectOption(u16::try_from(index).unwrap()),
            );
            expect_choice(&mut state, land, 2);
            act(&mut state, Action::ChooseEffectOption(u16::from(pay)));
            settled(&mut state);
            assert_eq!(zone(&state, land), Zone::Battlefield);
            assert_eq!(is_tapped(&state, land), !pay, "{chosen:?} pay={pay}");
            assert_eq!(life(&state, P0), if pay { 18 } else { 20 });
            for (other, _) in BASICS {
                assert_eq!(
                    has_subtype(&state, land, other),
                    other == chosen,
                    "{chosen:?} is the only type, not {other:?}"
                );
            }
            assert!(engine::object_has_type(&state, land, CardType::Land));
            // It has the mana ability of the chosen type once untapped.
            state.objects.get_mut(land).tapped = false;
            next(&mut state);
            act(&mut state, Action::ActivateManaAbility(land));
            assert_eq!(mana(&state, P0, color), 1, "{chosen:?}");
            assert_eq!(total_mana(&state, P0), 1, "{chosen:?}");
        }
    }
}

#[test]
fn multiversal_passage_without_two_life_enters_tapped_after_its_type_choice() {
    let mut state = plains();
    state.players[0].life = 1;
    let land = put(&mut state, P0, "Multiversal Passage", Zone::Hand);
    play(&mut state, land);
    expect_choice(&mut state, land, 5);
    act(&mut state, Action::ChooseEffectOption(2));
    expect_priority(&mut state);
    assert!(is_tapped(&state, land));
    assert_eq!(life(&state, P0), 1);
    assert!(has_subtype(&state, land, Subtype::Swamp));
}

#[test]
fn multiversal_passage_chooses_again_each_time_it_enters() {
    let mut state = plains();
    let land = put(&mut state, P0, "Multiversal Passage", Zone::Hand);
    play(&mut state, land);
    expect_choice(&mut state, land, 5);
    act(&mut state, Action::ChooseEffectOption(2));
    expect_choice(&mut state, land, 2);
    act(&mut state, Action::ChooseEffectOption(1));
    settled(&mut state);
    assert!(has_subtype(&state, land, Subtype::Swamp));
    // It leaves and comes back: the old choice is gone, a new one is made.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(land, Zone::Hand));
    assert!(!has_subtype(&state, land, Subtype::Swamp));
    enter(&mut state, land);
    expect_choice(&mut state, land, 5);
    act(&mut state, Action::ChooseEffectOption(4));
    expect_choice(&mut state, land, 2);
    act(&mut state, Action::ChooseEffectOption(0));
    settled(&mut state);
    assert!(has_subtype(&state, land, Subtype::Forest));
    assert!(!has_subtype(&state, land, Subtype::Swamp));
    assert!(is_tapped(&state, land));
    assert_eq!(life(&state, P0), 18, "only the first entry was paid");
}

#[test]
fn multiversal_passage_counts_for_a_verges_basic_type_check() {
    // Blazemire Verge needs a Swamp or a Mountain.
    for (index, enables) in [(0, false), (1, false), (2, true), (3, true), (4, false)] {
        let mut state = plains();
        let verge = put(&mut state, P0, "Blazemire Verge", Zone::Battlefield);
        let land = put(&mut state, P0, "Multiversal Passage", Zone::Hand);
        assert!(!taps_for(&state, verge, ManaColor::R));
        play(&mut state, land);
        expect_choice(&mut state, land, 5);
        act(&mut state, Action::ChooseEffectOption(index));
        expect_choice(&mut state, land, 2);
        act(&mut state, Action::ChooseEffectOption(1));
        settled(&mut state);
        assert_eq!(
            taps_for(&state, verge, ManaColor::R),
            enables,
            "Passage as {:?}",
            BASICS[usize::from(index)].0
        );
    }
}

// ---- Public sessions -------------------------------------------------------------------------

#[derive(Default)]
struct C2aEvents {
    lands_played: BTreeMap<&'static str, usize>,
    payment_choices: usize,
    payments_made: usize,
    type_choices: usize,
    cycles: usize,
}

impl C2aEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let Some(card_id) = semantic["source"]["card_db_id"].as_u64() else {
            return;
        };
        let name = CARD_DEFS[card_id as usize].name;
        match semantic["action_kind"].as_str() {
            Some("play_land") if C2A_CARDS.contains(&name) => {
                *self.lands_played.entry(name).or_default() += 1;
            }
            Some("activate_ability") if name.ends_with("Triome") => self.cycles += 1,
            Some("choose_effect_option") if semantic["option_count"] == 5 => {
                if name == "Multiversal Passage" {
                    self.type_choices += 1;
                }
            }
            Some("choose_effect_option") if semantic["option_count"] == 2 => {
                if SHOCKS.iter().any(|(shock, _, _)| *shock == name)
                    || name == "Multiversal Passage"
                {
                    self.payment_choices += 1;
                    self.payments_made += usize::from(semantic["option_index"] == 1);
                }
            }
            _ => {}
        }
    }

    fn add(&mut self, other: C2aEvents) {
        for (name, count) in other.lands_played {
            *self.lands_played.entry(name).or_default() += count;
        }
        self.payment_choices += other.payment_choices;
        self.payments_made += other.payments_made;
        self.type_choices += other.type_choices;
        self.cycles += other.cycles;
    }
}

#[test]
fn c2a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let first = deck(&[
        ("Blood Crypt", 2),
        ("Breeding Pool", 2),
        ("Sacred Foundry", 2),
        ("Steam Vents", 2),
        ("Stomping Ground", 2),
        ("Temple Garden", 2),
        ("Multiversal Passage", 2),
        ("Ketria Triome", 2),
        ("Raugrin Triome", 2),
        ("Commercial District", 2),
        ("Botanical Sanctum", 2),
        ("Lightning Bolt", 6),
        ("Jackal Pup", 6),
        ("Llanowar Elves", 6),
    ]);
    let second = deck(&[
        ("Godless Shrine", 2),
        ("Hallowed Fountain", 2),
        ("Overgrown Tomb", 2),
        ("Watery Grave", 2),
        ("Indatha Triome", 2),
        ("Savai Triome", 2),
        ("Zagoth Triome", 2),
        ("Raffine's Tower", 2),
        ("Xander's Lounge", 2),
        ("Blazemire Verge", 2),
        ("Bleachbone Verge", 2),
        ("Lightning Bolt", 6),
        ("Jackal Pup", 6),
        ("Llanowar Elves", 6),
    ]);

    let mut totals = C2aEvents::default();
    for (pair, first_seed) in [
        ([first.clone(), second.clone()], 0x4332_4100_u64),
        ([second, first], 0x4332_4110),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let mut events = C2aEvents::default();
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action)
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
                totals.add(events);
            }
        }
    }

    for name in C2A_CARDS {
        assert!(
            totals.lands_played.get(name).copied().unwrap_or(0) > 0,
            "{name} was never played"
        );
    }
    assert!(totals.payment_choices > 0, "no life payment was offered");
    assert!(totals.type_choices > 0, "no land type was chosen");
}
