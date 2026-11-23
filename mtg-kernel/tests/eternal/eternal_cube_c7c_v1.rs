//! Vintage Cube wave C7c: native rules and public-session tests for the
//! evoke, linked exile/duration, persist, backup and Sanctum Lurker.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::planeswalker_v1::loyalty;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

/// Dedicated activation index used by Sanctum Lurker's granted `[+2]` loyalty
/// ability on controlled planeswalkers (`eternal_cube_c7c_v1::LURKER_GRANTED_INDEX`).
const LURKER_GRANTED_INDEX: u8 = 254;

const C7C_CARDS: [&str; 7] = [
    "Vibrance",
    "Wistfulness",
    "Tidehollow Sculler",
    "The Wondrous Wasp",
    "Woodfall Primus",
    "Sanctum Lurker",
    "Guardian Scalelord",
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
        0x4355_4245_0000 + 0x3763,
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

/// Advances a staged cast or activation until it reaches the stack and a
/// player holds priority.
fn on_the_stack(state: &mut GameState) {
    for _ in 0..100 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => {
                assert!(!state.stack.is_empty());
                return;
            }
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => act(state, first_answer(other)),
        }
    }
    panic!("item never reached the stack");
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

/// Advances through a real turn cycle to `player`'s next first main phase.
fn to_main(state: &mut GameState, player: PlayerId) {
    pass_until(state, |state| state.active_player != player);
    pass_until(state, |state| {
        state.active_player == player
            && state.step == Step::Main1
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
    });
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

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
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
fn c7c_cards_are_full() {
    for name in C7C_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c7c_printed_characteristics() {
    let vibrance = def("Vibrance");
    assert_eq!(vibrance.cost.generic, 3);
    assert_eq!(
        vibrance.cost.pips,
        &[
            Pip::Hybrid(ManaColor::R, ManaColor::G),
            Pip::Hybrid(ManaColor::R, ManaColor::G),
        ]
    );
    assert_eq!(vibrance.mana_value, 5);
    assert_eq!(vibrance.types, &[CardType::Creature]);
    assert_eq!(
        vibrance.subtypes,
        &[Subtype::Elemental, Subtype::Incarnation]
    );
    assert_eq!((vibrance.power, vibrance.toughness), (Some(4), Some(4)));
    assert!(vibrance.alt_cost.is_some(), "Vibrance has Evoke");

    let wistfulness = def("Wistfulness");
    assert_eq!(wistfulness.cost.generic, 3);
    assert_eq!(
        wistfulness.cost.pips,
        &[
            Pip::Hybrid(ManaColor::G, ManaColor::U),
            Pip::Hybrid(ManaColor::G, ManaColor::U),
        ]
    );
    assert_eq!(wistfulness.mana_value, 5);
    assert_eq!(wistfulness.types, &[CardType::Creature]);
    assert_eq!(
        wistfulness.subtypes,
        &[Subtype::Elemental, Subtype::Incarnation]
    );
    assert_eq!(
        (wistfulness.power, wistfulness.toughness),
        (Some(6), Some(5))
    );
    assert!(wistfulness.alt_cost.is_some(), "Wistfulness has Evoke");

    let sculler = def("Tidehollow Sculler");
    assert_eq!(sculler.cost.generic, 0);
    assert_eq!(
        sculler.cost.pips,
        &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::B)]
    );
    assert_eq!(sculler.mana_value, 2);
    assert_eq!(sculler.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(sculler.subtypes, &[Subtype::Zombie]);
    assert_eq!((sculler.power, sculler.toughness), (Some(2), Some(2)));

    let wasp = def("The Wondrous Wasp");
    assert_eq!(wasp.cost.generic, 1);
    assert_eq!(wasp.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert_eq!(wasp.mana_value, 2);
    assert_eq!(wasp.supertypes, &[Supertype::Legendary]);
    assert_eq!(wasp.types, &[CardType::Creature]);
    assert_eq!(wasp.subtypes, &[Subtype::Human, Subtype::Hero]);
    assert_eq!((wasp.power, wasp.toughness), (Some(2), Some(1)));
    assert!(wasp.keywords.has(Keywords::FLASH));
    assert!(wasp.keywords.has(Keywords::FLYING));

    let primus = def("Woodfall Primus");
    assert_eq!(primus.cost.generic, 5);
    assert_eq!(
        primus.cost.pips,
        &[
            Pip::Colored(ManaColor::G),
            Pip::Colored(ManaColor::G),
            Pip::Colored(ManaColor::G),
        ]
    );
    assert_eq!(primus.mana_value, 8);
    assert_eq!(primus.types, &[CardType::Creature]);
    assert_eq!(primus.subtypes, &[Subtype::Treefolk, Subtype::Shaman]);
    assert_eq!((primus.power, primus.toughness), (Some(6), Some(6)));
    assert!(primus.keywords.has(Keywords::TRAMPLE));

    let lurker = def("Sanctum Lurker");
    assert_eq!(lurker.cost.generic, 2);
    assert_eq!(lurker.cost.pips, &[Pip::Colored(ManaColor::B)]);
    assert_eq!(lurker.mana_value, 3);
    assert_eq!(lurker.types, &[CardType::Creature]);
    assert_eq!(lurker.subtypes, &[Subtype::Horror]);
    assert_eq!((lurker.power, lurker.toughness), (Some(3), Some(2)));

    let scalelord = def("Guardian Scalelord");
    assert_eq!(scalelord.cost.generic, 4);
    assert_eq!(scalelord.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert_eq!(scalelord.mana_value, 5);
    assert_eq!(scalelord.types, &[CardType::Creature]);
    assert_eq!(scalelord.subtypes, &[Subtype::Dragon]);
    assert_eq!((scalelord.power, scalelord.toughness), (Some(3), Some(4)));
    assert!(scalelord.keywords.has(Keywords::FLYING));
}

// ---- Vibrance ------------------------------------------------------------------

#[test]
fn vibrance_normal_cast_color_spent_triggers_both_or_neither_and_stays() {
    // Paying {R}{R}{G}{G}{C} normally triggers BOTH ETB abilities and stays.
    let mut state = game(&["Forest", "Plains", "Plains", "Plains", "Plains"]);
    let forest = state.players[0].library[0];
    let vibrance = put(&mut state, P0, "Vibrance", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::R, ManaColor::G, ManaColor::G],
        1,
    );
    cast(&mut state, vibrance, &[]);
    let mut dealt = false;
    let mut searched = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Normal));
            assert!(options.contains(&CastMode::Alternative));
            Action::ChooseCastMode(CastMode::Normal)
        }
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(P1)));
            dealt = true;
            Action::ChooseTarget(Target::Player(P1))
        }
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(forest)));
            searched = true;
            Action::ChooseEffectTarget(Target::Object(forest))
        }
        other => first_answer(other),
    });
    assert!(dealt, "RR spent triggers 3 damage");
    assert!(searched, "GG spent triggers land search");
    assert_eq!(zone(&state, vibrance), Zone::Battlefield);
    assert_eq!(power_toughness(&state, vibrance), (4, 4));
    assert_eq!(life(&state, P1), 17);
    assert_eq!(life(&state, P0), 22);
    assert!(hand_contains(&state, P0, forest));

    // Paying {R}{G}{C}{C}{C} normally triggers NEITHER ETB ability and stays.
    let mut state = plains();
    let vibrance = put(&mut state, P0, "Vibrance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 3);
    cast(&mut state, vibrance, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Normal),
        other => panic!("neither color threshold met, unexpected {other:?}"),
    });
    assert_eq!(zone(&state, vibrance), Zone::Battlefield);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(life(&state, P0), 20);
    assert!(state.players[0].hand.is_empty());
}

