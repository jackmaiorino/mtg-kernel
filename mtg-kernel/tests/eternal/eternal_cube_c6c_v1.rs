//! Vintage Cube wave C6c: native rules and public-session tests for the
//! eight planeswalkers.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::planeswalker_v1::{change_loyalty, loyalty};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C6C_CARDS: [&str; 6] = [
    "Jace, the Mind Sculptor",
    "Narset, Parter of Veils",
    "Karn, Scion of Urza",
    "Teferi, Hero of Dominaria",
    "The Wandering Emperor",
    "Garruk, Curse Breaker",
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
        0x4355_4245_0000 + 0x3663,
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
fn c6c_cards_are_full() {
    for name in C6C_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn jace_printed_characteristics_and_cast_cost() {
    let card = def("Jace, the Mind Sculptor");
    assert_eq!(card.types, &[CardType::Planeswalker]);
    assert_eq!(card.subtypes, &[Subtype::Jace]);
    assert_eq!(card.supertypes, &[Supertype::Legendary]);
    assert_eq!(card.cost.generic, 2);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::U); 2]);
    assert_eq!(card.mana_value, 4);
    assert_eq!(card.colors, &[ManaColor::U]);
    assert_eq!((card.power, card.toughness), (None, None));
    assert_eq!(card.starting_loyalty, Some(3));
    assert_eq!(card.activated_abilities.len(), 4);
    let mut state = plains();
    let jace = put(&mut state, P0, card.name, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    assert!(!castable(&mut state, jace));
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, jace, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, jace), Zone::Battlefield);
    assert_eq!(loyalty(&state, jace), Some(3));
    assert_eq!(mana(&state, P0, ManaColor::U), 0);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert!(!activatable(&mut state, jace, 3));
}

fn cast_jace(state: &mut GameState) -> ObjectId {
    let jace = put(state, P0, "Jace, the Mind Sculptor", Zone::Hand);
    pool(state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(state, jace, &[]);
    settled(state);
    jace
}

fn on_the_stack(state: &mut GameState) {
    for _ in 0..100 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => {
                assert!(!state.stack.is_empty());
                return;
            }
            other => act(state, first_answer(other)),
        }
    }
    panic!("activation never reached the stack");
}

fn next_own_main(state: &mut GameState) {
    let turn = state.turn;
    pass_until(state, |state| {
        state.turn != turn && state.active_player == P0 && state.step == Step::Main1
    });
}

#[test]
fn jace_plus_two_private_look_keep_or_bottom_either_library() {
    for player in [P0, P1] {
        for bottom in [false, true] {
            let mut state = game(&["Island", "Plains", "Mountain", "Forest"]);
            let jace = cast_jace(&mut state);
            let original = state.players[player.index()].library.clone();
            let top = original[0];
            activate(&mut state, jace, 0, &[Target::Player(player)]);
            on_the_stack(&mut state);
            assert_eq!(loyalty(&state, jace), Some(5));
            // An activated ability remains independent of its source.
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(jace, Zone::Graveyard),
            );
            assert!(matches!(
                settle(&mut state),
                Some(Decision::ChooseEffectOption { player: P0, .. })
            ));
            assert!(state
                .known_library_cards(P0, player)
                .iter()
                .any(|entry| entry.object == top && entry.position == 0));
            assert!(state.known_library_cards(P1, player).is_empty());
            act(&mut state, Action::ChooseEffectOption(u16::from(bottom)));
            settled(&mut state);
            let mut expected = original;
            if bottom {
                expected.rotate_left(1);
            }
            assert_eq!(state.players[player.index()].library, expected);
            assert_eq!(zone(&state, top), Zone::Library);
            assert_eq!(
                state.objects.get(top).zone_change_count,
                0,
                "placement is not a zone change"
            );
            assert!(
                state.known_library_cards(P1, player).is_empty(),
                "the owner of an opponent library does not get the private look"
            );
            let position = if bottom { 3 } else { 0 };
            assert!(state
                .known_library_cards(P0, player)
                .iter()
                .any(|entry| entry.object == top && entry.position == position));
        }
    }
}

#[test]
fn jace_plus_two_empty_library_and_preexisting_owner_knowledge() {
    let mut empty = game(&[]);
    let jace = cast_jace(&mut empty);
    activate(&mut empty, jace, 0, &[Target::Player(P1)]);
    settled(&mut empty);
    assert_eq!(loyalty(&empty, jace), Some(5));
    assert!(empty.known_library_cards(P0, P1).is_empty());

    let mut known = plains();
    let other_jace = cast_jace(&mut known);
    let top = known.players[1].library[0];
    known.reveal_library_top(P1, P1, 1);
    activate(&mut known, other_jace, 0, &[Target::Player(P1)]);
    settle_option(&mut known, 1);
    assert!(known
        .known_library_cards(P1, P1)
        .iter()
        .any(|entry| entry.object == top && entry.position == 19));
}

#[test]
fn jace_zero_draws_three_and_puts_two_in_every_order_without_shuffle() {
    for [first, second] in [[0, 1], [0, 2], [1, 0], [1, 2], [2, 0], [2, 1]] {
        let mut state = game(&["Island", "Plains", "Mountain", "Forest"]);
        let jace = cast_jace(&mut state);
        let library = state.players[0].library.clone();
        activate(&mut state, jace, 1, &[]);
        on_the_stack(&mut state);
        assert_eq!(loyalty(&state, jace), Some(3));
        for pick in [library[first], library[second]] {
            assert!(matches!(
                settle(&mut state),
                Some(Decision::ChooseEffectTargets { player: P0, legal_targets, .. })
                    if legal_targets.contains(&Target::Object(pick))
            ));
            act(&mut state, Action::ChooseEffectTarget(Target::Object(pick)));
        }
        settled(&mut state);
        assert_eq!(
            state.players[0].library,
            vec![library[second], library[first], library[3]]
        );
        assert_eq!(state.players[0].hand.len(), 1);
        assert!(state.known_library_cards(P1, P0).is_empty());
        assert!(
            !activatable(&mut state, jace, 0),
            "zero also uses this turn's loyalty activation"
        );
        assert!(!activatable(&mut state, jace, 1));
        next_own_main(&mut state);
        assert!(activatable(&mut state, jace, 0));
    }
}

#[test]
fn jace_zero_can_return_preexisting_hand_cards_and_replace_a_draw_with_dredge() {
    let mut state = plains();
    let jace = cast_jace(&mut state);
    let old = put(&mut state, P0, "Mountain", Zone::Hand);
    let imp = put(&mut state, P0, "Stinkweed Imp", Zone::Graveyard);
    activate(&mut state, jace, 1, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets {
            player: P0, legal_targets, can_finish: true, ..
        }) if legal_targets == [Target::Object(imp)]
    ));
    act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { player: P0, legal_targets, .. })
            if legal_targets.contains(&Target::Object(old))
                && legal_targets.contains(&Target::Object(imp))
    ));
    assert_eq!(
        state.players[0].hand.len(),
        4,
        "one old card, dredged Imp, two draws"
    );
    assert_eq!(state.players[0].graveyard.len(), 5);
    act(&mut state, Action::ChooseEffectTarget(Target::Object(old)));
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { legal_targets, .. })
            if legal_targets.contains(&Target::Object(imp))
    ));
    act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
    settled(&mut state);
    assert_eq!(&state.players[0].library[..2], &[imp, old]);
    assert_eq!(state.players[0].library.len(), 15);
    assert_eq!(state.players[0].hand.len(), 2);
}

#[test]
fn jace_minus_one_bounces_to_owner_and_fizzles_on_a_new_incarnation() {
    for departure in 0..3 {
        let mut state = plains();
        let jace = cast_jace(&mut state);
        let creature = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
        activate(&mut state, jace, 2, &[Target::Object(creature)]);
        on_the_stack(&mut state);
        assert_eq!(loyalty(&state, jace), Some(2));
        if departure != 0 {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Exile),
            );
        }
        if departure == 2 {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Battlefield),
            );
        }
        settled(&mut state);
        let expected = [Zone::Hand, Zone::Exile, Zone::Battlefield][departure];
        assert_eq!(zone(&state, creature), expected);
        assert_eq!(hand_contains(&state, P1, creature), departure == 0);
    }
}

#[test]
fn jace_loyalty_timing_target_legality_and_insufficient_cost() {
    let mut state = plains();
    let jace = cast_jace(&mut state);
    let bogle = put(&mut state, P1, "Slippery Bogle", Zone::Battlefield);
    let bear = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    assert!(!activatable(&mut state, jace, 3));
    activate(&mut state, jace, 2, &[]);
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(bear)));
            assert!(!legal_targets.contains(&Target::Object(bogle)));
            assert!(!legal_targets.contains(&Target::Object(jace)));
            act(&mut state, Action::ChooseTarget(Target::Object(bear)));
        }
        other => panic!("expected creature targets: {other:?}"),
    }
    settled(&mut state);
    for index in 0..4 {
        assert!(!activatable(&mut state, jace, index));
    }
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1 && state.priority_player == P0
    });
    assert!(
        !activatable(&mut state, jace, 0),
        "not on the opponent's turn"
    );
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Upkeep
    });
    assert!(
        !activatable(&mut state, jace, 0),
        "not outside a main phase"
    );
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    on_the_stack(&mut state);
    assert!(
        !activatable(&mut state, jace, 0),
        "not with a nonempty stack"
    );
    settled(&mut state);
    change_loyalty(&mut state, jace, -1);
    assert_eq!(loyalty(&state, jace), Some(1));
    let last_bear = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    activate(&mut state, jace, 2, &[Target::Object(last_bear)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, jace),
        Zone::Graveyard,
        "paying the last loyalty is legal"
    );
    assert_eq!(
        zone(&state, last_bear),
        Zone::Hand,
        "the ability outlives Jace"
    );
}

