//! Vintage Cube wave C7b: native rules and public-session tests for the
//! extended mana abilities, damage modifiers and Palace Jailer.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{
    self, Action, AnyManaTypeV1, Decision, FreeCastV1, OptionalCostChoice, PlayOrCast,
    PlayPermission, PlayPermissionExpiry,
};
use mtg_kernel::eternal_cube_c7b_v1::C7bStateV1;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::planeswalker_v1::{change_loyalty, loyalty};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C7B_CARDS: [&str; 7] = [
    "Orcish Lumberjack",
    "Urza, Lord High Artificer",
    "Vivi Ornitier",
    "Black Widow, Super Spy",
    "Soul-Scar Mage",
    "Questing Beast",
    "Palace Jailer",
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
        0x4355_4245_0000 + 0x3762,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn game_with_libraries(p0_library: &[&str], p1_library: &[&str]) -> GameState {
    let p0_ids: Vec<u16> = p0_library.iter().map(|name| id(name)).collect();
    let p1_ids: Vec<u16> = p1_library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &p0_ids,
        &p1_ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4355_4245_0000 + 0x3762,
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

fn enter(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let card = put(state, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Battlefield));
    card
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

fn is_tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
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

fn mana_abilities(state: &mut GameState) -> Vec<ObjectId> {
    match next(state) {
        Decision::CastSpellOrPass { mana_abilities, .. } => mana_abilities,
        other => panic!("expected priority, got {other:?}"),
    }
}

fn land_drops(state: &mut GameState) -> Vec<ObjectId> {
    match next(state) {
        Decision::CastSpellOrPass { land_drops, .. } => land_drops,
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

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
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

// ---- Characteristics and state contracts ---------------------------------------

#[test]
fn c7b_cards_are_full() {
    for name in C7B_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }

    let lumberjack = def("Orcish Lumberjack");
    assert_eq!(lumberjack.cost.generic, 0);
    assert_eq!(lumberjack.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(lumberjack.types, &[CardType::Creature]);
    assert_eq!(lumberjack.subtypes, &[Subtype::Orc]);
    assert_eq!((lumberjack.power, lumberjack.toughness), (Some(1), Some(1)));

    let urza = def("Urza, Lord High Artificer");
    assert_eq!(urza.cost.generic, 2);
    assert_eq!(
        urza.cost.pips,
        &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)]
    );
    assert_eq!(urza.supertypes, &[Supertype::Legendary]);
    assert_eq!(urza.subtypes, &[Subtype::Human, Subtype::Artificer]);
    assert_eq!((urza.power, urza.toughness), (Some(1), Some(4)));

    let vivi = def("Vivi Ornitier");
    assert_eq!(vivi.cost.generic, 1);
    assert_eq!(
        vivi.cost.pips,
        &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::R)]
    );
    assert_eq!(vivi.supertypes, &[Supertype::Legendary]);
    assert_eq!(vivi.subtypes, &[Subtype::Wizard]);
    assert_eq!((vivi.power, vivi.toughness), (Some(0), Some(3)));

    let widow = def("Black Widow, Super Spy");
    assert_eq!(widow.cost.generic, 1);
    assert_eq!(widow.cost.pips, &[Pip::Colored(ManaColor::B)]);
    assert_eq!(widow.supertypes, &[Supertype::Legendary]);
    assert_eq!(
        widow.subtypes,
        &[Subtype::Human, Subtype::Spy, Subtype::Hero]
    );
    assert_eq!((widow.power, widow.toughness), (Some(2), Some(1)));
    assert_eq!(widow.keywords, Keywords::MENACE);

    let mage = def("Soul-Scar Mage");
    assert_eq!(mage.cost.generic, 0);
    assert_eq!(mage.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(mage.subtypes, &[Subtype::Human, Subtype::Wizard]);
    assert_eq!((mage.power, mage.toughness), (Some(1), Some(2)));

    let beast = def("Questing Beast");
    assert_eq!(beast.cost.generic, 2);
    assert_eq!(
        beast.cost.pips,
        &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)]
    );
    assert_eq!(beast.supertypes, &[Supertype::Legendary]);
    assert_eq!(beast.subtypes, &[Subtype::Beast]);
    assert_eq!((beast.power, beast.toughness), (Some(4), Some(4)));
    assert!(beast.keywords.has(Keywords::VIGILANCE));
    assert!(beast.keywords.has(Keywords::DEATHTOUCH));
    assert!(beast.keywords.has(Keywords::HASTE));

    let jailer = def("Palace Jailer");
    assert_eq!(jailer.cost.generic, 2);
    assert_eq!(
        jailer.cost.pips,
        &[Pip::Colored(ManaColor::W), Pip::Colored(ManaColor::W)]
    );
    assert_eq!(jailer.subtypes, &[Subtype::Human, Subtype::Soldier]);
    assert_eq!((jailer.power, jailer.toughness), (Some(2), Some(2)));
}

#[test]
fn empty_c7b_state_and_any_mana_type_permission_skip_serialization_and_hash_zero_bytes() {
    let empty = C7bStateV1::default();
    assert_eq!(serde_json::to_string(&empty).unwrap(), "{}");
    let baseline = DefaultHasher::new().finish();
    let mut hasher = DefaultHasher::new();
    empty.hash(&mut hasher);
    assert_eq!(hasher.finish(), baseline);

    let mut flag_hasher = DefaultHasher::new();
    AnyManaTypeV1(false).hash(&mut flag_hasher);
    assert_eq!(flag_hasher.finish(), baseline);

    let perm = PlayPermission {
        object: ObjectId(0),
        holder: P0,
        zone_change_generation: 1,
        play_or_cast: PlayOrCast::Cast,
        expiry: PlayPermissionExpiry::EndOfTurn,
        without_mana_cost: FreeCastV1(false),
        any_mana_type: AnyManaTypeV1(false),
    };
    let json = serde_json::to_string(&perm).unwrap();
    assert!(!json.contains("any_mana_type"), "default omitted: {json}");
}

// ---- Orcish Lumberjack ---------------------------------------------------------

