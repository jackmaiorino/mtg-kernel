//! Vintage Cube wave C5d: native rules and public-session tests for the
//! each-player decision spells and hidden-zone artifacts.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C5D_CARDS: [&str; 6] = [
    "Balance",
    "Council's Judgment",
    "Memory Jar",
    "Palantir of Orthanc",
    "Proft's Eidetic Memory",
    "Fractured Identity",
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
        0x4355_4245_0000 + 0x3564,
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
fn c5d_cards_are_full() {
    for name in C5D_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c5d_cards_have_their_printed_characteristics() {
    for (name, card_type, generic, pips, mana_value, legendary) in [
        ("Balance", CardType::Sorcery, 1u8, 1usize, 2u16, false),
        ("Council's Judgment", CardType::Sorcery, 1, 2, 3, false),
        ("Fractured Identity", CardType::Sorcery, 3, 2, 5, false),
        ("Memory Jar", CardType::Artifact, 5, 0, 5, false),
        ("Palantir of Orthanc", CardType::Artifact, 3, 0, 3, true),
        (
            "Proft's Eidetic Memory",
            CardType::Enchantment,
            1,
            1,
            2,
            true,
        ),
    ] {
        let card = def(name);
        assert_eq!(card.types, &[card_type], "{name}");
        assert!(card.subtypes.is_empty(), "{name}");
        assert_eq!(
            (card.cost.generic, card.cost.pips.len()),
            (generic, pips),
            "{name}"
        );
        assert_eq!(card.mana_value, mana_value, "{name}");
        assert_eq!(
            card.supertypes.contains(&Supertype::Legendary),
            legendary,
            "{name}"
        );
        assert!(!card.is_land && card.power.is_none(), "{name}");
        assert!(!card.is_token, "{name}");
    }
    assert_eq!(def("Balance").colors, &[ManaColor::W]);
    assert_eq!(def("Council's Judgment").colors, &[ManaColor::W]);
    assert_eq!(def("Proft's Eidetic Memory").colors, &[ManaColor::U]);
    let fractured = def("Fractured Identity").colors;
    assert_eq!(fractured.len(), 2);
    assert!(fractured.contains(&ManaColor::W) && fractured.contains(&ManaColor::U));
    assert!(def("Memory Jar").colors.is_empty());
    assert!(def("Palantir of Orthanc").colors.is_empty());

    // Their abilities: Memory Jar's one activated ability, the two triggers
    // of Proft's Eidetic Memory (its entry draw and its combat counters),
    // Palantir's end step trigger, and no printed ability elsewhere.
    assert_eq!(def("Memory Jar").activated_abilities.len(), 1);
    assert_eq!(def("Palantir of Orthanc").activated_abilities.len(), 0);
    assert_eq!(def("Proft's Eidetic Memory").activated_abilities.len(), 0);
    let trigger_count = |name: &str| mtg_kernel::trigger::triggers_for(id(name)).len();
    assert_eq!(trigger_count("Proft's Eidetic Memory"), 2);
    assert_eq!(trigger_count("Palantir of Orthanc"), 1);
    assert_eq!(trigger_count("Memory Jar"), 0);
    for name in ["Balance", "Council's Judgment", "Fractured Identity"] {
        assert_eq!(trigger_count(name), 0, "{name}");
    }
}

// ---- Helpers of these tests ----------------------------------------------------

/// Removes `object` from its zone to exile, as an effect that responds would.
fn exile(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
}

/// P0 draws a card, as an effect would.
fn draw_card(state: &mut GameState, player: PlayerId) {
    event::propose_and_commit(state, ProposedEvent::draw(player));
}

fn counters(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.plus1_plus1
}

fn hand_size(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

fn in_graveyard(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].graveyard.contains(&object)
}

/// Advances until the spell or ability just put on the stack is there with
/// its costs paid (the engine finishes a pending cast or activation as it
/// next advances).
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
    panic!("the cast or activation did not finish");
}

/// P0's end step, after everything the beginning of it set off has resolved.
fn p0_end_step_settled(state: &GameState) -> bool {
    state.active_player == P0
        && state.step == Step::End
        && state.stack.is_empty()
        && state.engine.pending_triggers.is_empty()
}

/// P1's end step, after everything the beginning of it set off has resolved.
fn p1_end_step_settled(state: &GameState) -> bool {
    state.active_player == P1
        && state.step == Step::End
        && state.stack.is_empty()
        && state.engine.pending_triggers.is_empty()
}

// ---- Proft's Eidetic Memory ----------------------------------------------------