#[test]
fn jace_zero_short_library_still_puts_back_as_many_as_possible_before_losing() {
    for count in 0..3 {
        let cards = ["Island", "Mountain"];
        let mut state = game(&cards[..count]);
        let jace = cast_jace(&mut state);
        let library = state.players[0].library.clone();
        activate(&mut state, jace, 1, &[]);
        loop {
            match settle(&mut state) {
                Some(Decision::GameOver { winner }) => {
                    assert_eq!(winner, Some(P1));
                    break;
                }
                Some(decision) => act(&mut state, first_answer(decision)),
                None => panic!("drawing from the empty library must lose"),
            }
        }
        assert!(state.players[0].hand.is_empty());
        assert_eq!(state.players[0].library.len(), count);
        assert!(library
            .iter()
            .all(|card| state.players[0].library.contains(card)));
    }
}

#[test]
fn jace_ultimate_after_five_real_turns_of_plus_two_exiles_then_shuffles_hand() {
    let mut state = plains();
    let jace = cast_jace(&mut state);
    for activation in 0..5 {
        assert!(!activatable(&mut state, jace, 3));
        activate(&mut state, jace, 0, &[Target::Player(P1)]);
        settle_option(&mut state, 0);
        assert_eq!(loyalty(&state, jace), Some(5 + 2 * activation));
        assert!(
            !activatable(&mut state, jace, 3),
            "no second loyalty activation this turn"
        );
        next_own_main(&mut state);
    }
    assert_eq!(loyalty(&state, jace), Some(13));
    assert!(activatable(&mut state, jace, 3));
    put(&mut state, P1, "Mountain", Zone::Hand);
    put(&mut state, P1, "Island", Zone::Hand);
    let old_library = state.players[1].library.clone();
    let old_hand = state.players[1].hand.clone();
    let own_library = state.players[0].library.clone();
    let own_hand = state.players[0].hand.clone();
    let mut replay = state.clone();
    for branch in [&mut state, &mut replay] {
        activate(branch, jace, 3, &[Target::Player(P1)]);
        settled(branch);
        assert_eq!(loyalty(branch, jace), Some(1));
        assert!(old_library
            .iter()
            .all(|&card| zone(branch, card) == Zone::Exile));
        assert_eq!(branch.exile.len(), old_library.len());
        assert!(branch.players[1].hand.is_empty());
        let mut shuffled = branch.players[1].library.clone();
        shuffled.sort();
        let mut expected = old_hand.clone();
        expected.sort();
        assert_eq!(shuffled, expected);
        assert_eq!(branch.players[0].library, own_library);
        assert_eq!(branch.players[0].hand, own_hand);
        assert!(branch.known_library_cards(P0, P1).is_empty());
        assert!(branch.known_library_cards(P1, P1).is_empty());
    }
    assert_eq!(
        state.players[1].library, replay.players[1].library,
        "the seeded shuffle replays identically"
    );
}

#[test]
fn jace_ultimate_handles_either_player_empty_zones_and_departed_source() {
    for player in [P0, P1] {
        for library_count in [0, 3] {
            for hand_count in [0, 3] {
                let cards = ["Island"; 3];
                let mut state = game(&cards[..library_count]);
                let jace = cast_jace(&mut state);
                change_loyalty(&mut state, jace, 9);
                for _ in 0..hand_count {
                    put(&mut state, player, "Mountain", Zone::Hand);
                }
                let library = state.players[player.index()].library.clone();
                let hand = state.players[player.index()].hand.clone();
                activate(&mut state, jace, 3, &[Target::Player(player)]);
                on_the_stack(&mut state);
                assert_eq!(
                    zone(&state, jace),
                    Zone::Graveyard,
                    "all twelve loyalty paid"
                );
                settled(&mut state);
                assert!(library
                    .iter()
                    .all(|&card| zone(&state, card) == Zone::Exile));
                assert!(hand.iter().all(|&card| zone(&state, card) == Zone::Library));
                assert_eq!(state.players[player.index()].library.len(), hand_count);
                assert!(state.players[player.index()].hand.is_empty());
            }
        }
    }
}

#[test]
fn jace_player_abilities_fizzle_if_the_target_gains_hexproof() {
    for index in [0, 3] {
        let mut state = plains();
        let jace = cast_jace(&mut state);
        change_loyalty(&mut state, jace, 10);
        put(&mut state, P1, "Island", Zone::Hand);
        let library = state.players[1].library.clone();
        let hand = state.players[1].hand.clone();
        activate(&mut state, jace, index, &[Target::Player(P1)]);
        on_the_stack(&mut state);
        let leyline = put(&mut state, P1, "Leyline of Sanctity", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(leyline, Zone::Battlefield),
        );
        settled(&mut state);
        assert_eq!(state.players[1].library, library);
        assert_eq!(state.players[1].hand, hand);
        assert!(state.exile.is_empty());
        assert!(state.known_library_cards(P0, P1).is_empty());
    }
}

fn cast_walker(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let walker = put(state, player, name, Zone::Hand);
    if name == "Narset, Parter of Veils" {
        pool(state, player, &[ManaColor::U, ManaColor::U], 1);
    } else {
        pool(state, player, &[], 4);
    }
    cast(state, walker, &[]);
    settled(state);
    walker
}

fn recall(state: &mut GameState, player: PlayerId) {
    let spell = put(state, P0, "Ancestral Recall", Zone::Hand);
    pool(state, P0, &[ManaColor::U], 0);
    cast(state, spell, &[Target::Player(player)]);
}

#[test]
fn narset_and_karn_printed_characteristics_costs_and_loyalty_timing() {
    for (name, subtype, generic, blue, abilities) in [
        ("Narset, Parter of Veils", Subtype::Narset, 1, 2, 1),
        ("Karn, Scion of Urza", Subtype::Karn, 4, 0, 3),
    ] {
        let card = def(name);
        assert_eq!(card.types, &[CardType::Planeswalker]);
        assert_eq!(card.subtypes, &[subtype]);
        assert_eq!(card.supertypes, &[Supertype::Legendary]);
        assert_eq!(card.cost.generic, generic);
        assert_eq!(card.cost.pips, vec![Pip::Colored(ManaColor::U); blue]);
        assert_eq!(card.mana_value, if blue == 0 { 4 } else { 3 });
        assert_eq!(
            card.colors,
            if blue == 0 {
                &[][..]
            } else {
                &[ManaColor::U][..]
            }
        );
        assert_eq!((card.power, card.toughness), (None, None));
        assert_eq!(card.starting_loyalty, Some(5));
        assert_eq!(card.activated_abilities.len(), abilities);
        let mut state = plains();
        let walker = put(&mut state, P0, name, Zone::Hand);
        let colors = vec![ManaColor::U; blue];
        pool(&mut state, P0, &colors, generic - 1);
        assert!(!castable(&mut state, walker));
        pool(&mut state, P0, &[], 1);
        cast(&mut state, walker, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, walker), Zone::Battlefield);
        assert_eq!(loyalty(&state, walker), Some(5));
        assert_eq!(mana(&state, P0, ManaColor::U), 0);
        assert_eq!(mana(&state, P0, ManaColor::C), 0);
        activate(&mut state, walker, 0, &[]);
        settle_first(&mut state);
        for index in 0..u8::try_from(abilities).unwrap() {
            assert!(!activatable(&mut state, walker, index));
        }
        pass_until(&mut state, |state| {
            state.active_player == P1 && state.step == Step::Main1 && state.priority_player == P0
        });
        assert!(!activatable(&mut state, walker, 0));
        pass_until(&mut state, |state| {
            state.active_player == P0 && state.step == Step::Upkeep
        });
        assert!(!activatable(&mut state, walker, 0));
        pass_until(&mut state, |state| {
            state.active_player == P0 && state.step == Step::Main1
        });
        assert!(activatable(&mut state, walker, 0));
        recall(&mut state, P0);
        on_the_stack(&mut state);
        assert!(!activatable(&mut state, walker, 0));
        settled(&mut state);
    }
    assert_eq!(Subtype::Narset.stable_id(), 720);
    assert!(!Subtype::Narset.is_creature_type());
}

