//! Vintage Cube wave C1a: native rules and public-session tests for the
//! white and blue instants and sorceries.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C1A_CARDS: [&str; 14] = [
    "Damn",
    "Memory Lapse",
    "Miscalculation",
    "Mystic Confluence",
    "Occult Epiphany",
    "Oust",
    "Path to Exile",
    "Remand",
    "Reprieve",
    "Stern Scolding",
    "Time Spiral",
    "Timetwister",
    "Unexpectedly Absent",
    "Winds of Abandon",
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
        0x4355_4245_0000 + 0x3161,
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

// ---- Characteristics -----------------------------------------------------------

#[test]
fn c1a_cards_are_full() {
    for name in C1A_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c1a_cards_have_their_printed_characteristics() {
    let rows: [(&str, u16, CardType); 14] = [
        ("Damn", 2, CardType::Sorcery),
        ("Winds of Abandon", 2, CardType::Sorcery),
        ("Unexpectedly Absent", 2, CardType::Instant),
        ("Mystic Confluence", 5, CardType::Instant),
        ("Occult Epiphany", 1, CardType::Instant),
        ("Time Spiral", 6, CardType::Sorcery),
        ("Oust", 1, CardType::Sorcery),
        ("Path to Exile", 1, CardType::Instant),
        ("Reprieve", 2, CardType::Instant),
        ("Memory Lapse", 2, CardType::Instant),
        ("Remand", 2, CardType::Instant),
        ("Stern Scolding", 1, CardType::Instant),
        ("Miscalculation", 2, CardType::Instant),
        ("Timetwister", 3, CardType::Sorcery),
    ];
    for (name, mana_value, card_type) in rows {
        let card = def(name);
        assert_eq!(card.mana_value, mana_value, "{name}");
        assert_eq!(card.types, &[card_type], "{name}");
    }
    assert_eq!(def("Oust").target_spec, TargetSpec::Creature);
    assert_eq!(def("Path to Exile").target_spec, TargetSpec::Creature);
    for name in ["Reprieve", "Memory Lapse", "Remand", "Miscalculation"] {
        assert_eq!(def(name).target_spec, TargetSpec::AnySpellOnStack, "{name}");
    }
    assert_eq!(def("Timetwister").target_spec, TargetSpec::None);
    assert_eq!(def("Damn").target_spec, TargetSpec::Creature);
    assert_eq!(
        def("Winds of Abandon").target_spec,
        TargetSpec::OpponentControlledCreature
    );
    assert_eq!(
        def("Unexpectedly Absent").target_spec,
        TargetSpec::NonlandPermanent
    );
    assert_eq!(def("Occult Epiphany").target_spec, TargetSpec::None);
    assert_eq!(def("Time Spiral").target_spec, TargetSpec::None);
    // The overloaded cards have a second form and Mystic Confluence ten
    // modes.
    for name in ["Damn", "Winds of Abandon"] {
        assert!(def(name).adventure.is_some(), "{name} overload");
    }
    assert!(def("Mystic Confluence").mode2.is_some());
    assert!(def("Mystic Confluence").mode3.is_some());
    assert_eq!(def("Mystic Confluence").additional_modes.len(), 7);
    // Miscalculation's cycling is its only ability.
    assert_eq!(def("Miscalculation").activated_abilities.len(), 1);
}

// ---- Helpers for this batch ---------------------------------------------------------

/// P1 casts `name` on its own turn (so a creature spell is legal), pays for
/// it from its pool and passes priority, leaving the spell on the stack with
/// P0 holding priority.
fn opponent_casts(
    state: &mut GameState,
    name: &str,
    colored: &[ManaColor],
    colorless: u8,
    targets: &[Target],
) -> ObjectId {
    state.active_player = P1;
    state.priority_player = P1;
    let spell = put(state, P1, name, Zone::Hand);
    pool(state, P1, colored, colorless);
    cast(state, spell, targets);
    spell
}

/// P0 passes priority with its spell on the stack and P1 answers with a
/// Lightning Bolt at `target`.
fn opponent_responds_with_bolt(state: &mut GameState, target: Target) -> ObjectId {
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(state, Action::Pass);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    cast(state, bolt, &[target]);
    bolt
}

/// Passes priority from P1 so P0 may respond.
fn pass_to_p0(state: &mut GameState) {
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(state, Action::Pass);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
}

fn hand_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

fn library_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].library.len()
}

