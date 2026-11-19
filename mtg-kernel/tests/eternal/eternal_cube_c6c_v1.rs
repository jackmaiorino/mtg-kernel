//! Vintage Cube wave C6c: native rules and public-session tests for the
//! eight planeswalkers.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

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

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C6C_CARDS: [&str; 1] = ["Jace, the Mind Sculptor"];

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
            assert!(
                matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player: P0, legal_targets, .. }) if legal_targets.contains(&Target::Object(pick)))
            );
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
    assert!(
        matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player: P0, legal_targets, can_finish: true, .. }) if legal_targets == [Target::Object(imp)])
    );
    act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
    assert!(
        matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player: P0, legal_targets, .. }) if legal_targets.contains(&Target::Object(old)) && legal_targets.contains(&Target::Object(imp)))
    );
    assert_eq!(
        state.players[0].hand.len(),
        4,
        "one old card, dredged Imp, two draws"
    );
    assert_eq!(state.players[0].graveyard.len(), 5);
    act(&mut state, Action::ChooseEffectTarget(Target::Object(old)));
    assert!(
        matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { legal_targets, .. }) if legal_targets.contains(&Target::Object(imp)))
    );
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
        let creature = put(&mut state, P1, "Grizzly Bears", Zone::Battlefield);
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
    let bear = put(&mut state, P0, "Grizzly Bears", Zone::Battlefield);
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
    let last_bear = put(&mut state, P0, "Grizzly Bears", Zone::Battlefield);
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