#[test]
fn narset_looks_privately_selects_or_declines_and_random_bottom_replays() {
    for take in [false, true] {
        let mut state = game(&[
            "Wild Mongrel",
            "Lightning Bolt",
            "Island",
            "Sol Ring",
            "Forest",
            "Mountain",
        ]);
        let narset = cast_walker(&mut state, P0, "Narset, Parter of Veils");
        let library = state.players[0].library.clone();
        let mut replay = state.clone();
        for branch in [&mut state, &mut replay] {
            activate(branch, narset, 0, &[]);
            on_the_stack(branch);
            assert_eq!(loyalty(branch, narset), Some(3));
            match settle(branch) {
                Some(Decision::ChooseEffectTargets {
                    player,
                    legal_targets,
                    can_finish,
                    ..
                }) => {
                    assert_eq!(player, P0);
                    assert!(can_finish);
                    assert_eq!(
                        legal_targets,
                        vec![Target::Object(library[1]), Target::Object(library[3])]
                    );
                    assert!(branch.known_library_cards(P1, P0).is_empty());
                    act(
                        branch,
                        if take {
                            Action::ChooseEffectTarget(Target::Object(library[1]))
                        } else {
                            Action::FinishEffectSelection
                        },
                    );
                }
                other => panic!("expected optional selection: {other:?}"),
            }
            settled(branch);
            assert_eq!(&branch.players[0].library[..2], &library[4..]);
            let mut bottom = branch.players[0].library[2..].to_vec();
            bottom.sort();
            let mut expected = library[..4].to_vec();
            if take {
                expected.retain(|card| *card != library[1]);
            }
            expected.sort();
            assert_eq!(bottom, expected);
            assert_eq!(hand_contains(branch, P0, library[1]), take);
            assert_eq!(
                branch.players[0].draws_this_turn, 0,
                "putting into hand is not drawing"
            );
            if take {
                assert!(branch
                    .known_hand_cards(P1, P0)
                    .iter()
                    .any(|entry| entry.object == library[1]));
            }
        }
        assert_eq!(state.players[0].library, replay.players[0].library);
    }
}

#[test]
fn narset_short_or_ineligible_libraries_and_last_loyalty() {
    for count in 0..=4 {
        let cards = ["Plains"; 4];
        let mut state = game(&cards[..count]);
        let narset = cast_walker(&mut state, P0, "Narset, Parter of Veils");
        change_loyalty(&mut state, narset, -3);
        activate(&mut state, narset, 0, &[]);
        on_the_stack(&mut state);
        assert_eq!(zone(&state, narset), Zone::Graveyard);
        settled(&mut state);
        assert_eq!(state.players[0].library.len(), count);
        assert!(state.players[0].hand.is_empty());
        assert!(!state.players[0].drew_from_empty);
    }
    let mut state = plains();
    let narset = cast_walker(&mut state, P0, "Narset, Parter of Veils");
    for _ in 0..2 {
        activate(&mut state, narset, 0, &[]);
        settled(&mut state);
        next_own_main(&mut state);
    }
    assert_eq!(loyalty(&state, narset), Some(1));
    assert!(!activatable(&mut state, narset, 0));
}

#[test]
fn narset_limits_only_opponents_and_resets_on_each_players_real_turn() {
    let mut state = plains();
    let narset = cast_walker(&mut state, P0, "Narset, Parter of Veils");
    recall(&mut state, P1);
    settled(&mut state);
    assert_eq!(state.players[1].hand.len(), 1);
    assert_eq!(state.players[1].draws_this_turn, 1);
    recall(&mut state, P1);
    settled(&mut state);
    assert_eq!(state.players[1].hand.len(), 1);
    recall(&mut state, P0);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 3);
    assert_eq!(state.players[0].draws_this_turn, 3);
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1 && state.priority_player == P0
    });
    assert_eq!(
        state.players[1].hand.len(),
        2,
        "new turn allows the draw step draw"
    );
    assert_eq!(state.players[1].draws_this_turn, 1);
    recall(&mut state, P1);
    settled(&mut state);
    assert_eq!(
        state.players[1].hand.len(),
        2,
        "draw step used the allowance"
    );
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    assert_eq!(state.players[1].draws_this_turn, 0);
    recall(&mut state, P1);
    settled(&mut state);
    assert_eq!(
        state.players[1].hand.len(),
        3,
        "opponent also gets one on this turn"
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(narset, Zone::Graveyard),
    );
    recall(&mut state, P1);
    settled(&mut state);
    assert_eq!(
        state.players[1].hand.len(),
        6,
        "restriction ends with its source"
    );
}

#[test]
fn narset_counts_draws_before_entry_and_prevents_empty_library_replacements() {
    let mut short = game(&["Island"]);
    cast_walker(&mut short, P0, "Narset, Parter of Veils");
    recall(&mut short, P1);
    settled(&mut short);
    assert_eq!(short.players[1].hand.len(), 1);
    assert!(short.players[1].library.is_empty());
    assert!(!short.players[1].drew_from_empty);

    let mut state = game(&["Island"]);
    // A single ordinary draw before Narset enters uses the allowance.
    event::propose_and_commit(&mut state, ProposedEvent::draw(P1));
    cast_walker(&mut state, P0, "Narset, Parter of Veils");
    let jace = put(&mut state, P1, "Jace, Wielder of Mysteries", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(jace, Zone::Battlefield),
    );
    recall(&mut state, P1);
    settled(&mut state);
    assert_eq!(state.players[1].hand.len(), 1);
    assert!(state.players[1].library.is_empty());
    assert!(!state.players[1].drew_from_empty);
    assert_eq!(zone(&state, jace), Zone::Battlefield);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
}

#[test]
fn narset_allows_replacing_first_draw_but_never_offers_dredge_after_an_actual_draw() {
    for dredge in [false, true] {
        let mut state = plains();
        cast_walker(&mut state, P0, "Narset, Parter of Veils");
        let imp = put(&mut state, P1, "Stinkweed Imp", Zone::Graveyard);
        recall(&mut state, P1);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets {
            player: P1, legal_targets, can_finish: true, ..
        }) if legal_targets == vec![Target::Object(imp)])
        );
        act(
            &mut state,
            if dredge {
                Action::ChooseEffectTarget(Target::Object(imp))
            } else {
                Action::FinishEffectSelection
            },
        );
        settled(&mut state);
        assert_eq!(state.players[1].draws_this_turn, 1);
        assert_eq!(state.players[1].hand.len(), if dredge { 2 } else { 1 });
        assert_eq!(state.players[1].library.len(), if dredge { 14 } else { 19 });
        assert_eq!(
            zone(&state, imp),
            if dredge { Zone::Hand } else { Zone::Graveyard }
        );
        recall(&mut state, P1);
        settled(&mut state);
        assert_eq!(state.players[1].draws_this_turn, 1);
    }
}

#[test]
fn karn_plus_reveals_both_cards_opponent_chooses_and_source_can_leave() {
    for option in [0, 1] {
        let mut state = game(&["Island", "Mountain", "Forest"]);
        let karn = cast_walker(&mut state, P0, "Karn, Scion of Urza");
        let cards = state.players[0].library.clone();
        // Neither ability targets the opponent or draws cards.
        put(&mut state, P1, "Leyline of Sanctity", Zone::Battlefield);
        put(&mut state, P1, "Narset, Parter of Veils", Zone::Battlefield);
        activate(&mut state, karn, 0, &[]);
        on_the_stack(&mut state);
        assert_eq!(loyalty(&state, karn), Some(6));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(karn, Zone::Graveyard),
        );
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectOption {
                player: P1,
                option_count: 2,
                ..
            })
        ));
        for observer in [P0, P1] {
            for card in &cards[..2] {
                assert!(state
                    .known_library_cards(observer, P0)
                    .iter()
                    .any(|entry| entry.object == *card));
            }
        }
        act(&mut state, Action::ChooseEffectOption(option));
        settled(&mut state);
        assert_eq!(zone(&state, cards[usize::from(option)]), Zone::Hand);
        assert!(state
            .known_hand_cards(P1, P0)
            .iter()
            .any(|entry| entry.object == cards[usize::from(option)]));
        assert_eq!(zone(&state, cards[1 - usize::from(option)]), Zone::Exile);
        assert_eq!(state.players[0].library, vec![cards[2]]);
        assert_eq!(state.players[0].draws_this_turn, 0);
        let silver = &state.eternal_v1.as_ref().unwrap().c6c_silver_cards;
        assert_eq!(silver.len(), 1);
        assert_eq!(silver[0].object, cards[1 - usize::from(option)]);
        assert_eq!(
            silver[0].zone_change_count,
            state.objects.get(silver[0].object).zone_change_count
        );
    }
}

#[test]
fn karn_plus_zero_or_one_card_and_minus_one_with_no_silver() {
    for count in 0..=1 {
        let cards = ["Island"];
        let mut state = game(&cards[..count]);
        let karn = cast_walker(&mut state, P0, "Karn, Scion of Urza");
        activate(&mut state, karn, 0, &[]);
        settled(&mut state);
        assert_eq!(state.players[0].hand.len(), count);
        assert!(state.players[0].library.is_empty());
        assert!(state.exile.is_empty());
        assert!(!state.players[0].drew_from_empty);
    }
    let mut state = plains();
    let karn = cast_walker(&mut state, P0, "Karn, Scion of Urza");
    let unmarked = put(&mut state, P0, "Island", Zone::Exile);
    activate(&mut state, karn, 1, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, karn), Some(4));
    assert_eq!(zone(&state, unmarked), Zone::Exile);
}