#[test]
fn orcish_lumberjack_adds_every_three_mana_rg_combination() {
    for (choice, expected_r, expected_g) in [
        (ManaColor::R, 3, 0),
        (ManaColor::W, 2, 1),
        (ManaColor::U, 1, 2),
        (ManaColor::G, 0, 3),
    ] {
        let mut state = plains();
        let lumberjack = put(&mut state, P0, "Orcish Lumberjack", Zone::Battlefield);
        let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
        next(&mut state);
        act(
            &mut state,
            Action::ActivateManaAbilityWithCostTarget(lumberjack, choice, forest),
        );
        assert!(is_tapped(&state, lumberjack));
        assert_eq!(zone(&state, forest), Zone::Graveyard);
        assert!(
            state.stack.is_empty(),
            "mana ability must not use the stack"
        );
        assert_eq!(mana(&state, P0, ManaColor::R), expected_r, "{choice:?}");
        assert_eq!(mana(&state, P0, ManaColor::G), expected_g, "{choice:?}");
        assert_eq!(mana(&state, P0, ManaColor::W), 0);
        assert_eq!(mana(&state, P0, ManaColor::U), 0);
        assert_eq!(mana(&state, P0, ManaColor::B), 0);
        assert_eq!(mana(&state, P0, ManaColor::C), 0);
        assert!(
            !mana_abilities(&mut state).contains(&lumberjack),
            "tapped Lumberjack cannot activate again"
        );
    }
}

#[test]
fn orcish_lumberjack_sacrifices_typed_duals_and_yavimaya_lands_and_respects_sickness() {
    let mut state = plains();
    let lumberjack = put(&mut state, P0, "Orcish Lumberjack", Zone::Battlefield);
    let mountain = put(&mut state, P0, "Mountain", Zone::Battlefield);
    assert!(
        !mana_abilities(&mut state).contains(&lumberjack),
        "needs a Forest to sacrifice"
    );
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityWithCostTarget(lumberjack, ManaColor::R, mountain),
    )
    .is_err());

    // Typed dual land with subtype Forest is a legal sacrifice, unless sick.
    let taiga = put(&mut state, P0, "Taiga", Zone::Battlefield);
    state.objects.get_mut(lumberjack).summoning_sick = true;
    assert!(
        !mana_abilities(&mut state).contains(&lumberjack),
        "tap symbol respects summoning sickness"
    );
    state.objects.get_mut(lumberjack).summoning_sick = false;
    assert!(mana_abilities(&mut state).contains(&lumberjack));

    // Floating RGG plus one floating colorless pays for Questing Beast ({2}{G}{G}).
    let beast = put(&mut state, P0, "Questing Beast", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    act(
        &mut state,
        Action::ActivateManaAbilityWithCostTarget(lumberjack, ManaColor::U, taiga),
    );
    assert_eq!(zone(&state, taiga), Zone::Graveyard);
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 2);
    cast(&mut state, beast, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, beast), Zone::Battlefield);

    // Yavimaya makes a Plains a Forest, so Lumberjack can sacrifice it too.
    let mut yavimaya_state = plains();
    let jack = put(
        &mut yavimaya_state,
        P0,
        "Orcish Lumberjack",
        Zone::Battlefield,
    );
    let plains_land = put(&mut yavimaya_state, P0, "Plains", Zone::Battlefield);
    assert!(!mana_abilities(&mut yavimaya_state).contains(&jack));
    put(
        &mut yavimaya_state,
        P0,
        "Yavimaya, Cradle of Growth",
        Zone::Battlefield,
    );
    assert!(mana_abilities(&mut yavimaya_state).contains(&jack));
    act(
        &mut yavimaya_state,
        Action::ActivateManaAbilityWithCostTarget(jack, ManaColor::W, plains_land),
    );
    assert_eq!(zone(&yavimaya_state, plains_land), Zone::Graveyard);
    assert_eq!(mana(&yavimaya_state, P0, ManaColor::R), 2);
    assert_eq!(mana(&yavimaya_state, P0, ManaColor::G), 1);
}

// ---- Urza, Lord High Artificer -------------------------------------------------

#[test]
fn urza_enters_creating_scaling_karn_construct_and_taps_artifacts_for_blue() {
    let mut state = plains();
    let urza = put(&mut state, P0, "Urza, Lord High Artificer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(&mut state, urza, &[]);
    settled(&mut state);

    assert_eq!(zone(&state, urza), Zone::Battlefield);
    assert!(state.objects.get(urza).summoning_sick);
    let constructs = named(&state, P0, "Karn Construct Token");
    assert_eq!(constructs.len(), 1);
    let construct = constructs[0];
    assert!(state.objects.get(construct).summoning_sick);
    assert_eq!(power_toughness(&state, construct), (1, 1));

    let sol_ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let seat = put(&mut state, P0, "Seat of the Synod", Zone::Battlefield);
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    assert_eq!(
        power_toughness(&state, construct),
        (3, 3),
        "Construct scales with all controlled artifacts"
    );

    // Urza has no tap symbol: works while Urza is summoning sick and tapped,
    // and can tap a summoning-sick artifact creature, mana rock or artifact land.
    state.objects.get_mut(urza).tapped = true;
    for artifact in [construct, sol_ring, seat] {
        assert!(mana_abilities(&mut state).contains(&urza));
        act(
            &mut state,
            Action::ActivateManaAbilityWithCostTarget(urza, ManaColor::U, artifact),
        );
        assert!(is_tapped(&state, artifact));
        assert!(state.stack.is_empty());
    }
    assert_eq!(mana(&state, P0, ManaColor::U), 3);
    assert!(
        !mana_abilities(&mut state).contains(&urza),
        "no untapped artifacts left to tap"
    );
    assert!(
        engine::step(
            &mut state,
            Action::ActivateManaAbilityWithCostTarget(urza, ManaColor::U, mongrel),
        )
        .is_err(),
        "non-artifact creature cannot pay Urza's mana ability"
    );
}

#[test]
fn urza_activated_ability_shuffles_exiles_top_and_plays_spell_or_land_free_until_eot() {
    // Case 1: exile a spell and cast it without paying its mana cost.
    let mut state = game_with_libraries(&["Thought-Knot Seer"], &["Plains"; 10]);
    let urza = put(
        &mut state,
        P0,
        "Urza, Lord High Artificer",
        Zone::Battlefield,
    );
    let seer = state.players[0].library[0];
    pool(&mut state, P0, &[], 5);
    activate(&mut state, urza, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, seer), Zone::Exile);
    assert!(castable(&mut state, seer), "free cast with empty mana pool");
    cast(&mut state, seer, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, seer), Zone::Battlefield);

    // Case 2: exile a land and play it using the turn's land drop.
    let mut land_state = game_with_libraries(&["Volcanic Island"], &["Plains"; 10]);
    let urza = put(
        &mut land_state,
        P0,
        "Urza, Lord High Artificer",
        Zone::Battlefield,
    );
    let volc = land_state.players[0].library[0];
    pool(&mut land_state, P0, &[], 5);
    activate(&mut land_state, urza, 0, &[]);
    settled(&mut land_state);
    assert_eq!(zone(&land_state, volc), Zone::Exile);
    assert!(land_drops(&mut land_state).contains(&volc));
    act(&mut land_state, Action::PlayLand(volc));
    settled(&mut land_state);
    assert_eq!(zone(&land_state, volc), Zone::Battlefield);

    // Case 3: permission expires at end of turn.
    let mut expiry_state = game_with_libraries(&["Wild Mongrel"], &["Plains"; 10]);
    let urza = put(
        &mut expiry_state,
        P0,
        "Urza, Lord High Artificer",
        Zone::Battlefield,
    );
    let mongrel = expiry_state.players[0].library[0];
    pool(&mut expiry_state, P0, &[], 5);
    activate(&mut expiry_state, urza, 0, &[]);
    settled(&mut expiry_state);
    assert!(castable(&mut expiry_state, mongrel));
    pass_until(&mut expiry_state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_eq!(zone(&expiry_state, mongrel), Zone::Exile);
    assert!(expiry_state.engine.exile_play_permissions.is_empty());

    // Case 4: empty library resolves cleanly as a no-op.
    let mut empty_state = game_with_libraries(&[], &["Plains"; 10]);
    let urza = put(
        &mut empty_state,
        P0,
        "Urza, Lord High Artificer",
        Zone::Battlefield,
    );
    pool(&mut empty_state, P0, &[], 5);
    activate(&mut empty_state, urza, 0, &[]);
    settled(&mut empty_state);
    assert!(empty_state.exile.is_empty());
}

