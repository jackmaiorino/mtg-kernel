//! Vintage Cube wave C9a: native rules and public-session tests for the
//! Aang, Badgermole Cub, Batroc, Blightsteel, Crabomination, and Fear of Missing Out.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code, unused_imports)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::EternalStateV1;
use mtg_kernel::eternal_cube_c9a_v1::C9aStateV1;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::planeswalker_v1::loyalty;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C9A_CARDS: [&str; 6] = [
    "Aang, Swift Savior",
    "Badgermole Cub",
    "Batroc the Leaper",
    "Blightsteel Colossus",
    "Crabomination",
    "Fear of Missing Out",
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
        0x4355_4245_0000 + 0x3961,
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
fn c9a_cards_are_full() {
    for name in C9A_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c9a_printed_characteristics_and_empty_state_isolation() {
    // Aang, Swift Savior // Aang and La, Ocean's Fury.
    let aang = def("Aang, Swift Savior");
    assert_eq!(aang.mana_value, 3);
    assert_eq!(aang.power, Some(2));
    assert_eq!(aang.toughness, Some(3));
    assert_eq!(aang.colors, &[ManaColor::W, ManaColor::U]);
    assert!(aang.supertypes.contains(&Supertype::Legendary));
    assert!(aang.subtypes.contains(&Subtype::Human));
    assert!(aang.subtypes.contains(&Subtype::Avatar));
    assert!(aang.subtypes.contains(&Subtype::Ally));
    assert!(aang.keywords.has(Keywords::FLASH));
    assert!(aang.keywords.has(Keywords::FLYING));
    assert_eq!(aang.activated_abilities.len(), 9);
    let back = aang
        .transform_face
        .as_ref()
        .expect("Aang has back face Aang and La, Ocean's Fury");
    assert_eq!(back.name, "Aang and La, Ocean's Fury");
    assert_eq!(back.power, Some(5));
    assert_eq!(back.toughness, Some(5));
    assert!(back.colors.is_empty(), "back face is colorless");
    assert_eq!(back.types, &[CardType::Creature]);
    assert_eq!(
        back.subtypes,
        &[Subtype::Avatar, Subtype::Spirit, Subtype::Ally]
    );
    assert!(back.keywords.has(Keywords::REACH));
    assert!(back.keywords.has(Keywords::TRAMPLE));
    assert!(!back.keywords.has(Keywords::FLASH));
    assert!(!back.keywords.has(Keywords::FLYING));

    // Badgermole Cub.
    let cub = def("Badgermole Cub");
    assert_eq!(cub.mana_value, 2);
    assert_eq!(cub.power, Some(2));
    assert_eq!(cub.toughness, Some(2));
    assert_eq!(cub.colors, &[ManaColor::G]);
    assert_eq!(cub.subtypes, &[Subtype::Badger, Subtype::Mole]);
    assert!(
        !cub.has_mana_ability(),
        "triggered mana ability is not a tap ability"
    );

    // Batroc the Leaper.
    let batroc = def("Batroc the Leaper");
    assert_eq!(batroc.mana_value, 2);
    assert_eq!(batroc.power, Some(2));
    assert_eq!(batroc.toughness, Some(2));
    assert_eq!(batroc.colors, &[ManaColor::R]);
    assert!(batroc.supertypes.contains(&Supertype::Legendary));
    assert_eq!(batroc.subtypes, &[Subtype::Human, Subtype::Villain]);

    // Blightsteel Colossus.
    let colossus = def("Blightsteel Colossus");
    assert_eq!(colossus.mana_value, 12);
    assert_eq!(colossus.power, Some(11));
    assert_eq!(colossus.toughness, Some(11));
    assert!(colossus.colors.is_empty());
    assert!(colossus.has_type(CardType::Artifact));
    assert!(colossus.has_type(CardType::Creature));
    assert_eq!(colossus.subtypes, &[Subtype::Phyrexian, Subtype::Golem]);
    assert!(colossus.keywords.has(Keywords::TRAMPLE));
    assert!(colossus.keywords.has(Keywords::INFECT));
    assert!(colossus.keywords.has(Keywords::INDESTRUCTIBLE));

    // Crabomination.
    let crab = def("Crabomination");
    assert_eq!(crab.mana_value, 6);
    assert_eq!(crab.power, Some(5));
    assert_eq!(crab.toughness, Some(5));
    assert_eq!(crab.colors, &[ManaColor::B]);
    assert_eq!(crab.subtypes, &[Subtype::Crab, Subtype::Demon]);
    assert!(crab.alt_cost.is_some());

    // Fear of Missing Out.
    let fomo = def("Fear of Missing Out");
    assert_eq!(fomo.mana_value, 2);
    assert_eq!(fomo.power, Some(2));
    assert_eq!(fomo.toughness, Some(3));
    assert_eq!(fomo.colors, &[ManaColor::R]);
    assert!(fomo.has_type(CardType::Enchantment));
    assert!(fomo.has_type(CardType::Creature));
    assert_eq!(fomo.subtypes, &[Subtype::Nightmare]);

    // Empty C9aStateV1 skips serialization and hashes to zero bytes.
    let empty = C9aStateV1::default();
    assert!(empty.is_empty());
    let mut h_empty = DefaultHasher::new();
    empty.hash(&mut h_empty);
    let h_zero = DefaultHasher::new();
    assert_eq!(
        h_empty.finish(),
        h_zero.finish(),
        "empty C9aStateV1 must hash zero bytes"
    );
    let eternal_json = serde_json::to_value(EternalStateV1::default()).expect("serialize");
    assert!(
        eternal_json.get("c9a").is_none(),
        "empty c9a state must skip serialization: {eternal_json}"
    );
}

// ---- Aang, Swift Savior // Aang and La, Ocean's Fury ----------------------------

#[test]
fn aang_flash_airbend_creature_and_owner_casts_for_two() {
    // 1. Cast Aang with Flash on opponent's turn; airbend opponent's Sheoldred;
    //    Aang cannot target itself; P0 cannot cast opponent's exiled card;
    //    P1 can cast it on a later turn for {2} (colorless mana suffices).
    let mut state = plains();
    state.active_player = P1;
    state.priority_player = P0;
    let sheoldred = put(
        &mut state,
        P1,
        "Sheoldred, the Apocalypse",
        Zone::Battlefield,
    );
    let aang = put(&mut state, P0, "Aang, Swift Savior", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 1);

    assert!(castable(&mut state, aang), "Aang has Flash on P1's turn");
    cast(&mut state, aang, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        }) => {
            assert!(can_finish, "up to one target");
            assert!(legal_targets.contains(&Target::Object(sheoldred)));
            assert!(
                !legal_targets.contains(&Target::Object(aang)),
                "must target another creature or spell, not Aang itself"
            );
        }
        other => panic!("expected Aang ETB targets, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(sheoldred)));
    settled(&mut state);

    assert_eq!(zone(&state, aang), Zone::Battlefield);
    assert!(keyword(&state, aang, Keywords::FLYING));
    assert_eq!(zone(&state, sheoldred), Zone::Exile);

    // P0 cannot cast P1's airbent card even with mana.
    pool(&mut state, P0, &[], 5);
    assert!(!castable(&mut state, sheoldred));

    // P1's turn: P1 cannot cast with only {1}, CAN cast with {2} colorless.
    state.priority_player = P1;
    state.players[P1.index()].mana_pool = [0; 6];
    pool(&mut state, P1, &[], 1);
    assert!(!castable(&mut state, sheoldred));
    pool(&mut state, P1, &[], 1);
    assert!(
        castable(&mut state, sheoldred),
        "owner can cast airbent card for {{2}} instead of {{2}}{{B}}{{B}}"
    );
    cast(&mut state, sheoldred, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, sheoldred), Zone::Battlefield);
    assert_eq!(state.objects.get(sheoldred).controller, P1);

    // 2. Taxes (Thalia) still apply on top of airbend's {2}; X spells have X=0;
    //    alternative-cost spells (Force of Will) cast for {2} with no alt-cost
    //    prompt; Dryad Arbor (land) gets no cast permission; 0 targets and
    //    illegal-on-resolution targets resolve cleanly.
    let mut state = plains();
    let ballista = put(&mut state, P1, "Walking Ballista", Zone::Battlefield);
    state.objects.get_mut(ballista).counters.plus1_plus1 = 2;
    let aang = put(&mut state, P0, "Aang, Swift Savior", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 1);
    cast(&mut state, aang, &[]);
    settle_choosing(&mut state, ballista);
    assert_eq!(zone(&state, ballista), Zone::Exile);

    // Advance to P1's turn to verify AsLongAsExiled survives across turns, and
    // casting Walking Ballista for {2} sets X = 0 (dies to SBA as 0/0).
    let start_turn = state.turn;
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1 && s.turn >= start_turn
    });
    pool(&mut state, P1, &[], 6);
    assert!(castable(&mut state, ballista));
    cast(&mut state, ballista, &[]);
    settled(&mut state);
    assert_eq!(
        zone(&state, ballista),
        Zone::Graveyard,
        "airbent Walking Ballista casts with X=0 and dies as 0/0"
    );
    assert_eq!(
        mana(&state, P1, ManaColor::C),
        4,
        "paid exactly {{2}} of {{6}}"
    );

    // Dryad Arbor (creature land) exiled by airbend cannot be cast.
    let mut state = plains();
    let arbor = put(&mut state, P1, "Dryad Arbor", Zone::Battlefield);
    let aang = put(&mut state, P0, "Aang, Swift Savior", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 1);
    cast(&mut state, aang, &[]);
    settle_choosing(&mut state, arbor);
    assert_eq!(zone(&state, arbor), Zone::Exile);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[], 5);
    assert!(
        !castable(&mut state, arbor),
        "land exiled by airbend cannot be cast"
    );

    // Thalia tax applies on top of airbend's {2} for noncreature spells.
    let mut state = plains();
    let _thalia = put(
        &mut state,
        P0,
        "Thalia, Guardian of Thraben",
        Zone::Battlefield,
    );
    state.priority_player = P1;
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 1);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    let aang = put(&mut state, P0, "Aang, Swift Savior", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 1);
    cast(&mut state, aang, &[]);
    settle_choosing(&mut state, bolt);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    // With Thalia on board, airbent Lightning Bolt costs {2} + {1} = {3}.
    state.players[P1.index()].mana_pool = [0; 6];
    state.priority_player = P1;
    pool(&mut state, P1, &[], 2);
    assert!(
        !castable(&mut state, bolt),
        "Thalia taxes airbent noncreature spell to {{3}}"
    );
    pool(&mut state, P1, &[], 1);
    assert!(castable(&mut state, bolt));
    cast(&mut state, bolt, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 17);

    // Airbending a spell with its own alternative cost (Force of Will):
    // casts for {2} with no alternative-cost prompt (CR 118.9a).
    let mut state = plains();
    let dummy_bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::W, ManaColor::U],
        1,
    );
    cast(&mut state, dummy_bolt, &[Target::Player(P1)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let fow = put(&mut state, P1, "Force of Will", Zone::Hand);
    let _blue_pitch = put(&mut state, P1, "Brainstorm", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 3);
    assert!(castable(&mut state, fow));
    act(&mut state, Action::CastSpell(fow));
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    act(&mut state, Action::ChooseTarget(Target::Object(dummy_bolt)));
    assert!(matches!(next(&mut state), Decision::ChooseCastMode { .. }));
    act(&mut state, Action::ChooseCastMode(CastMode::Normal));
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    let aang = put(&mut state, P0, "Aang, Swift Savior", Zone::Hand);
    cast(&mut state, aang, &[]);
    // Resolve Aang (P0 pass, P1 pass), then airbend Force of Will.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    act(&mut state, Action::ChooseTarget(Target::Object(fow)));
    settled(&mut state);
    assert_eq!(zone(&state, fow), Zone::Exile);
    // P0 casts another Bolt; P1 casts airbent Force of Will for {2} (no alt-cost prompt).
    let bolt2 = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt2, &[Target::Player(P1)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    state.players[P1.index()].mana_pool = [0; 6];
    pool(&mut state, P1, &[], 2);
    assert!(castable(&mut state, fow));
    cast(&mut state, fow, &[Target::Object(bolt2)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt2), Zone::Graveyard);
    assert_eq!(
        life(&state, P1),
        17,
        "bolt2 was countered by airbent Force of Will"
    );

    // Airbending 0 targets (FinishEffectSelection) and target becoming illegal.
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let aang = put(&mut state, P0, "Aang, Swift Savior", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 1);
    cast(&mut state, aang, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets {
            can_finish: true,
            ..
        })
    ));
    act(&mut state, Action::FinishEffectSelection);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield);

    // Target dies in response to Aang's ETB -> fizzles cleanly.
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let aang = put(&mut state, P0, "Aang, Swift Savior", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::U, ManaColor::R],
        1,
    );
    cast(&mut state, aang, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { .. })
    ));
    act(&mut state, Action::ChooseTarget(Target::Object(elves)));
    // P0 holds priority with Aang's ETB on stack; kills Elves in response.
    cast(&mut state, bolt, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
}