#[test]
fn vibrance_evoke_triggers_matching_color_and_sacrifices() {
    // Evoke with {R}{R}: deals 3 damage and sacrifices.
    let mut state = plains();
    let vibrance = put(&mut state, P0, "Vibrance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 0);
    cast(&mut state, vibrance, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(P1)));
            Action::ChooseTarget(Target::Player(P1))
        }
        other => panic!("unexpected choice on RR evoke: {other:?}"),
    });
    assert_eq!(zone(&state, vibrance), Zone::Graveyard);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(life(&state, P0), 20);
    assert!(state.players[0].hand.is_empty());

    // Evoke with {G}{G}: searches for a land, gains 2 life, and sacrifices.
    let mut state = game(&["Taiga", "Plains", "Plains", "Plains"]);
    let taiga = state.players[0].library[0];
    let vibrance = put(&mut state, P0, "Vibrance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 0);
    cast(&mut state, vibrance, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(taiga)));
            Action::ChooseEffectTarget(Target::Object(taiga))
        }
        other => panic!("unexpected choice on GG evoke: {other:?}"),
    });
    assert_eq!(zone(&state, vibrance), Zone::Graveyard);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(life(&state, P0), 22);
    assert!(hand_contains(&state, P0, taiga));

    // Evoke with {R}{G}: triggers only sacrifice, neither color ability.
    let mut state = plains();
    let vibrance = put(&mut state, P0, "Vibrance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
    cast(&mut state, vibrance, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        other => panic!("unexpected choice on RG evoke: {other:?}"),
    });
    assert_eq!(zone(&state, vibrance), Zone::Graveyard);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(life(&state, P0), 20);
    assert!(state.players[0].hand.is_empty());
}

#[test]
fn vibrance_treasure_mana_counts_noncast_entry_triggers_nothing_and_damage_fizzles() {
    // Treasure mana counts for the chosen color ({R}{R}).
    let mut state = plains();
    let t1 = token(&mut state, P0, "Treasure Token");
    let t2 = token(&mut state, P0, "Treasure Token");
    let vibrance = put(&mut state, P0, "Vibrance", Zone::Hand);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(t1, ManaColor::R),
    );
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(t2, ManaColor::R),
    );
    cast(&mut state, vibrance, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(P1)));
            Action::ChooseTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, vibrance), Zone::Graveyard);
    assert_eq!(life(&state, P1), 17);

    // Entering without being cast triggers nothing and is not sacrificed.
    let mut state = plains();
    let vibrance = put(&mut state, P0, "Vibrance", Zone::Graveyard);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(vibrance, Zone::Battlefield),
    );
    settled(&mut state);
    assert_eq!(zone(&state, vibrance), Zone::Battlefield);
    assert_eq!(life(&state, P0), 20);
    assert_eq!(life(&state, P1), 20);

    // Target leaving before the {R}{R} damage trigger resolves fizzles cleanly.
    let mut state = plains();
    let lions = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let vibrance = put(&mut state, P0, "Vibrance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 0);
    cast(&mut state, vibrance, &[]);
    let mut targeted = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseCastMode { .. } => {
                act(&mut state, Action::ChooseCastMode(CastMode::Alternative));
            }
            Decision::OrderTriggers { pending, .. } => {
                act(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                );
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(lions)));
                act(&mut state, Action::ChooseTarget(Target::Object(lions)));
                targeted = true;
            }
            Decision::CastSpellOrPass { .. } if targeted => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(targeted);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(lions, Zone::Exile));
    settled(&mut state);
    assert_eq!(zone(&state, lions), Zone::Exile);
    assert_eq!(zone(&state, vibrance), Zone::Graveyard);
}