#[test]
fn karn_silver_survives_source_departure_and_later_karn_chooses_either_card() {
    for option in [0, 1] {
        let mut state = plains();
        let mut exiled = Vec::new();
        for _ in 0..2 {
            let karn = cast_walker(&mut state, P0, "Karn, Scion of Urza");
            exiled.push(state.players[0].library[1]);
            activate(&mut state, karn, 0, &[]);
            settle_option(&mut state, 0);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(karn, Zone::Graveyard),
            );
        }
        let later = cast_walker(&mut state, P0, "Karn, Scion of Urza");
        activate(&mut state, later, 1, &[]);
        on_the_stack(&mut state);
        assert_eq!(loyalty(&state, later), Some(4));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(later, Zone::Graveyard),
        );
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectOption {
                player: P0,
                option_count: 2,
                ..
            })
        ));
        act(&mut state, Action::ChooseEffectOption(option));
        settled(&mut state);
        assert_eq!(zone(&state, exiled[usize::from(option)]), Zone::Hand);
        assert_eq!(zone(&state, exiled[1 - usize::from(option)]), Zone::Exile);
    }
}

#[test]
fn karn_silver_is_owned_not_controlled_and_does_not_follow_a_reexiled_card() {
    let mut state = plains();
    state.active_player = P1;
    state.priority_player = P1;
    let opponent_karn = cast_walker(&mut state, P1, "Karn, Scion of Urza");
    let opponents_card = state.players[1].library[1];
    activate(&mut state, opponent_karn, 0, &[]);
    settle_option(&mut state, 0);
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    let karn = cast_walker(&mut state, P0, "Karn, Scion of Urza");
    activate(&mut state, karn, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, opponents_card), Zone::Exile);
    next_own_main(&mut state);
    let card = state.players[0].library[1];
    activate(&mut state, karn, 0, &[]);
    settle_option(&mut state, 0);
    for destination in [Zone::Hand, Zone::Exile] {
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, destination));
    }
    next_own_main(&mut state);
    activate(&mut state, karn, 1, &[]);
    settled(&mut state);
    assert_eq!(
        zone(&state, card),
        Zone::Exile,
        "new incarnation has no silver counter"
    );
    assert_eq!(zone(&state, opponents_card), Zone::Exile);
}

#[test]
fn karn_construct_has_all_characteristics_and_counts_only_controlled_artifacts() {
    let mut state = plains();
    let karn = cast_walker(&mut state, P0, "Karn, Scion of Urza");
    activate(&mut state, karn, 2, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, karn), Some(3));
    let construct = named(&state, P0, "Karn Construct Token")[0];
    let card = &CARD_DEFS[state.objects.get(construct).card_def as usize];
    assert!(card.types.contains(&CardType::Artifact));
    assert!(card.types.contains(&CardType::Creature));
    assert_eq!(card.subtypes, &[Subtype::Construct]);
    assert_eq!((card.power, card.toughness), (Some(0), Some(0)));
    assert!(card.colors.is_empty());
    assert_eq!(card.keywords, Keywords::NONE);
    assert!(state.objects.get(construct).v4.is_token);
    assert!(state.objects.get(construct).summoning_sick);
    assert_eq!(power_toughness(&state, construct), (1, 1));
    put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    assert_eq!(power_toughness(&state, construct), (1, 1));
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, construct), (2, 2));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(ring, Zone::Graveyard),
    );
    assert_eq!(power_toughness(&state, construct), (1, 1));
    next_own_main(&mut state);
    activate(&mut state, karn, 2, &[]);
    on_the_stack(&mut state);
    assert_eq!(loyalty(&state, karn), Some(1));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(karn, Zone::Graveyard),
    );
    settled(&mut state);
    let constructs = named(&state, P0, "Karn Construct Token");
    assert_eq!(constructs.len(), 2);
    for token in constructs {
        assert_eq!(power_toughness(&state, token), (2, 2));
    }
}

#[test]
fn karn_one_silver_card_returns_automatically_even_when_last_loyalty_is_paid() {
    let mut state = plains();
    let karn = cast_walker(&mut state, P0, "Karn, Scion of Urza");
    let card = state.players[0].library[1];
    activate(&mut state, karn, 0, &[]);
    settle_option(&mut state, 0);
    next_own_main(&mut state);
    change_loyalty(&mut state, karn, -5);
    assert_eq!(loyalty(&state, karn), Some(1));
    assert!(!activatable(&mut state, karn, 2));
    activate(&mut state, karn, 1, &[]);
    on_the_stack(&mut state);
    assert_eq!(zone(&state, karn), Zone::Graveyard);
    settled(&mut state);
    assert_eq!(zone(&state, card), Zone::Hand);
    assert!(state
        .known_hand_cards(P1, P0)
        .iter()
        .any(|entry| entry.object == card));
}

#[test]
fn narset_draw_step_cannot_dredge_after_drawing_in_upkeep() {
    let mut state = plains();
    cast_walker(&mut state, P0, "Narset, Parter of Veils");
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Upkeep && state.priority_player == P0
    });
    recall(&mut state, P1);
    settled(&mut state);
    assert_eq!(state.players[1].draws_this_turn, 1);
    let imp = put(&mut state, P1, "Stinkweed Imp", Zone::Graveyard);
    let library = state.players[1].library.clone();
    run_until(
        &mut state,
        |state| state.step == Step::Main1,
        |_, decision| {
            assert!(
                !matches!(
                    decision,
                    Decision::ChooseEffectOption { .. } | Decision::ChooseEffectTargets { .. }
                ),
                "the forbidden draw must not offer a replacement"
            );
            None
        },
    );
    assert_eq!(state.players[1].library, library);
    assert_eq!(zone(&state, imp), Zone::Graveyard);
    assert_eq!(state.players[1].hand.len(), 1);
}

fn recall_replacing_every_draw_with_dredge(state: &mut GameState, player: PlayerId) {
    let imps: Vec<_> = (0..3)
        .map(|_| put(state, player, "Stinkweed Imp", Zone::Graveyard))
        .collect();
    recall(state, player);
    let mut replaced = 0;
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player: chooser,
            legal_targets,
            can_finish: true,
            ..
        } => {
            assert_eq!(chooser, player);
            assert!(legal_targets.iter().all(|target| {
                matches!(target, Target::Object(object) if imps.contains(object))
            }));
            replaced += 1;
            Action::ChooseEffectTarget(legal_targets[0])
        }
        other => first_answer(other),
    });
    assert_eq!(replaced, 3);
    assert!(imps.iter().all(|&imp| hand_contains(state, player, imp)));
}

#[test]
fn narset_does_not_count_dredges_replaced_draws_as_actual_draws() {
    let mut state = plains();
    cast_walker(&mut state, P0, "Narset, Parter of Veils");
    recall_replacing_every_draw_with_dredge(&mut state, P1);
    assert_eq!(state.players[1].draws_this_turn, 0);
    assert_eq!(state.players[1].hand.len(), 3);
    assert_eq!(state.players[1].library.len(), 5);
}

fn cast_emperor(state: &mut GameState) -> ObjectId {
    let walker = put(state, P0, "The Wandering Emperor", Zone::Hand);
    pool(state, P0, &[ManaColor::W; 2], 2);
    cast(state, walker, &[]);
    settled(state);
    // On the opponent's turn, they receive priority after resolution.
    if state.priority_player != P0 {
        act(state, Action::Pass);
    }
    walker
}

#[test]
fn emperor_printed_characteristics_flash_cost_and_starting_loyalty() {
    let card = def("The Wandering Emperor");
    assert_eq!(card.types, &[CardType::Planeswalker]);
    assert!(card.subtypes.is_empty());
    assert_eq!(card.supertypes, &[Supertype::Legendary]);
    assert_eq!(card.cost.generic, 2);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::W); 2]);
    assert_eq!(card.mana_value, 4);
    assert_eq!(card.colors, &[ManaColor::W]);
    assert_eq!((card.power, card.toughness), (None, None));
    assert_eq!(card.starting_loyalty, Some(3));
    assert_eq!(card.activated_abilities.len(), 3);
    let mut state = plains();
    state.active_player = P1;
    state.step = Step::BeginCombat;
    let walker = put(&mut state, P0, card.name, Zone::Hand);
    let ordinary = put(&mut state, P0, "Wild Mongrel", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::G], 2);
    assert!(!castable(&mut state, walker));
    assert!(!castable(&mut state, ordinary));
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, walker, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, walker), Zone::Battlefield);
    assert_eq!(loyalty(&state, walker), Some(3));
    assert!(keyword(&state, walker, Keywords::FLASH));
    assert_eq!(mana(&state, P0, ManaColor::W), 0);
}