// ---- Vivi Ornitier -------------------------------------------------------------

#[test]
fn vivi_noncreature_trigger_grows_vivi_and_damages_opponent() {
    let mut state = plains();
    let vivi = put(&mut state, P0, "Vivi Ornitier", Zone::Battlefield);
    assert_eq!(power_toughness(&state, vivi), (0, 3));

    // Noncreature spell puts a +1/+1 counter on Vivi and deals 1 to opponent.
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, vivi), (1, 4));
    assert_eq!(life(&state, P1), 16);

    // Creature spell does NOT trigger Vivi.
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, mongrel, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, vivi), (1, 4));
    assert_eq!(life(&state, P1), 16);

    // If Vivi leaves the battlefield before the trigger resolves, the trigger
    // still deals 1 damage to the opponent.
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, ritual, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let swords = put(&mut state, P1, "Swords to Plowshares", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, swords, &[Target::Object(vivi)]);
    settled(&mut state);
    assert_eq!(zone(&state, vivi), Zone::Exile);
    assert_eq!(life(&state, P1), 15);
}

#[test]
fn vivi_mana_ability_offers_every_ur_split_and_enforces_own_turn_and_once_per_turn() {
    // At power 0: activating adds 0 mana immediately (no choice) and spends the
    // once-per-turn activation.
    let mut zero_state = plains();
    let vivi = put(&mut zero_state, P0, "Vivi Ornitier", Zone::Battlefield);
    assert!(mana_abilities(&mut zero_state).contains(&vivi));
    act(&mut zero_state, Action::ActivateManaAbility(vivi));
    assert!(matches!(
        next(&mut zero_state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    assert_eq!(mana(&zero_state, P0, ManaColor::U), 0);
    assert_eq!(mana(&zero_state, P0, ManaColor::R), 0);
    assert!(
        !mana_abilities(&mut zero_state).contains(&vivi),
        "only once each turn"
    );

    // At power 3: offers 4 splits (UUU, UUR, URR, RRR) without using the stack.
    for (option, expected_u, expected_r) in [(0_u16, 3, 0), (1, 2, 1), (2, 1, 2), (3, 0, 3)] {
        let mut state = plains();
        let vivi = put(&mut state, P0, "Vivi Ornitier", Zone::Battlefield);
        state.objects.get_mut(vivi).counters.plus1_plus1 = 3;
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(vivi));
        assert!(state.stack.is_empty(), "mana ability never uses the stack");
        assert_eq!(
            next(&mut state),
            Decision::ChooseEffectOption {
                player: P0,
                source: vivi,
                option_count: 4,
            }
        );
        act(&mut state, Action::ChooseEffectOption(option));
        assert!(matches!(
            next(&mut state),
            Decision::CastSpellOrPass { player: P0, .. }
        ));
        assert_eq!(
            mana(&state, P0, ManaColor::U),
            expected_u,
            "option {option}"
        );
        assert_eq!(
            mana(&state, P0, ManaColor::R),
            expected_r,
            "option {option}"
        );
        assert!(!mana_abilities(&mut state).contains(&vivi));
        assert!(engine::step(&mut state, Action::ActivateManaAbility(vivi)).is_err());

        // Check that the floated mana immediately pays for spells (option 1 = UUR).
        if option == 1 {
            let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
            cast(&mut state, bolt, &[Target::Player(P1)]);
            settled(&mut state);
            assert_eq!(mana(&state, P0, ManaColor::U), 2);
            assert_eq!(mana(&state, P0, ManaColor::R), 0);
            assert_eq!(life(&state, P1), 16);
        }
    }

    // Cannot activate during the opponent's turn.
    let mut opp_turn = plains();
    let vivi = put(&mut opp_turn, P0, "Vivi Ornitier", Zone::Battlefield);
    state_add_counters(&mut opp_turn, vivi, 2);
    pass_until(&mut opp_turn, |s| {
        s.active_player == P1 && s.step == Step::Main1 && s.priority_player == P0
    });
    assert!(!mana_abilities(&mut opp_turn).contains(&vivi));
    assert!(engine::step(&mut opp_turn, Action::ActivateManaAbility(vivi)).is_err());
}

fn state_add_counters(state: &mut GameState, object: ObjectId, count: i32) {
    state.objects.get_mut(object).counters.plus1_plus1 += count;
}

// ---- Black Widow, Super Spy ----------------------------------------------------

#[test]
fn black_widow_menace_blocks_solo_blocker_and_allows_pair() {
    let mut state = plains();
    let widow = put(&mut state, P0, "Black Widow, Super Spy", Zone::Battlefield);
    let b1 = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let b2 = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    assert!(keyword(&state, widow, Keywords::MENACE));
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![widow]));
    pass_until(&mut state, |s| s.step == Step::DeclareBlockers);
    assert!(matches!(next(&mut state), Decision::DeclareBlockers { .. }));
    assert!(
        engine::step(&mut state, Action::DeclareBlockers(vec![(b1, widow)])).is_err(),
        "Menace rejects a single blocker"
    );
    act(
        &mut state,
        Action::DeclareBlockers(vec![(b1, widow), (b2, widow)]),
    );
}