/// Moves `object` (owned by P1) under P0's control.
fn steal(state: &mut GameState, object: ObjectId) {
    state.players[P1.index()]
        .battlefield
        .retain(|&o| o != object);
    state.players[P0.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = P0;
}

// ---- Oust ---------------------------------------------------------------------------

#[test]
fn oust_puts_the_creature_second_from_the_top_and_its_controller_gains_three() {
    for (caster, victim_seat) in [(P0, P1), (P0, P0), (P1, P0), (P1, P1)] {
        let mut state = plains();
        state.active_player = caster;
        state.priority_player = caster;
        let angler = put(&mut state, victim_seat, "Gurmag Angler", Zone::Battlefield);
        let oust = put(&mut state, caster, "Oust", Zone::Hand);
        let top = state.players[victim_seat.index()].library[0];
        let before = library_len(&state, victim_seat);
        pool(&mut state, caster, &[ManaColor::W], 0);
        cast(&mut state, oust, &[Target::Object(angler)]);
        settled(&mut state);
        let library = &state.players[victim_seat.index()].library;
        assert_eq!(library.len(), before + 1, "{caster:?} on {victim_seat:?}");
        assert_eq!(library[0], top);
        assert_eq!(library[1], angler);
        assert_eq!(zone(&state, angler), Zone::Library);
        assert_eq!(life(&state, victim_seat), 23);
        assert_eq!(life(&state, victim_seat.opponent()), 20);
        assert_eq!(zone(&state, oust), Zone::Graveyard);
    }
}

#[test]
fn oust_goes_to_the_owners_library_and_its_controller_gains_the_life() {
    let mut state = plains();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    steal(&mut state, angler);
    let oust = put(&mut state, P0, "Oust", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, oust, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(state.players[P1.index()].library[1], angler);
    assert!(!state.players[P0.index()].library.contains(&angler));
    assert_eq!(life(&state, P0), 23, "its controller gains the life");
    assert_eq!(life(&state, P1), 20);
}

#[test]
fn oust_handles_empty_and_one_card_libraries_and_tokens() {
    // Empty library: the creature is the only card.
    let mut state = game(&[]);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let oust = put(&mut state, P0, "Oust", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, oust, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(state.players[P1.index()].library, vec![angler]);
    assert_eq!(life(&state, P1), 23);

    // One card: it goes below that card, which is second from the top.
    let mut state = game(&["Plains"]);
    let only = state.players[P1.index()].library[0];
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let oust = put(&mut state, P0, "Oust", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, oust, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(state.players[P1.index()].library, vec![only, angler]);

    // A token ceases to exist in the library, but its controller still
    // gains the life.
    let mut state = plains();
    let before = library_len(&state, P1);
    let token = token(&mut state, P1, "Gurmag Angler");
    let oust = put(&mut state, P0, "Oust", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, oust, &[Target::Object(token)]);
    settled(&mut state);
    assert!(!state.players[P1.index()].battlefield.contains(&token));
    assert!(!state.players[P1.index()].library.contains(&token));
    assert_eq!(library_len(&state, P1), before);
    assert_eq!(life(&state, P1), 23);
}

#[test]
fn oust_does_nothing_when_its_target_left_before_it_resolved() {
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let oust = put(&mut state, P0, "Oust", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, oust, &[Target::Object(elves)]);
    opponent_responds_with_bolt(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "the Bolt killed it");
    assert_eq!(life(&state, P1), 20, "all targets illegal: no life gained");
    assert_eq!(library_len(&state, P1), 20);
    assert_eq!(zone(&state, oust), Zone::Graveyard);
}

// ---- Path to Exile ------------------------------------------------------------------

/// Settles a Path to Exile (or Winds of Abandon), answering its
/// controller's choice to search and the search with `land` when `Some`,
/// and declining (or finding nothing, for Winds) otherwise. Returns who was
/// asked first.
fn settle_path(state: &mut GameState, land: Option<ObjectId>) -> Option<PlayerId> {
    let mut asked = None;
    settle_with(state, |_, decision| match decision {
        // Path to Exile's "may search": search only to find `land`.
        Decision::ChooseEffectOption {
            player,
            option_count: 2,
            ..
        } => {
            if asked.is_none() {
                asked = Some(player);
            }
            Action::ChooseEffectOption(u16::from(land.is_some()))
        }
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            selected_count: 0,
            ..
        } => {
            if asked.is_none() {
                asked = Some(player);
            }
            match land {
                Some(land) => {
                    assert!(legal_targets.contains(&Target::Object(land)));
                    Action::ChooseEffectTarget(Target::Object(land))
                }
                None => Action::FinishEffectSelection,
            }
        }
        Decision::ChooseEffectTargets { .. } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    asked
}

#[test]
fn path_to_exile_exiles_and_its_controller_may_fetch_a_tapped_basic_land() {
    for (caster, victim_seat) in [(P0, P1), (P0, P0), (P1, P0), (P1, P1)] {
        let mut state = game(&["Forest", "Island", "Island"]);
        state.active_player = caster;
        state.priority_player = caster;
        let forest = state.players[victim_seat.index()].library[0];
        let other_library = state.players[victim_seat.opponent().index()]
            .library
            .clone();
        let angler = put(&mut state, victim_seat, "Gurmag Angler", Zone::Battlefield);
        let path = put(&mut state, caster, "Path to Exile", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::W], 0);
        cast(&mut state, path, &[Target::Object(angler)]);
        let asked = settle_path(&mut state, Some(forest));
        assert_eq!(
            asked,
            Some(victim_seat),
            "the creature's controller searches"
        );
        assert_eq!(zone(&state, angler), Zone::Exile);
        assert_eq!(zone(&state, forest), Zone::Battlefield);
        assert!(state.objects.get(forest).tapped, "it enters tapped");
        assert_eq!(state.objects.get(forest).controller, victim_seat);
        assert_eq!(life(&state, victim_seat), 20, "no life is gained");
        assert_eq!(
            state.players[victim_seat.opponent().index()].library,
            other_library,
            "the other library is untouched"
        );
        assert_eq!(zone(&state, path), Zone::Graveyard);
    }
}

#[test]
fn path_to_exile_search_may_find_nothing_and_finds_only_basic_lands() {
    let mut state = game(&["Plateau", "Plains", "Lightning Bolt"]);
    let plateau = state.players[P1.index()].library[0];
    let plains_card = state.players[P1.index()].library[1];
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let path = put(&mut state, P0, "Path to Exile", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, path, &[Target::Object(angler)]);
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        // Search.
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            offered = legal_targets;
            Action::FinishEffectSelection
        }
        other => first_answer(other),
    });
    assert!(offered.contains(&Target::Object(plains_card)));
    assert!(
        !offered.contains(&Target::Object(plateau)),
        "a dual land is not basic"
    );
    assert_eq!(offered.len(), 1);
    assert_eq!(zone(&state, angler), Zone::Exile);
    assert_eq!(zone(&state, plains_card), Zone::Library, "found nothing");
    assert_eq!(library_len(&state, P1), 3);
}

#[test]
fn path_to_exile_shuffles_only_when_its_controller_chooses_to_search() {
    // A player who chooses not to search does not shuffle; a player who
    // searches and finds nothing still does (701.19). Library order is the
    // observable: sixteen distinct cards keep it after no shuffle.
    for (library, search) in [
        ("Plains", false),
        ("Plains", true),
        ("Lightning Bolt", true),
    ] {
        let mut state = game(&[library; 16]);
        let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
        let before = state.players[P1.index()].library.clone();
        let path = put(&mut state, P0, "Path to Exile", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::W], 0);
        cast(&mut state, path, &[Target::Object(angler)]);
        match settle(&mut state) {
            Some(Decision::ChooseEffectOption {
                player,
                option_count,
                ..
            }) => assert_eq!((player, option_count), (P1, 2), "search or not"),
            other => panic!("expected the choice to search, got {other:?}"),
        }
        act(&mut state, Action::ChooseEffectOption(u16::from(search)));
        // A search with no candidate may end without a prompt.
        if let (true, Some(decision)) = (search, settle(&mut state)) {
            match decision {
                Decision::ChooseEffectTargets {
                    player, can_finish, ..
                } => assert!(player == P1 && can_finish, "{library}: may find nothing"),
                other => panic!("expected the search, got {other:?}"),
            }
            act(&mut state, Action::FinishEffectSelection);
        }
        settled(&mut state);
        assert_eq!(zone(&state, angler), Zone::Exile);
        let after = &state.players[P1.index()].library;
        assert_eq!(after.len(), before.len());
        if search {
            assert_ne!(*after, before, "{library}: a search shuffles");
        } else {
            assert_eq!(*after, before, "declining does not shuffle");
        }
    }
}

#[test]
fn path_to_exile_without_a_basic_land_in_the_library_just_exiles() {
    let mut state = game(&["Lightning Bolt", "Lightning Bolt"]);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let path = put(&mut state, P0, "Path to Exile", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, path, &[Target::Object(angler)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, angler), Zone::Exile);
    assert_eq!(library_len(&state, P1), 2);
}

#[test]
fn path_to_exile_gives_a_stolen_creatures_controller_the_search() {
    let mut state = game(&["Forest", "Island", "Island"]);
    let own_forest = state.players[P0.index()].library[0];
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    steal(&mut state, angler);
    let path = put(&mut state, P0, "Path to Exile", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, path, &[Target::Object(angler)]);
    let asked = settle_path(&mut state, Some(own_forest));
    assert_eq!(asked, Some(P0));
    assert_eq!(zone(&state, angler), Zone::Exile);
    assert_eq!(zone(&state, own_forest), Zone::Battlefield);
}

#[test]
fn path_to_exile_does_nothing_when_its_target_left_before_it_resolved() {
    let mut state = game(&["Forest", "Island", "Island"]);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let path = put(&mut state, P0, "Path to Exile", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, path, &[Target::Object(elves)]);
    opponent_responds_with_bolt(&mut state, Target::Object(elves));
    let asked = settle_path(&mut state, None);
    assert_eq!(asked, None, "no search when the spell fizzles");
    assert_eq!(zone(&state, elves), Zone::Graveyard, "the Bolt killed it");
    assert_eq!(library_len(&state, P1), 3);
    assert_eq!(zone(&state, path), Zone::Graveyard);
}

// ---- Reprieve -----------------------------------------------------------------------

#[test]
fn reprieve_returns_the_spell_to_its_owners_hand_and_draws_a_card() {
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let reprieve = put(&mut state, P0, "Reprieve", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert_eq!(hand_len(&state, P0), 1);
    cast(&mut state, reprieve, &[Target::Object(bolt)]);
    settled(&mut state);
    assert!(
        hand_contains(&state, P1, bolt),
        "Bolt is back in its owner's hand"
    );
    assert_eq!(life(&state, P0), 20, "Bolt never resolved");
    assert_eq!(
        hand_len(&state, P0),
        1,
        "the Reprieve was replaced by a draw"
    );
    assert_eq!(library_len(&state, P0), 19);
    assert_eq!(zone(&state, reprieve), Zone::Graveyard);
}

#[test]
fn reprieve_returns_a_spell_that_cant_be_countered() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let decay = opponent_casts(
        &mut state,
        "Abrupt Decay",
        &[ManaColor::B, ManaColor::G],
        0,
        &[Target::Object(elves)],
    );
    pass_to_p0(&mut state);
    let reprieve = put(&mut state, P0, "Reprieve", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, reprieve, &[Target::Object(decay)]);
    settled(&mut state);
    assert!(hand_contains(&state, P1, decay));
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(hand_len(&state, P0), 1);
}