// ---- Wistfulness ---------------------------------------------------------------

#[test]
fn wistfulness_normal_cast_triggers_both_and_stays() {
    let mut state = plains();
    let own_ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let wistfulness = put(&mut state, P0, "Wistfulness", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::G, ManaColor::U, ManaColor::U],
        1,
    );
    cast(&mut state, wistfulness, &[]);
    let mut targeted_exile = false;
    let mut discarded = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Normal));
            assert!(options.contains(&CastMode::Alternative));
            Action::ChooseCastMode(CastMode::Normal)
        }
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(opp_ring)));
            assert!(
                !legal_targets.contains(&Target::Object(own_ring)),
                "cannot target own artifact"
            );
            targeted_exile = true;
            Action::ChooseTarget(Target::Object(opp_ring))
        }
        Decision::Discard { count, choices, .. } => {
            assert_eq!(count, 1);
            discarded = true;
            Action::Discard(vec![choices[0]])
        }
        other => first_answer(other),
    });
    assert!(targeted_exile);
    assert!(discarded);
    assert_eq!(zone(&state, wistfulness), Zone::Battlefield);
    assert_eq!(power_toughness(&state, wistfulness), (6, 5));
    assert_eq!(zone(&state, opp_ring), Zone::Exile);
    assert_eq!(zone(&state, own_ring), Zone::Battlefield);
    // Drew 2, discarded 1 -> 1 card in hand, 1 in graveyard.
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[0].graveyard.len(), 1);
}

#[test]
fn wistfulness_evoke_branches_and_fizzle() {
    // Evoke {G}{G}: exiles opponent's enchantment and sacrifices.
    let mut state = plains();
    let seal = put(&mut state, P1, "Seal of Cleansing", Zone::Battlefield);
    let wistfulness = put(&mut state, P0, "Wistfulness", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 0);
    cast(&mut state, wistfulness, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(seal)]);
            Action::ChooseTarget(Target::Object(seal))
        }
        other => panic!("unexpected choice on GG evoke: {other:?}"),
    });
    assert_eq!(zone(&state, seal), Zone::Exile);
    assert_eq!(zone(&state, wistfulness), Zone::Graveyard);
    assert!(state.players[0].hand.is_empty());

    // Evoke {U}{U}: draws 2, discards 1, and sacrifices.
    let mut state = plains();
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let wistfulness = put(&mut state, P0, "Wistfulness", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, wistfulness, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::Discard { count, choices, .. } => {
            assert_eq!(count, 1);
            Action::Discard(vec![choices[0]])
        }
        other => panic!("unexpected choice on UU evoke: {other:?}"),
    });
    assert_eq!(zone(&state, opp_ring), Zone::Battlefield);
    assert_eq!(zone(&state, wistfulness), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1);

    // Evoke {G}{U}: triggers only sacrifice.
    let mut state = plains();
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let wistfulness = put(&mut state, P0, "Wistfulness", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 0);
    cast(&mut state, wistfulness, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        other => panic!("unexpected choice on GU evoke: {other:?}"),
    });
    assert_eq!(zone(&state, opp_ring), Zone::Battlefield);
    assert_eq!(zone(&state, wistfulness), Zone::Graveyard);
    assert!(state.players[0].hand.is_empty());

    // Target leaving before {G}{G} exile resolves fizzles cleanly.
    let mut state = plains();
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let wistfulness = put(&mut state, P0, "Wistfulness", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 0);
    cast(&mut state, wistfulness, &[]);
    let mut targeted = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseCastMode { .. } => {
                act(&mut state, Action::ChooseCastMode(CastMode::Alternative));
            }
            Decision::OrderTriggers { pending, .. } => {
                act(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                );
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(opp_ring)));
                act(&mut state, Action::ChooseTarget(Target::Object(opp_ring)));
                targeted = true;
            }
            Decision::CastSpellOrPass { .. } if targeted => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(targeted);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(opp_ring, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, opp_ring), Zone::Graveyard);
    assert_eq!(zone(&state, wistfulness), Zone::Graveyard);
}

#[test]
fn wistfulness_blue_draw_supports_dredge_replacement() {
    let mut state = plains();
    let imp1 = put(&mut state, P0, "Stinkweed Imp", Zone::Graveyard);
    let imp2 = put(&mut state, P0, "Stinkweed Imp", Zone::Graveyard);
    let wistfulness = put(&mut state, P0, "Wistfulness", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, wistfulness, &[]);
    let mut dredged = 0usize;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: true,
            ..
        } => {
            assert!(legal_targets
                .iter()
                .all(|target| *target == Target::Object(imp1) || *target == Target::Object(imp2)));
            dredged += 1;
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::Discard { count, choices, .. } => {
            assert_eq!(count, 1);
            Action::Discard(vec![choices[0]])
        }
        other => first_answer(other),
    });
    assert_eq!(dredged, 2, "both draws replaced by Dredge");
    assert_eq!(state.players[0].draws_this_turn, 0);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(zone(&state, wistfulness), Zone::Graveyard);
}

// ---- Tidehollow Sculler --------------------------------------------------------