#[test]
fn aang_airbend_spell_on_stack_uncounterable_and_copy() {
    // 1. Airbending Supreme Verdict (can't be countered) exiles it off the
    //    stack cleanly without countering it; owner can cast it later for {2}.
    let mut state = plains();
    let bear = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.active_player = P1;
    state.priority_player = P1;
    let verdict = put(&mut state, P1, "Supreme Verdict", Zone::Hand);
    pool(
        &mut state,
        P1,
        &[ManaColor::W, ManaColor::W, ManaColor::U],
        1,
    );
    cast(&mut state, verdict, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);

    let aang = put(&mut state, P0, "Aang, Swift Savior", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 1);
    cast(&mut state, aang, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseTargets { legal_targets, .. }) => {
            assert!(
                legal_targets.contains(&Target::Object(verdict)),
                "uncounterable spell on stack is a legal airbend target"
            );
        }
        other => panic!("expected Aang ETB targets, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(verdict)));
    settled(&mut state);

    assert_eq!(
        zone(&state, bear),
        Zone::Battlefield,
        "Verdict did not resolve"
    );
    assert_eq!(zone(&state, aang), Zone::Battlefield);
    assert_eq!(zone(&state, verdict), Zone::Exile);

    // Owner (P1) can cast Supreme Verdict from exile for {2}.
    state.active_player = P1;
    state.priority_player = P1;
    state.players[P1.index()].mana_pool = [0; 6];
    pool(&mut state, P1, &[], 2);
    assert!(castable(&mut state, verdict));
    cast(&mut state, verdict, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bear), Zone::Graveyard);
    assert_eq!(zone(&state, aang), Zone::Graveyard);

    // 2. Airbending a spell copy on the stack (from Lutri, the Spellchaser):
    //    the copy ceases to exist and gets NO play permission in exile.
    let mut state = plains();
    state.active_player = P1;
    state.priority_player = P1;
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let lutri = put(&mut state, P1, "Lutri, the Spellchaser", Zone::Hand);
    pool(
        &mut state,
        P1,
        &[ManaColor::R, ManaColor::U, ManaColor::U],
        1,
    );
    cast(&mut state, bolt, &[Target::Player(P0)]);
    cast(&mut state, lutri, &[]);
    // Resolve Lutri -> choose Bolt as target -> keep target (option 0).
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { .. })
    ));
    act(&mut state, Action::ChooseTarget(Target::Object(bolt)));
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectOption { .. })
    ));
    act(&mut state, Action::ChooseEffectOption(0));
    // Now Bolt + Bolt copy are on the stack; P1 passes to P0.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    let copy_id = state
        .stack
        .last()
        .expect("Bolt copy on top of stack")
        .source;
    assert_ne!(copy_id, bolt);
    act(&mut state, Action::Pass);

    let aang = put(&mut state, P0, "Aang, Swift Savior", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 1);
    cast(&mut state, aang, &[]);
    // Resolve Aang (pass P0, pass P1) -> target the Bolt copy.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(copy_id)));
        }
        other => panic!("expected Aang ETB targeting copy, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(copy_id)));
    settled(&mut state);

    assert!(
        !state.exile.contains(&copy_id),
        "exiled spell copy ceased to exist"
    );
    assert!(
        state
            .engine
            .exile_play_permissions
            .iter()
            .all(|perm| perm.object != copy_id),
        "spell copy must not receive an exile play permission"
    );
    assert_eq!(
        life(&state, P0),
        17,
        "only original Bolt resolved, not copy"
    );
}