#[test]
fn profts_eidetic_memory_draws_a_card_when_it_enters() {
    let mut state = plains();
    let proft = put(&mut state, P0, "Proft's Eidetic Memory", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, proft, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, proft), Zone::Battlefield);
    assert_eq!(hand_size(&state, P0), 1, "the entry draw");
    assert_eq!(state.players[0].draws_this_turn, 1);
    assert_eq!(hand_size(&state, P1), 0);
}

#[test]
fn profts_eidetic_memory_removes_the_maximum_hand_size() {
    let mut state = plains();
    put(&mut state, P0, "Proft's Eidetic Memory", Zone::Battlefield);
    for _ in 0..9 {
        put(&mut state, P0, "Plains", Zone::Hand);
    }
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(hand_size(&state, P0), 9, "no discard at cleanup");

    // Without it, the cleanup step discards down to seven.
    let mut state = plains();
    for _ in 0..9 {
        put(&mut state, P0, "Plains", Zone::Hand);
    }
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(hand_size(&state, P0), 7);
}

#[test]
fn profts_eidetic_memory_puts_counters_for_the_cards_drawn_beyond_the_first() {
    let mut state = plains();
    put(&mut state, P0, "Proft's Eidetic Memory", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let foe = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    for _ in 0..3 {
        draw_card(&mut state, P0);
    }
    assert_eq!(state.players[0].draws_this_turn, 3);
    run_until(
        &mut state,
        |state| state.step == Step::DeclareAttackers,
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(elves)));
                assert!(legal_targets.contains(&Target::Object(pup)));
                assert!(
                    !legal_targets.contains(&Target::Object(foe)),
                    "a creature you control"
                );
                Some(Action::ChooseTarget(Target::Object(elves)))
            }
            _ => None,
        },
    );
    assert_eq!(counters(&state, elves), 2, "three cards drawn minus one");
    assert_eq!(power_toughness(&state, elves), (3, 3));
    assert_eq!(counters(&state, pup), 0);
    assert_eq!(counters(&state, foe), 0);
}

#[test]
fn profts_eidetic_memory_does_nothing_with_one_card_drawn_or_none() {
    for draws in 0..2 {
        let mut state = plains();
        put(&mut state, P0, "Proft's Eidetic Memory", Zone::Battlefield);
        let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
        for _ in 0..draws {
            draw_card(&mut state, P0);
        }
        run_until(
            &mut state,
            |state| state.step == Step::DeclareAttackers,
            |_, decision| match decision {
                Decision::ChooseTargets { .. } => {
                    panic!("the trigger needs more than one card drawn: {draws}")
                }
                _ => None,
            },
        );
        assert_eq!(counters(&state, elves), 0, "{draws} cards drawn");
    }
}