#[test]
fn reprieve_fizzles_without_drawing_when_the_spell_already_left() {
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let reprieve = put(&mut state, P0, "Reprieve", Zone::Hand);
    let remand = put(&mut state, P0, "Remand", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 2);
    cast(&mut state, reprieve, &[Target::Object(bolt)]);
    cast(&mut state, remand, &[Target::Object(bolt)]);
    settled(&mut state);
    // Remand resolved first: Bolt went to P1's hand and P0 drew one card.
    // Reprieve then had no legal target: no second draw.
    assert!(hand_contains(&state, P1, bolt));
    assert_eq!(hand_len(&state, P0), 1);
    assert_eq!(library_len(&state, P0), 19);
    assert_eq!(zone(&state, reprieve), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn reprieve_makes_a_copy_of_a_spell_cease_to_exist() {
    let mut state = plains();
    // P1 casts Lightning Bolt, then Brain Freeze: its storm trigger copies
    // Brain Freeze once.
    opponent_casts(
        &mut state,
        "Lightning Bolt",
        &[ManaColor::R],
        0,
        &[Target::Player(P0)],
    );
    let freeze = opponent_casts(
        &mut state,
        "Brain Freeze",
        &[ManaColor::U],
        1,
        &[Target::Player(P0)],
    );
    // Pass priority until the copy sits on the stack with P0 to act.
    for _ in 0..40 {
        if state.stack.iter().any(|item| item.is_copy) {
            if let Decision::CastSpellOrPass { player: P0, .. } = next(&mut state) {
                break;
            }
        }
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            other => act(&mut state, first_answer(other)),
        }
    }
    let copy = state
        .stack
        .iter()
        .find(|item| item.is_copy)
        .expect("a storm copy on the stack")
        .source;
    assert_ne!(copy, freeze);
    let reprieve = put(&mut state, P0, "Reprieve", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, reprieve, &[Target::Object(copy)]);
    settle_first(&mut state);
    assert!(state.stack.is_empty());
    assert!(
        !state.players[P0.index()].hand.contains(&copy),
        "a copy is not a card"
    );
    assert_eq!(hand_len(&state, P0), 1, "only the Reprieve draw");
    assert_eq!(life(&state, P0), 17, "Bolt resolved");
    // The original Brain Freeze milled three; the copy never resolved.
    assert_eq!(library_len(&state, P0), 20 - 1 - 3);
}

// ---- Memory Lapse -------------------------------------------------------------------

#[test]
fn memory_lapse_counters_and_puts_the_spell_on_top_of_its_owners_library() {
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let lapse = put(&mut state, P0, "Memory Lapse", Zone::Hand);
    let before = library_len(&state, P1);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, lapse, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Library);
    assert_eq!(state.players[P1.index()].library[0], bolt, "on top");
    assert_eq!(library_len(&state, P1), before + 1);
    assert_eq!(life(&state, P0), 20);
    assert_eq!(zone(&state, lapse), Zone::Graveyard);
    assert!(state.players[P1.index()].graveyard.is_empty());
}

#[test]
fn memory_lapse_leaves_a_spell_that_cant_be_countered_alone() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let decay = opponent_casts(
        &mut state,
        "Abrupt Decay",
        &[ManaColor::B, ManaColor::G],
        0,
        &[Target::Object(elves)],
    );
    pass_to_p0(&mut state);
    let lapse = put(&mut state, P0, "Memory Lapse", Zone::Hand);
    let before = library_len(&state, P1);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, lapse, &[Target::Object(decay)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "Decay resolved");
    assert_eq!(zone(&state, decay), Zone::Graveyard);
    assert_eq!(library_len(&state, P1), before);
}

#[test]
fn memory_lapse_fizzles_when_the_spell_already_left_the_stack() {
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let first = put(&mut state, P0, "Memory Lapse", Zone::Hand);
    let second = put(&mut state, P0, "Memory Lapse", Zone::Hand);
    let before = library_len(&state, P1);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(&mut state, first, &[Target::Object(bolt)]);
    cast(&mut state, second, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(state.players[P1.index()].library[0], bolt);
    assert_eq!(library_len(&state, P1), before + 1, "put on top only once");
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
}

// ---- Remand -------------------------------------------------------------------------

#[test]
fn remand_counters_returns_the_spell_to_its_owners_hand_and_draws() {
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let remand = put(&mut state, P0, "Remand", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, remand, &[Target::Object(bolt)]);
    settled(&mut state);
    assert!(hand_contains(&state, P1, bolt));
    assert!(state.players[P1.index()].graveyard.is_empty());
    assert_eq!(life(&state, P0), 20);
    assert_eq!(hand_len(&state, P0), 1, "drew a card");
    assert_eq!(library_len(&state, P0), 19);
    assert_eq!(zone(&state, remand), Zone::Graveyard);
}

#[test]
fn remand_still_draws_when_the_spell_cant_be_countered() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let decay = opponent_casts(
        &mut state,
        "Abrupt Decay",
        &[ManaColor::B, ManaColor::G],
        0,
        &[Target::Object(elves)],
    );
    pass_to_p0(&mut state);
    let remand = put(&mut state, P0, "Remand", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, remand, &[Target::Object(decay)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "Decay resolved");
    assert_eq!(zone(&state, decay), Zone::Graveyard);
    assert_eq!(hand_len(&state, P0), 1, "the draw is not conditional");
}

#[test]
fn remand_fizzles_without_drawing_when_the_spell_already_left() {
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let first = put(&mut state, P0, "Remand", Zone::Hand);
    let second = put(&mut state, P0, "Remand", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(&mut state, first, &[Target::Object(bolt)]);
    cast(&mut state, second, &[Target::Object(bolt)]);
    settled(&mut state);
    assert!(hand_contains(&state, P1, bolt));
    assert_eq!(
        hand_len(&state, P0),
        1,
        "only the Remand that resolved drew"
    );
    assert_eq!(library_len(&state, P0), 19);
}

// ---- Stern Scolding -----------------------------------------------------------------

#[test]
fn stern_scolding_targets_creature_spells_with_power_or_toughness_two_or_less() {
    // (card, colored mana, colorless mana, legal target). Power and toughness:
    // 1/1, 2/2, 3/2 (toughness qualifies), 0/4 (power qualifies), 3/3
    // (neither qualifies), and a noncreature spell. A creature spell needs an
    // empty stack, so each case is its own game.
    let cases: [(&str, ManaColor, u8, bool); 6] = [
        ("Llanowar Elves", ManaColor::G, 0, true),
        ("Hexing Squelcher", ManaColor::R, 1, true),
        ("Mukotai Ambusher", ManaColor::B, 3, true),
        ("Overgrown Battlement", ManaColor::G, 1, true),
        ("Anointed Peacekeeper", ManaColor::W, 2, false),
        ("Lightning Bolt", ManaColor::R, 0, false),
    ];
    for (name, color, colorless, legal) in cases {
        let mut state = plains();
        let targets: &[Target] = if name == "Lightning Bolt" {
            &[Target::Player(P0)]
        } else {
            &[]
        };
        let spell = opponent_casts(&mut state, name, &[color], colorless, targets);
        pass_to_p0(&mut state);
        let scolding = put(&mut state, P0, "Stern Scolding", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U], 0);
        if legal {
            assert_eq!(
                cast_targets(&state, scolding),
                vec![Target::Object(spell)],
                "{name}"
            );
        } else {
            assert!(
                !castable(&mut state, scolding),
                "{name} is not a legal target"
            );
        }
    }
}

#[test]
fn stern_scolding_counters_the_creature_spell() {
    let mut state = plains();
    let elves = opponent_casts(&mut state, "Llanowar Elves", &[ManaColor::G], 0, &[]);
    pass_to_p0(&mut state);
    let scolding = put(&mut state, P0, "Stern Scolding", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, scolding, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert!(named(&state, P1, "Llanowar Elves").is_empty());
    assert_eq!(zone(&state, scolding), Zone::Graveyard);
}

#[test]
fn stern_scolding_does_not_counter_a_creature_spell_that_cant_be_countered() {
    let mut state = plains();
    let squelcher = opponent_casts(&mut state, "Hexing Squelcher", &[ManaColor::R], 1, &[]);
    pass_to_p0(&mut state);
    let scolding = put(&mut state, P0, "Stern Scolding", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, scolding, &[Target::Object(squelcher)]);
    settled(&mut state);
    assert_eq!(zone(&state, squelcher), Zone::Battlefield);
}

// ---- Miscalculation -----------------------------------------------------------------

/// Answers a counter-unless-pays choice with `pay`.
fn settle_paying(state: &mut GameState, pay: bool) -> bool {
    let mut asked = false;
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectBoolean { .. } => {
            asked = true;
            Action::ChooseEffectBoolean(pay)
        }
        other => first_answer(other),
    });
    asked
}

#[test]
fn miscalculation_counters_unless_its_controller_pays_two() {
    // Not paid.
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let calc = put(&mut state, P0, "Miscalculation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, calc, &[Target::Object(bolt)]);
    settle_paying(&mut state, false);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "countered");
    assert_eq!(life(&state, P0), 20);

    // Paid with two mana.
    let mut state = plains();
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let calc = put(&mut state, P0, "Miscalculation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, calc, &[Target::Object(bolt)]);
    pool(&mut state, P1, &[], 2);
    assert!(
        settle_paying(&mut state, true),
        "P1 was offered the payment"
    );
    assert_eq!(life(&state, P0), 17, "Bolt resolved");
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.players[P1.index()].mana_pool, [0; 6], "two mana paid");
}