#[test]
fn aang_waterbend_transform_and_back_face_attack_trigger() {
    let mut state = plains();
    let aang = put(&mut state, P0, "Aang, Swift Savior", Zone::Battlefield);
    // 4 other untapped creatures/artifacts (including summoning-sick creatures).
    let sick_elf = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(sick_elf).summoning_sick = true;
    let mox1 = put(&mut state, P0, "Mox Pearl", Zone::Battlefield);
    let mox2 = put(&mut state, P0, "Mox Sapphire", Zone::Battlefield);
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Battlefield);
    // Total tappable artifacts/creatures = 5 (aang, sick_elf, mox1, mox2, bauble).
    // Give {3} mana so ability index 5 ({3} + 5 taps) is payable, while
    // index 0 ({8} + 0 taps) and index 6 ({2} + 6 taps) are not.
    pool(&mut state, P0, &[], 3);

    assert!(!activatable(&mut state, aang, 0));
    assert!(activatable(&mut state, aang, 5));
    assert!(!activatable(&mut state, aang, 6));

    act(&mut state, Action::ActivateAbility(aang, 5));
    settle_first(&mut state);

    // Aang transformed into Aang and La, Ocean's Fury (face_index 1).
    assert_eq!(state.objects.get(aang).v4.face_index, 1);
    assert_eq!(power_toughness(&state, aang), (5, 5));
    assert_eq!(
        engine::object_color_mask(&state, aang),
        0,
        "Aang and La, Ocean's Fury is colorless"
    );
    assert!(keyword(&state, aang, Keywords::REACH));
    assert!(keyword(&state, aang, Keywords::TRAMPLE));
    assert!(!keyword(&state, aang, Keywords::FLYING));
    assert!(!keyword(&state, aang, Keywords::FLASH));
    assert!(engine::has_effective_subtype(&state, aang, Subtype::Avatar));
    assert!(engine::has_effective_subtype(&state, aang, Subtype::Spirit));
    assert!(engine::has_effective_subtype(&state, aang, Subtype::Ally));
    assert!(state.objects.get(sick_elf).tapped);
    assert!(state.objects.get(mox1).tapped);
    assert!(state.objects.get(mox2).tapped);
    assert!(state.objects.get(bauble).tapped);

    // Face-0 Waterbend is no longer activatable even with {8} mana.
    pool(&mut state, P0, &[], 8);
    assert!(!activatable(&mut state, aang, 0));

    // Untap Aang and attack alongside another tapped creature and an untapped creature.
    state.objects.get_mut(aang).tapped = false;
    let tapped_ally = put(&mut state, P0, "Monastery Swiftspear", Zone::Battlefield);
    state.objects.get_mut(tapped_ally).tapped = true;
    let untapped_ally = put(&mut state, P0, "Goblin Anarchomancer", Zone::Battlefield);
    state.objects.get_mut(untapped_ally).tapped = false;

    attack_with(&mut state, vec![aang], |_, _| None);

    // Aang and La tapped to attack -> got +1/+1 counter (6/6, dealt 6!).
    // tapped_ally and sick_elf were tapped -> each got +1/+1 counter.
    // untapped_ally stayed untapped -> 0 counters.
    assert_eq!(state.objects.get(aang).counters.plus1_plus1, 1);
    assert_eq!(power_toughness(&state, aang), (6, 6));
    assert_eq!(life(&state, P1), 14);
    assert_eq!(state.objects.get(tapped_ally).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(sick_elf).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(untapped_ally).counters.plus1_plus1, 0);
}

// ---- Badgermole Cub -------------------------------------------------------------