#[test]
fn emperor_all_loyalty_modes_work_in_opponent_combat_once_per_turn() {
    for index in 0..3 {
        let mut state = plains();
        state.active_player = P1;
        state.step = Step::BeginCombat;
        let creature = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
        state.objects.get_mut(creature).tapped = true;
        let walker = cast_emperor(&mut state);
        for mode in 0..3 {
            assert!(activatable(&mut state, walker, mode));
        }
        let targets = if index == 1 {
            vec![]
        } else {
            vec![Target::Object(creature)]
        };
        activate(&mut state, walker, index, &targets);
        on_the_stack(&mut state);
        assert_eq!(loyalty(&state, walker), Some([4, 2, 1][usize::from(index)]));
        for mode in 0..3 {
            assert!(!activatable(&mut state, walker, mode));
        }
        settled(&mut state);
        match index {
            0 => {
                assert_eq!(power_toughness(&state, creature), (3, 3));
                assert!(keyword(&state, creature, Keywords::FIRST_STRIKE));
            }
            1 => assert_eq!(named(&state, P0, "Samurai Token").len(), 1),
            2 => {
                assert_eq!(zone(&state, creature), Zone::Exile);
                assert_eq!(state.players[0].life, 22);
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn emperor_flashes_in_after_attackers_and_exiles_the_attacker_before_damage() {
    let mut state = plains();
    state.active_player = P1;
    state.priority_player = P1;
    let attacker = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![attacker]));
    run_until(
        &mut state,
        |state| state.step == Step::DeclareAttackers && state.priority_player == P0,
        |_, _| None,
    );
    assert!(state.objects.get(attacker).tapped);
    let walker = cast_emperor(&mut state);
    activate(&mut state, walker, 2, &[Target::Object(attacker)]);
    settled(&mut state);
    assert_eq!(zone(&state, attacker), Zone::Exile);
    pass_until(&mut state, |state| state.step == Step::Main2);
    assert_eq!(state.players[0].life, 22);
    assert_eq!(loyalty(&state, walker), Some(1));
}

#[test]
fn emperor_can_activate_in_response_but_other_walkers_cannot() {
    let mut state = plains();
    let other = cast_jace(&mut state);
    let walker = cast_emperor(&mut state);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    on_the_stack(&mut state);
    assert!(!activatable(&mut state, other, 0));
    activate(&mut state, walker, 1, &[]);
    on_the_stack(&mut state);
    assert_eq!(state.stack.len(), 2);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Samurai Token").len(), 1);
    assert_eq!(state.players[1].life, 17);
}

#[test]
fn emperor_entry_exception_expires_each_actual_turn_and_flicker_renews_it() {
    let mut state = plains();
    let walker = cast_emperor(&mut state);
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1 && state.priority_player == P0
    });
    // Both players' turns can have the same round number.
    assert!(!state.objects.get(walker).v4.entered_battlefield_this_turn);
    for index in 0..2 {
        assert!(!activatable(&mut state, walker, index));
    }
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(walker, Zone::Exile));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(walker, Zone::Battlefield),
    );
    activate(&mut state, walker, 1, &[]);
    settled(&mut state);
    if state.priority_player != P0 {
        act(&mut state, Action::Pass);
    }
    assert!(!activatable(&mut state, walker, 1));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(walker, Zone::Exile));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(walker, Zone::Battlefield),
    );
    activate(&mut state, walker, 1, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Samurai Token").len(), 2);
    next_own_main(&mut state);
    assert!(!state.objects.get(walker).v4.entered_battlefield_this_turn);
    assert!(activatable(&mut state, walker, 1));
    pass_until(&mut state, |state| {
        state.step == Step::BeginCombat && state.priority_player == P0
    });
    assert!(!activatable(&mut state, walker, 1));
}

#[test]
fn emperor_plus_one_targets_either_player_or_zero_and_only_keyword_expires() {
    for target_owner in [P0, P1] {
        let mut state = plains();
        let creature = put(&mut state, target_owner, "Wild Mongrel", Zone::Battlefield);
        let walker = cast_emperor(&mut state);
        activate(&mut state, walker, 0, &[Target::Object(creature)]);
        settled(&mut state);
        assert_eq!(state.objects.get(creature).counters.plus1_plus1, 1);
        assert_eq!(power_toughness(&state, creature), (3, 3));
        assert!(keyword(&state, creature, Keywords::FIRST_STRIKE));
        pass_until(&mut state, |state| {
            state.active_player == P1 && state.step == Step::Main1
        });
        assert!(!keyword(&state, creature, Keywords::FIRST_STRIKE));
        assert_eq!(state.objects.get(creature).counters.plus1_plus1, 1);
    }
    for has_creature in [false, true] {
        let mut state = plains();
        let creature = has_creature.then(|| put(&mut state, P0, "Wild Mongrel", Zone::Battlefield));
        let walker = cast_emperor(&mut state);
        activate(&mut state, walker, 0, &[]);
        settle_first(&mut state);
        assert_eq!(loyalty(&state, walker), Some(4));
        assert!(!keyword(&state, walker, Keywords::FIRST_STRIKE));
        if let Some(creature) = creature {
            assert_eq!(state.objects.get(creature).counters.plus1_plus1, 0);
            assert!(!keyword(&state, creature, Keywords::FIRST_STRIKE));
        }
    }
}

#[test]
fn emperor_target_rosters_exclude_illegal_targets_and_minus_two_requires_one() {
    let mut state = plains();
    let walker = cast_emperor(&mut state);
    assert!(!activatable(&mut state, walker, 2));
    let own = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let opposing = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let untapped = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let bogle = put(&mut state, P1, "Slippery Bogle", Zone::Battlefield);
    let land = put(&mut state, P1, "Plains", Zone::Battlefield);
    for object in [own, opposing, bogle, land] {
        state.objects.get_mut(object).tapped = true;
    }
    let mut plus = state.clone();
    activate(&mut plus, walker, 0, &[]);
    match next(&mut plus) {
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(can_finish);
            for object in [own, opposing, untapped] {
                assert!(legal_targets.contains(&Target::Object(object)));
            }
            for object in [walker, land, bogle] {
                assert!(!legal_targets.contains(&Target::Object(object)));
            }
        }
        other => panic!("expected optional creature target: {other:?}"),
    }
    activate(&mut state, walker, 2, &[]);
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets.len(), 2);
            assert!(legal_targets.contains(&Target::Object(own)));
            assert!(legal_targets.contains(&Target::Object(opposing)));
            assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
            act(&mut state, Action::ChooseTarget(Target::Object(own)));
        }
        other => panic!("expected mandatory tapped creature: {other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, own), Zone::Exile);
    assert_eq!(zone(&state, opposing), Zone::Battlefield);
    assert_eq!(state.players[0].life, 22);
}

#[test]
fn emperor_targeted_abilities_fizzle_on_departure_or_new_incarnation_and_outlive_source() {
    for index in [0, 2] {
        for departure in 0..3 {
            let mut state = plains();
            let walker = cast_emperor(&mut state);
            let creature = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
            state.objects.get_mut(creature).tapped = true;
            activate(&mut state, walker, index, &[Target::Object(creature)]);
            on_the_stack(&mut state);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(walker, Zone::Graveyard),
            );
            if departure != 0 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(creature, Zone::Exile),
                );
            }
            if departure == 2 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(creature, Zone::Battlefield),
                );
                state.objects.get_mut(creature).tapped = true;
            }
            settled(&mut state);
            let resolved = departure == 0;
            assert_eq!(
                state.players[0].life,
                if index == 2 && resolved { 22 } else { 20 }
            );
            assert_eq!(
                state.objects.get(creature).counters.plus1_plus1,
                i32::from(index == 0 && resolved)
            );
            assert_eq!(
                keyword(&state, creature, Keywords::FIRST_STRIKE),
                index == 0 && resolved
            );
            let expected_zone = if departure == 2 || index == 0 && resolved {
                Zone::Battlefield
            } else {
                Zone::Exile
            };
            assert_eq!(zone(&state, creature), expected_zone);
        }
    }
}

#[test]
fn emperor_minus_two_fizzles_if_creature_untaps_and_insufficient_loyalty_cannot_pay() {
    let mut state = plains();
    let walker = cast_emperor(&mut state);
    let creature = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    state.objects.get_mut(creature).tapped = true;
    let mut poor = state.clone();
    change_loyalty(&mut poor, walker, -2);
    assert!(activatable(&mut poor, walker, 1));
    assert!(!activatable(&mut poor, walker, 2));
    activate(&mut poor, walker, 1, &[]);
    settled(&mut poor);
    assert_eq!(zone(&poor, walker), Zone::Graveyard);
    assert_eq!(named(&poor, P0, "Samurai Token").len(), 1);
    activate(&mut state, walker, 2, &[Target::Object(creature)]);
    on_the_stack(&mut state);
    state.objects.get_mut(creature).tapped = false;
    settled(&mut state);
    assert_eq!(zone(&state, creature), Zone::Battlefield);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(loyalty(&state, walker), Some(1));
}

#[test]
fn emperor_samurai_has_exact_characteristics_and_attacks_without_tapping() {
    let mut state = plains();
    let walker = cast_emperor(&mut state);
    activate(&mut state, walker, 1, &[]);
    settled(&mut state);
    let samurai = named(&state, P0, "Samurai Token")[0];
    let card = &CARD_DEFS[state.objects.get(samurai).card_def as usize];
    assert_eq!(card.types, &[CardType::Creature]);
    assert_eq!(card.subtypes, &[Subtype::Samurai]);
    assert_eq!(card.colors, &[ManaColor::W]);
    assert!(card.supertypes.is_empty());
    assert_eq!(card.mana_value, 0);
    assert!(state.objects.get(samurai).v4.is_token);
    assert_eq!(power_toughness(&state, samurai), (2, 2));
    assert!(keyword(&state, samurai, Keywords::VIGILANCE));
    assert!(!keyword(&state, samurai, Keywords::FIRST_STRIKE));
    assert!(state.objects.get(samurai).summoning_sick);
    next_own_main(&mut state);
    attack_with(&mut state, vec![samurai], |_, _| None);
    assert!(!state.objects.get(samurai).tapped);
    assert_eq!(state.players[1].life, 18);
}