#[test]
fn profts_eidetic_memory_triggers_only_at_its_controllers_combat() {
    let mut state = plains();
    put(&mut state, P0, "Proft's Eidetic Memory", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    for _ in 0..3 {
        draw_card(&mut state, P0);
    }
    // The opponent's turn: P0 has drawn three cards, but it is not P0's combat.
    state.active_player = P1;
    state.priority_player = P1;
    state.step = Step::Main1;
    run_until(
        &mut state,
        p1_end_step_settled,
        |_, decision| match decision {
            Decision::ChooseTargets { .. } => panic!("it is the opponent's combat"),
            _ => None,
        },
    );
    assert_eq!(counters(&state, elves), 0);
}

#[test]
fn profts_eidetic_memory_counts_the_cards_drawn_as_the_trigger_resolves() {
    let mut state = plains();
    put(&mut state, P0, "Proft's Eidetic Memory", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    for _ in 0..2 {
        draw_card(&mut state, P0);
    }
    let chosen = std::cell::Cell::new(false);
    run_until(
        &mut state,
        |state| chosen.get() && !state.stack.is_empty(),
        |_, decision| match decision {
            Decision::ChooseTargets { .. } => {
                chosen.set(true);
                Some(Action::ChooseTarget(Target::Object(elves)))
            }
            _ => None,
        },
    );
    // One more card in response: three drawn this turn, so two counters.
    draw_card(&mut state, P0);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    assert_eq!(counters(&state, elves), 2);
}

#[test]
fn profts_eidetic_memory_does_nothing_when_its_target_is_gone() {
    let mut state = plains();
    let proft = put(&mut state, P0, "Proft's Eidetic Memory", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    for _ in 0..3 {
        draw_card(&mut state, P0);
    }
    let chosen = std::cell::Cell::new(false);
    run_until(
        &mut state,
        |state| chosen.get() && !state.stack.is_empty(),
        |_, decision| match decision {
            Decision::ChooseTargets { .. } => {
                chosen.set(true);
                Some(Action::ChooseTarget(Target::Object(elves)))
            }
            _ => None,
        },
    );
    exile(&mut state, elves);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    assert_eq!(zone(&state, elves), Zone::Exile);
    assert_eq!(counters(&state, pup), 0, "nothing else was chosen");
    assert_eq!(zone(&state, proft), Zone::Battlefield);
}

// ---- Council's Judgment --------------------------------------------------------

/// P0 holds Council's Judgment with its mana in the pool. P0 controls a
/// creature and a land; P1 controls a creature, an artifact and a land.
fn judgment_state() -> (
    GameState,
    ObjectId,
    ObjectId,
    ObjectId,
    ObjectId,
    ObjectId,
    ObjectId,
) {
    let mut state = plains();
    let spell = put(&mut state, P0, "Council's Judgment", Zone::Hand);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let my_land = put(&mut state, P0, "Plains", Zone::Battlefield);
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let clamp = put(&mut state, P1, "Skullclamp", Zone::Battlefield);
    let foe_land = put(&mut state, P1, "Plains", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 1);
    (state, spell, mine, my_land, pup, clamp, foe_land)
}

/// Casts Council's Judgment and answers each vote from `votes` (P0's, then
/// P1's), returning who voted and how many permanents each could choose from.
fn vote(
    state: &mut GameState,
    spell: ObjectId,
    forbidden: &[ObjectId],
    votes: [ObjectId; 2],
) -> Vec<(PlayerId, usize)> {
    cast(state, spell, &[]);
    let mut asked = Vec::new();
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } => {
            for object in forbidden {
                assert!(
                    !legal_targets.contains(&Target::Object(*object)),
                    "{object:?} may not be voted for"
                );
            }
            asked.push((player, legal_targets.len()));
            let vote = votes[player.index()];
            assert!(legal_targets.contains(&Target::Object(vote)));
            Action::ChooseEffectTarget(Target::Object(vote))
        }
        other => panic!("unexpected choice: {other:?}"),
    });
    asked
}

#[test]
fn councils_judgment_exiles_every_permanent_tied_for_the_most_votes() {
    let (mut state, spell, mine, my_land, pup, clamp, foe_land) = judgment_state();
    let asked = vote(&mut state, spell, &[mine, my_land, foe_land], [pup, clamp]);
    // The caster votes first, then the opponent, each among the two nonland
    // permanents the caster does not control.
    assert_eq!(asked, vec![(P0, 2), (P1, 2)]);
    assert_eq!(zone(&state, pup), Zone::Exile, "one vote");
    assert_eq!(zone(&state, clamp), Zone::Exile, "one vote, tied");
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(zone(&state, my_land), Zone::Battlefield);
    assert_eq!(zone(&state, foe_land), Zone::Battlefield);
    assert_eq!(zone(&state, spell), Zone::Graveyard);
}

#[test]
fn councils_judgment_exiles_only_the_permanent_with_more_votes() {
    let (mut state, spell, mine, my_land, pup, clamp, foe_land) = judgment_state();
    let asked = vote(
        &mut state,
        spell,
        &[mine, my_land, foe_land],
        [clamp, clamp],
    );
    assert_eq!(asked, vec![(P0, 2), (P1, 2)]);
    assert_eq!(zone(&state, clamp), Zone::Exile, "two votes");
    assert_eq!(zone(&state, pup), Zone::Battlefield, "no votes");
    assert_eq!(zone(&state, mine), Zone::Battlefield);
}

#[test]
fn councils_judgment_does_not_target() {
    // A hexproof creature cannot be targeted by an opponent's spell, but it
    // can be voted for and exiled.
    let (mut state, spell, mine, my_land, _pup, _clamp, foe_land) = judgment_state();
    let ward = put(&mut state, P1, "Gladecover Scout", Zone::Battlefield);
    assert!(keyword(&state, ward, Keywords::HEXPROOF));
    let asked = vote(&mut state, spell, &[mine, my_land, foe_land], [ward, ward]);
    assert_eq!(asked, vec![(P0, 3), (P1, 3)]);
    assert_eq!(zone(&state, ward), Zone::Exile);
}

#[test]
fn councils_judgment_with_no_nonland_permanent_to_vote_for_does_nothing() {
    let mut state = plains();
    let spell = put(&mut state, P0, "Council's Judgment", Zone::Hand);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let foe_land = put(&mut state, P1, "Plains", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 1);
    cast(&mut state, spell, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, spell), Zone::Graveyard);
    assert_eq!(
        zone(&state, mine),
        Zone::Battlefield,
        "yours are not offered"
    );
    assert_eq!(
        zone(&state, foe_land),
        Zone::Battlefield,
        "lands are not offered"
    );
}