#[test]
fn badgermole_cub_earthbend_animation_and_delayed_return() {
    // 1. Earthbend 1 animates a freshly-played Forest into a 0/0 creature with
    //    Haste that is still a land, puts a +1/+1 counter on it (1/1), and
    //    returns it tapped under owner's control when it dies.
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    state.objects.get_mut(forest).summoning_sick = true;
    let cub = put(&mut state, P0, "Badgermole Cub", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);

    cast(&mut state, cub, &[]);
    settle_choosing(&mut state, forest);

    assert!(engine::object_has_type(&state, forest, CardType::Land));
    assert!(engine::object_has_type(&state, forest, CardType::Creature));
    assert!(keyword(&state, forest, Keywords::HASTE));
    assert_eq!(state.objects.get(forest).counters.plus1_plus1, 1);
    assert_eq!(power_toughness(&state, forest), (1, 1));

    // Because it has Haste, it can tap for mana immediately AND triggers Cub!
    act(&mut state, Action::ActivateManaAbility(forest));
    assert_eq!(
        mana(&state, P0, ManaColor::G),
        2,
        "Forest + Cub extra {{G}}"
    );

    // Kill the earthbent Forest with Lightning Bolt -> returns tapped, no longer a creature.
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    state.priority_player = P1;
    cast(&mut state, bolt, &[Target::Object(forest)]);
    settled(&mut state);

    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert!(state.objects.get(forest).tapped, "returns tapped");
    assert!(!engine::object_has_type(&state, forest, CardType::Creature));
    assert!(engine::object_has_type(&state, forest, CardType::Land));
    assert_eq!(state.objects.get(forest).counters.plus1_plus1, 0);
    assert!(!keyword(&state, forest, Keywords::HASTE));

    // 2. Exiling an earthbent land (Swords to Plowshares) ALSO returns it tapped.
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let cub = put(&mut state, P0, "Badgermole Cub", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, cub, &[]);
    settle_choosing(&mut state, forest);
    let swords = put(&mut state, P1, "Swords to Plowshares", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W], 0);
    state.priority_player = P1;
    cast(&mut state, swords, &[Target::Object(forest)]);
    settled(&mut state);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert!(state.objects.get(forest).tapped);
    assert!(!engine::object_has_type(&state, forest, CardType::Creature));

    // 3. Earthbent land dying under Rest in Peace (replaced -> Exile) still returns.
    let mut state = plains();
    let _rip = put(&mut state, P1, "Rest in Peace", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let cub = put(&mut state, P0, "Badgermole Cub", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, cub, &[]);
    settle_choosing(&mut state, forest);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    state.priority_player = P1;
    cast(&mut state, bolt, &[Target::Object(forest)]);
    settled(&mut state);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert!(state.objects.get(forest).tapped);

    // 4. Bouncing an earthbent land to hand does NOT return it.
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let cub = put(&mut state, P0, "Badgermole Cub", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, cub, &[]);
    settle_choosing(&mut state, forest);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(forest, Zone::Hand));
    settled(&mut state);
    assert_eq!(zone(&state, forest), Zone::Hand);

    // 5. If the land leaves graveyard before the delayed trigger resolves,
    //    the trigger fails its zone_change_count check and does not return it.
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let cub = put(&mut state, P0, "Badgermole Cub", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, cub, &[]);
    settle_choosing(&mut state, forest);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    state.priority_player = P1;
    cast(&mut state, bolt, &[Target::Object(forest)]);
    // Resolve Bolt (pass P1, pass P0) -> Forest dies and return trigger goes on stack.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(zone(&state, forest), Zone::Graveyard);
    assert_eq!(state.stack.len(), 1, "earthbend return trigger on stack");
    // Move Forest to exile before trigger resolves -> zcc changes.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(forest, Zone::Exile));
    settled(&mut state);
    assert_eq!(zone(&state, forest), Zone::Exile);
}

#[test]
fn badgermole_cub_triggered_mana_ability_manual_and_auto() {
    // 1. Manual activation: Llanowar Elves adds {G}{G}; two Cubs add {G}{G}{G};
    //    Birds of Paradise tapping for {U} adds {U} + {G}; non-creature Forest
    //    adds only {G}; opponent's Llanowar Elves does not trigger P0's Cub.
    let mut state = plains();
    let _cub1 = put(&mut state, P0, "Badgermole Cub", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let birds = put(&mut state, P0, "Birds of Paradise", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let opp_elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);

    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(elves));
    assert_eq!(mana(&state, P0, ManaColor::G), 2);
    assert!(
        state.stack.is_empty(),
        "triggered mana ability uses no stack"
    );

    act(
        &mut state,
        Action::ActivateManaAbilityChoice(birds, ManaColor::U),
    );
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 3);

    act(&mut state, Action::ActivateManaAbility(forest));
    assert_eq!(
        mana(&state, P0, ManaColor::G),
        4,
        "non-creature land adds only 1 {{G}}"
    );

    // Second Badgermole Cub stacks.
    let _cub2 = put(&mut state, P0, "Badgermole Cub", Zone::Battlefield);
    let mystic = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    act(&mut state, Action::ActivateManaAbility(mystic));
    assert_eq!(
        mana(&state, P0, ManaColor::G),
        7,
        "+1 base + 2 from two Cubs"
    );

    // Opponent tapping their own creature does not trigger P0's Cubs.
    state.priority_player = P1;
    act(&mut state, Action::ActivateManaAbility(opp_elves));
    assert_eq!(mana(&state, P1, ManaColor::G), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 7);

    // 2. Automatic mana payment (`pay_plan`): tapping Llanowar Elves to pay for
    //    a {G} spell leaves the extra {G} from Badgermole Cub floating in pool!
    let mut state = plains();
    let _cub = put(&mut state, P0, "Badgermole Cub", Zone::Battlefield);
    let elves_bf = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let elves_hand = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    cast(&mut state, elves_hand, &[]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(state.objects.get(elves_bf).tapped);
    assert_eq!(
        mana(&state, P0, ManaColor::G),
        1,
        "extra {{G}} from Badgermole Cub floats after auto-payment"
    );
    settled(&mut state);
}

// ---- Batroc the Leaper ----------------------------------------------------------