#[test]
fn tidehollow_sculler_exiles_chosen_nonland_and_returns_on_leave() {
    let mut state = plains();
    let opp_land = put(&mut state, P1, "Plains", Zone::Hand);
    let opp_bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    let sculler = put(&mut state, P0, "Tidehollow Sculler", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::B], 0);
    cast(&mut state, sculler, &[]);
    let mut chose_nonland = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Player(P1)]);
            Action::ChooseTarget(Target::Player(P1))
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(!can_finish, "choosing a nonland card is mandatory");
            assert!(legal_targets.contains(&Target::Object(opp_bolt)));
            assert!(legal_targets.contains(&Target::Object(opp_ring)));
            assert!(
                !legal_targets.contains(&Target::Object(opp_land)),
                "lands cannot be chosen"
            );
            chose_nonland = true;
            Action::ChooseEffectTarget(Target::Object(opp_bolt))
        }
        other => first_answer(other),
    });
    assert!(chose_nonland);
    assert_eq!(zone(&state, sculler), Zone::Battlefield);
    assert_eq!(zone(&state, opp_bolt), Zone::Exile);
    assert_eq!(zone(&state, opp_ring), Zone::Hand);
    assert_eq!(zone(&state, opp_land), Zone::Hand);

    // Leaving the battlefield triggers the return of the exiled card to its owner's hand.
    let kill = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, kill, &[Target::Object(sculler)]);
    settled(&mut state);
    assert_eq!(zone(&state, sculler), Zone::Graveyard);
    assert_eq!(zone(&state, opp_bolt), Zone::Hand);
    assert!(hand_contains(&state, P1, opp_bolt));

    // When opponent's hand contains only lands, nothing is exiled.
    let mut state = plains();
    let only_land = put(&mut state, P1, "Plains", Zone::Hand);
    let sculler = put(&mut state, P0, "Tidehollow Sculler", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::B], 0);
    cast(&mut state, sculler, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, sculler), Zone::Battlefield);
    assert_eq!(zone(&state, only_land), Zone::Hand);
}

#[test]
fn tidehollow_sculler_leaving_before_etb_resolves_exiles_permanently() {
    let mut state = plains();
    let opp_land = put(&mut state, P1, "Plains", Zone::Hand);
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    let opp_bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let sculler = put(&mut state, P0, "Tidehollow Sculler", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::B], 0);
    cast(&mut state, sculler, &[]);
    // Resolve Sculler onto the battlefield and put its ETB trigger on the stack.
    let mut etb_on_stack = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert_eq!(legal_targets, vec![Target::Player(P1)]);
                act(&mut state, Action::ChooseTarget(Target::Player(P1)));
            }
            Decision::CastSpellOrPass { .. }
                if zone(&state, sculler) == Zone::Battlefield && !state.stack.is_empty() =>
            {
                etb_on_stack = true;
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(etb_on_stack);

    // P1 kills Sculler while its ETB is still on the stack: the LTB trigger
    // resolves first (finding nothing), then the ETB exiles Sol Ring permanently.
    opponent_bolts(&mut state, Target::Object(sculler));
    settle_choosing(&mut state, opp_ring);
    assert_eq!(zone(&state, sculler), Zone::Graveyard);
    assert_eq!(zone(&state, opp_ring), Zone::Exile);
    assert_eq!(zone(&state, opp_land), Zone::Hand);
    assert_eq!(zone(&state, opp_bolt), Zone::Hand);
    assert!(
        state.engine.linked_exile_records.is_empty(),
        "no live link remains after LTB already resolved"
    );
}

// ---- The Wondrous Wasp ---------------------------------------------------------

#[test]
fn the_wondrous_wasp_flash_and_removes_keywords_activated_and_triggered_abilities_until_it_leaves()
{
    // Flash: castable on opponent's turn.
    let mut state = plains();
    state.active_player = P1;
    state.priority_player = P0;
    state.step = Step::Main1;
    let wasp = put(&mut state, P0, "The Wondrous Wasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(castable(&mut state, wasp), "Wasp has Flash");

    // Static keywords (Flying, Lifelink) and activated abilities (Wild Mongrel)
    // are removed while Wasp remains on the battlefield, and immediately restored
    // when Wasp leaves.
    let mut state = plains();
    let hawk = put(&mut state, P1, "Sage of the Skies", Zone::Battlefield);
    assert!(keyword(&state, hawk, Keywords::FLYING));
    assert!(keyword(&state, hawk, Keywords::LIFELINK));
    let wasp = put(&mut state, P0, "The Wondrous Wasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, wasp, &[]);
    settle_choosing(&mut state, hawk);
    assert_eq!(zone(&state, wasp), Zone::Battlefield);
    assert!(state.objects.get(hawk).tapped, "Wasp taps target creature");
    assert!(!keyword(&state, hawk, Keywords::FLYING));
    assert!(!keyword(&state, hawk, Keywords::LIFELINK));

    // Kill Wasp -> Hawk immediately regains Flying and Lifelink.
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(wasp)]);
    settled(&mut state);
    assert_eq!(zone(&state, wasp), Zone::Graveyard);
    assert!(keyword(&state, hawk, Keywords::FLYING));
    assert!(keyword(&state, hawk, Keywords::LIFELINK));

    // Indestructible + flickering Wasp ends the old sting immediately.
    let mut state = plains();
    let angel = put(&mut state, P1, "Darksteel Angel", Zone::Battlefield);
    assert!(keyword(&state, angel, Keywords::INDESTRUCTIBLE));
    assert!(keyword(&state, angel, Keywords::FLYING));
    let wasp = put(&mut state, P0, "The Wondrous Wasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, wasp, &[]);
    settle_choosing(&mut state, angel);
    assert!(!keyword(&state, angel, Keywords::INDESTRUCTIBLE));
    assert!(!keyword(&state, angel, Keywords::FLYING));

    // Flicker Wasp with Ephemerate and decline the new ETB target (0 targets):
    // old incarnation left, so Angel immediately regains Indestructible and Flying.
    let ephemerate = put(&mut state, P0, "Ephemerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, ephemerate, &[Target::Object(wasp)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(zone(&state, wasp), Zone::Battlefield);
    assert!(keyword(&state, angel, Keywords::INDESTRUCTIBLE));
    assert!(keyword(&state, angel, Keywords::FLYING));

    // Activated ability removal (Wild Mongrel).
    let mut state = plains();
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    put(&mut state, P0, "Plains", Zone::Hand);
    assert!(activatable(&mut state, mongrel, 0));
    let wasp = put(&mut state, P0, "The Wondrous Wasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, wasp, &[]);
    settle_choosing(&mut state, mongrel);
    assert!(
        !activatable(&mut state, mongrel, 0),
        "stung creature loses activated abilities"
    );
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(wasp)]);
    settled(&mut state);
    assert!(
        activatable(&mut state, mongrel, 0),
        "activated ability returns when Wasp leaves"
    );

    // Triggered ability removal (Avatar of Burgeoning Echoes landfall).
    let mut state = plains();
    let avatar = put(
        &mut state,
        P0,
        "Avatar of Burgeoning Echoes",
        Zone::Battlefield,
    );
    let wasp = put(&mut state, P0, "The Wondrous Wasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, wasp, &[]);
    settle_choosing(&mut state, avatar);
    let land1 = put(&mut state, P0, "Plains", Zone::Hand);
    play_land(&mut state, land1);
    assert!(
        named(&state, P0, "Jace Token").is_empty(),
        "stung Avatar does not trigger Landfall"
    );
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(wasp)]);
    settled(&mut state);
    to_main(&mut state, P0);
    let land2 = put(&mut state, P0, "Plains", Zone::Hand);
    play_land(&mut state, land2);
    assert_eq!(
        named(&state, P0, "Jace Token").len(),
        1,
        "Avatar triggers Landfall again after Wasp leaves"
    );
}