// ---- Balance -------------------------------------------------------------------

/// A prompt of Balance: who chooses, among how many cards, keeping how many.
type KeepPrompt = (PlayerId, usize, u16);

/// Settles, answering each keep prompt with the first of the offered cards
/// that is in `keep`, and returns the prompts in order.
fn settle_keeping(state: &mut GameState, keep: &[ObjectId]) -> Vec<KeepPrompt> {
    let mut prompts = Vec::new();
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            selected_count,
            min_targets,
            legal_targets,
            ..
        } => {
            if selected_count == 0 {
                prompts.push((player, legal_targets.len(), min_targets));
            }
            let choice = legal_targets
                .into_iter()
                .find(|target| matches!(target, Target::Object(object) if keep.contains(object)))
                .expect("a card to keep is offered");
            Action::ChooseEffectTarget(choice)
        }
        other => panic!("unexpected choice: {other:?}"),
    });
    prompts
}

#[test]
fn balance_equalizes_lands_then_cards_in_hand_then_creatures() {
    let mut state = plains();
    let balance = put(&mut state, P0, "Balance", Zone::Hand);
    let lands: Vec<ObjectId> = (0..3)
        .map(|_| put(&mut state, P0, "Plains", Zone::Battlefield))
        .collect();
    let foe_land = put(&mut state, P1, "Plains", Zone::Battlefield);
    let cards: Vec<ObjectId> = (0..3)
        .map(|_| put(&mut state, P0, "Lightning Bolt", Zone::Hand))
        .collect();
    let foe_card = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let pups: Vec<ObjectId> = (0..2)
        .map(|_| put(&mut state, P0, "Jackal Pup", Zone::Battlefield))
        .collect();
    let foes: Vec<ObjectId> = (0..3)
        .map(|_| put(&mut state, P1, "Jackal Pup", Zone::Battlefield))
        .collect();
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, balance, &[]);
    let keep = [lands[2], cards[1], foes[1], foes[2]];
    let prompts = settle_keeping(&mut state, &keep);
    // P0 keeps one of three lands (the opponent has one), then one of three
    // cards (the opponent holds one), then the opponent keeps two of three
    // creatures (P0 has two).
    assert_eq!(prompts, vec![(P0, 3, 1), (P0, 3, 1), (P1, 3, 2)]);
    for (index, land) in lands.iter().enumerate() {
        let expected = if index == 2 {
            Zone::Battlefield
        } else {
            Zone::Graveyard
        };
        assert_eq!(zone(&state, *land), expected, "land {index}");
    }
    assert_eq!(zone(&state, foe_land), Zone::Battlefield);
    for (index, card) in cards.iter().enumerate() {
        let expected = if index == 1 {
            Zone::Hand
        } else {
            Zone::Graveyard
        };
        assert_eq!(zone(&state, *card), expected, "card {index}");
    }
    assert_eq!(zone(&state, foe_card), Zone::Hand);
    assert_eq!(hand_size(&state, P0), 1);
    for pup in &pups {
        assert_eq!(zone(&state, *pup), Zone::Battlefield);
    }
    assert_eq!(zone(&state, foes[0]), Zone::Graveyard);
    assert_eq!(zone(&state, foes[1]), Zone::Battlefield);
    assert_eq!(zone(&state, foes[2]), Zone::Battlefield);
    assert_eq!(zone(&state, balance), Zone::Graveyard);
}

#[test]
fn balance_makes_the_player_with_more_choose_and_empties_out_to_the_fewest() {
    let mut state = plains();
    let balance = put(&mut state, P0, "Balance", Zone::Hand);
    let my_land = put(&mut state, P0, "Plains", Zone::Battlefield);
    let foe_lands: Vec<ObjectId> = (0..2)
        .map(|_| put(&mut state, P1, "Plains", Zone::Battlefield))
        .collect();
    let foe_cards: Vec<ObjectId> = (0..2)
        .map(|_| put(&mut state, P1, "Lightning Bolt", Zone::Hand))
        .collect();
    let foe_pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let foe_token = token(&mut state, P1, "Jackal Pup");
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, balance, &[]);
    let prompts = settle_keeping(&mut state, &[foe_lands[1]]);
    // Only the opponent has more lands than the fewest, so only the
    // opponent chooses; with no cards in hand and no creatures on P0's side
    // the opponent discards and sacrifices everything with no choice.
    assert_eq!(prompts, vec![(P1, 2, 1)]);
    assert_eq!(zone(&state, my_land), Zone::Battlefield);
    assert_eq!(zone(&state, foe_lands[0]), Zone::Graveyard);
    assert_eq!(zone(&state, foe_lands[1]), Zone::Battlefield);
    for card in &foe_cards {
        assert_eq!(zone(&state, *card), Zone::Graveyard);
    }
    assert_eq!(zone(&state, foe_pup), Zone::Graveyard);
    assert_ne!(
        zone(&state, foe_token),
        Zone::Battlefield,
        "a token counts as a creature"
    );
}