#[test]
fn batroc_multikicker_entry_counters_etb_damage_and_lki() {
    // 1. Cast Batroc with 0 kicks: 2/2, 0 counters, NO target prompt.
    let mut state = plains();
    let batroc0 = put(&mut state, P0, "Batroc the Leaper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    act(&mut state, Action::CastSpell(batroc0));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2, "0 or 1 kick with {{3}}{{R}}");
        }
        other => panic!("expected multikicker choice, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(0));
    settled(&mut state);
    assert_eq!(zone(&state, batroc0), Zone::Battlefield);
    assert_eq!(state.objects.get(batroc0).counters.plus1_plus1, 0);
    assert_eq!(power_toughness(&state, batroc0), (2, 2));
    assert_eq!(life(&state, P1), 20);

    // 2. Cast Batroc kicked 2 times ({5}{R}): enters as 4/4, targets 2 distinct
    //    any-targets (P1 + Sheoldred) and deals 4 damage to each simultaneously.
    let mut state = plains();
    let sheoldred = put(
        &mut state,
        P1,
        "Sheoldred, the Apocalypse",
        Zone::Battlefield,
    );
    let batroc = put(&mut state, P0, "Batroc the Leaper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 5);
    act(&mut state, Action::CastSpell(batroc));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 3, "0, 1, or 2 kicks with {{5}}{{R}}");
        }
        other => panic!("expected multikicker choice, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(2));
    match settle(&mut state) {
        Some(Decision::ChooseTargets {
            remaining,
            legal_targets,
            can_finish,
            ..
        }) => {
            assert_eq!(remaining, 2);
            assert!(can_finish);
            assert!(legal_targets.contains(&Target::Player(P1)));
            assert!(legal_targets.contains(&Target::Object(sheoldred)));
        }
        other => panic!("expected Batroc ETB targets, got {other:?}"),
    }
    assert_eq!(state.objects.get(batroc).counters.plus1_plus1, 2);
    assert_eq!(power_toughness(&state, batroc), (4, 4));
    act(&mut state, Action::ChooseTarget(Target::Player(P1)));
    match next(&mut state) {
        Decision::ChooseTargets {
            remaining,
            legal_targets,
            can_finish,
            ..
        } => {
            assert_eq!(remaining, 1);
            assert!(can_finish);
            assert!(
                !legal_targets.contains(&Target::Player(P1)),
                "distinct targets only"
            );
            assert!(legal_targets.contains(&Target::Object(sheoldred)));
        }
        other => panic!("expected 2nd Batroc target, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(sheoldred)));
    settled(&mut state);
    assert_eq!(life(&state, P1), 16);
    assert_eq!(state.objects.get(sheoldred).damage, 4);

    // 3. Up to X targets: kicked twice, choose only 1 target then FinishEffectSelection.
    let mut state = plains();
    let batroc = put(&mut state, P0, "Batroc the Leaper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 5);
    act(&mut state, Action::CastSpell(batroc));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption { .. }
    ));
    act(&mut state, Action::ChooseEffectOption(2));
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { remaining: 2, .. })
    ));
    act(&mut state, Action::ChooseTarget(Target::Player(P1)));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseTargets {
            remaining: 1,
            can_finish: true,
            ..
        }
    ));
    act(&mut state, Action::FinishEffectSelection);
    settled(&mut state);
    assert_eq!(life(&state, P1), 16);

    // 4. LKI when Batroc leaves in response to ETB + partial target illegality:
    //    kicked twice (4/4), targets P1 and Llanowar Elves; P1 exiles Batroc
    //    with Swords and kills Elves with Bolt -> P1 STILL takes 4 LKI damage!
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let batroc = put(&mut state, P0, "Batroc the Leaper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 5);
    act(&mut state, Action::CastSpell(batroc));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption { .. }
    ));
    act(&mut state, Action::ChooseEffectOption(2));
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { .. })
    ));
    act(&mut state, Action::ChooseTarget(Target::Player(P1)));
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    act(&mut state, Action::ChooseTarget(Target::Object(elves)));
    // P0 passes to P1 with Batroc's ETB on stack.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let swords = put(&mut state, P1, "Swords to Plowshares", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W, ManaColor::R], 0);
    cast(&mut state, swords, &[Target::Object(batroc)]);
    cast(&mut state, bolt, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, batroc), Zone::Exile);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(
        life(&state, P1),
        16,
        "Batroc LKI power (4) still damages remaining legal target P1"
    );

    // 5. Torpor Orb suppresses Batroc's ETB trigger while entry counters still apply.
    let mut state = plains();
    let _orb = put(&mut state, P1, "Torpor Orb", Zone::Battlefield);
    let batroc = put(&mut state, P0, "Batroc the Leaper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 5);
    act(&mut state, Action::CastSpell(batroc));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption { .. }
    ));
    act(&mut state, Action::ChooseEffectOption(2));
    settled(&mut state);
    assert_eq!(state.objects.get(batroc).counters.plus1_plus1, 2);
    assert_eq!(power_toughness(&state, batroc), (4, 4));
    assert_eq!(life(&state, P1), 20);
}

// ---- Blightsteel Colossus -------------------------------------------------------

#[test]
fn blightsteel_colossus_infect_combat_noncombat_and_trample() {
    // 1. Unblocked attack deals 11 poison counters (0 life loss) -> immediate SBA win.
    let mut state = plains();
    let colossus = put(&mut state, P0, "Blightsteel Colossus", Zone::Battlefield);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![colossus]));
    let mut game_over = None;
    for _ in 0..100 {
        match next(&mut state) {
            Decision::GameOver { winner } => {
                game_over = Some(winner);
                break;
            }
            Decision::DeclareBlockers { .. } => act(&mut state, Action::DeclareBlockers(vec![])),
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(game_over, Some(Some(P0)));
    assert_eq!(life(&state, P1), 20, "Infect deals poison, not life loss");
    assert_eq!(state.players[P1.index()].poison_counters.0, 11);

    // 2. Trample + Infect over Sheoldred (4/5): assigns 5 -1/-1 counters to
    //    Sheoldred (dies as 0/0) and 6 poison counters to P1 (0 life loss).
    let mut state = plains();
    let colossus = put(&mut state, P0, "Blightsteel Colossus", Zone::Battlefield);
    let sheoldred = put(
        &mut state,
        P1,
        "Sheoldred, the Apocalypse",
        Zone::Battlefield,
    );
    attack_with(&mut state, vec![colossus], |_, decision| match decision {
        Decision::DeclareBlockers { .. } => {
            Some(Action::DeclareBlockers(vec![(sheoldred, colossus)]))
        }
        // Assign minimum lethal (5) to blocker so 6 tramples over to P1.
        Decision::ChooseCombatDamageRange { .. } => {
            Some(Action::ChooseCombatDamageRange { upper_half: false })
        }
        _ => None,
    });
    assert_eq!(zone(&state, sheoldred), Zone::Graveyard);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(state.players[P1.index()].poison_counters.0, 6);

    // 3. Infect vs Indestructible: Blightsteel blocking Blightsteel puts 11
    //    -1/-1 counters on each -> both reach 0 toughness and die despite
    //    Indestructible -> both shuffle into owners' libraries!
    let mut state = plains();
    let p0_col = put(&mut state, P0, "Blightsteel Colossus", Zone::Battlefield);
    let p1_col = put(&mut state, P1, "Blightsteel Colossus", Zone::Battlefield);
    attack_with(&mut state, vec![p0_col], |_, decision| match decision {
        Decision::DeclareBlockers { .. } => Some(Action::DeclareBlockers(vec![(p1_col, p0_col)])),
        _ => None,
    });
    assert_eq!(zone(&state, p0_col), Zone::Library);
    assert_eq!(zone(&state, p1_col), Zone::Library);
    assert!(state.players[P0.index()].graveyard.is_empty());
    assert!(state.players[P1.index()].graveyard.is_empty());

    // 4. Noncombat Infect damage: -1/-1 counters on creature (0 marked damage),
    //    poison counters on player (0 life loss), normal loyalty loss on PW.
    let mut state = plains();
    let colossus = put(&mut state, P0, "Blightsteel Colossus", Zone::Battlefield);
    let sheoldred = put(
        &mut state,
        P1,
        "Sheoldred, the Apocalypse",
        Zone::Battlefield,
    );
    let jace = put(&mut state, P1, "Jace, the Mind Sculptor", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(jace, Zone::Battlefield),
    );
    assert_eq!(loyalty(&state, jace), Some(3));

    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(colossus, Target::Object(sheoldred), 3),
    );
    assert_eq!(state.objects.get(sheoldred).damage, 0);
    assert_eq!(state.objects.get(sheoldred).counters.minus1_minus1, 3);
    assert_eq!(power_toughness(&state, sheoldred), (1, 2));

    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(colossus, Target::Player(P1), 4),
    );
    assert_eq!(life(&state, P1), 20);
    assert_eq!(state.players[P1.index()].poison_counters.0, 4);

    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(colossus, Target::Object(jace), 2),
    );
    assert_eq!(loyalty(&state, jace), Some(1));

    // 5. Darksteel Angel prevents -1/-1 counters; Melira caps poison at 1.
    let mut state = plains();
    let colossus = put(&mut state, P0, "Blightsteel Colossus", Zone::Battlefield);
    let _angel = put(&mut state, P1, "Darksteel Angel", Zone::Battlefield);
    let melira = put(&mut state, P1, "Melira, the Living Cure", Zone::Battlefield);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(colossus, Target::Object(melira), 2),
    );
    assert_eq!(state.objects.get(melira).damage, 0);
    assert_eq!(state.objects.get(melira).counters.minus1_minus1, 0);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(colossus, Target::Player(P1), 5),
    );
    assert_eq!(state.players[P1.index()].poison_counters.0, 1);
}