#[test]
fn the_wondrous_wasp_up_to_one_zero_targets_fizzle_and_leaves_before_etb() {
    // Choosing 0 targets ("up to one") resolves without tapping or removing abilities.
    let mut state = plains();
    let hawk = put(&mut state, P1, "Sage of the Skies", Zone::Battlefield);
    let wasp = put(&mut state, P0, "The Wondrous Wasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, wasp, &[]);
    let mut saw_optional = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            can_finish: true, ..
        } => {
            saw_optional = true;
            Action::FinishEffectSelection
        }
        other => first_answer(other),
    });
    assert!(saw_optional);
    assert!(!state.objects.get(hawk).tapped);
    assert!(keyword(&state, hawk, Keywords::FLYING));

    // Target leaving before ETB resolves fizzles cleanly.
    let mut state = plains();
    let hawk = put(&mut state, P1, "Sage of the Skies", Zone::Battlefield);
    let wasp = put(&mut state, P0, "The Wondrous Wasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, wasp, &[]);
    let mut targeted = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(hawk)));
                act(&mut state, Action::ChooseTarget(Target::Object(hawk)));
                targeted = true;
            }
            Decision::CastSpellOrPass { .. } if targeted => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(targeted);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(hawk, Zone::Exile));
    settled(&mut state);
    assert_eq!(zone(&state, hawk), Zone::Exile);

    // Wasp leaving before its ETB resolves still taps the target, but does not
    // apply the ability-loss duration (CR 611.2b).
    let mut state = plains();
    let hawk = put(&mut state, P1, "Sage of the Skies", Zone::Battlefield);
    let wasp = put(&mut state, P0, "The Wondrous Wasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, wasp, &[]);
    let mut targeted = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(hawk)));
                act(&mut state, Action::ChooseTarget(Target::Object(hawk)));
                targeted = true;
            }
            Decision::CastSpellOrPass { .. } if targeted => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(targeted);
    opponent_bolts(&mut state, Target::Object(wasp));
    settled(&mut state);
    assert_eq!(zone(&state, wasp), Zone::Graveyard);
    assert!(state.objects.get(hawk).tapped, "still taps the creature");
    assert!(
        keyword(&state, hawk, Keywords::FLYING) && keyword(&state, hawk, Keywords::LIFELINK),
        "ability loss does not begin after Wasp already left"
    );
}

// ---- Woodfall Primus -----------------------------------------------------------

#[test]
fn woodfall_primus_etb_destroys_noncreature_permanent_and_excludes_creatures() {
    let mut state = plains();
    let creature = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let artifact = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let enchantment = put(&mut state, P1, "Seal of Cleansing", Zone::Battlefield);
    let land = put(&mut state, P1, "Plains", Zone::Battlefield);
    let walker = put(&mut state, P1, "Wrenn and Six", Zone::Battlefield);
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, walker, 3);
    let primus = put(&mut state, P0, "Woodfall Primus", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::G, ManaColor::G],
        5,
    );
    cast(&mut state, primus, &[]);
    let mut checked = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(!can_finish, "target noncreature permanent is mandatory");
            assert!(legal_targets.contains(&Target::Object(artifact)));
            assert!(legal_targets.contains(&Target::Object(enchantment)));
            assert!(legal_targets.contains(&Target::Object(land)));
            assert!(legal_targets.contains(&Target::Object(walker)));
            assert!(!legal_targets.contains(&Target::Object(creature)));
            assert!(!legal_targets.contains(&Target::Object(primus)));
            checked = true;
            Action::ChooseTarget(Target::Object(artifact))
        }
        other => first_answer(other),
    });
    assert!(checked);
    assert_eq!(zone(&state, primus), Zone::Battlefield);
    assert!(keyword(&state, primus, Keywords::TRAMPLE));
    assert_eq!(zone(&state, artifact), Zone::Graveyard);
    assert_eq!(zone(&state, creature), Zone::Battlefield);
}