fn cast_garruk(state: &mut GameState) -> ObjectId {
    let walker = put(state, P0, "Garruk, Curse Breaker", Zone::Hand);
    pool(state, P0, &[ManaColor::G, ManaColor::G], 3);
    cast(state, walker, &[]);
    settled(state);
    walker
}

fn garruk_untap(state: &mut GameState, walker: ObjectId, targets: &[Target]) {
    activate(state, walker, 0, targets);
    if targets.len() < 2 {
        assert!(matches!(
            next(state),
            Decision::ChooseEffectTargets {
                can_finish: true,
                ..
            }
        ));
        act(state, Action::FinishEffectSelection);
    }
}

#[test]
fn c6c_printed_characteristics_include_garruk_and_honest_unimplemented_cards() {
    for name in C6C_CARDS {
        let card = def(name);
        assert_eq!(card.types, &[CardType::Planeswalker]);
        assert_eq!(card.supertypes, &[Supertype::Legendary]);
        assert_eq!((card.power, card.toughness), (None, None));
    }
    let card = def("Garruk, Curse Breaker");
    assert_eq!(card.subtypes, &[Subtype::Garruk]);
    assert_eq!(card.cost.generic, 3);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::G); 2]);
    assert_eq!(card.mana_value, 5);
    assert_eq!(card.colors, &[ManaColor::G]);
    assert_eq!(card.starting_loyalty, Some(5));
    assert_eq!(card.activated_abilities.len(), 3);
    for name in ["Chandra, Torch of Defiance", "Saheeli, Sublime Artificer"] {
        assert!(card_id_by_name(name).is_none(), "{name} stays out");
    }
    let mut state = plains();
    let walker = put(&mut state, P0, card.name, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 3);
    assert!(!castable(&mut state, walker));
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, walker, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, walker), Some(5));
    assert_eq!(mana(&state, P0, ManaColor::G), 0);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    activate(&mut state, walker, 1, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, walker), Some(2));
    assert!(!activatable(&mut state, walker, 0));
    next_own_main(&mut state);
    assert!(activatable(&mut state, walker, 0));
    assert!(!activatable(&mut state, walker, 1));
    assert!(!activatable(&mut state, walker, 2));
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1 && s.priority_player == P0
    });
    assert!(!activatable(&mut state, walker, 0));
}

#[test]
fn garruk_untaps_zero_one_or_two_distinct_lands_of_either_player() {
    for count in 0..=2 {
        let mut state = plains();
        let walker = cast_garruk(&mut state);
        let own = put(&mut state, P0, "Forest", Zone::Battlefield);
        let other = put(&mut state, P1, "Island", Zone::Battlefield);
        let creature = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
        state.objects.get_mut(own).tapped = true;
        state.objects.get_mut(other).tapped = true;
        activate(&mut state, walker, 0, &[]);
        for target in [own, other].into_iter().take(count) {
            let Decision::ChooseEffectTargets {
                legal_targets,
                can_finish,
                ..
            } = next(&mut state)
            else {
                panic!("land targets")
            };
            assert!(can_finish);
            assert!(!legal_targets.contains(&Target::Object(creature)));
            if target == other {
                assert!(!legal_targets.contains(&Target::Object(own)));
            }
            assert!(legal_targets.contains(&Target::Object(target)));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(target)),
            );
        }
        if count < 2 {
            assert!(matches!(
                next(&mut state),
                Decision::ChooseEffectTargets {
                    can_finish: true,
                    ..
                }
            ));
            act(&mut state, Action::FinishEffectSelection);
        }
        on_the_stack(&mut state);
        assert_eq!(loyalty(&state, walker), Some(7));
        settled(&mut state);
        assert_eq!(state.objects.get(own).tapped, count == 0);
        assert_eq!(state.objects.get(other).tapped, count < 2);
    }
}

#[test]
fn garruk_untap_rechecks_targets_individually_and_survives_its_source() {
    for both in [false, true] {
        let mut state = plains();
        let walker = cast_garruk(&mut state);
        let lands = [
            put(&mut state, P0, "Forest", Zone::Battlefield),
            put(&mut state, P1, "Island", Zone::Battlefield),
        ];
        for land in lands {
            state.objects.get_mut(land).tapped = true;
        }
        garruk_untap(&mut state, walker, &lands.map(Target::Object));
        on_the_stack(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(walker, Zone::Graveyard),
        );
        for land in lands.into_iter().take(if both { 2 } else { 1 }) {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(land, Zone::Exile));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(land, Zone::Battlefield),
            );
            state.objects.get_mut(land).tapped = true;
        }
        settled(&mut state);
        assert!(state.objects.get(lands[0]).tapped);
        assert_eq!(state.objects.get(lands[1]).tapped, both);
    }
}

#[test]
fn garruk_beast_is_green_four_four_trample_and_its_entry_draw_is_replaceable() {
    for replace in [false, true] {
        let mut state = plains();
        let walker = cast_garruk(&mut state);
        let imp = replace.then(|| put(&mut state, P0, "Stinkweed Imp", Zone::Graveyard));
        let hand = state.players[0].hand.len();
        activate(&mut state, walker, 1, &[]);
        if let Some(imp) = imp {
            assert!(matches!(
                settle(&mut state),
                Some(Decision::ChooseEffectTargets { legal_targets, .. })
                    if legal_targets.contains(&Target::Object(imp))
            ));
            act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
        }
        settled(&mut state);
        let beast = named(&state, P0, "Garruk Beast Token")[0];
        let token_def = def("Garruk Beast Token");
        assert_eq!(token_def.types, &[CardType::Creature]);
        assert_eq!(token_def.subtypes, &[Subtype::Beast]);
        assert_eq!(token_def.colors, &[ManaColor::G]);
        assert_eq!(token_def.mana_value, 0);
        assert!(token_def.supertypes.is_empty());
        assert_eq!(token_def.keywords, Keywords::TRAMPLE);
        assert_eq!(power_toughness(&state, beast), (4, 4));
        assert!(state.objects.get(beast).v4.is_token);
        assert!(state.objects.get(beast).summoning_sick);
        assert!(keyword(&state, beast, Keywords::TRAMPLE));
        assert_eq!(state.players[0].hand.len(), hand + 1);
        assert_eq!(state.players[0].draws_this_turn, u32::from(!replace));
        if let Some(imp) = imp {
            assert_eq!(zone(&state, imp), Zone::Hand);
        }
    }
}

#[test]
fn garruk_entry_threshold_includes_counters_and_excludes_opponents_and_small_creatures() {
    for x in [3, 4, 5] {
        let mut state = plains();
        cast_garruk(&mut state);
        let ballista = put(&mut state, P0, "Walking Ballista", Zone::Hand);
        pool(&mut state, P0, &[], 10);
        cast(&mut state, ballista, &[]);
        settle_option(&mut state, x);
        assert_eq!(
            power_toughness(&state, ballista),
            (i32::from(x), i32::from(x))
        );
        assert_eq!(state.players[0].draws_this_turn, u32::from(x >= 4));
    }
    let mut state = plains();
    cast_garruk(&mut state);
    let bear = put(&mut state, P0, "Wild Mongrel", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, bear, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].draws_this_turn, 0);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::create_token(id("Garruk Beast Token"), P1),
    );
    settled(&mut state);
    assert_eq!(state.players[0].draws_this_turn, 0);
    assert_eq!(state.players[1].draws_this_turn, 0);
}

#[test]
fn garruk_last_three_loyalty_still_makes_a_beast_but_absent_garruk_does_not_draw() {
    let mut state = plains();
    let walker = cast_garruk(&mut state);
    change_loyalty(&mut state, walker, -2);
    activate(&mut state, walker, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, walker), Zone::Graveyard);
    assert_eq!(named(&state, P0, "Garruk Beast Token").len(), 1);
    assert_eq!(state.players[0].draws_this_turn, 0);
}

#[test]
fn garruk_captures_simultaneous_entries_once_and_draws_after_source_and_entrants_leave() {
    let mut state = plains();
    let walker = cast_garruk(&mut state);
    let first = put(&mut state, P0, "Ravenous Baloth", Zone::Hand);
    let second = put(&mut state, P0, "Ravenous Baloth", Zone::Hand);
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::zone_change(first, Zone::Battlefield),
            ProposedEvent::zone_change(second, Zone::Battlefield),
        ],
    );
    event::propose_and_commit_batch(
        &mut state,
        [walker, first, second]
            .map(|object| ProposedEvent::zone_change(object, Zone::Graveyard))
            .to_vec(),
    );
    settled(&mut state);
    assert_eq!(state.players[0].draws_this_turn, 2);
    assert_eq!(state.players[1].draws_this_turn, 0);
}