#[test]
fn black_widow_combat_hit_exiles_until_nonland_and_offers_counter_or_any_mana_cast() {
    // Option 0: put a +1/+1 counter on Black Widow; exiled card is NOT castable.
    let mut counter_state = game_with_libraries(
        &["Plains"; 10],
        &["Plains", "Forest", "Wild Mongrel", "Mountain"],
    );
    let widow = put(
        &mut counter_state,
        P0,
        "Black Widow, Super Spy",
        Zone::Battlefield,
    );
    let top_plains = counter_state.players[1].library[0];
    let top_forest = counter_state.players[1].library[1];
    let mongrel = counter_state.players[1].library[2];
    let rest_mountain = counter_state.players[1].library[3];
    attack_with(&mut counter_state, vec![widow], |_, decision| {
        matches!(
            decision,
            Decision::ChooseEffectOption {
                option_count: 2,
                ..
            }
        )
        .then_some(Action::ChooseEffectOption(0))
    });
    assert_eq!(zone(&counter_state, top_plains), Zone::Exile);
    assert_eq!(zone(&counter_state, top_forest), Zone::Exile);
    assert_eq!(zone(&counter_state, mongrel), Zone::Exile);
    assert_eq!(zone(&counter_state, rest_mountain), Zone::Library);
    assert_eq!(power_toughness(&counter_state, widow), (3, 2));
    pool(&mut counter_state, P0, &[ManaColor::G], 1);
    assert!(
        !castable(&mut counter_state, mongrel),
        "choosing the counter does not grant cast permission"
    );

    // Option 1: decline counter -> cast exiled nonland using mana of any type
    // (paying {1}{G} using {B}{C}, with no green mana at all).
    let mut cast_state =
        game_with_libraries(&["Plains"; 10], &["Plains", "Wild Mongrel", "Mountain"]);
    let widow = put(
        &mut cast_state,
        P0,
        "Black Widow, Super Spy",
        Zone::Battlefield,
    );
    let mongrel = cast_state.players[1].library[1];
    attack_with(&mut cast_state, vec![widow], |_, decision| {
        matches!(
            decision,
            Decision::ChooseEffectOption {
                option_count: 2,
                ..
            }
        )
        .then_some(Action::ChooseEffectOption(1))
    });
    assert_eq!(power_toughness(&cast_state, widow), (2, 1));
    assert_eq!(zone(&cast_state, mongrel), Zone::Exile);
    pool(&mut cast_state, P0, &[ManaColor::B], 1);
    assert!(
        castable(&mut cast_state, mongrel),
        "colored pip {{G}} can be paid with {{B}} or {{C}}"
    );
    cast(&mut cast_state, mongrel, &[]);
    settled(&mut cast_state);
    assert_eq!(zone(&cast_state, mongrel), Zone::Battlefield);
    assert_eq!(cast_state.objects.get(mongrel).controller, P0);

    // Colorless {C} pip (Thought-Knot Seer {3}{C}) can also be paid with
    // colored mana ({B}{B}{B}{B}) under "mana of any type".
    let mut eldrazi_state =
        game_with_libraries(&["Plains"; 10], &["Thought-Knot Seer", "Mountain"]);
    let widow = put(
        &mut eldrazi_state,
        P0,
        "Black Widow, Super Spy",
        Zone::Battlefield,
    );
    let seer = eldrazi_state.players[1].library[0];
    attack_with(&mut eldrazi_state, vec![widow], |_, decision| {
        matches!(
            decision,
            Decision::ChooseEffectOption {
                option_count: 2,
                ..
            }
        )
        .then_some(Action::ChooseEffectOption(1))
    });
    pool(
        &mut eldrazi_state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::B, ManaColor::B],
        0,
    );
    assert!(
        castable(&mut eldrazi_state, seer),
        "{{C}} pip can be paid with colored mana"
    );
    cast(&mut eldrazi_state, seer, &[]);
    settle_first(&mut eldrazi_state);
    assert_eq!(zone(&eldrazi_state, seer), Zone::Battlefield);
    assert_eq!(eldrazi_state.objects.get(seer).controller, P0);
}

#[test]
fn black_widow_all_lands_library_expiry_and_departed_source_edge_cases() {
    // All-land opponent library: exiles all cards, still offers +1/+1 counter.
    let mut lands_only = game_with_libraries(&["Plains"; 10], &["Plains", "Forest"]);
    let widow = put(
        &mut lands_only,
        P0,
        "Black Widow, Super Spy",
        Zone::Battlefield,
    );
    let l0 = lands_only.players[1].library[0];
    let l1 = lands_only.players[1].library[1];
    attack_with(&mut lands_only, vec![widow], |_, decision| {
        matches!(
            decision,
            Decision::ChooseEffectOption {
                option_count: 2,
                ..
            }
        )
        .then_some(Action::ChooseEffectOption(1))
    });
    assert_eq!(zone(&lands_only, l0), Zone::Exile);
    assert_eq!(zone(&lands_only, l1), Zone::Exile);
    assert!(lands_only.players[1].library.is_empty());
    assert!(lands_only.engine.exile_play_permissions.is_empty());

    // Departed Black Widow before trigger resolves: cannot put counter on it,
    // so the cast permission is granted directly without a dead counter option.
    let mut departed = game_with_libraries(&["Plains"; 10], &["Wild Mongrel", "Mountain"]);
    let widow = put(
        &mut departed,
        P0,
        "Black Widow, Super Spy",
        Zone::Battlefield,
    );
    let mongrel = departed.players[1].library[0];
    let swords = put(&mut departed, P1, "Swords to Plowshares", Zone::Hand);
    put(&mut departed, P1, "Plains", Zone::Battlefield);
    let mut killed = false;
    attack_with(&mut departed, vec![widow], |state, decision| {
        if !killed
            && !state.stack.is_empty()
            && matches!(decision, Decision::CastSpellOrPass { player: P1, .. })
        {
            killed = true;
            return Some(Action::CastSpell(swords));
        }
        if matches!(decision, Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(widow)))
        {
            return Some(Action::ChooseTarget(Target::Object(widow)));
        }
        None
    });
    assert!(killed);
    assert_eq!(zone(&departed, widow), Zone::Exile);
    assert_eq!(zone(&departed, mongrel), Zone::Exile);
    pool(&mut departed, P0, &[ManaColor::B], 1);
    assert!(
        castable(&mut departed, mongrel),
        "permission granted directly when Black Widow already left"
    );

    // Passing to next turn expires the exile cast permission.
    pass_until(&mut departed, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert!(departed.engine.exile_play_permissions.is_empty());
}