#[test]
fn miscalculation_does_not_counter_a_spell_that_cant_be_countered_and_cycles_for_two() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let decay = opponent_casts(
        &mut state,
        "Abrupt Decay",
        &[ManaColor::B, ManaColor::G],
        0,
        &[Target::Object(elves)],
    );
    pass_to_p0(&mut state);
    let calc = put(&mut state, P0, "Miscalculation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, calc, &[Target::Object(decay)]);
    settle_paying(&mut state, false);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "Decay resolved");
}

#[test]
fn miscalculation_cycling_costs_two_and_draws_a_card() {
    let mut state = plains();
    let calc = put(&mut state, P0, "Miscalculation", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, calc, 0), "cycling costs {{2}}");
    pool(&mut state, P0, &[], 1);
    activate(&mut state, calc, 0, &[]);
    next(&mut state);
    assert_eq!(zone(&state, calc), Zone::Graveyard, "discarded as a cost");
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 1, "drew a card");
    assert_eq!(library_len(&state, P0), 19);
    assert_eq!(state.players[P0.index()].mana_pool, [0; 6]);
}

// ---- Timetwister --------------------------------------------------------------------

#[test]
fn timetwister_shuffles_hands_and_graveyards_into_libraries_and_each_player_draws_seven() {
    let mut state = plains();
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    put(&mut state, P1, "Island", Zone::Hand);
    put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let twister = put(&mut state, P0, "Timetwister", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, twister, &[]);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 7);
    assert_eq!(hand_len(&state, P1), 7);
    assert_eq!(state.players[P0.index()].graveyard, vec![twister]);
    assert!(state.players[P1.index()].graveyard.is_empty());
    assert_eq!(library_len(&state, P0), 15);
    assert_eq!(library_len(&state, P1), 15);
}

#[test]
fn timetwister_with_empty_hands_and_graveyards_still_draws_seven_each() {
    for caster in [P0, P1] {
        let mut state = plains();
        state.active_player = caster;
        state.priority_player = caster;
        let twister = put(&mut state, caster, "Timetwister", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::U], 2);
        cast(&mut state, twister, &[]);
        settled(&mut state);
        assert_eq!(hand_len(&state, P0), 7, "{caster:?}");
        assert_eq!(hand_len(&state, P1), 7, "{caster:?}");
        assert_eq!(library_len(&state, P0), 13);
        assert_eq!(library_len(&state, P1), 13);
        assert_eq!(zone(&state, twister), Zone::Graveyard);
    }
}

// ---- Helpers for batch c1a-2 ---------------------------------------------------------

/// Casts `spell` announcing X as `x` and naming `targets` in order.
fn cast_with_x(state: &mut GameState, spell: ObjectId, x: u16, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    let mut targets = targets.iter();
    loop {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = targets.next().expect("a target prompt for no target");
                assert!(
                    legal_targets.contains(target),
                    "{target:?} not in {legal_targets:?}"
                );
                act(state, Action::ChooseTarget(*target));
            }
            Decision::ChooseEffectOption { option_count, .. } => {
                assert!(x < option_count, "X = {x} of {option_count} options");
                act(state, Action::ChooseEffectOption(x));
            }
            Decision::ChooseFullRangeCastXValueV59 { .. } => {
                act(state, Action::ChooseEffectOption(x));
            }
            _ => break,
        }
    }
    assert!(targets.next().is_none(), "a target was never asked for");
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

/// Settles, choosing `wanted` in every card selection and recording who was
/// asked, in order.
fn settle_picking(state: &mut GameState, wanted: &[ObjectId]) -> Vec<PlayerId> {
    let mut asked = Vec::new();
    settle_with(state, |_, decision| {
        if let Decision::ChooseEffectTargets {
            player,
            selected_count: 0,
            ..
        } = &decision
        {
            asked.push(*player);
        }
        selection(wanted, &decision).unwrap_or_else(|| first_answer(decision))
    });
    asked
}

/// A tapped Plains for `player`.
fn tapped_plains(state: &mut GameState, player: PlayerId) -> ObjectId {
    let land = put(state, player, "Plains", Zone::Battlefield);
    state.objects.get_mut(land).tapped = true;
    land
}

fn is_tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
}

// ---- Damn ----------------------------------------------------------------------------

#[test]
fn damn_destroys_target_creature_that_cant_be_regenerated() {
    let mut state = plains();
    let hollow = put(&mut state, P0, "Yavimaya Hollow", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    // A regeneration shield on the Elves.
    pool(&mut state, P0, &[ManaColor::G], 0);
    activate(&mut state, hollow, 0, &[Target::Object(elves)]);
    settled(&mut state);
    let damn = put(&mut state, P0, "Damn", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    cast_mode(&mut state, damn, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "no regeneration");
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(zone(&state, damn), Zone::Graveyard);

    // Any creature is a target, yours or not.
    let mut state = plains();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let damn = put(&mut state, P0, "Damn", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    cast_mode(&mut state, damn, 0, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Graveyard);
}

#[test]
fn damn_overload_destroys_each_creature_without_regeneration() {
    for caster in [P0, P1] {
        let other = caster.opponent();
        let mut state = plains();
        state.active_player = caster;
        state.priority_player = caster;
        let hollow = put(&mut state, caster, "Yavimaya Hollow", Zone::Battlefield);
        let shielded = put(&mut state, caster, "Llanowar Elves", Zone::Battlefield);
        let angler = put(&mut state, other, "Gurmag Angler", Zone::Battlefield);
        let soldier = token(&mut state, other, "Llanowar Elves");
        let ring = put(&mut state, other, "Sol Ring", Zone::Battlefield);
        pool(&mut state, caster, &[ManaColor::G], 0);
        activate(&mut state, hollow, 0, &[Target::Object(shielded)]);
        settled(&mut state);
        let damn = put(&mut state, caster, "Damn", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::W, ManaColor::W], 2);
        cast_mode(&mut state, damn, 1, &[]);
        assert!(
            state.stack.last().expect("the spell").targets.is_empty(),
            "the overloaded spell has no targets"
        );
        settled(&mut state);
        assert_eq!(zone(&state, shielded), Zone::Graveyard, "{caster:?}");
        assert_eq!(zone(&state, angler), Zone::Graveyard);
        assert_ne!(zone(&state, soldier), Zone::Battlefield);
        assert_eq!(zone(&state, ring), Zone::Battlefield);
        assert_eq!(zone(&state, hollow), Zone::Battlefield);
        assert_eq!(zone(&state, damn), Zone::Graveyard);
    }
}

#[test]
fn damn_offers_overload_only_when_its_cost_can_be_paid() {
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let damn = put(&mut state, P0, "Damn", Zone::Hand);
    // {B}{B} alone: the ordinary spell.
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    act(&mut state, Action::CastSpell(damn));
    match next(&mut state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(!legal_modes.contains(&1), "{legal_modes:?}")
        }
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(elves)))
        }
        other => panic!("unexpected {other:?}"),
    }

    // {2}{W}{W} too: both forms.
    let mut state = plains();
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let damn = put(&mut state, P0, "Damn", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::W, ManaColor::W],
        0,
    );
    act(&mut state, Action::CastSpell(damn));
    match next(&mut state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&0) && legal_modes.contains(&1));
        }
        other => panic!("expected the form choice, got {other:?}"),
    }

    // {W}{W}{1}: the overload needs four mana.
    let mut state = plains();
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let damn = put(&mut state, P0, "Damn", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 1);
    assert!(!castable(&mut state, damn), "neither form is payable");
}