#[test]
fn garruk_grant_pumps_only_creatures_attacking_the_opponent_and_expires_next_turn() {
    let mut state = plains();
    let walker = cast_garruk(&mut state);
    let first = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let second = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let stay = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let enemy = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let opposing_walker = put(&mut state, P1, "Jace, the Mind Sculptor", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(opposing_walker, Zone::Battlefield),
    );
    settled(&mut state);
    activate(&mut state, walker, 2, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, walker), Some(1));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(walker, Zone::Graveyard),
    );
    attack_with(&mut state, vec![first, second], |_, decision| {
        if let Decision::ChooseAttackTarget { attacker, .. } = decision {
            return Some(Action::ChooseAttackTarget(if *attacker == first {
                Target::Player(P1)
            } else {
                Target::Object(opposing_walker)
            }));
        }
        None
    });
    assert_eq!(power_toughness(&state, first), (4, 4));
    assert!(keyword(&state, first, Keywords::TRAMPLE));
    for object in [second, stay, enemy] {
        assert_eq!(power_toughness(&state, object), (2, 2));
        assert!(!keyword(&state, object, Keywords::TRAMPLE));
    }
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_eq!(power_toughness(&state, first), (2, 2));
    assert!(!keyword(&state, first, Keywords::TRAMPLE));
    assert_eq!(
        state.eternal_v1.as_ref().unwrap().c6c.garruk_grants.len(),
        1
    );
    attack_with(&mut state, vec![enemy], |_, _| None);
    assert_eq!(power_toughness(&state, enemy), (2, 2));
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    assert!(state
        .eternal_v1
        .as_ref()
        .unwrap()
        .c6c
        .garruk_grants
        .is_empty());
    attack_with(&mut state, vec![first], |_, _| None);
    assert_eq!(power_toughness(&state, first), (2, 2));
}

#[test]
fn garruk_attack_trigger_freezes_all_attackers_and_does_not_follow_blinked_creatures() {
    let mut state = plains();
    let walker = cast_garruk(&mut state);
    let first = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let second = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    activate(&mut state, walker, 2, &[]);
    settled(&mut state);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![first, second]));
    on_the_stack(&mut state);
    assert_eq!(state.stack.len(), 1, "one trigger for both attackers");
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(first, Zone::Exile));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(first, Zone::Battlefield),
    );
    settled(&mut state);
    assert_eq!(power_toughness(&state, first), (2, 2));
    assert!(!keyword(&state, first, Keywords::TRAMPLE));
    assert_eq!(power_toughness(&state, second), (4, 4));
    assert!(keyword(&state, second, Keywords::TRAMPLE));
}

#[test]
fn garruk_samples_entry_static_power_and_does_not_redraw_when_power_increases_later() {
    for rings in [2, 3] {
        let mut state = plains();
        cast_garruk(&mut state);
        let karn = cast_walker(&mut state, P0, "Karn, Scion of Urza");
        for _ in 0..rings {
            put(&mut state, P0, "Sol Ring", Zone::Battlefield);
        }
        activate(&mut state, karn, 2, &[]);
        settled(&mut state);
        let construct = named(&state, P0, "Karn Construct Token")[0];
        assert_eq!(power_toughness(&state, construct), (rings + 1, rings + 1));
        assert_eq!(state.players[0].draws_this_turn, u32::from(rings == 3));
        let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
        pool(&mut state, P0, &[], 1);
        cast(&mut state, ring, &[]);
        settled(&mut state);
        assert_eq!(power_toughness(&state, construct), (rings + 2, rings + 2));
        assert_eq!(state.players[0].draws_this_turn, u32::from(rings == 3));
    }
}

#[test]
fn garruk_vigilance_attack_triggers_without_events_and_multiple_grants_stack() {
    let mut state = plains();
    let first = cast_garruk(&mut state);
    activate(&mut state, first, 2, &[]);
    settled(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(first, Zone::Graveyard),
    );
    let second = cast_garruk(&mut state);
    activate(&mut state, second, 2, &[]);
    settled(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(second, Zone::Graveyard),
    );
    let samurai = token(&mut state, P0, "Samurai Token");
    assert!(keyword(&state, samurai, Keywords::VIGILANCE));
    attack_with(&mut state, vec![samurai], |_, _| None);
    assert!(!state.objects.get(samurai).tapped);
    assert_eq!(power_toughness(&state, samurai), (6, 6));
    assert!(keyword(&state, samurai, Keywords::TRAMPLE));
    assert_eq!(state.players[1].life, 14);
}

#[test]
fn garruk_beast_tramples_and_zero_attackers_do_not_trigger_the_grant() {
    let mut state = plains();
    let walker = cast_garruk(&mut state);
    activate(&mut state, walker, 1, &[]);
    settled(&mut state);
    let beast = named(&state, P0, "Garruk Beast Token")[0];
    next_own_main(&mut state);
    let blocker = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    attack_with(&mut state, vec![beast], |_, decision| match decision {
        Decision::DeclareBlockers { .. } => Some(Action::DeclareBlockers(vec![(blocker, beast)])),
        Decision::ChooseCombatDamageRange {
            source, recipient, ..
        } if *source == beast && *recipient == Target::Object(blocker) => {
            Some(Action::ChooseCombatDamageRange { upper_half: false })
        }
        _ => None,
    });
    assert_eq!(state.players[1].life, 18);
    assert_eq!(zone(&state, blocker), Zone::Graveyard);
    next_own_main(&mut state);
    change_loyalty(&mut state, walker, 2);
    activate(&mut state, walker, 2, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, walker), Zone::Graveyard);
    attack_with(&mut state, Vec::new(), |_, _| None);
    assert_eq!(power_toughness(&state, beast), (4, 4));
    assert_eq!(state.players[1].life, 18);
}

#[derive(Default)]
struct C6cEvents {
    casts: [usize; C6C_CARDS.len()],
}

impl C6cEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        if semantic["action_kind"].as_str() != Some("cast_spell") {
            return;
        }
        let Some(card_id) = semantic["source"]["card_db_id"]
            .as_u64()
            .and_then(|id| usize::try_from(id).ok())
        else {
            return;
        };
        let name = CARD_DEFS[card_id].name;
        if let Some(index) = C6C_CARDS.iter().position(|&card| card == name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c6c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let control = deck(&[
        ("Tundra", 20),
        ("Island", 4),
        ("Jace, the Mind Sculptor", 8),
        ("Narset, Parter of Veils", 8),
        ("Teferi, Hero of Dominaria", 8),
        ("The Wandering Emperor", 8),
        ("Thraben Inspector", 4),
    ]);
    let beasts = deck(&[
        ("Forest", 24),
        ("Garruk, Curse Breaker", 12),
        ("Karn, Scion of Urza", 12),
        ("Wild Mongrel", 8),
        ("Ravenous Baloth", 4),
    ]);
    let mut events = C6cEvents::default();
    // Twelve games plus their twelve replays, with both policies and seats.
    for pair in [[control.clone(), beasts.clone()], [beasts, control]] {
        for seed in 0xC6C0_u64..0xC6C3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played = play_observed(pair.clone(), None, seed, policy, &mut |d, a| {
                    events.observe(d, a)
                });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, casts) in C6C_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} never cast through a public session");
    }
}

fn cast_teferi(state: &mut GameState) -> ObjectId {
    let walker = put(state, P0, "Teferi, Hero of Dominaria", Zone::Hand);
    pool(state, P0, &[ManaColor::W, ManaColor::U], 3);
    cast(state, walker, &[]);
    settled(state);
    walker
}

#[test]
fn teferi_printed_characteristics_cast_cost_and_loyalty_timing() {
    let card = def("Teferi, Hero of Dominaria");
    assert_eq!(card.types, &[CardType::Planeswalker]);
    assert_eq!(card.subtypes, &[Subtype::Teferi]);
    assert_eq!(card.supertypes, &[Supertype::Legendary]);
    assert_eq!(card.cost.generic, 3);
    assert_eq!(
        card.cost.pips,
        &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::U)]
    );
    assert_eq!(card.mana_value, 5);
    assert_eq!(card.colors, &[ManaColor::W, ManaColor::U]);
    assert_eq!((card.power, card.toughness), (None, None));
    assert_eq!(card.starting_loyalty, Some(4));
    assert_eq!(card.activated_abilities.len(), 3);
    let mut state = plains();
    let walker = put(&mut state, P0, card.name, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    assert!(!castable(&mut state, walker));
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, walker, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, walker), Zone::Battlefield);
    assert_eq!(loyalty(&state, walker), Some(4));
    for color in [ManaColor::C, ManaColor::W, ManaColor::U] {
        assert_eq!(mana(&state, P0, color), 0);
    }
    assert!(!activatable(&mut state, walker, 2));
    let land = put(&mut state, P1, "Island", Zone::Battlefield);
    let bogle = put(&mut state, P1, "Slippery Bogle", Zone::Battlefield);
    let mut probe = state.clone();
    activate(&mut probe, walker, 1, &[]);
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(land)));
            assert!(!legal_targets.contains(&Target::Object(bogle)));
            assert!(legal_targets.contains(&Target::Object(walker)));
            act(&mut probe, Action::ChooseTarget(Target::Object(walker)));
        }
        other => panic!("expected nonland targets: {other:?}"),
    }
    settled(&mut probe);
    assert_eq!(probe.players[0].library[2], walker);
    activate(&mut state, walker, 0, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, walker), Some(5));
    assert!(!activatable(&mut state, walker, 0));
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1 && state.priority_player == P0
    });
    assert!(!activatable(&mut state, walker, 0));
    next_own_main(&mut state);
    assert!(activatable(&mut state, walker, 0));
}