#[test]
fn woodfall_primus_persist_returns_with_minus_one_counter_once_and_respects_other_counters() {
    // Dying with no -1/-1 counters returns under OWNER's control (even if stolen)
    // with one -1/-1 counter (5/5) and triggers ETB again.
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let primus = put(&mut state, P0, "Woodfall Primus", Zone::Battlefield);
    // Simulate opponent controlling P0's Primus before it dies.
    state.players[0].battlefield.retain(|&id| id != primus);
    state.players[1].battlefield.push(primus);
    state.objects.get_mut(primus).controller = P1;

    // Deal 6 lethal damage via two Lightning Bolts.
    let b1 = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let b2 = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 0);
    cast(&mut state, b1, &[Target::Object(primus)]);
    settled(&mut state);
    cast(&mut state, b2, &[Target::Object(primus)]);
    settle_choosing(&mut state, ring);
    assert_eq!(zone(&state, primus), Zone::Battlefield);
    assert_eq!(
        state.objects.get(primus).controller,
        P0,
        "Persist returns under owner's control"
    );
    assert_eq!(state.objects.get(primus).counters.minus1_minus1, 1);
    assert_eq!(power_toughness(&state, primus), (5, 5));
    assert_eq!(zone(&state, ring), Zone::Graveyard, "ETB triggers again");

    // Dying a second time WITH a -1/-1 counter does NOT trigger Persist.
    let dismember = put(&mut state, P0, "Dismember", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 1);
    cast(&mut state, dismember, &[Target::Object(primus)]);
    settled(&mut state);
    assert_eq!(zone(&state, primus), Zone::Graveyard);

    // Dying with a +1/+1 counter (and no -1/-1 counter) DOES trigger Persist.
    let mut state = plains();
    let primus = put(&mut state, P0, "Woodfall Primus", Zone::Battlefield);
    state.objects.get_mut(primus).counters.plus1_plus1 = 1;
    assert_eq!(power_toughness(&state, primus), (7, 7));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(primus, Zone::Graveyard),
    );
    settle_first(&mut state);
    assert_eq!(zone(&state, primus), Zone::Battlefield);
    assert_eq!(state.objects.get(primus).counters.minus1_minus1, 1);
    assert_eq!(state.objects.get(primus).counters.plus1_plus1, 0);
    assert_eq!(power_toughness(&state, primus), (5, 5));

    // CR 704.5q: adding a +1/+1 counter cancels the -1/-1 counter via SBA,
    // allowing Persist to trigger again on the next death.
    let scalelord = put(&mut state, P0, "Guardian Scalelord", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 4);
    cast(&mut state, scalelord, &[]);
    settle_choosing(&mut state, primus);
    assert_eq!(state.objects.get(primus).counters.plus1_plus1, 0);
    assert_eq!(state.objects.get(primus).counters.minus1_minus1, 0);
    assert_eq!(power_toughness(&state, primus), (6, 6));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(primus, Zone::Graveyard),
    );
    settle_first(&mut state);
    assert_eq!(zone(&state, primus), Zone::Battlefield);
    assert_eq!(state.objects.get(primus).counters.minus1_minus1, 1);
}

#[test]
fn woodfall_primus_persist_does_not_return_if_exiled_or_leaves_graveyard() {
    // Exiling Primus with Swords to Plowshares does not trigger Persist.
    let mut state = plains();
    let primus = put(&mut state, P0, "Woodfall Primus", Zone::Battlefield);
    let swords = put(&mut state, P0, "Swords to Plowshares", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, swords, &[Target::Object(primus)]);
    settled(&mut state);
    assert_eq!(zone(&state, primus), Zone::Exile);

    // Leaving the graveyard before Persist resolves prevents returning.
    let mut state = plains();
    let primus = put(&mut state, P0, "Woodfall Primus", Zone::Battlefield);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(primus, Zone::Graveyard),
    );
    on_the_stack(&mut state);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(primus, Zone::Exile));
    settled(&mut state);
    assert_eq!(zone(&state, primus), Zone::Exile);
}

// ---- Sanctum Lurker ------------------------------------------------------------

#[test]
fn sanctum_lurker_empowers_jace_keeps_zero_loyalty_planeswalkers_alive_and_grants_plus_two() {
    let mut state = plains();
    let lurker = put(&mut state, P0, "Sanctum Lurker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, lurker, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, lurker), Zone::Battlefield);
    assert!(
        !activatable(&mut state, lurker, 0),
        "Sanctum Lurker itself cannot activate the granted loyalty ability"
    );

    // ETB empowers Jace 1: creates a blue Jace Token with 1 loyalty.
    let jaces = named(&state, P0, "Jace Token");
    assert_eq!(jaces.len(), 1);
    let jace = jaces[0];
    assert_eq!(loyalty(&state, jace), Some(1));

    // Activating [-1]: Surveil 1 brings Jace Token to 0 loyalty; it survives!
    activate(&mut state, jace, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, jace), Zone::Battlefield);
    assert_eq!(loyalty(&state, jace), Some(0));
    assert!(
        !activatable(&mut state, jace, LURKER_GRANTED_INDEX),
        "cannot activate a second loyalty ability on the same turn"
    );

    // Next main phase: 0-loyalty Jace Token can activate granted [+2].
    to_main(&mut state, P0);
    assert!(activatable(&mut state, jace, LURKER_GRANTED_INDEX));
    activate(&mut state, jace, LURKER_GRANTED_INDEX, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, jace), Some(2));
    assert_eq!(life(&state, P1), 19);
    assert_eq!(life(&state, P0), 21);
    assert!(
        !activatable(&mut state, jace, 0),
        "once-per-turn loyalty rule applies after granted [+2]"
    );
    assert!(!activatable(&mut state, jace, LURKER_GRANTED_INDEX));

    // Also applies to other planeswalkers you control (Wrenn and Six), and not
    // to opponent's planeswalkers.
    let wrenn = put(&mut state, P0, "Wrenn and Six", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
    cast(&mut state, wrenn, &[]);
    settled(&mut state);
    assert!(activatable(&mut state, wrenn, LURKER_GRANTED_INDEX));
    activate(&mut state, wrenn, LURKER_GRANTED_INDEX, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, wrenn), Some(5));
    assert_eq!(life(&state, P1), 18);
    assert_eq!(life(&state, P0), 22);

    // Opponent's planeswalker does NOT get [+2] and does NOT survive 0 loyalty.
    let opp_narset = put(&mut state, P1, "Narset, Parter of Veils", Zone::Battlefield);
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, opp_narset, 2);
    to_main(&mut state, P1);
    assert!(
        !activatable(&mut state, opp_narset, LURKER_GRANTED_INDEX),
        "opponent's planeswalker does not gain Lurker's [+2]"
    );
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, opp_narset, -2);
    settled(&mut state);
    assert_eq!(zone(&state, opp_narset), Zone::Graveyard);
}