#[test]
fn damn_does_nothing_when_its_target_left_before_it_resolved() {
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let damn = put(&mut state, P0, "Damn", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    cast_mode(&mut state, damn, 0, &[Target::Object(elves)]);
    opponent_responds_with_bolt(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "the Bolt killed it");
    assert_eq!(zone(&state, damn), Zone::Graveyard);
}

// ---- Winds of Abandon ----------------------------------------------------------------

#[test]
fn winds_of_abandon_exiles_a_creature_you_dont_control_and_its_controller_searches() {
    for (caster, victim_seat) in [(P0, P1), (P1, P0)] {
        let mut state = game(&["Forest", "Island", "Island"]);
        state.active_player = caster;
        state.priority_player = caster;
        let forest = state.players[victim_seat.index()].library[0];
        let casters_library = state.players[caster.index()].library.clone();
        let angler = put(&mut state, victim_seat, "Gurmag Angler", Zone::Battlefield);
        let mine = put(&mut state, caster, "Llanowar Elves", Zone::Battlefield);
        let winds = put(&mut state, caster, "Winds of Abandon", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::W], 1);
        assert_eq!(
            cast_targets(&state, winds),
            vec![Target::Object(angler)],
            "only a creature you don't control"
        );
        cast(&mut state, winds, &[Target::Object(angler)]);
        let asked = settle_path(&mut state, Some(forest));
        assert_eq!(
            asked,
            Some(victim_seat),
            "{caster:?}: its controller searches"
        );
        assert_eq!(zone(&state, angler), Zone::Exile);
        assert_eq!(zone(&state, forest), Zone::Battlefield);
        assert!(is_tapped(&state, forest), "it enters tapped");
        assert_eq!(state.objects.get(forest).controller, victim_seat);
        assert_eq!(zone(&state, mine), Zone::Battlefield);
        assert_eq!(state.players[caster.index()].library, casters_library);
        assert_eq!(zone(&state, winds), Zone::Graveyard);
    }
}

#[test]
fn winds_of_abandon_search_may_find_nothing_and_finds_only_basic_lands() {
    // Declining the search.
    let mut state = game(&["Plateau", "Plains", "Lightning Bolt"]);
    let plains_card = state.players[P1.index()].library[1];
    let plateau = state.players[P1.index()].library[0];
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let winds = put(&mut state, P0, "Winds of Abandon", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, winds, &[Target::Object(angler)]);
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            offered = legal_targets;
            Action::FinishEffectSelection
        }
        other => first_answer(other),
    });
    assert_eq!(offered, vec![Target::Object(plains_card)]);
    assert!(!offered.contains(&Target::Object(plateau)));
    assert_eq!(zone(&state, angler), Zone::Exile);
    assert_eq!(zone(&state, plains_card), Zone::Library);
    assert_eq!(library_len(&state, P1), 3);

    // No basic land card at all.
    let mut state = game(&["Lightning Bolt", "Lightning Bolt"]);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let winds = put(&mut state, P0, "Winds of Abandon", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, winds, &[Target::Object(angler)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, angler), Zone::Exile);
    assert_eq!(library_len(&state, P1), 2);
}

#[test]
fn winds_of_abandon_does_nothing_when_its_target_left_before_it_resolved() {
    let mut state = game(&["Forest", "Island", "Island"]);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let winds = put(&mut state, P0, "Winds of Abandon", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, winds, &[Target::Object(elves)]);
    opponent_responds_with_bolt(&mut state, Target::Object(elves));
    let asked = settle_path(&mut state, None);
    assert_eq!(asked, None, "no search when the spell fizzles");
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(library_len(&state, P1), 3);
}

#[test]
fn winds_of_abandon_overload_exiles_each_creature_you_dont_control() {
    for (caster, victim_seat) in [(P0, P1), (P1, P0)] {
        let mut state = game(&["Forest", "Island", "Plains", "Swamp"]);
        state.active_player = caster;
        state.priority_player = caster;
        let lands: Vec<ObjectId> = state.players[victim_seat.index()].library[..3].to_vec();
        let casters_library = state.players[caster.index()].library.clone();
        let angler = put(&mut state, victim_seat, "Gurmag Angler", Zone::Battlefield);
        let elves = put(&mut state, victim_seat, "Llanowar Elves", Zone::Battlefield);
        let soldier = token(&mut state, victim_seat, "Llanowar Elves");
        let ring = put(&mut state, victim_seat, "Sol Ring", Zone::Battlefield);
        let mine = put(&mut state, caster, "Kor Skyfisher", Zone::Battlefield);
        let winds = put(&mut state, caster, "Winds of Abandon", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::W, ManaColor::W], 4);
        cast_mode(&mut state, winds, 1, &[]);
        assert!(state.stack.last().expect("the spell").targets.is_empty());
        let asked = settle_picking(&mut state, &lands);
        assert_eq!(asked, vec![victim_seat], "one search for their controller");
        assert_eq!(zone(&state, angler), Zone::Exile);
        assert_eq!(zone(&state, elves), Zone::Exile);
        assert_ne!(zone(&state, soldier), Zone::Battlefield);
        assert_eq!(zone(&state, ring), Zone::Battlefield);
        assert_eq!(zone(&state, mine), Zone::Battlefield, "yours stays");
        for land in lands {
            assert_eq!(zone(&state, land), Zone::Battlefield);
            assert!(is_tapped(&state, land));
            assert_eq!(state.objects.get(land).controller, victim_seat);
        }
        assert_eq!(library_len(&state, victim_seat), 1);
        assert_eq!(state.players[caster.index()].library, casters_library);
    }
}

#[test]
fn winds_of_abandon_overload_searches_for_one_land_per_creature_at_most() {
    // Two creatures but three basic lands: at most two may be fetched.
    let mut state = game(&["Forest", "Island", "Plains"]);
    let lands: Vec<ObjectId> = state.players[P1.index()].library.clone();
    put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let winds = put(&mut state, P0, "Winds of Abandon", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 4);
    cast_mode(&mut state, winds, 1, &[]);
    let mut maximum = None;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            max_targets,
            selected_count,
            legal_targets,
            ..
        } => {
            maximum = Some(max_targets);
            if (selected_count as usize) < legal_targets.len().min(3) {
                Action::ChooseEffectTarget(legal_targets[0])
            } else {
                Action::FinishEffectSelection
            }
        }
        other => first_answer(other),
    });
    assert_eq!(maximum, Some(2));
    let fetched = lands
        .iter()
        .filter(|&&land| zone(&state, land) == Zone::Battlefield)
        .count();
    assert_eq!(fetched, 2);

    // The searcher may also take fewer, or none.
    let mut state = game(&["Forest", "Island", "Plains"]);
    let lands: Vec<ObjectId> = state.players[P1.index()].library.clone();
    put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let winds = put(&mut state, P0, "Winds of Abandon", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 4);
    cast_mode(&mut state, winds, 1, &[]);
    settle_picking(&mut state, &lands[..1]);
    let fetched = lands
        .iter()
        .filter(|&&land| zone(&state, land) == Zone::Battlefield)
        .count();
    assert_eq!(fetched, 1);
}