#[test]
fn balance_does_nothing_when_the_counts_already_match() {
    let mut state = plains();
    let balance = put(&mut state, P0, "Balance", Zone::Hand);
    let lands = [
        put(&mut state, P0, "Plains", Zone::Battlefield),
        put(&mut state, P1, "Plains", Zone::Battlefield),
    ];
    let cards = [
        put(&mut state, P0, "Lightning Bolt", Zone::Hand),
        put(&mut state, P1, "Lightning Bolt", Zone::Hand),
    ];
    let pups = [
        put(&mut state, P0, "Jackal Pup", Zone::Battlefield),
        put(&mut state, P1, "Jackal Pup", Zone::Battlefield),
    ];
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, balance, &[]);
    settled(&mut state);
    for land in lands {
        assert_eq!(zone(&state, land), Zone::Battlefield);
    }
    for card in cards {
        assert_eq!(zone(&state, card), Zone::Hand);
    }
    for pup in pups {
        assert_eq!(zone(&state, pup), Zone::Battlefield);
    }
}

#[test]
fn balance_sacrifices_all_lands_of_the_richer_player_and_still_trims_the_hand() {
    // The opponent has no lands, so P0 sacrifices all of theirs with no
    // choice; the opponent's hand is the smaller, so P0 chooses one card to
    // keep.
    let mut state = plains();
    let balance = put(&mut state, P0, "Balance", Zone::Hand);
    let lands: Vec<ObjectId> = (0..2)
        .map(|_| put(&mut state, P0, "Plains", Zone::Battlefield))
        .collect();
    let cards: Vec<ObjectId> = (0..2)
        .map(|_| put(&mut state, P0, "Lightning Bolt", Zone::Hand))
        .collect();
    let foe_card = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, balance, &[]);
    let prompts = settle_keeping(&mut state, &[cards[0]]);
    assert_eq!(prompts, vec![(P0, 2, 1)], "only the hand is a choice");
    for land in lands {
        assert_eq!(zone(&state, land), Zone::Graveyard, "no land to keep");
    }
    assert_eq!(zone(&state, cards[0]), Zone::Hand);
    assert_eq!(zone(&state, cards[1]), Zone::Graveyard);
    assert_eq!(zone(&state, foe_card), Zone::Hand);
}

// ---- Palantir of Orthanc -------------------------------------------------------

/// Plays on to P0's end step and through everything it sets off, with P1
/// answering Palantir's question with `option` (0 lets P0 draw, 1 declines).
fn palantir_end_step(state: &mut GameState, option: u16) {
    run_until(state, p0_end_step_settled, |_, decision| match decision {
        Decision::ChooseEffectOption {
            player,
            option_count,
            ..
        } => {
            assert_eq!(*player, P1, "the opponent decides");
            assert_eq!(*option_count, 2);
            Some(Action::ChooseEffectOption(option))
        }
        _ => None,
    });
}

#[test]
fn palantir_of_orthanc_lets_its_controller_draw_when_the_opponent_agrees() {
    let mut state = game(&["Skullclamp"; 20]);
    let palantir = put(&mut state, P0, "Palantir of Orthanc", Zone::Battlefield);
    let opponent_life = life(&state, P1);
    let library = state.players[0].library.len();
    palantir_end_step(&mut state, 0);
    assert_eq!(hand_size(&state, P0), 1, "the drawn card");
    assert_eq!(state.players[0].library.len(), library - 1);
    assert!(state.players[0].graveyard.is_empty(), "nothing was milled");
    assert_eq!(life(&state, P1), opponent_life);
    assert_eq!(zone(&state, palantir), Zone::Battlefield);
}