#[test]
fn blightsteel_colossus_graveyard_replacement_from_all_zones() {
    // 1. Battlefield -> Graveyard (sacrifice): shuffles into owner's library.
    let mut state = plains();
    let colossus = put(&mut state, P0, "Blightsteel Colossus", Zone::Battlefield);
    let lib_before = state.players[P0.index()].library.len();
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(colossus, Zone::Graveyard),
    );
    assert_eq!(zone(&state, colossus), Zone::Library);
    assert!(state.players[P0.index()].graveyard.is_empty());
    assert_eq!(state.players[P0.index()].library.len(), lib_before + 1);
    assert!(state.players[P0.index()].library.contains(&colossus));

    // 2. Hand -> Graveyard (discard): shuffles into owner's library.
    let mut state = plains();
    let colossus = put(&mut state, P0, "Blightsteel Colossus", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(colossus, Zone::Graveyard),
    );
    assert_eq!(zone(&state, colossus), Zone::Library);
    assert!(state.players[P0.index()].graveyard.is_empty());

    // 3. Library -> Graveyard (mill via Thought Scour): stays in library and shuffles.
    let mut state = plains();
    let colossus = put(&mut state, P0, "Blightsteel Colossus", Zone::Library);
    let scour = put(&mut state, P0, "Thought Scour", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, scour, &[Target::Player(P0)]);
    // Thought Scour's MillCards(2) asks the owner to order the 2-card mill batch
    // before committing, where Blightsteel's replacement intercepts and shuffles.
    settle_first(&mut state);
    assert!(
        zone(&state, colossus) != Zone::Graveyard,
        "milled Blightsteel never hits graveyard"
    );
    assert!(
        !state.players[P0.index()].graveyard.contains(&colossus),
        "graveyard does not contain Blightsteel"
    );

    // 4. Stack -> Graveyard (Counterspell): shuffles into owner's library.
    let mut state = plains();
    let colossus = put(&mut state, P0, "Blightsteel Colossus", Zone::Hand);
    pool(&mut state, P0, &[], 12);
    cast(&mut state, colossus, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let counter = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, counter, &[Target::Object(colossus)]);
    settled(&mut state);
    assert_eq!(zone(&state, colossus), Zone::Library);
    assert!(state.players[P0.index()].graveyard.is_empty());

    // 5. Dress Down removes Blightsteel's abilities on battlefield -> goes to graveyard.
    let mut state = plains();
    let _dress = put(&mut state, P1, "Dress Down", Zone::Battlefield);
    let colossus = put(&mut state, P0, "Blightsteel Colossus", Zone::Battlefield);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(colossus, Zone::Graveyard),
    );
    assert_eq!(
        zone(&state, colossus),
        Zone::Graveyard,
        "ability-removed Blightsteel on battlefield goes to graveyard"
    );
}

// ---- Crabomination --------------------------------------------------------------

#[test]
fn crabomination_emerge_from_artifact_cost_and_candidate_filtering() {
    // 1. Without artifacts, cannot cast for {2}{B}{B}; with MV 3 artifact
    //    (Coalition Relic), Emerge costs {2}{B}{B} and sacrifices the artifact.
    let mut state = plains();
    let crab = put(&mut state, P0, "Crabomination", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    assert!(!castable(&mut state, crab));

    let relic = put(&mut state, P0, "Coalition Relic", Zone::Battlefield);
    assert!(castable(&mut state, crab));
    cast(&mut state, crab, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, relic), Zone::Graveyard);
    assert_eq!(zone(&state, crab), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(mana(&state, P0, ManaColor::B), 0);

    // 2. MV 6 artifact (Wurmcoil Engine) hits the {B}{B} colored floor.
    let mut state = plains();
    let wurm = put(&mut state, P0, "Wurmcoil Engine", Zone::Battlefield);
    let crab = put(&mut state, P0, "Crabomination", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    assert!(castable(&mut state, crab));
    cast(&mut state, crab, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, wurm), Zone::Graveyard);
    assert_eq!(zone(&state, crab), Zone::Battlefield);

    // 3. Candidate filtering: with {2}{B}{B} and two non-mana artifacts,
    //    Mishra's Bauble (MV 0) leaves Emerge unpayable ({5}{B}{B}) while
    //    Crucible of Worlds (MV 3) makes it payable ({2}{B}{B}) -> only Crucible
    //    is offered (no dead-end abort on Mishra's Bauble, and Normal {4}{B}{B}
    //    is unaffordable).
    let mut state = plains();
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Battlefield);
    let crucible = put(&mut state, P0, "Crucible of Worlds", Zone::Battlefield);
    let crab = put(&mut state, P0, "Crabomination", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    assert!(castable(&mut state, crab));
    act(&mut state, Action::CastSpell(crab));
    settle_first(&mut state);
    assert_eq!(
        zone(&state, bauble),
        Zone::Battlefield,
        "unpayable MV 0 Bauble untouched"
    );
    assert_eq!(
        zone(&state, crucible),
        Zone::Graveyard,
        "payable MV 3 Crucible sacrificed"
    );
    assert_eq!(zone(&state, crab), Zone::Battlefield);

    // 4. Both Normal ({4}{B}{B}) and Emerge affordable -> ChooseCastMode offers both.
    let mut state = plains();
    let relic = put(&mut state, P0, "Coalition Relic", Zone::Battlefield);
    let crab = put(&mut state, P0, "Crabomination", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 4);
    act(&mut state, Action::CastSpell(crab));
    match next(&mut state) {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Normal));
            assert!(options.contains(&CastMode::Alternative));
        }
        other => panic!("expected Normal/Emerge cast mode choice, got {other:?}"),
    }
    act(&mut state, Action::ChooseCastMode(CastMode::Alternative));
    settle_first(&mut state);
    assert_eq!(zone(&state, relic), Zone::Graveyard);
    assert_eq!(zone(&state, crab), Zone::Battlefield);
    assert_eq!(
        mana(&state, P0, ManaColor::C),
        2,
        "Emerge with MV 3 Relic spent {{2}}{{B}}{{B}} leaving {{2}}"
    );
}