#[test]
fn sanctum_lurker_leaving_causes_zero_loyalty_planeswalker_to_die_to_sba() {
    let mut state = plains();
    let lurker1 = put(&mut state, P0, "Sanctum Lurker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, lurker1, &[]);
    settled(&mut state);
    let jace = named(&state, P0, "Jace Token")[0];
    assert_eq!(loyalty(&state, jace), Some(1));

    // A second Sanctum Lurker empowers the existing Jace Token rather than creating a second one.
    let lurker2 = put(&mut state, P0, "Sanctum Lurker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, lurker2, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Jace Token").len(), 1);
    assert_eq!(loyalty(&state, jace), Some(2));

    // Bring Jace Token to 0 loyalty (damage or loyalty loss), remove both Lurkers:
    // Jace survives while one Lurker stays, and dies immediately when the last leaves.
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, jace, -2);
    settled(&mut state);
    assert_eq!(zone(&state, jace), Zone::Battlefield);
    assert_eq!(loyalty(&state, jace), Some(0));

    let b1 = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, b1, &[Target::Object(lurker1)]);
    settled(&mut state);
    assert_eq!(zone(&state, lurker1), Zone::Graveyard);
    assert_eq!(
        zone(&state, jace),
        Zone::Battlefield,
        "second Lurker still alive"
    );

    let b2 = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, b2, &[Target::Object(lurker2)]);
    settled(&mut state);
    assert_eq!(zone(&state, lurker2), Zone::Graveyard);
    assert_eq!(
        zone(&state, jace),
        Zone::Graveyard,
        "0-loyalty Jace dies to SBA once Sanctum Lurker leaves"
    );
}

// ---- Guardian Scalelord --------------------------------------------------------

#[test]
fn guardian_scalelord_backup_self_vs_another_creature_and_end_of_turn_expiry() {
    // Backup 1 targeting itself: becomes 4/5 and has only one attack trigger.
    let mut state = plains();
    let baloth_gy = put(&mut state, P0, "Ravenous Baloth", Zone::Graveyard);
    let scalelord = put(&mut state, P0, "Guardian Scalelord", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 4);
    cast(&mut state, scalelord, &[]);
    settle_choosing(&mut state, scalelord);
    assert_eq!(power_toughness(&state, scalelord), (4, 5));
    state.objects.get_mut(scalelord).summoning_sick = false;
    let mut triggers = 0usize;
    attack_with(&mut state, vec![scalelord], |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(*legal_targets, vec![Target::Object(baloth_gy)]);
            triggers += 1;
            Some(Action::ChooseTarget(Target::Object(baloth_gy)))
        }
        _ => None,
    });
    assert_eq!(
        triggers, 1,
        "backing up itself must not duplicate attack trigger"
    );
    assert_eq!(zone(&state, baloth_gy), Zone::Battlefield);

    // Backup 1 targeting another creature (Ravenous Baloth, 4/4 -> 5/5 with Flying):
    // X is the backed-up creature's power (5), NOT Scalelord's power (3)!
    let mut state = plains();
    let baloth = put(&mut state, P0, "Ravenous Baloth", Zone::Battlefield);
    let angel_gy = put(&mut state, P0, "Arrogant Wurm", Zone::Graveyard);
    let land_gy = put(&mut state, P0, "Plains", Zone::Graveyard);
    let bolt_gy = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let scalelord = put(&mut state, P0, "Guardian Scalelord", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 4);
    cast(&mut state, scalelord, &[]);
    settle_choosing(&mut state, baloth);
    assert_eq!(power_toughness(&state, baloth), (5, 5));
    assert!(keyword(&state, baloth, Keywords::FLYING));
    assert_eq!(power_toughness(&state, scalelord), (3, 4));

    let mut returned = false;
    attack_with(&mut state, vec![baloth], |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&Target::Object(angel_gy)),
                "MV 5 <= Baloth power 5"
            );
            assert!(!legal_targets.contains(&Target::Object(land_gy)));
            assert!(!legal_targets.contains(&Target::Object(bolt_gy)));
            returned = true;
            Some(Action::ChooseTarget(Target::Object(angel_gy)))
        }
        _ => None,
    });
    assert!(returned);
    assert_eq!(zone(&state, angel_gy), Zone::Battlefield);

    // Next turn: Baloth keeps its +1/+1 counter, but loses Flying and the attack trigger.
    to_main(&mut state, P0);
    let ring_gy = put(&mut state, P0, "Sol Ring", Zone::Graveyard);
    assert_eq!(power_toughness(&state, baloth), (5, 5));
    assert!(!keyword(&state, baloth, Keywords::FLYING));
    let mut triggered_next_turn = false;
    attack_with(&mut state, vec![baloth], |_, decision| {
        if matches!(decision, Decision::ChooseTargets { .. }) {
            triggered_next_turn = true;
        }
        None
    });
    assert!(
        !triggered_next_turn,
        "granted attack trigger expires at end of turn"
    );
    assert_eq!(zone(&state, ring_gy), Zone::Graveyard);
}