#[test]
fn palantir_of_orthanc_mills_and_drains_when_the_opponent_declines() {
    // One influence counter after the first end step: it mills one card, an
    // Umezawa's Jitte, and the opponent loses life equal to its mana value
    // (two), not the number of cards.
    let mut state = game(&["Umezawa's Jitte"; 20]);
    put(&mut state, P0, "Palantir of Orthanc", Zone::Battlefield);
    let opponent_life = life(&state, P1);
    palantir_end_step(&mut state, 1);
    assert_eq!(state.players[0].graveyard.len(), 1);
    assert_eq!(hand_size(&state, P0), 0, "no draw");
    assert_eq!(life(&state, P1), opponent_life - 2);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn palantir_of_orthanc_counts_its_influence_counters_across_turns() {
    // Every card is a Skullclamp (mana value one). The first end step adds a
    // counter and the opponent lets P0 draw; the second adds another, so a
    // decline mills two cards and costs two life.
    let mut state = game(&["Skullclamp"; 20]);
    put(&mut state, P0, "Palantir of Orthanc", Zone::Battlefield);
    palantir_end_step(&mut state, 0);
    assert!(state.players[0].graveyard.is_empty());
    pass_until(&mut state, |state| state.active_player == P1);
    let opponent_life = life(&state, P1);
    run_until(
        &mut state,
        p0_end_step_settled,
        |_, decision| match decision {
            Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
            _ => None,
        },
    );
    assert_eq!(
        state.players[0].graveyard.len(),
        2,
        "two influence counters"
    );
    assert_eq!(life(&state, P1), opponent_life - 2);
}

#[test]
fn palantir_of_orthanc_triggers_only_at_its_controllers_end_step() {
    let mut state = game(&["Skullclamp"; 20]);
    put(&mut state, P1, "Palantir of Orthanc", Zone::Battlefield);
    // P0's end step: P1's Palantir does nothing.
    run_until(
        &mut state,
        p0_end_step_settled,
        |_, decision| match decision {
            Decision::ChooseEffectOption { .. } => panic!("not P1's end step"),
            _ => None,
        },
    );
    assert!(state.players[1].graveyard.is_empty());
    assert_eq!(hand_size(&state, P0), 0);
}

// ---- Fractured Identity --------------------------------------------------------

/// Casts Fractured Identity at `target` with mana from the pool and settles
/// it with first answers.
fn fracture(state: &mut GameState, spell: ObjectId, target: ObjectId) {
    pool(state, P0, &[ManaColor::W, ManaColor::U], 3);
    cast(state, spell, &[Target::Object(target)]);
    settle_first(state);
}

#[test]
fn fractured_identity_exiles_an_opposing_permanent_and_gives_the_caster_a_token_copy() {
    let mut state = plains();
    let spell = put(&mut state, P0, "Fractured Identity", Zone::Hand);
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    fracture(&mut state, spell, pup);
    assert_eq!(zone(&state, pup), Zone::Exile);
    assert!(named(&state, P1, "Jackal Pup").is_empty());
    let copies = named(&state, P0, "Jackal Pup");
    assert_eq!(copies.len(), 1, "the caster creates the copy");
    let copy = copies[0];
    assert!(state.objects.get(copy).v4.is_token);
    assert_ne!(copy, pup);
    assert_eq!(power_toughness(&state, copy), (2, 1));
    assert_eq!(zone(&state, spell), Zone::Graveyard);
}

#[test]
fn fractured_identity_on_your_own_permanent_gives_the_opponent_the_copy() {
    let mut state = plains();
    let spell = put(&mut state, P0, "Fractured Identity", Zone::Hand);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    fracture(&mut state, spell, pup);
    assert_eq!(zone(&state, pup), Zone::Exile);
    assert!(named(&state, P0, "Jackal Pup").is_empty());
    let copies = named(&state, P1, "Jackal Pup");
    assert_eq!(copies.len(), 1, "each player other than its controller");
    assert!(state.objects.get(copies[0]).v4.is_token);
}

#[test]
fn fractured_identity_copies_a_nonland_noncreature_permanent_and_a_token() {
    let mut state = plains();
    let spell = put(&mut state, P0, "Fractured Identity", Zone::Hand);
    let clamp = put(&mut state, P1, "Skullclamp", Zone::Battlefield);
    fracture(&mut state, spell, clamp);
    assert_eq!(zone(&state, clamp), Zone::Exile);
    let copies = named(&state, P0, "Skullclamp");
    assert_eq!(copies.len(), 1);
    assert!(state.objects.get(copies[0]).v4.is_token);

    // A token copy of a token: the exiled token ceases to exist.
    let mut state = plains();
    let spell = put(&mut state, P0, "Fractured Identity", Zone::Hand);
    let foe_token = token(&mut state, P1, "Jackal Pup");
    fracture(&mut state, spell, foe_token);
    assert_ne!(zone(&state, foe_token), Zone::Battlefield);
    let copies = named(&state, P0, "Jackal Pup");
    assert_eq!(copies.len(), 1);
    assert!(state.objects.get(copies[0]).v4.is_token);
}

#[test]
fn fractured_identity_does_nothing_when_its_target_is_gone() {
    let mut state = plains();
    let spell = put(&mut state, P0, "Fractured Identity", Zone::Hand);
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 3);
    cast(&mut state, spell, &[Target::Object(pup)]);
    on_the_stack(&mut state);
    exile(&mut state, pup);
    settle_first(&mut state);
    assert!(named(&state, P0, "Jackal Pup").is_empty(), "no copy");
    assert!(named(&state, P1, "Jackal Pup").is_empty());
    assert_eq!(zone(&state, spell), Zone::Graveyard);
}