#[test]
fn teferi_draw_then_next_end_step_untaps_zero_one_or_two_lands_of_either_player() {
    for count in 0..=2 {
        let mut state = plains();
        let walker = cast_teferi(&mut state);
        let own = put(&mut state, P0, "Island", Zone::Battlefield);
        let opposing = put(&mut state, P1, "Forest", Zone::Battlefield);
        let extra = put(&mut state, P0, "Plains", Zone::Battlefield);
        for land in [own, opposing, extra] {
            state.objects.get_mut(land).tapped = true;
        }
        // Main phase two still schedules this turn's end step.
        pass_until(&mut state, |state| state.step == Step::Main2);
        let before = state.players[0].hand.len();
        activate(&mut state, walker, 0, &[]);
        settled(&mut state);
        assert_eq!(state.players[0].hand.len(), before + 1);
        assert!([own, opposing, extra]
            .iter()
            .all(|&land| state.objects.get(land).tapped));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(walker, Zone::Graveyard),
        );
        run_until(
            &mut state,
            |state| {
                state.step == Step::End
                    && state.stack.is_empty()
                    && state.engine.pending_triggers.is_empty()
                    && state.engine.pending_effect.is_none()
            },
            |_, decision| {
                if let Decision::ChooseEffectTargets {
                    player,
                    legal_targets,
                    selected_count,
                    can_finish,
                    ..
                } = decision
                {
                    assert_eq!(*player, P0);
                    if usize::from(*selected_count) < count {
                        let target = [own, opposing][usize::from(*selected_count)];
                        assert!(legal_targets.contains(&Target::Object(target)));
                        return Some(Action::ChooseEffectTarget(Target::Object(target)));
                    }
                    assert!(*can_finish);
                    return Some(Action::FinishEffectSelection);
                }
                None
            },
        );
        assert_eq!(state.objects.get(own).tapped, count == 0);
        assert_eq!(state.objects.get(opposing).tapped, count < 2);
        assert!(state.objects.get(extra).tapped);
        assert!(state.eternal_v1.as_ref().unwrap().c6c.untaps.is_empty());
    }
}

#[test]
fn teferi_plus_one_dredge_replacement_still_schedules_the_untap() {
    let mut state = plains();
    let walker = cast_teferi(&mut state);
    let imp = put(&mut state, P0, "Stinkweed Imp", Zone::Graveyard);
    activate(&mut state, walker, 0, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { legal_targets, .. })
            if legal_targets.contains(&Target::Object(imp))
    ));
    act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
    settled(&mut state);
    assert_eq!(zone(&state, imp), Zone::Hand);
    assert_eq!(state.players[0].draws_this_turn, 0);
    assert_eq!(state.players[0].library.len(), 15);
    assert_eq!(state.eternal_v1.as_ref().unwrap().c6c.untaps.len(), 1);
    // With no tapped lands the delayed trigger resolves without a prompt.
    pass_until(&mut state, |state| {
        state.step == Step::End
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
    });
    assert!(state.eternal_v1.as_ref().unwrap().c6c.untaps.is_empty());
}

#[test]
fn teferi_minus_three_inserts_third_or_bottom_in_owners_library_and_removes_tokens() {
    for size in 0..=3 {
        let cards = ["Island", "Mountain", "Forest"];
        let mut state = game(&cards[..size]);
        let walker = cast_teferi(&mut state);
        let creature = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
        let original = state.players[1].library.clone();
        activate(&mut state, walker, 1, &[Target::Object(creature)]);
        settled(&mut state);
        let mut expected = original;
        expected.insert(size.min(2), creature);
        assert_eq!(state.players[1].library, expected);
        assert_eq!(loyalty(&state, walker), Some(1));
        assert!(!activatable(&mut state, walker, 1));
    }
    let mut state = plains();
    let walker = cast_teferi(&mut state);
    let stolen = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(stolen, Zone::Exile));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change_to_battlefield_under_controller(stolen, P0),
    );
    activate(&mut state, walker, 1, &[Target::Object(stolen)]);
    settled(&mut state);
    assert_eq!(state.players[1].library[2], stolen);
    assert!(!state.players[0].library.contains(&stolen));

    let mut tokens = plains();
    let token_walker = cast_teferi(&mut tokens);
    let servo = token(&mut tokens, P1, "Servo Token");
    activate(&mut tokens, token_walker, 1, &[Target::Object(servo)]);
    settled(&mut tokens);
    assert!(!tokens.players[1].library.contains(&servo));
    assert!(!tokens.players[1].battlefield.contains(&servo));
}

#[test]
fn teferi_minus_three_fizzles_on_departed_or_blinked_targets_and_outlives_source() {
    for departure in 0..=2 {
        let mut state = plains();
        let walker = cast_teferi(&mut state);
        let creature = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
        activate(&mut state, walker, 1, &[Target::Object(creature)]);
        on_the_stack(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(walker, Zone::Graveyard),
        );
        if departure != 0 {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Exile),
            );
        }
        if departure == 2 {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Battlefield),
            );
        }
        settled(&mut state);
        assert_eq!(
            zone(&state, creature),
            [Zone::Library, Zone::Exile, Zone::Battlefield][departure]
        );
    }
}

fn teferi_emblem(state: &mut GameState) -> ObjectId {
    let walker = cast_teferi(state);
    change_loyalty(state, walker, 4);
    activate(state, walker, 2, &[]);
    settled(state);
    assert_eq!(zone(state, walker), Zone::Graveyard);
    walker
}

#[test]
fn teferi_emblems_persist_stack_and_trigger_only_on_their_owners_actual_draws() {
    let mut state = plains();
    let first = teferi_emblem(&mut state);
    let second = teferi_emblem(&mut state);
    assert_eq!(
        state.eternal_v1.as_ref().unwrap().c6c.teferi_emblems.len(),
        2
    );
    let opposing: Vec<_> = (0..6)
        .map(|_| put(&mut state, P1, "Island", Zone::Battlefield))
        .collect();
    let own = put(&mut state, P0, "Island", Zone::Battlefield);
    recall(&mut state, P1);
    settled(&mut state);
    assert!(opposing
        .iter()
        .all(|&land| zone(&state, land) == Zone::Battlefield));
    recall(&mut state, P0);
    let mut chosen = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            player,
            legal_targets,
            ..
        } => {
            assert_eq!(player, P0);
            assert!(!legal_targets.contains(&Target::Object(own)));
            let target = opposing[chosen];
            chosen += 1;
            assert!(legal_targets.contains(&Target::Object(target)));
            Action::ChooseTarget(Target::Object(target))
        }
        other => first_answer(other),
    });
    assert_eq!(chosen, 6);
    assert!(opposing
        .iter()
        .all(|&land| zone(&state, land) == Zone::Exile));
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
    // Drawing with no opposing permanents never strands target placement.
    recall(&mut state, P0);
    settled(&mut state);
}

#[test]
fn teferi_emblem_does_not_trigger_for_draws_replaced_by_dredge() {
    let mut state = plains();
    teferi_emblem(&mut state);
    let target = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    recall_replacing_every_draw_with_dredge(&mut state, P0);
    assert_eq!(state.players[0].draws_this_turn, 0);
    assert_eq!(state.players[0].hand.len(), 3);
    assert_eq!(state.players[0].library.len(), 5);
    assert_eq!(zone(&state, target), Zone::Battlefield);
}

#[test]
fn teferi_emblem_target_rechecks_departed_and_returned_incarnations() {
    for departure in 0..=2 {
        let mut state = plains();
        teferi_emblem(&mut state);
        let walker = cast_teferi(&mut state);
        let target = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
        activate(&mut state, walker, 0, &[]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseTargets { legal_targets, .. })
                if legal_targets.contains(&Target::Object(target))
        ));
        act(&mut state, Action::ChooseTarget(Target::Object(target)));
        on_the_stack(&mut state);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Exile));
        if departure == 1 {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Battlefield),
            );
        }
        if departure == 2 {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change_to_battlefield_under_controller(target, P0),
            );
        }
        settled(&mut state);
        assert_eq!(
            zone(&state, target),
            if departure == 0 {
                Zone::Exile
            } else {
                Zone::Battlefield
            }
        );
    }
}

#[test]
fn teferi_emblem_keeps_its_owner_when_its_maker_returns_under_opponent_control() {
    let mut state = plains();
    let maker = teferi_emblem(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change_to_battlefield_under_controller(maker, P1),
    );
    let bogle = put(&mut state, P1, "Slippery Bogle", Zone::Battlefield);
    recall(&mut state, P1);
    settled(&mut state);
    assert_eq!(zone(&state, maker), Zone::Battlefield);
    recall(&mut state, P0);
    let mut choices = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            player,
            legal_targets,
            ..
        } => {
            assert_eq!(player, P0);
            assert_eq!(legal_targets, vec![Target::Object(maker)]);
            assert!(!legal_targets.contains(&Target::Object(bogle)));
            choices += 1;
            Action::ChooseTarget(Target::Object(maker))
        }
        other => first_answer(other),
    });
    assert_eq!(choices, 3);
    assert_eq!(zone(&state, maker), Zone::Exile);
    assert_eq!(
        state.eternal_v1.as_ref().unwrap().c6c.teferi_emblems[0].controller,
        P0
    );
}