// ---- Soul-Scar Mage ------------------------------------------------------------

#[test]
fn soul_scar_mage_prowess_and_noncombat_damage_replacement_rules() {
    let mut state = plains();
    let mage = put(&mut state, P0, "Soul-Scar Mage", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(mongrel)]);
    settled(&mut state);
    assert_eq!(
        power_toughness(&state, mage),
        (2, 3),
        "Prowess buffs Soul-Scar Mage"
    );
    assert_eq!(zone(&state, mongrel), Zone::Graveyard);

    // Indestructible creature (Kaldra Compleat's 5/5 Germ) dies to 0 toughness
    // from two Lightning Bolts converted into six -1/-1 counters.
    let mut kaldra_state = plains();
    put(&mut kaldra_state, P0, "Soul-Scar Mage", Zone::Battlefield);
    let kaldra = put(&mut kaldra_state, P1, "Kaldra Compleat", Zone::Hand);
    kaldra_state.active_player = P1;
    kaldra_state.priority_player = P1;
    pool(&mut kaldra_state, P1, &[], 7);
    cast(&mut kaldra_state, kaldra, &[]);
    settled(&mut kaldra_state);
    let germ = named(&kaldra_state, P1, "Phyrexian Germ Token")[0];
    assert!(keyword(&kaldra_state, germ, Keywords::INDESTRUCTIBLE));
    assert_eq!(power_toughness(&kaldra_state, germ), (5, 5));
    pass_until(&mut kaldra_state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    for _ in 0..2 {
        let bolt = put(&mut kaldra_state, P0, "Lightning Bolt", Zone::Hand);
        pool(&mut kaldra_state, P0, &[ManaColor::R], 0);
        cast(&mut kaldra_state, bolt, &[Target::Object(germ)]);
        settled(&mut kaldra_state);
    }
    assert_ne!(
        zone(&kaldra_state, germ),
        Zone::Battlefield,
        "-1/-1 counters kill an indestructible creature via 0 toughness"
    );

    // Screaming Nemesis dies to three -1/-1 counters WITHOUT triggering its
    // "whenever this creature is dealt damage" trigger.
    let mut nemesis_state = plains();
    put(&mut nemesis_state, P0, "Soul-Scar Mage", Zone::Battlefield);
    let nemesis = put(
        &mut nemesis_state,
        P1,
        "Screaming Nemesis",
        Zone::Battlefield,
    );
    let bolt = put(&mut nemesis_state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut nemesis_state, P0, &[ManaColor::R], 0);
    cast(&mut nemesis_state, bolt, &[Target::Object(nemesis)]);
    settled(&mut nemesis_state);
    assert_eq!(zone(&nemesis_state, nemesis), Zone::Graveyard);
    assert_eq!(
        life(&nemesis_state, P0),
        20,
        "no damage was dealt, so Screaming Nemesis never triggered"
    );

    // Departed source (Mogg Fanatic sacrificing itself) still converts to -1/-1 counters.
    let mut fanatic_state = plains();
    put(&mut fanatic_state, P0, "Soul-Scar Mage", Zone::Battlefield);
    let fanatic = put(&mut fanatic_state, P0, "Mogg Fanatic", Zone::Battlefield);
    let victim = put(&mut fanatic_state, P1, "Wild Mongrel", Zone::Battlefield);
    activate(&mut fanatic_state, fanatic, 0, &[Target::Object(victim)]);
    settled(&mut fanatic_state);
    assert_eq!(fanatic_state.objects.get(victim).counters.minus1_minus1, 1);
    assert_eq!(fanatic_state.objects.get(victim).damage, 0);
    assert_eq!(power_toughness(&fanatic_state, victim), (1, 1));

    // When opponent controls Darksteel Angel ("Creatures you control can't have
    // -1/-1 counters put on them"), Soul-Scar Mage still replaces the damage
    // (614.6): 0 damage marked and 0 -1/-1 counters placed.
    let mut angel_state = plains();
    put(&mut angel_state, P0, "Soul-Scar Mage", Zone::Battlefield);
    put(&mut angel_state, P1, "Darksteel Angel", Zone::Battlefield);
    let victim = put(&mut angel_state, P1, "Wild Mongrel", Zone::Battlefield);
    let bolt = put(&mut angel_state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut angel_state, P0, &[ManaColor::R], 0);
    cast(&mut angel_state, bolt, &[Target::Object(victim)]);
    settled(&mut angel_state);
    assert_eq!(zone(&angel_state, victim), Zone::Battlefield);
    assert_eq!(angel_state.objects.get(victim).damage, 0);
    assert_eq!(angel_state.objects.get(victim).counters.minus1_minus1, 0);
}