#[test]
fn fractured_identity_offers_only_nonland_permanents() {
    let mut state = plains();
    let spell = put(&mut state, P0, "Fractured Identity", Zone::Hand);
    let land = put(&mut state, P1, "Plains", Zone::Battlefield);
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 3);
    let targets = cast_targets(&state, spell);
    assert!(targets.contains(&Target::Object(pup)));
    assert!(!targets.contains(&Target::Object(land)));
}

#[test]
fn fractured_identity_token_copy_of_an_aura_asks_what_it_enchants() {
    let mut state = plains();
    let rancor = put(&mut state, P0, "Rancor", Zone::Hand);
    let spell = put(&mut state, P0, "Fractured Identity", Zone::Hand);
    let mine = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, rancor, &[Target::Object(mine)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, rancor), Zone::Battlefield);
    assert_eq!(state.objects.get(mine).attachments, vec![rancor]);

    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 3);
    cast(&mut state, spell, &[Target::Object(rancor)]);
    // The opponent creates the copy, so the opponent chooses what it
    // enchants, among the two creatures.
    let mut asked = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } => {
            asked.push((player, legal_targets.len()));
            assert!(legal_targets.contains(&Target::Object(mine)));
            assert!(legal_targets.contains(&Target::Object(theirs)));
            Action::ChooseEffectTarget(Target::Object(theirs))
        }
        other => first_answer(other),
    });
    assert_eq!(asked, vec![(P1, 2)]);
    assert_eq!(zone(&state, rancor), Zone::Exile);
    assert!(state.objects.get(mine).attachments.is_empty());
    let attached = state.objects.get(theirs).attachments.clone();
    assert_eq!(attached.len(), 1);
    let copy = attached[0];
    assert!(state.objects.get(copy).v4.is_token);
    assert_eq!(state.objects.get(copy).controller, P1);
    assert_eq!(
        CARD_DEFS[state.objects.get(copy).card_def as usize].name,
        "Rancor"
    );
}

#[test]
fn fractured_identity_aura_copy_with_one_possible_host_needs_no_choice() {
    let mut state = plains();
    let rancor = put(&mut state, P0, "Rancor", Zone::Hand);
    let spell = put(&mut state, P0, "Fractured Identity", Zone::Hand);
    let mine = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, rancor, &[Target::Object(mine)]);
    settle_first(&mut state);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 3);
    cast(&mut state, spell, &[Target::Object(rancor)]);
    settled(&mut state);
    let attached = state.objects.get(mine).attachments.clone();
    assert_eq!(
        attached.len(),
        1,
        "the opponent's copy enchants the only creature"
    );
    assert!(state.objects.get(attached[0]).v4.is_token);
    assert_eq!(state.objects.get(attached[0]).controller, P1);
}

// ---- Memory Jar ----------------------------------------------------------------

/// P0 controls a Memory Jar and holds three cards; P1 holds two.
fn jar_state() -> (GameState, ObjectId, Vec<ObjectId>, Vec<ObjectId>) {
    let mut state = plains();
    let jar = put(&mut state, P0, "Memory Jar", Zone::Battlefield);
    let mine = vec![
        put(&mut state, P0, "Lightning Bolt", Zone::Hand),
        put(&mut state, P0, "Lightning Bolt", Zone::Hand),
        put(&mut state, P0, "Skullclamp", Zone::Hand),
    ];
    let theirs = vec![
        put(&mut state, P1, "Lightning Bolt", Zone::Hand),
        put(&mut state, P1, "Skullclamp", Zone::Hand),
    ];
    (state, jar, mine, theirs)
}

fn activate_jar(state: &mut GameState, jar: ObjectId) {
    activate(state, jar, 0, &[]);
    on_the_stack(state);
    assert_eq!(zone(state, jar), Zone::Graveyard, "sacrificed as a cost");
    settled(state);
}