#[test]
fn crabomination_etb_exile_three_zones_and_optional_free_cast() {
    // 1. Exiles top of P1's library, random GY card, and random hand card;
    //    P0 casts a Sorcery (Time Walk) without paying its mana cost during
    //    resolution, leaving the other two exiled cards in exile!
    let mut state = plains();
    let lib_bolt = put(&mut state, P1, "Lightning Bolt", Zone::Library);
    let gy_brainstorm = put(&mut state, P1, "Brainstorm", Zone::Graveyard);
    let hand_walk = put(&mut state, P1, "Time Walk", Zone::Hand);
    let crab = put(&mut state, P0, "Crabomination", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 4);

    cast(&mut state, crab, &[]);
    // Crabomination resolves -> ETB targets opponent P1 -> exiles all 3 -> offers choice.
    let mut saw_cast_choice = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Player(P1)]);
            Action::ChooseTarget(Target::Player(P1))
        }
        Decision::ChooseEffectTargets {
            min_targets,
            max_targets,
            legal_targets,
            can_finish,
            ..
        } => {
            saw_cast_choice = true;
            assert_eq!(min_targets, 0);
            assert_eq!(max_targets, 1);
            assert!(can_finish);
            assert!(legal_targets.contains(&Target::Object(lib_bolt)));
            assert!(legal_targets.contains(&Target::Object(gy_brainstorm)));
            assert!(legal_targets.contains(&Target::Object(hand_walk)));
            Action::ChooseEffectTarget(Target::Object(hand_walk))
        }
        other => first_answer(other),
    });
    assert!(saw_cast_choice);
    assert_eq!(zone(&state, lib_bolt), Zone::Exile);
    assert_eq!(zone(&state, gy_brainstorm), Zone::Exile);
    assert_eq!(
        zone(&state, hand_walk),
        Zone::Graveyard,
        "cast Time Walk resolved into owner's graveyard"
    );
    assert!(
        state.extra_turn_v1.is_some(),
        "free-cast Time Walk granted P0 an extra turn"
    );

    // 2. Declining the free cast leaves all three cards in exile.
    let mut state = plains();
    let lib_bolt = put(&mut state, P1, "Lightning Bolt", Zone::Library);
    let gy_shock = put(&mut state, P1, "Shock", Zone::Graveyard);
    let hand_bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let crab = put(&mut state, P0, "Crabomination", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 4);
    cast(&mut state, crab, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, lib_bolt), Zone::Exile);
    assert_eq!(zone(&state, gy_shock), Zone::Exile);
    assert_eq!(zone(&state, hand_bolt), Zone::Exile);

    // 3. Empty GY/hand and land on top of library -> exiles land, no cast prompt.
    let mut state = plains();
    let top_plains = state.players[P1.index()].library[0];
    let crab = put(&mut state, P0, "Crabomination", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 4);
    cast(&mut state, crab, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseEffectTargets { .. } => {
            panic!("land-only exile must not offer cast prompt")
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, top_plains), Zone::Exile);
}

// ---- Fear of Missing Out --------------------------------------------------------

#[test]
fn fear_of_missing_out_etb_and_delirium_extra_combat() {
    // 1. ETB with card in hand: mandatory discard 1, then draw 1.
    let mut state = plains();
    let fodder = put(&mut state, P0, "Shock", Zone::Hand);
    let fomo = put(&mut state, P0, "Fear of Missing Out", Zone::Hand);
    let top_lib = state.players[P0.index()].library[0];
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, fomo, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, fodder), Zone::Graveyard);
    assert!(hand_contains(&state, P0, top_lib));

    // 2. ETB with empty hand: skips discard, still draws 1.
    let mut state = plains();
    let fomo = put(&mut state, P0, "Fear of Missing Out", Zone::Hand);
    let top_lib = state.players[P0.index()].library[0];
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, fomo, &[]);
    settled(&mut state);
    assert!(hand_contains(&state, P0, top_lib));

    // 3. Delirium (4 card types in P0's graveyard): first attack triggers,
    //    untaps FoMO, grants a second combat phase before Main2; FoMO attacks
    //    a second time in Combat 2 (dealing 2 + 2 = 4 total) and does NOT
    //    trigger again; after Combat 2, advances cleanly to Main2.
    let mut state = plains();
    let fomo = put(&mut state, P0, "Fear of Missing Out", Zone::Battlefield);
    // 4 distinct types in P0's graveyard: Land, Instant, Sorcery, Artifact.
    put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Thoughtseize", Zone::Graveyard);
    put(&mut state, P0, "Sol Ring", Zone::Graveyard);

    let start_turn = state.turn;
    let mut declare_attack_windows = 0usize;
    let mut trigger_target_prompts = 0usize;
    run_until(
        &mut state,
        |s| {
            s.step == Step::Main2
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
                && s.engine.pending_effect.is_none()
        },
        |_, decision| match decision {
            Decision::DeclareAttackers { eligible, .. } => {
                declare_attack_windows += 1;
                if eligible.contains(&fomo) {
                    Some(Action::DeclareAttackers(vec![fomo]))
                } else {
                    Some(Action::DeclareAttackers(vec![]))
                }
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                trigger_target_prompts += 1;
                assert!(legal_targets.contains(&Target::Object(fomo)));
                Some(Action::ChooseTarget(Target::Object(fomo)))
            }
            _ => None,
        },
    );
    assert_eq!(state.turn, start_turn);
    assert_eq!(state.step, Step::Main2);
    assert_eq!(
        declare_attack_windows, 2,
        "two combat phases in the same turn"
    );
    assert_eq!(
        trigger_target_prompts, 1,
        "FoMO triggers only on its first attack each turn"
    );
    assert_eq!(life(&state, P1), 16, "2 damage in Combat 1 + 2 in Combat 2");

    // 4. Attacking WITHOUT Delirium (only 3 types in GY) does NOT trigger and
    //    still records FoMO's first attack this turn so a later combat phase
    //    this turn cannot trigger it even if Delirium is reached later.
    let mut state = plains();
    let fomo1 = put(&mut state, P0, "Fear of Missing Out", Zone::Battlefield);
    // Only 3 card types in graveyard initially (Land, Instant, Sorcery).
    put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Thoughtseize", Zone::Graveyard);
    let mut windows = 0usize;
    run_until(
        &mut state,
        |s| s.step == Step::Main2 && s.stack.is_empty(),
        |_, decision| match decision {
            Decision::DeclareAttackers { .. } => {
                windows += 1;
                Some(Action::DeclareAttackers(vec![fomo1]))
            }
            _ => None,
        },
    );
    assert_eq!(windows, 1, "no Delirium -> no extra combat");
    assert_eq!(life(&state, P1), 18);
    assert!(
        state
            .eternal_v1
            .as_ref()
            .is_some_and(|e| e.c9a.fomo_attacked_this_turn.len() == 1),
        "first attack without Delirium still counts as first attack this turn"
    );

    // 5. Intervening-if Delirium recheck on resolution: losing Delirium in
    //    response fizzles the trigger (no untap, no extra combat).
    let mut state = plains();
    let fomo = put(&mut state, P0, "Fear of Missing Out", Zone::Battlefield);
    let gy_land = put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Thoughtseize", Zone::Graveyard);
    put(&mut state, P0, "Sol Ring", Zone::Graveyard);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![fomo]));
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    act(&mut state, Action::ChooseTarget(Target::Object(fomo)));
    // Trigger is on the stack; remove 4th card type from graveyard before it resolves.
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(gy_land, Zone::Exile));
    let mut combat_windows = 0usize;
    run_until(
        &mut state,
        |s| s.step == Step::Main2 && s.stack.is_empty(),
        |_, decision| match decision {
            Decision::DeclareAttackers { .. } => {
                combat_windows += 1;
                Some(Action::DeclareAttackers(vec![]))
            }
            _ => None,
        },
    );
    assert_eq!(combat_windows, 0, "lost Delirium -> no extra combat phase");
    assert!(
        state.objects.get(fomo).tapped,
        "fizzled trigger did not untap FoMO"
    );

    // 6. Target creature dying in response fizzles the trigger (no extra combat).
    let mut state = plains();
    let fomo = put(&mut state, P0, "Fear of Missing Out", Zone::Battlefield);
    let elf = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Thoughtseize", Zone::Graveyard);
    put(&mut state, P0, "Sol Ring", Zone::Graveyard);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![fomo]));
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    act(&mut state, Action::ChooseTarget(Target::Object(elf)));
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    act(&mut state, Action::Pass);
    cast(&mut state, bolt, &[Target::Object(elf)]);
    let mut extra_windows = 0usize;
    run_until(
        &mut state,
        |s| s.step == Step::Main2 && s.stack.is_empty(),
        |_, decision| match decision {
            Decision::DeclareAttackers { .. } => {
                extra_windows += 1;
                Some(Action::DeclareAttackers(vec![]))
            }
            _ => None,
        },
    );
    assert_eq!(extra_windows, 0, "illegal target -> no extra combat phase");
}