#[test]
fn soul_scar_mage_leaves_combat_player_planeswalker_and_own_creature_damage_alone() {
    // Combat damage to opponent's creature is NOT replaced with -1/-1 counters.
    let mut combat_state = plains();
    let mage = put(&mut combat_state, P0, "Soul-Scar Mage", Zone::Battlefield);
    let blocker = put(&mut combat_state, P1, "Wild Mongrel", Zone::Battlefield);
    attack_with(&mut combat_state, vec![mage], |_, decision| {
        matches!(decision, Decision::DeclareBlockers { .. })
            .then_some(Action::DeclareBlockers(vec![(blocker, mage)]))
    });
    assert_eq!(combat_state.objects.get(blocker).damage, 1);
    assert_eq!(combat_state.objects.get(blocker).counters.minus1_minus1, 0);

    // Noncombat damage to opponent player, opponent planeswalker, and own
    // creature is unaffected; removing abilities with Dress Down also stops it.
    let mut state = plains();
    put(&mut state, P0, "Soul-Scar Mage", Zone::Battlefield);
    let jace = enter(&mut state, P1, "Jace, the Mind Sculptor");
    assert_eq!(loyalty(&state, jace), Some(3));
    let own_creature = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let opp_creature = put(&mut state, P1, "Palace Sentinels", Zone::Battlefield);

    // Player
    let b1 = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, b1, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);

    // Planeswalker
    let b2 = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, b2, &[Target::Object(jace)]);
    settled(&mut state);
    assert_eq!(zone(&state, jace), Zone::Graveyard);

    // Own creature (1 damage via Mogg Fanatic -> marked damage, 0 -1/-1 counters)
    let fanatic = put(&mut state, P0, "Mogg Fanatic", Zone::Battlefield);
    activate(&mut state, fanatic, 0, &[Target::Object(own_creature)]);
    settled(&mut state);
    assert_eq!(state.objects.get(own_creature).damage, 1);
    assert_eq!(state.objects.get(own_creature).counters.minus1_minus1, 0);

    // Dress Down removes Soul-Scar Mage's static ability: damage is marked normally.
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress, &[]);
    settled(&mut state);
    let b3 = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, b3, &[Target::Object(opp_creature)]);
    settled(&mut state);
    assert_eq!(zone(&state, opp_creature), Zone::Battlefield);
    assert_eq!(state.objects.get(opp_creature).damage, 3);
    assert_eq!(state.objects.get(opp_creature).counters.minus1_minus1, 0);
}

// ---- Questing Beast ------------------------------------------------------------

#[test]
fn questing_beast_keywords_evasion_and_unpreventable_combat_damage() {
    // Evasion: power <= 2 cannot block; power >= 3 can block.
    let mut state = plains();
    let beast = put(&mut state, P0, "Questing Beast", Zone::Battlefield);
    let small = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let big = put(
        &mut state,
        P1,
        "Okoye, Mighty and Adored",
        Zone::Battlefield,
    );
    assert!(keyword(&state, beast, Keywords::VIGILANCE));
    assert!(keyword(&state, beast, Keywords::DEATHTOUCH));
    assert!(keyword(&state, beast, Keywords::HASTE));
    let blockers = legal_blockers(&mut state, beast);
    assert!(!blockers.contains(&small), "power 2 cannot block");
    assert!(blockers.contains(&big), "power 3 can block");

    // Pumping power 2 to power 3 makes it a legal blocker.
    let mut pumped_state = plains();
    let beast = put(&mut pumped_state, P0, "Questing Beast", Zone::Battlefield);
    let mongrel = put(&mut pumped_state, P1, "Wild Mongrel", Zone::Battlefield);
    state_add_counters(&mut pumped_state, mongrel, 1);
    assert!(legal_blockers(&mut pumped_state, beast).contains(&mongrel));

    // Unpreventable combat damage beats Moment's Peace for ALL creatures P0
    // controls, while P0's own Moment's Peace still prevents P1's combat damage.
    let mut fog_state = plains();
    let beast = put(&mut fog_state, P0, "Questing Beast", Zone::Battlefield);
    let ally = put(&mut fog_state, P0, "Wild Mongrel", Zone::Battlefield);
    let opp_attacker = put(&mut fog_state, P1, "Wild Mongrel", Zone::Battlefield);
    let opp_peace = put(&mut fog_state, P1, "Moment's Peace", Zone::Hand);
    put(&mut fog_state, P1, "Forest", Zone::Battlefield);
    put(&mut fog_state, P1, "Plains", Zone::Battlefield);
    let mut fog_cast = false;
    attack_with(&mut fog_state, vec![beast, ally], |_, decision| {
        if !fog_cast && matches!(decision, Decision::CastSpellOrPass { player: P1, .. }) {
            fog_cast = true;
            return Some(Action::CastSpell(opp_peace));
        }
        None
    });
    assert!(fog_cast);
    assert!(
        !is_tapped(&fog_state, beast),
        "Vigilance keeps Beast untapped"
    );
    assert_eq!(
        life(&fog_state, P1),
        14,
        "both Questing Beast (4) and ally (2) deal damage through Moment's Peace"
    );

    // On P1's turn, P0 casts Moment's Peace -> P1's attacker IS prevented.
    put(&mut fog_state, P0, "Forest", Zone::Battlefield);
    put(&mut fog_state, P0, "Plains", Zone::Battlefield);
    pass_until(&mut fog_state, |s| {
        s.active_player == P1 && s.step == Step::DeclareAttackers
    });
    act(&mut fog_state, Action::DeclareAttackers(vec![opp_attacker]));
    let own_peace = put(&mut fog_state, P0, "Moment's Peace", Zone::Hand);
    let mut own_fog_cast = false;
    run_until(
        &mut fog_state,
        |s| s.active_player == P1 && s.step == Step::Main2,
        |_, decision| {
            if !own_fog_cast && matches!(decision, Decision::CastSpellOrPass { player: P0, .. }) {
                own_fog_cast = true;
                return Some(Action::CastSpell(own_peace));
            }
            None
        },
    );
    assert!(own_fog_cast);
    assert_eq!(
        life(&fog_state, P0),
        20,
        "opponent's combat damage is still prevented"
    );

    // Beats Maze of Ith's combat damage prevention (attacker untaps, still deals 4).
    let mut maze_state = plains();
    let beast = put(&mut maze_state, P0, "Questing Beast", Zone::Battlefield);
    let maze = put(&mut maze_state, P1, "Maze of Ith", Zone::Battlefield);
    let mut maze_used = false;
    attack_with(&mut maze_state, vec![beast], |_, decision| {
        if !maze_used && matches!(decision, Decision::CastSpellOrPass { player: P1, .. }) {
            maze_used = true;
            return Some(Action::ActivateAbility(maze, 0));
        }
        if matches!(decision, Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(beast)))
        {
            return Some(Action::ChooseTarget(Target::Object(beast)));
        }
        None
    });
    assert!(maze_used);
    assert_eq!(
        life(&maze_state, P1),
        16,
        "Maze of Ith cannot prevent Questing Beast's combat damage"
    );

    // Beats The One Ring's protection from everything.
    let mut ring_state = plains();
    ring_state.active_player = P1;
    ring_state.priority_player = P1;
    let ring = put(&mut ring_state, P1, "The One Ring", Zone::Hand);
    pool(&mut ring_state, P1, &[], 4);
    cast(&mut ring_state, ring, &[]);
    settled(&mut ring_state);
    pass_until(&mut ring_state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    let beast = put(&mut ring_state, P0, "Questing Beast", Zone::Battlefield);
    attack_with(&mut ring_state, vec![beast], |_, _| None);
    assert_eq!(
        life(&ring_state, P1),
        16,
        "Questing Beast deals combat damage through The One Ring's protection"
    );
}