#[test]
fn memory_jar_exiles_both_hands_face_down_and_each_player_draws_seven() {
    let (mut state, jar, mine, theirs) = jar_state();
    activate_jar(&mut state, jar);
    for card in mine.iter().chain(&theirs) {
        assert_eq!(zone(&state, *card), Zone::Exile);
        let owner = state.objects.get(*card).owner;
        let face = state
            .objects
            .get(*card)
            .v4
            .face_down_v1
            .expect("exiled face down");
        assert_eq!(face.lookers, 1 << owner.index(), "only its owner may look");
    }
    assert_eq!(hand_size(&state, P0), 7);
    assert_eq!(hand_size(&state, P1), 7);
    assert_eq!(state.players[0].draws_this_turn, 7);
    assert_eq!(state.players[1].draws_this_turn, 7);
}

#[test]
fn memory_jar_returns_each_players_exiled_cards_at_the_end_step_and_discards_the_rest() {
    let (mut state, jar, mine, theirs) = jar_state();
    activate_jar(&mut state, jar);
    pass_until(&mut state, p0_end_step_settled);
    for (cards, player) in [(&mine, P0), (&theirs, P1)] {
        assert_eq!(hand_size(&state, player), cards.len(), "{player:?}");
        for card in cards {
            assert_eq!(zone(&state, *card), Zone::Hand);
            assert!(hand_contains(&state, player, *card));
            assert!(state.objects.get(*card).v4.face_down_v1.is_none());
        }
    }
    // The seven drawn cards were discarded (the Jar is in P0's graveyard too).
    assert_eq!(state.players[0].graveyard.len(), 8);
    assert_eq!(state.players[1].graveyard.len(), 7);
}

#[test]
fn memory_jar_activated_on_the_opponents_turn_returns_the_cards_at_that_end_step() {
    let (mut state, jar, mine, theirs) = jar_state();
    state.active_player = P1;
    state.priority_player = P0;
    state.step = Step::Main1;
    activate_jar(&mut state, jar);
    for card in mine.iter().chain(&theirs) {
        assert_eq!(zone(&state, *card), Zone::Exile);
    }
    pass_until(&mut state, p1_end_step_settled);
    assert_eq!(hand_size(&state, P0), mine.len());
    assert_eq!(hand_size(&state, P1), theirs.len());
    for card in mine.iter().chain(&theirs) {
        assert_eq!(zone(&state, *card), Zone::Hand);
    }
}

#[test]
fn memory_jar_activated_in_an_end_step_waits_for_the_next_one() {
    let (mut state, jar, mine, theirs) = jar_state();
    state.step = Step::End;
    activate_jar(&mut state, jar);
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Upkeep
    });
    for card in mine.iter().chain(&theirs) {
        assert_eq!(zone(&state, *card), Zone::Exile, "still exiled");
    }
    pass_until(&mut state, p1_end_step_settled);
    assert_eq!(hand_size(&state, P0), mine.len());
    assert_eq!(hand_size(&state, P1), theirs.len());
    for card in mine.iter().chain(&theirs) {
        assert_eq!(zone(&state, *card), Zone::Hand);
    }
}

#[test]
fn memory_jar_hides_the_exiled_cards_from_the_other_player() {
    let (mut state, jar, mine, _) = jar_state();
    state.enable_hand_departures_by_name_v1();
    state.reveal_hand_card(P1, P0, mine[0]).unwrap();
    assert_eq!(state.known_hand_cards(P1, P0).len(), 1);
    activate_jar(&mut state, jar);
    assert!(
        state.known_hand_cards(P1, P0).is_empty(),
        "the other player cannot tell which cards left the hand"
    );
}

// ---- Public sessions -----------------------------------------------------------

#[test]
fn c5d_decks_finish_and_replay_through_public_sessions_both_seats() {
    let white_blue = deck(&[
        ("Plains", 9),
        ("Island", 9),
        ("Balance", 4),
        ("Council's Judgment", 4),
        ("Fractured Identity", 4),
        ("Proft's Eidetic Memory", 4),
        ("Palantir of Orthanc", 3),
        ("Thraben Inspector", 3),
    ]);
    let red_green = deck(&[
        ("Mountain", 10),
        ("Forest", 10),
        ("Memory Jar", 4),
        ("Palantir of Orthanc", 4),
        ("Jackal Pup", 4),
        ("Llanowar Elves", 4),
        ("Lightning Bolt", 4),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    for (pair, first_seed) in [
        ([white_blue.clone(), red_green.clone()], 0x4335_6400_u64),
        ([red_green, white_blue], 0x4335_6410),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let game = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
                for seat in &game.cast_cards {
                    for (&card, &count) in seat {
                        *casts.entry(card).or_default() += count;
                    }
                }
            }
        }
    }
    for name in C5D_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