#[test]
fn guardian_scalelord_reanimates_nonland_permanent_types_and_checks_power_on_resolution() {
    // Verify Artifact, Enchantment, Aura (attaches!), and Planeswalker are all
    // legal targets, while Land, Instant, and MV > power are excluded.
    let mut state = plains();
    let scalelord = put(&mut state, P0, "Guardian Scalelord", Zone::Battlefield);
    state.objects.get_mut(scalelord).counters.plus1_plus1 = 1; // 4/5
    let ring = put(&mut state, P0, "Sol Ring", Zone::Graveyard); // Artifact MV 1
    let seal = put(&mut state, P0, "Seal of Cleansing", Zone::Graveyard); // Enchantment MV 2
    let rancor = put(&mut state, P0, "Rancor", Zone::Graveyard); // Aura MV 1
    let wrenn = put(&mut state, P0, "Wrenn and Six", Zone::Graveyard); // Planeswalker MV 2
    let land = put(&mut state, P0, "Plains", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let too_big = put(&mut state, P0, "Arrogant Wurm", Zone::Graveyard); // MV 5 > power 4

    // First attack: return Wrenn and Six (enters with starting loyalty 3).
    let mut checked = false;
    attack_with(&mut state, vec![scalelord], |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(ring)));
            assert!(legal_targets.contains(&Target::Object(seal)));
            assert!(legal_targets.contains(&Target::Object(rancor)));
            assert!(legal_targets.contains(&Target::Object(wrenn)));
            assert!(!legal_targets.contains(&Target::Object(land)));
            assert!(!legal_targets.contains(&Target::Object(bolt)));
            assert!(!legal_targets.contains(&Target::Object(too_big)));
            checked = true;
            Some(Action::ChooseTarget(Target::Object(wrenn)))
        }
        _ => None,
    });
    assert!(checked);
    assert_eq!(zone(&state, wrenn), Zone::Battlefield);
    assert_eq!(loyalty(&state, wrenn), Some(3));

    // Next turn: return Rancor (Aura attaches to Scalelord: +2/+0 and Trample!).
    to_main(&mut state, P0);
    attack_with(&mut state, vec![scalelord], |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(rancor)));
            Some(Action::ChooseTarget(Target::Object(rancor)))
        }
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(scalelord)));
            Some(Action::ChooseEffectTarget(Target::Object(scalelord)))
        }
        _ => None,
    });
    assert_eq!(zone(&state, rancor), Zone::Battlefield);
    assert!(state.objects.get(scalelord).attachments.contains(&rancor));

    // Shrinking attacker before resolution fizzles the trigger when MV > new power.
    let mut state = plains();
    let scalelord = put(&mut state, P0, "Guardian Scalelord", Zone::Battlefield);
    state.objects.get_mut(scalelord).counters.plus1_plus1 = 1; // 4/5
    let baloth_gy = put(&mut state, P0, "Ravenous Baloth", Zone::Graveyard); // MV 4
    let intruder = put(&mut state, P1, "Brinebarrow Intruder", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 0);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![scalelord]));
    // Choose Ravenous Baloth for the attack trigger.
    let mut targeted = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert_eq!(legal_targets, vec![Target::Object(baloth_gy)]);
                act(&mut state, Action::ChooseTarget(Target::Object(baloth_gy)));
                targeted = true;
            }
            Decision::CastSpellOrPass { player: P0, .. } if targeted => {
                // P0 passes priority so P1 can flash in Brinebarrow Intruder.
                act(&mut state, Action::Pass);
            }
            Decision::CastSpellOrPass { player: P1, .. } if targeted => break,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(targeted);
    cast(&mut state, intruder, &[]);
    settle_choosing(&mut state, scalelord);
    assert_eq!(power_toughness(&state, scalelord), (2, 5));
    assert_eq!(
        zone(&state, baloth_gy),
        Zone::Graveyard,
        "MV 4 > shrunk power 2 fizzles trigger on resolution"
    );
}

// ---- Public Sessions -----------------------------------------------------------

#[derive(Default)]
struct C7cEvents {
    casts: [usize; C7C_CARDS.len()],
}

impl C7cEvents {
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
        if let Some(index) = C7C_CARDS.iter().position(|name| *name == card.name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c7c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let elementals = deck(&[
        ("Taiga", 10),
        ("Tropical Island", 10),
        ("Sol Ring", 6),
        ("Mox Emerald", 4),
        ("Vibrance", 6),
        ("Wistfulness", 6),
        ("Woodfall Primus", 6),
    ]);
    let esper_tempo = deck(&[
        ("Scrubland", 10),
        ("Tundra", 10),
        ("Sol Ring", 4),
        ("Tidehollow Sculler", 6),
        ("The Wondrous Wasp", 5),
        ("Sanctum Lurker", 5),
        ("Guardian Scalelord", 4),
    ]);
    let mut events = C7cEvents::default();
    // 2 seats x 3 seeds x 2 policies x (original + replay) = 24 games total.
    for pair in [
        [elementals.clone(), esper_tempo.clone()],
        [esper_tempo, elementals],
    ] {
        for seed in 0x4337_6300_u64..0x4337_6303 {
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
    for (name, casts) in C7C_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} was never cast in a public session");
    }
}