#[test]
fn questing_beast_combat_damage_triggers_matching_planeswalker_damage() {
    let mut state = plains();
    let beast = put(&mut state, P0, "Questing Beast", Zone::Battlefield);
    let teferi = enter(&mut state, P1, "Teferi, Hero of Dominaria");
    change_loyalty(&mut state, teferi, 2);
    assert_eq!(loyalty(&state, teferi), Some(6));

    let mut targeted_teferi = false;
    attack_with(&mut state, vec![beast], |_, decision| {
        if let Decision::ChooseTargets { legal_targets, .. } = decision {
            assert_eq!(legal_targets, &vec![Target::Object(teferi)]);
            targeted_teferi = true;
            return Some(Action::ChooseTarget(Target::Object(teferi)));
        }
        None
    });
    assert!(targeted_teferi);
    assert_eq!(life(&state, P1), 16);
    assert_eq!(
        loyalty(&state, teferi),
        Some(2),
        "Questing Beast deals 4 damage to target planeswalker opponent controls"
    );

    // If opponent controls no planeswalker, no trigger goes on the stack.
    let mut no_pw = plains();
    let beast = put(&mut no_pw, P0, "Questing Beast", Zone::Battlefield);
    attack_with(&mut no_pw, vec![beast], |_, decision| {
        assert!(
            !matches!(decision, Decision::ChooseTargets { .. }),
            "must not prompt for a planeswalker target when opponent has none"
        );
        None
    });
    assert_eq!(life(&no_pw, P1), 16);

    // If the targeted planeswalker leaves before resolution, the trigger fizzles.
    let mut fizzle = plains();
    let beast = put(&mut fizzle, P0, "Questing Beast", Zone::Battlefield);
    let jace = enter(&mut fizzle, P1, "Jace, the Mind Sculptor");
    pass_until(&mut fizzle, |s| s.step == Step::DeclareAttackers);
    act(&mut fizzle, Action::DeclareAttackers(vec![beast]));
    run_until(
        &mut fizzle,
        |s| !s.stack.is_empty(),
        |_, decision| {
            if let Decision::ChooseTargets { legal_targets, .. } = decision {
                assert_eq!(legal_targets, &vec![Target::Object(jace)]);
                return Some(Action::ChooseTarget(Target::Object(jace)));
            }
            None
        },
    );
    event::propose_and_commit(&mut fizzle, ProposedEvent::zone_change(jace, Zone::Exile));
    settled(&mut fizzle);
    assert_eq!(zone(&fizzle, jace), Zone::Exile);
}

// ---- Palace Jailer -------------------------------------------------------------

#[test]
fn palace_jailer_etb_monarch_and_exile_persists_after_jailer_leaves_until_opponent_becomes_monarch()
{
    let mut state = plains();
    let victim = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let opp_attacker = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let jailer = put(&mut state, P0, "Palace Jailer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, jailer, &[]);
    let mut saw_two_triggers = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::OrderTriggers { pending, .. } => {
            assert_eq!(pending.len(), 2);
            saw_two_triggers = true;
            Action::OrderTriggers(vec![0, 1])
        }
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(victim)));
            Action::ChooseTarget(Target::Object(victim))
        }
        other => first_answer(other),
    });
    assert!(saw_two_triggers);
    assert_eq!(state.monarch, Some(P0));
    assert_eq!(zone(&state, victim), Zone::Exile);

    // Removing Palace Jailer does NOT release the exiled creature (CR 610.3).
    state.priority_player = P1;
    let swords = put(&mut state, P1, "Swords to Plowshares", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, swords, &[Target::Object(jailer)]);
    settled(&mut state);
    assert_eq!(zone(&state, jailer), Zone::Exile);
    assert_eq!(
        zone(&state, victim),
        Zone::Exile,
        "exile lasts until an opponent becomes the monarch, even after Jailer leaves"
    );

    // P0 becoming monarch again does NOT release the exiled creature.
    let sentinels = put(&mut state, P0, "Palace Sentinels", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 3);
    cast(&mut state, sentinels, &[]);
    settled(&mut state);
    assert_eq!(state.monarch, Some(P0));
    assert_eq!(zone(&state, victim), Zone::Exile);

    // On P1's turn, P1 deals combat damage to P0 -> becomes the monarch ->
    // victim immediately returns to the battlefield under P1's control.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::DeclareAttackers
    });
    act(&mut state, Action::DeclareAttackers(vec![opp_attacker]));
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main2
    });
    assert_eq!(state.monarch, Some(P1));
    assert_eq!(zone(&state, victim), Zone::Battlefield);
    assert_eq!(state.objects.get(victim).controller, P1);
}