// ---- Public Sessions ------------------------------------------------------------

#[derive(Default)]
struct C9aEvents {
    casts: [usize; C9A_CARDS.len()],
}

impl C9aEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        if semantic["action_kind"].as_str() != Some("cast_spell") {
            return;
        }
        let Some(card) = semantic["source"]["card_db_id"]
            .as_u64()
            .and_then(|id| usize::try_from(id).ok())
            .and_then(|id| CARD_DEFS.get(id))
        else {
            return;
        };
        if let Some(index) = C9A_CARDS.iter().position(|name| *name == card.name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c9a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let four_color_tempo = deck(&[
        ("Tundra", 6),
        ("Taiga", 6),
        ("Volcanic Island", 6),
        ("Sol Ring", 4),
        ("Mox Emerald", 4),
        ("Mishra's Bauble", 4),
        ("Lotus Petal", 2),
        ("Lightning Bolt", 4),
        ("Aang, Swift Savior", 6),
        ("Badgermole Cub", 6),
        ("Batroc the Leaper", 6),
        ("Fear of Missing Out", 6),
    ]);
    let artifact_black_ramp = deck(&[
        ("Mishra's Workshop", 4),
        ("Ancient Tomb", 4),
        ("Underground Sea", 8),
        ("Sol Ring", 4),
        ("Mana Crypt", 4),
        ("Mana Vault", 4),
        ("Grim Monolith", 4),
        ("Basalt Monolith", 4),
        ("Mox Jet", 4),
        ("Mox Sapphire", 4),
        ("Mox Ruby", 4),
        ("Blightsteel Colossus", 6),
        ("Crabomination", 6),
    ]);
    let colossus_ramp = deck(&[
        ("Mishra's Workshop", 4),
        ("Ancient Tomb", 8),
        ("Sol Ring", 4),
        ("Mana Crypt", 4),
        ("Mana Vault", 4),
        ("Grim Monolith", 4),
        ("Basalt Monolith", 4),
        ("Mox Pearl", 4),
        ("Mox Sapphire", 4),
        ("Mox Jet", 4),
        ("Mox Ruby", 4),
        ("Mox Emerald", 4),
        ("Blightsteel Colossus", 8),
    ]);
    let mut events = C9aEvents::default();
    // 4 matchups x 3 seeds x 2 policies x (original + replay) = 48 games total.
    for (pair, first_seed) in [
        (
            [four_color_tempo.clone(), artifact_black_ramp.clone()],
            0x4339_6100_u64,
        ),
        (
            [artifact_black_ramp, four_color_tempo.clone()],
            0x4339_6110_u64,
        ),
        (
            [colossus_ramp.clone(), four_color_tempo.clone()],
            0x4339_6120_u64,
        ),
        ([four_color_tempo, colossus_ramp], 0x4339_6130_u64),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action);
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, casts) in C9A_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} was never cast in a public session");
    }
}

#[test]
fn fomo_delirium_extra_combat_public_session_both_seats() {
    let delirium_red = deck(&[
        ("Badlands", 10),
        ("Taiga", 8),
        ("Mishra's Bauble", 8),
        ("Lotus Petal", 6),
        ("Thoughtseize", 8),
        ("Lightning Bolt", 8),
        ("Fear of Missing Out", 12),
    ]);
    let opponent = deck(&[
        ("Tundra", 12),
        ("Sol Ring", 4),
        ("Aang, Swift Savior", 12),
        ("Badgermole Cub", 10),
        ("Batroc the Leaper", 10),
        ("Taiga", 12),
    ]);
    let mut fomo_casts = 0usize;
    for pair in [
        [delirium_red.clone(), opponent.clone()],
        [opponent, delirium_red],
    ] {
        for seed in 0x4339_6180_u64..0x4339_6182 {
            let played = play_observed(
                pair.clone(),
                None,
                seed,
                Policy::Ranked,
                &mut |_, action| {
                    let semantic = &action["semantic"];
                    if semantic["action_kind"].as_str() == Some("cast_spell") {
                        if let Some(card) = semantic["source"]["card_db_id"]
                            .as_u64()
                            .and_then(|id| usize::try_from(id).ok())
                            .and_then(|id| CARD_DEFS.get(id))
                        {
                            if card.name == "Fear of Missing Out" {
                                fomo_casts += 1;
                            }
                        }
                    }
                },
            );
            let replay = play_observed(pair.clone(), None, seed, Policy::Ranked, &mut |_, _| {});
            assert_eq!(played.transcript_sha256, replay.transcript_sha256);
        }
    }
    assert!(
        fomo_casts > 0,
        "Fear of Missing Out was never cast in delirium public sessions"
    );
}