#[test]
fn winds_of_abandon_overload_with_no_creature_to_exile_does_nothing() {
    let mut state = game(&["Forest", "Island"]);
    let mine = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let winds = put(&mut state, P0, "Winds of Abandon", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 4);
    cast_mode(&mut state, winds, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(library_len(&state, P1), 2);
    assert_eq!(zone(&state, winds), Zone::Graveyard);
}

// ---- Unexpectedly Absent -------------------------------------------------------------

#[test]
fn unexpectedly_absent_puts_the_permanent_beneath_the_top_x_cards() {
    for caster in [P0, P1] {
        for x in [0_u16, 1, 3] {
            let mut state = game(&["Forest", "Island", "Plains", "Swamp", "Mountain"]);
            state.active_player = caster;
            state.priority_player = caster;
            let owner = caster.opponent();
            let top = state.players[owner.index()].library.clone();
            let angler = put(&mut state, owner, "Gurmag Angler", Zone::Battlefield);
            let spell = put(&mut state, caster, "Unexpectedly Absent", Zone::Hand);
            pool(&mut state, caster, &[ManaColor::W, ManaColor::W], x as u8);
            cast_with_x(&mut state, spell, x, &[Target::Object(angler)]);
            settled(&mut state);
            let library = &state.players[owner.index()].library;
            assert_eq!(library.len(), 6, "{caster:?} X = {x}");
            assert_eq!(library[usize::from(x)], angler, "X = {x}");
            assert_eq!(library[..usize::from(x)], top[..usize::from(x)]);
            assert_eq!(library[usize::from(x) + 1..], top[usize::from(x)..]);
            assert_eq!(zone(&state, spell), Zone::Graveyard);
        }
    }
}

#[test]
fn unexpectedly_absent_with_a_short_library_puts_the_permanent_on_the_bottom() {
    let mut state = game(&["Forest", "Island"]);
    let top = state.players[P1.index()].library.clone();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let spell = put(&mut state, P0, "Unexpectedly Absent", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 5);
    cast_with_x(&mut state, spell, 5, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(
        state.players[P1.index()].library,
        vec![top[0], top[1], angler]
    );

    // An empty library holds just the permanent.
    let mut state = game(&[]);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let spell = put(&mut state, P0, "Unexpectedly Absent", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast_with_x(&mut state, spell, 2, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(state.players[P1.index()].library, vec![angler]);
}

#[test]
fn unexpectedly_absent_targets_any_nonland_permanent_and_uses_its_owners_library() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let land = put(&mut state, P1, "Plains", Zone::Battlefield);
    let stolen = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    steal(&mut state, stolen);
    let spell = put(&mut state, P0, "Unexpectedly Absent", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 0);
    let legal = cast_targets(&state, spell);
    assert!(legal.contains(&Target::Object(mine)), "your own permanent");
    assert!(legal.contains(&Target::Object(theirs)));
    assert!(legal.contains(&Target::Object(stolen)));
    assert!(!legal.contains(&Target::Object(land)), "not a land");
    cast_with_x(&mut state, spell, 0, &[Target::Object(stolen)]);
    settled(&mut state);
    assert_eq!(state.players[P1.index()].library[0], stolen, "its owner's");
    assert!(!state.players[P0.index()].library.contains(&stolen));
}

#[test]
fn unexpectedly_absent_makes_a_token_cease_to_exist_and_fizzles_without_its_target() {
    let mut state = plains();
    let before = library_len(&state, P1);
    let token = token(&mut state, P1, "Llanowar Elves");
    let spell = put(&mut state, P0, "Unexpectedly Absent", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 0);
    cast_with_x(&mut state, spell, 0, &[Target::Object(token)]);
    settled(&mut state);
    assert!(!state.players[P1.index()].battlefield.contains(&token));
    assert!(!state.players[P1.index()].library.contains(&token));
    assert_eq!(library_len(&state, P1), before);

    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let spell = put(&mut state, P0, "Unexpectedly Absent", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 0);
    cast_with_x(&mut state, spell, 0, &[Target::Object(elves)]);
    opponent_responds_with_bolt(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(
        library_len(&state, P1),
        20,
        "nothing was put into a library"
    );
}

// ---- Mystic Confluence ---------------------------------------------------------------

/// P0 holds Mystic Confluence with its five mana; P1's Lightning Bolt at
/// P0 is on the stack and P1 has a Gurmag Angler and a Llanowar Elves.
fn mystic_board() -> (GameState, ObjectId, ObjectId, ObjectId, ObjectId) {
    let mut state = plains();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let mystic = put(&mut state, P0, "Mystic Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    (state, mystic, bolt, angler, elves)
}

/// Settles, answering each "pay {3}?" prompt (option 0 pays, option 1 does
/// not) from `answers` in order, and recording who was asked.
fn settle_taxing(state: &mut GameState, answers: &[u16]) -> Vec<PlayerId> {
    let mut asked = Vec::new();
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption {
            player,
            option_count: 2,
            ..
        } => {
            let answer = answers.get(asked.len()).copied().unwrap_or(1);
            asked.push(player);
            Action::ChooseEffectOption(answer)
        }
        other => first_answer(other),
    });
    asked
}

#[test]
fn mystic_confluence_offers_the_modes_its_targets_allow() {
    let modes = |state: &mut GameState, mystic: ObjectId| {
        act(state, Action::CastSpell(mystic));
        match next(state) {
            Decision::ChooseSpellMode {
                mode_count,
                legal_modes,
                ..
            } => {
                assert_eq!(mode_count, 10);
                legal_modes
            }
            other => panic!("expected the mode choice, got {other:?}"),
        }
    };
    // A spell and a creature: every multiset.
    let (mut state, mystic, ..) = mystic_board();
    assert_eq!(modes(&mut state, mystic), (0..10).collect::<Vec<u8>>());
    // No spell to counter: only the bounce and draw modes.
    let mut state = plains();
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let mystic = put(&mut state, P0, "Mystic Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    assert_eq!(modes(&mut state, mystic), vec![6, 7, 8, 9]);
    // No creature: only the counter and draw modes.
    let mut state = plains();
    opponent_bolts(&mut state, Target::Player(P0));
    let mystic = put(&mut state, P0, "Mystic Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    assert_eq!(modes(&mut state, mystic), vec![0, 2, 5, 9]);
}

#[test]
fn mystic_confluence_counters_unless_its_controller_pays_three_and_draws() {
    // ccd with the same Bolt twice and no mana: countered by the first
    // counter, the second finds nothing, and a card is drawn.
    let (mut state, mystic, bolt, ..) = mystic_board();
    cast_mode(
        &mut state,
        mystic,
        2,
        &[Target::Object(bolt), Target::Object(bolt)],
    );
    let asked = settle_taxing(&mut state, &[]);
    assert!(asked.is_empty(), "P1 cannot pay: no question");
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "countered");
    assert_eq!(life(&state, P0), 20);
    assert_eq!(hand_len(&state, P0), 1, "drew a card");
    assert_eq!(zone(&state, mystic), Zone::Graveyard);

    // P1 pays {3}: Bolt survives the first counter, and the second counter
    // asks again, which P1 cannot pay.
    let (mut state, mystic, bolt, ..) = mystic_board();
    pool(&mut state, P1, &[], 3);
    cast_mode(
        &mut state,
        mystic,
        2,
        &[Target::Object(bolt), Target::Object(bolt)],
    );
    let asked = settle_taxing(&mut state, &[0]);
    assert_eq!(asked, vec![P1], "asked once; the second tax is unpayable");
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20, "the second counter countered it");
    assert_eq!(state.players[P1.index()].mana_pool, [0; 6], "three paid");

    // P1 pays twice with six mana: Bolt resolves.
    let (mut state, mystic, bolt, ..) = mystic_board();
    pool(&mut state, P1, &[], 6);
    cast_mode(
        &mut state,
        mystic,
        2,
        &[Target::Object(bolt), Target::Object(bolt)],
    );
    let asked = settle_taxing(&mut state, &[0, 0]);
    assert_eq!(asked, vec![P1, P1]);
    assert_eq!(life(&state, P0), 17, "Bolt resolved");
    assert_eq!(hand_len(&state, P0), 1);

    // P1 may decline to pay.
    let (mut state, mystic, bolt, ..) = mystic_board();
    pool(&mut state, P1, &[], 3);
    cast_mode(&mut state, mystic, 5, &[Target::Object(bolt)]);
    let asked = settle_taxing(&mut state, &[1]);
    assert_eq!(asked, vec![P1]);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "countered");
    assert_eq!(life(&state, P0), 20);
    assert_eq!(hand_len(&state, P0), 2, "cdd draws two cards");
    assert_eq!(
        state.players[P1.index()].mana_pool[ManaColor::C.pool_index()],
        3
    );
}

#[test]
fn mystic_confluence_asks_its_own_controller_when_it_counters_its_own_spell() {
    // P0 counters a spell P0 controls (a Bolt of its own on the stack).
    let mut state = plains();
    let own = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, own, &[Target::Player(P1)]);
    let mystic = put(&mut state, P0, "Mystic Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 6);
    cast_mode(&mut state, mystic, 5, &[Target::Object(own)]);
    let asked = settle_taxing(&mut state, &[0]);
    assert_eq!(asked, vec![P0], "its controller is asked, here the caster");
    assert_eq!(life(&state, P1), 17, "paid, so the Bolt resolved");
}

#[test]
fn mystic_confluence_leaves_a_spell_that_cant_be_countered_alone() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let decay = opponent_casts(
        &mut state,
        "Abrupt Decay",
        &[ManaColor::B, ManaColor::G],
        0,
        &[Target::Object(elves)],
    );
    pass_to_p0(&mut state);
    let mystic = put(&mut state, P0, "Mystic Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast_mode(&mut state, mystic, 5, &[Target::Object(decay)]);
    let asked = settle_taxing(&mut state, &[]);
    assert!(asked.is_empty(), "nothing to pay for");
    assert_eq!(zone(&state, elves), Zone::Graveyard, "Decay resolved");
    assert_eq!(hand_len(&state, P0), 2, "the two draws still happen");
}

#[test]
fn mystic_confluence_returns_creatures_and_resolves_modes_in_printed_order() {
    // cbd: a counter, a bounce and a draw.
    let (mut state, mystic, bolt, angler, _) = mystic_board();
    cast_mode(
        &mut state,
        mystic,
        4,
        &[Target::Object(bolt), Target::Object(angler)],
    );
    settle_taxing(&mut state, &[]);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert!(hand_contains(&state, P1, angler), "returned to its owner");
    assert_eq!(hand_len(&state, P0), 1);

    // bbb: the same creature twice and another once.
    let (mut state, mystic, _, angler, elves) = mystic_board();
    cast_mode(
        &mut state,
        mystic,
        6,
        &[
            Target::Object(angler),
            Target::Object(elves),
            Target::Object(angler),
        ],
    );
    settled(&mut state);
    assert!(hand_contains(&state, P1, angler));
    assert!(hand_contains(&state, P1, elves));
    assert_eq!(hand_len(&state, P1), 2, "the Angler returned only once");
    assert_eq!(hand_len(&state, P0), 0);

    // ccb: two counters of different spells and a bounce; the first spell
    // is countered, the second asks its controller and the bounce happens.
    let (mut state, mystic, bolt, angler, _) = mystic_board();
    let second = opponent_bolts(&mut state, Target::Player(P0));
    cast_mode(
        &mut state,
        mystic,
        1,
        &[
            Target::Object(bolt),
            Target::Object(second),
            Target::Object(angler),
        ],
    );
    pool(&mut state, P1, &[], 3);
    let asked = settle_taxing(&mut state, &[1, 0]);
    assert_eq!(asked, vec![P1, P1]);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(life(&state, P0), 17, "the second Bolt's tax was paid");
    assert!(hand_contains(&state, P1, angler));
}

#[test]
fn mystic_confluence_draws_three_with_no_target() {
    let mut state = plains();
    let mystic = put(&mut state, P0, "Mystic Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast_mode(&mut state, mystic, 9, &[]);
    assert!(state.stack.last().expect("the spell").targets.is_empty());
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 3);
    assert_eq!(library_len(&state, P0), 17);
    assert_eq!(zone(&state, mystic), Zone::Graveyard);
}

#[test]
fn mystic_confluence_draws_offer_competing_draw_replacements() {
    // Its controller draws from an empty library with Jace, Wielder of
    // Mysteries while the opponent controls Hullbreacher: the drawing
    // player chooses which replacement applies.
    let mut state = game(&[]);
    put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    let jace = put(&mut state, P0, "Jace, Wielder of Mysteries", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(jace, Zone::Battlefield),
    );
    let mystic = put(&mut state, P0, "Mystic Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast_mode(&mut state, mystic, 9, &[]);
    let choice = settle(&mut state).expect("Jace competes with Hullbreacher");
    assert!(matches!(&choice, Decision::ChooseEffectTargets {
        player, source, legal_targets, can_finish: true, .. }
        if *player == P0 && *source == mystic
            && legal_targets.as_slice() == [Target::Object(jace)]));
    act(&mut state, Action::ChooseEffectTarget(Target::Object(jace)));
    assert!(matches!(
        next(&mut state),
        Decision::GameOver { winner: Some(P0) }
    ));
}

#[test]
fn mystic_confluence_does_nothing_when_every_target_is_illegal_and_skips_illegal_ones() {
    // bdd whose creature died: doesn't resolve, so no cards are drawn.
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let mystic = put(&mut state, P0, "Mystic Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast_mode(&mut state, mystic, 8, &[Target::Object(elves)]);
    opponent_responds_with_bolt(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(hand_len(&state, P0), 0, "it didn't resolve");
    assert_eq!(zone(&state, mystic), Zone::Graveyard);

    // bbd with one of two creatures gone: the other returns and a card is
    // drawn.
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let mystic = put(&mut state, P0, "Mystic Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast_mode(
        &mut state,
        mystic,
        7,
        &[Target::Object(elves), Target::Object(angler)],
    );
    opponent_responds_with_bolt(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert!(hand_contains(&state, P1, angler));
    assert_eq!(hand_len(&state, P0), 1);
}

// ---- Occult Epiphany -----------------------------------------------------------------

fn spirits(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            live.v4.is_token
                && engine::object_has_type(state, object, CardType::Creature)
                && CARD_DEFS[live.card_def as usize].subtypes == [Subtype::Spirit]
        })
        .collect()
}

#[test]
fn occult_epiphany_draws_x_discards_x_and_makes_a_spirit_per_card_type() {
    // (cards drawn and in hand, the cards discarded, Spirits made)
    let cases: [(&str, [&str; 2], usize); 4] = [
        ("land and creature", ["Plains", "Gurmag Angler"], 2),
        ("two lands", ["Plains", "Plains"], 1),
        ("artifact creature", ["Mukotai Ambusher", "Plains"], 3),
        ("instant and artifact", ["Lightning Bolt", "Sol Ring"], 2),
    ];
    for caster in [P0, P1] {
        for (what, wanted, spirit_count) in cases {
            let mut state = game(&[wanted[0], wanted[1], "Island", "Island"]);
            state.active_player = caster;
            state.priority_player = caster;
            // The two drawn cards and one more, so one stays in hand.
            let keep = put(&mut state, caster, "Brazen Borrower", Zone::Hand);
            let epiphany = put(&mut state, caster, "Occult Epiphany", Zone::Hand);
            pool(&mut state, caster, &[ManaColor::U], 2);
            let drawn: Vec<ObjectId> = state.players[caster.index()].library[..2].to_vec();
            cast_with_x(&mut state, epiphany, 2, &[]);
            settle_with(&mut state, choose_cards(drawn.clone()));
            for &card in &drawn {
                assert_eq!(zone(&state, card), Zone::Graveyard, "{what}");
            }
            assert_eq!(zone(&state, keep), Zone::Hand, "{what}");
            assert_eq!(spirits(&state, caster).len(), spirit_count, "{what}");
            assert_eq!(life(&state, caster), 20);
        }
    }
}

#[test]
fn occult_epiphany_spirits_are_one_one_white_fliers() {
    let mut state = game(&["Gurmag Angler", "Island", "Island"]);
    let epiphany = put(&mut state, P0, "Occult Epiphany", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast_with_x(&mut state, epiphany, 1, &[]);
    settled(&mut state);
    let made = spirits(&state, P0);
    assert_eq!(made.len(), 1, "the drawn creature was the only card");
    let spirit = made[0];
    assert_eq!(power_toughness(&state, spirit), (1, 1));
    assert!(keyword(&state, spirit, Keywords::FLYING));
    assert_eq!(
        engine::object_color_mask(&state, spirit),
        mtg_kernel::card_def::mana_color_mask(ManaColor::W)
    );
    assert_eq!(state.objects.get(spirit).controller, P0);
}

#[test]
fn occult_epiphany_chooses_what_to_discard_from_the_whole_hand() {
    let mut state = game(&["Island", "Plains", "Island"]);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Hand);
    let epiphany = put(&mut state, P0, "Occult Epiphany", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast_with_x(&mut state, epiphany, 1, &[]);
    // The one card drawn is not forced: discard the Angler instead.
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| {
        if let Decision::ChooseEffectTargets {
            legal_targets,
            min_targets,
            max_targets,
            selected_count: 0,
            ..
        } = &decision
        {
            assert_eq!((*min_targets, *max_targets), (1, 1));
            offered = legal_targets.clone();
        }
        selection(&[angler], &decision).unwrap_or_else(|| first_answer(decision))
    });
    assert_eq!(offered.len(), 3, "the Bolt, the Angler and the drawn card");
    assert_eq!(zone(&state, angler), Zone::Graveyard);
    assert_eq!(zone(&state, bolt), Zone::Hand);
    assert_eq!(spirits(&state, P0).len(), 1, "a creature card");
}

#[test]
fn occult_epiphany_with_x_zero_or_a_hand_of_exactly_x_needs_no_choice() {
    // X = 0: nothing happens.
    let mut state = plains();
    let epiphany = put(&mut state, P0, "Occult Epiphany", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast_with_x(&mut state, epiphany, 0, &[]);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(library_len(&state, P0), 20);
    assert!(spirits(&state, P0).is_empty());

    // Two cards drawn into an otherwise empty hand: X = 2 discards the whole
    // hand (a land and a creature) without a choice: two Spirits.
    let mut state = game(&["Plains", "Gurmag Angler"]);
    let epiphany = put(&mut state, P0, "Occult Epiphany", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast_with_x(&mut state, epiphany, 2, &[]);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(state.players[P0.index()].graveyard.len(), 3);
    assert_eq!(spirits(&state, P0).len(), 2);
}

// ---- Time Spiral ---------------------------------------------------------------------

#[test]
fn time_spiral_exiles_itself_shuffles_and_draws_seven_and_untaps_up_to_six_lands() {
    for caster in [P0, P1] {
        let other = caster.opponent();
        let mut state = plains();
        state.active_player = caster;
        state.priority_player = caster;
        put(&mut state, caster, "Lightning Bolt", Zone::Hand);
        put(&mut state, caster, "Gurmag Angler", Zone::Graveyard);
        put(&mut state, other, "Island", Zone::Hand);
        put(&mut state, other, "Lightning Bolt", Zone::Graveyard);
        let lands: Vec<ObjectId> = (0..7).map(|_| tapped_plains(&mut state, caster)).collect();
        let their_land = tapped_plains(&mut state, other);
        let spiral = put(&mut state, caster, "Time Spiral", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::U, ManaColor::U], 4);
        cast(&mut state, spiral, &[]);
        let asked = settle_picking(&mut state, &lands[..6]);
        assert_eq!(
            asked,
            vec![caster],
            "{caster:?}: the caster chooses the lands"
        );
        assert_eq!(
            zone(&state, spiral),
            Zone::Exile,
            "exiled, not in the graveyard"
        );
        assert_eq!(hand_len(&state, caster), 7);
        assert_eq!(hand_len(&state, other), 7);
        assert!(state.players[caster.index()].graveyard.is_empty());
        assert!(state.players[other.index()].graveyard.is_empty());
        assert_eq!(library_len(&state, caster), 20 + 1 + 1 - 7);
        assert_eq!(library_len(&state, other), 20 + 1 + 1 - 7);
        for &land in &lands[..6] {
            assert!(!is_tapped(&state, land), "untapped");
        }
        assert!(is_tapped(&state, lands[6]), "up to six only");
        assert!(is_tapped(&state, their_land), "not chosen");
    }
}

#[test]
fn time_spiral_untaps_fewer_lands_or_none_and_asks_for_nothing_without_tapped_lands() {
    // Choosing only two of three tapped lands.
    let mut state = plains();
    let lands: Vec<ObjectId> = (0..3).map(|_| tapped_plains(&mut state, P0)).collect();
    let spiral = put(&mut state, P0, "Time Spiral", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 4);
    cast(&mut state, spiral, &[]);
    settle_picking(&mut state, &lands[..2]);
    assert!(!is_tapped(&state, lands[0]) && !is_tapped(&state, lands[1]));
    assert!(is_tapped(&state, lands[2]));

    // No tapped land: no question, and the draws still happen.
    let mut state = plains();
    put(&mut state, P0, "Plains", Zone::Battlefield);
    let spiral = put(&mut state, P0, "Time Spiral", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 4);
    cast(&mut state, spiral, &[]);
    let asked = settle_picking(&mut state, &[]);
    assert!(asked.is_empty());
    assert_eq!(hand_len(&state, P0), 7);
    assert_eq!(hand_len(&state, P1), 7);
}

#[test]
fn time_spiral_draws_offer_competing_draw_replacements() {
    // The opponent draws from an empty library with Jace, Wielder of
    // Mysteries while the caster controls Hullbreacher: both replacements
    // apply to the same draw, so the drawing player chooses.
    for jace_wins in [true, false] {
        let mut state = game(&[]);
        for _ in 0..12 {
            put(&mut state, P0, "Plains", Zone::Library);
        }
        put(&mut state, P0, "Hullbreacher", Zone::Battlefield);
        let jace = put(&mut state, P1, "Jace, Wielder of Mysteries", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(jace, Zone::Battlefield),
        );
        let spiral = put(&mut state, P0, "Time Spiral", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 4);
        cast(&mut state, spiral, &[]);
        let choice = settle(&mut state).expect("Jace competes with Hullbreacher");
        assert!(matches!(&choice, Decision::ChooseEffectTargets {
            player, source, legal_targets, can_finish: true, .. }
            if *player == P1 && *source == spiral
                && legal_targets.as_slice() == [Target::Object(jace)]));
        assert_eq!(hand_len(&state, P0), 7, "the caster drew first");
        if jace_wins {
            act(&mut state, Action::ChooseEffectTarget(Target::Object(jace)));
            assert!(matches!(
                next(&mut state),
                Decision::GameOver { winner: Some(P1) }
            ));
        } else {
            // Declining Jace each time lets Hullbreacher replace all seven
            // draws with Treasures for the caster.
            settle_with(&mut state, |_, decision| match decision {
                Decision::ChooseEffectTargets { .. } => Action::FinishEffectSelection,
                other => first_answer(other),
            });
            assert_eq!(hand_len(&state, P1), 0);
            assert_eq!(named(&state, P0, "Treasure Token").len(), 7);
            assert_eq!(zone(&state, spiral), Zone::Exile);
        }
    }
}

#[test]
fn time_spiral_ends_the_game_when_the_first_draw_wins_it() {
    // Both players would win on their draw from an empty library; the
    // caster draws first, and the game is over before the other draws.
    let mut state = game(&[]);
    for player in [P0, P1] {
        let jace = put(&mut state, player, "Jace, Wielder of Mysteries", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(jace, Zone::Battlefield),
        );
    }
    let spiral = put(&mut state, P0, "Time Spiral", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 4);
    cast(&mut state, spiral, &[]);
    let mut over = None;
    for _ in 0..40 {
        match next(&mut state) {
            Decision::GameOver { winner } => {
                over = Some(winner);
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => act(&mut state, first_answer(other)),
        }
    }
    assert_eq!(over, Some(Some(P0)));
}

// ---- Public-session consumer games --------------------------------------------------

#[derive(Default)]
struct C1aEvents {
    casts: std::collections::BTreeMap<&'static str, usize>,
}

impl C1aEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let Some(card_id) = semantic["source"]["card_db_id"].as_u64() else {
            return;
        };
        let name = CARD_DEFS[card_id as usize].name;
        let Some(card) = C1A_CARDS.iter().find(|card| **card == name) else {
            return;
        };
        // Cycling Miscalculation is an activated ability of the card.
        if matches!(
            semantic["action_kind"].as_str(),
            Some("cast_spell" | "activate_ability")
        ) {
            *self.casts.entry(card).or_default() += 1;
        }
    }
}

#[test]
fn c1a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let control = deck(&[
        ("Plains", 10),
        ("Island", 10),
        ("Brazen Borrower", 4),
        ("Flickerwisp", 4),
        ("Anointed Peacekeeper", 4),
        ("Oust", 3),
        ("Path to Exile", 3),
        ("Reprieve", 3),
        ("Memory Lapse", 3),
        ("Remand", 3),
        ("Stern Scolding", 3),
        ("Miscalculation", 3),
        ("Timetwister", 3),
    ]);
    // Batch c1a-2's cards: four of each over a three-color mana base.
    let control2 = deck(&[
        ("Plains", 6),
        ("Island", 7),
        ("Swamp", 6),
        ("Brazen Borrower", 3),
        ("Flickerwisp", 3),
        ("Damn", 4),
        ("Winds of Abandon", 4),
        ("Unexpectedly Absent", 4),
        ("Mystic Confluence", 4),
        ("Occult Epiphany", 4),
        ("Time Spiral", 4),
    ]);
    let aggro = deck(&[("Mountain", 14), ("Jackal Pup", 14), ("Lightning Bolt", 12)]);

    let mut totals = C1aEvents::default();
    for (pair, first_seed) in [
        ([control.clone(), aggro.clone()], 0x4331_4100_u64),
        ([aggro.clone(), control.clone()], 0x4331_4110),
        ([control2.clone(), aggro.clone()], 0x4331_4120),
        ([aggro, control2.clone()], 0x4331_4130),
        ([control2.clone(), control.clone()], 0x4331_4140),
        ([control, control2], 0x4331_4150),
    ] {
        for seed in first_seed..first_seed + 2 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let mut events = C1aEvents::default();
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action)
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
                for (card, count) in events.casts {
                    *totals.casts.entry(card).or_default() += count;
                }
            }
        }
    }
    for card in C1A_CARDS {
        assert!(
            totals.casts.get(card).copied().unwrap_or(0) > 0,
            "{card} was never cast: {:?}",
            totals.casts
        );
    }
}