#[test]
fn palace_jailer_exiles_even_if_opponent_already_monarch_and_releases_on_later_monarch_change() {
    let mut state = plains();
    state.monarch = Some(P1);
    let victim = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let jailer = put(&mut state, P0, "Palace Jailer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, jailer, &[]);

    // Order BecomeMonarch on bottom (0) and Exile on top (1), so Exile resolves
    // while P1 is ALREADY the monarch.
    loop {
        match next(&mut state) {
            Decision::OrderTriggers { .. } => {
                act(&mut state, Action::OrderTriggers(vec![0, 1]));
            }
            Decision::ChooseTargets { .. } => {
                act(&mut state, Action::ChooseTarget(Target::Object(victim)));
            }
            Decision::CastSpellOrPass { .. } if state.stack.len() == 2 => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected: {other:?}"),
        }
    }
    // Resolve top trigger (Exile) only while P1 is still monarch.
    act(&mut state, Action::Pass);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    act(&mut state, Action::Pass);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    assert_eq!(state.monarch, Some(P1));
    assert_eq!(
        zone(&state, victim),
        Zone::Exile,
        "exiles even when an opponent is already the monarch"
    );
    // Now resolve BecomeMonarch -> P0 becomes monarch, victim stays exiled.
    settled(&mut state);
    assert_eq!(state.monarch, Some(P0));
    assert_eq!(zone(&state, victim), Zone::Exile);

    // Next turn P1 casts Palace Sentinels -> P1 becomes the monarch -> victim
    // returns immediately under P1's control.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    let sentinels = put(&mut state, P1, "Palace Sentinels", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W], 3);
    cast(&mut state, sentinels, &[]);
    settled(&mut state);
    assert_eq!(state.monarch, Some(P1));
    assert_eq!(zone(&state, victim), Zone::Battlefield);
    assert_eq!(state.objects.get(victim).controller, P1);

    // CR 724.3: if P1 is ALREADY the monarch and casts another Palace Sentinels
    // after a new Jailer exiled P1's creature while P1 stayed monarch, P1 does
    // not become the monarch again and the creature stays exiled.
    let mut same_monarch = plains();
    same_monarch.monarch = Some(P1);
    let victim2 = put(&mut same_monarch, P1, "Wild Mongrel", Zone::Battlefield);
    let jailer2 = put(&mut same_monarch, P0, "Palace Jailer", Zone::Hand);
    pool(&mut same_monarch, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut same_monarch, jailer2, &[]);
    loop {
        match next(&mut same_monarch) {
            Decision::OrderTriggers { .. } => {
                act(&mut same_monarch, Action::OrderTriggers(vec![0, 1]));
            }
            Decision::ChooseTargets { .. } => {
                act(
                    &mut same_monarch,
                    Action::ChooseTarget(Target::Object(victim2)),
                );
            }
            Decision::CastSpellOrPass { .. } if same_monarch.stack.len() == 2 => break,
            Decision::CastSpellOrPass { .. } => act(&mut same_monarch, Action::Pass),
            other => panic!("unexpected: {other:?}"),
        }
    }
    // Resolve Exile only, then P1 counters P0's BecomeMonarch with Stifle so
    // P1 remains monarch. When P1 later casts Palace Sentinels while already
    // monarch, P1 does not become the monarch again (CR 724.3) and victim2 stays exiled.
    act(&mut same_monarch, Action::Pass);
    assert!(matches!(
        next(&mut same_monarch),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut same_monarch, Action::Pass);
    assert!(matches!(
        next(&mut same_monarch),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    assert_eq!(zone(&same_monarch, victim2), Zone::Exile);
    assert_eq!(same_monarch.stack.len(), 1);
    let monarch_trigger = same_monarch.stack[0].v4.stack_item_id;
    act(&mut same_monarch, Action::Pass);
    let stifle = put(&mut same_monarch, P1, "Stifle", Zone::Hand);
    pool(&mut same_monarch, P1, &[ManaColor::U], 0);
    cast(
        &mut same_monarch,
        stifle,
        &[Target::StackItem(monarch_trigger)],
    );
    settled(&mut same_monarch);
    assert_eq!(same_monarch.monarch, Some(P1));
    assert_eq!(zone(&same_monarch, victim2), Zone::Exile);

    pass_until(&mut same_monarch, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    let extra_sentinels = put(&mut same_monarch, P1, "Palace Sentinels", Zone::Hand);
    pool(&mut same_monarch, P1, &[ManaColor::W], 3);
    cast(&mut same_monarch, extra_sentinels, &[]);
    settled(&mut same_monarch);
    assert_eq!(same_monarch.monarch, Some(P1));
    assert_eq!(
        zone(&same_monarch, victim2),
        Zone::Exile,
        "already-monarch opponent does not become the monarch again (CR 724.3)"
    );

    // Target leaving before Palace Jailer's exile trigger resolves fizzles cleanly.
    let mut fizzle_state = plains();
    let target = put(&mut fizzle_state, P1, "Wild Mongrel", Zone::Battlefield);
    let j = put(&mut fizzle_state, P0, "Palace Jailer", Zone::Hand);
    pool(&mut fizzle_state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut fizzle_state, j, &[]);
    loop {
        match next(&mut fizzle_state) {
            Decision::OrderTriggers { .. } => {
                act(&mut fizzle_state, Action::OrderTriggers(vec![0, 1]));
            }
            Decision::ChooseTargets { .. } => {
                act(
                    &mut fizzle_state,
                    Action::ChooseTarget(Target::Object(target)),
                );
            }
            Decision::CastSpellOrPass { .. } if fizzle_state.stack.len() == 2 => break,
            Decision::CastSpellOrPass { .. } => act(&mut fizzle_state, Action::Pass),
            other => panic!("unexpected: {other:?}"),
        }
    }
    event::propose_and_commit(
        &mut fizzle_state,
        ProposedEvent::zone_change(target, Zone::Graveyard),
    );
    settled(&mut fizzle_state);
    assert_eq!(zone(&fizzle_state, target), Zone::Graveyard);
    assert!(fizzle_state
        .eternal_v1
        .as_ref()
        .is_none_or(|e| e.c7b.jailer_exiles.is_empty()));
}

// ---- Public sessions -----------------------------------------------------------

#[test]
fn c7b_decks_finish_and_replay_through_public_sessions_both_seats() {
    let temur_artifacts = deck(&[
        ("Volcanic Island", 4),
        ("Tropical Island", 4),
        ("Taiga", 4),
        ("Forest", 4),
        ("Seat of the Synod", 2),
        ("Sol Ring", 2),
        ("Orcish Lumberjack", 4),
        ("Urza, Lord High Artificer", 4),
        ("Vivi Ornitier", 4),
        ("Soul-Scar Mage", 4),
        ("Lightning Bolt", 4),
    ]);
    let abzan_midrange = deck(&[
        ("Savannah", 4),
        ("Scrubland", 4),
        ("Bayou", 4),
        ("Mana Confluence", 4),
        ("Forest", 2),
        ("Llanowar Elves", 4),
        ("Dark Ritual", 2),
        ("Swords to Plowshares", 2),
        ("Black Widow, Super Spy", 4),
        ("Questing Beast", 4),
        ("Palace Jailer", 4),
        ("Wild Mongrel", 2),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    for (pair, first_seed) in [
        (
            [temur_artifacts.clone(), abzan_midrange.clone()],
            0x4337_4200_u64,
        ),
        ([abzan_midrange, temur_artifacts], 0x4337_4210),
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
    for name in C7B_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
