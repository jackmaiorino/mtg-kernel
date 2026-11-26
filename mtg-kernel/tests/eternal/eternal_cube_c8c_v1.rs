//! Vintage Cube wave C8c: native rules and public-session tests for the
//! Wave C8C Vintage Cube cards.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code, unused_imports)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::EternalStateV1;
use mtg_kernel::eternal_cube_c8c_v1::C8cStateV1;
use mtg_kernel::eternal_lurrus_v1::{add_companions, companion_condition_met};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::planeswalker_v1::loyalty;
use mtg_kernel::state::{GameObject, GameState, ObjectLinkV4, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C8C_CARDS: [&str; 7] = [
    "Pyrogoyf",
    "Elite Spellbinder",
    "Wan Shi Tong, All-Knowing",
    "Wan Shi Tong, Librarian",
    "Thieving Skydiver",
    "Zirda, the Dawnwaker",
    "Absorbing Man",
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
        0x4355_4245_0000 + 0x3863,
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

// ---- Characteristics & Isolation -----------------------------------------------

#[test]
fn c8c_cards_are_full() {
    for name in C8C_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
        assert!(!card.is_token, "{name}");
    }
    let spirit = def("Wan Shi Tong Spirit Token");
    assert_eq!(spirit.capability, CardCapability::Full);
    assert!(spirit.is_executable());
    assert!(spirit.is_token);
    assert_eq!(spirit.object_name, "Spirit Token");
    assert_eq!(spirit.colors, &[]);
    assert_eq!(spirit.power, Some(1));
    assert_eq!(spirit.toughness, Some(1));
    assert!(spirit.subtypes.contains(&Subtype::Spirit));

    assert!(def("Elite Spellbinder").keywords.has(Keywords::FLYING));
    assert!(def("Wan Shi Tong, All-Knowing")
        .keywords
        .has(Keywords::FLYING));
    assert!(def("Wan Shi Tong, Librarian").keywords.has(Keywords::FLASH));
    assert!(def("Wan Shi Tong, Librarian")
        .keywords
        .has(Keywords::FLYING));
    assert!(def("Wan Shi Tong, Librarian")
        .keywords
        .has(Keywords::VIGILANCE));
    assert!(def("Thieving Skydiver").keywords.has(Keywords::FLYING));
    assert!(def("Thieving Skydiver")
        .kicker_cost
        .is_some_and(|cost| cost.x_count == 1));
    assert!(def("Absorbing Man").keywords.has(Keywords::VIGILANCE));
    assert!(def("Zirda, the Dawnwaker").subtypes.contains(&Subtype::Fox));
}

#[test]
fn c8c_empty_state_skips_serde_and_hashes_zero_bytes() {
    let empty = C8cStateV1::default();
    assert!(empty.is_empty());

    let serialized = serde_json::to_value(EternalStateV1::default()).unwrap();
    assert!(
        serialized.get("c8c").is_none(),
        "empty c8c state must be skipped during serialization: {serialized}"
    );

    let mut with_empty = DefaultHasher::new();
    empty.hash(&mut with_empty);
    let baseline = DefaultHasher::new();
    assert_eq!(
        with_empty.finish(),
        baseline.finish(),
        "empty C8cStateV1 must hash zero bytes"
    );
}

// ---- Pyrogoyf ------------------------------------------------------------------

#[test]
fn pyrogoyf_cda_counts_distinct_card_types_across_both_graveyards_in_all_zones() {
    let mut state = plains();
    let in_hand = put(&mut state, P0, "Pyrogoyf", Zone::Hand);
    let in_play = put(&mut state, P0, "Pyrogoyf", Zone::Battlefield);
    let in_gy = put(&mut state, P1, "Pyrogoyf", Zone::Graveyard);

    // Pyrogoyf in P1's graveyard contributes Creature (1 type -> 1/2).
    assert_eq!(power_toughness(&state, in_hand), (1, 2));
    assert_eq!(power_toughness(&state, in_play), (1, 2));
    assert_eq!(power_toughness(&state, in_gy), (1, 2));

    // Duplicate Creature in P0's graveyard does not increase the count.
    put(&mut state, P0, "Elvish Mystic", Zone::Graveyard);
    assert_eq!(power_toughness(&state, in_play), (1, 2));

    // Add Land (P0), Instant (P1), Sorcery (P0), Artifact (P1), Enchantment (P0),
    // Planeswalker (P1) -> 7 distinct card types (7/8).
    put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Demonic Tutor", Zone::Graveyard);
    put(&mut state, P1, "Sol Ring", Zone::Graveyard);
    put(&mut state, P0, "Sylvan Library", Zone::Graveyard);
    put(&mut state, P1, "Jace, the Mind Sculptor", Zone::Graveyard);
    assert_eq!(power_toughness(&state, in_hand), (7, 8));
    assert_eq!(power_toughness(&state, in_play), (7, 8));
    assert_eq!(power_toughness(&state, in_gy), (7, 8));

    // Empty graveyards -> 0/1.
    let mut empty = plains();
    let goyf = put(&mut empty, P0, "Pyrogoyf", Zone::Battlefield);
    assert_eq!(power_toughness(&empty, goyf), (0, 1));

    // Dress Down (Layer 6 ability removal): Pyrogoyf loses its CDA and becomes
    // 0/1 (surviving SBAs) even with multiple card types in graveyards.
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(dress, Zone::Battlefield),
    );
    settled(&mut state);
    assert_eq!(
        zone(&state, in_play),
        Zone::Battlefield,
        "Pyrogoyf becomes 0/1 under Dress Down and survives SBAs"
    );
    assert_eq!(power_toughness(&state, in_play), (0, 1));
}

#[test]
fn pyrogoyf_self_and_other_lhurgoyf_entries_deal_entering_creatures_damage() {
    let mut state = plains();
    // Put 2 card types in graveyards (Land + Instant -> Lhurgoyf power = 2).
    put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);

    let opp_jace = put(&mut state, P1, "Jace, the Mind Sculptor", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(opp_jace, Zone::Battlefield),
    );
    settled(&mut state);
    assert_eq!(loyalty(&state, opp_jace), Some(3));

    // 1. Pyrogoyf entering triggers itself and deals 2 damage to Jace.
    let pyro = put(&mut state, P0, "Pyrogoyf", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, pyro, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(opp_jace)));
            assert!(legal_targets.contains(&Target::Player(P1)));
            Action::ChooseTarget(Target::Object(opp_jace))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, pyro), Zone::Battlefield);
    assert_eq!(loyalty(&state, opp_jace), Some(1));

    // 2. Barrowgoyf (Deathtouch + Lifelink) entering under P0's control triggers
    //    Pyrogoyf; BARROWGOYF deals 2 damage, so Deathtouch kills a 6/5 Troll
    //    and Lifelink gains P0 2 life!
    let opp_troll = put(&mut state, P1, "Troll of Khazad-dum", Zone::Battlefield);
    assert_eq!(power_toughness(&state, opp_troll), (6, 5));
    let life_before = life(&state, P0);
    let barrow = put(&mut state, P0, "Barrowgoyf", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, barrow, &[]);
    settle_choosing(&mut state, opp_troll);
    assert_eq!(
        zone(&state, opp_troll),
        Zone::Graveyard,
        "Barrowgoyf's deathtouch kills 6/5 Troll with 2 damage"
    );
    assert_eq!(
        life(&state, P0),
        life_before + 2,
        "Barrowgoyf's lifelink gains life equal to damage dealt"
    );

    // 3. A Changeling (Masked Vandal, 1/3) is a Lhurgoyf and triggers Pyrogoyf,
    //    dealing 1 damage to P1.
    let opp_life_before = life(&state, P1);
    let vandal = put(&mut state, P0, "Masked Vandal", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, vandal, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Player(P1)) =>
        {
            Action::ChooseTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    assert_eq!(life(&state, P1), opp_life_before - 1);

    // 4. Opponent's Nethergoyf entering does NOT trigger P0's Pyrogoyf.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    let opp_nether = put(&mut state, P1, "Nethergoyf", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B], 0);
    cast(&mut state, opp_nether, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, opp_nether), Zone::Battlefield);
}

#[test]
fn pyrogoyf_uses_lki_when_entering_lhurgoyf_dies_and_respects_torpor_orb_and_recheck() {
    // 1. Entering Barrowgoyf dies while Pyrogoyf's trigger is on the stack:
    //    trigger still deals Barrowgoyf's LKI power (2) with lifelink!
    let mut state = plains();
    put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P1, "Sol Ring", Zone::Graveyard);
    put(&mut state, P0, "Pyrogoyf", Zone::Battlefield);
    let barrow = put(&mut state, P0, "Barrowgoyf", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, barrow, &[]);

    let mut trigger_on_stack = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Player(P1)));
                act(&mut state, Action::ChooseTarget(Target::Player(P1)));
            }
            Decision::CastSpellOrPass { .. }
                if zone(&state, barrow) == Zone::Battlefield && !state.stack.is_empty() =>
            {
                trigger_on_stack = true;
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(trigger_on_stack);
    // P1 exiles Barrowgoyf before Pyrogoyf's trigger resolves (it had power 2
    // and lifelink on the battlefield).
    let life_before_p0 = life(&state, P0);
    let life_before_p1 = life(&state, P1);
    state.priority_player = P1;
    let plow = put(&mut state, P1, "Swords to Plowshares", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, plow, &[Target::Object(barrow)]);
    settled(&mut state);
    assert_eq!(zone(&state, barrow), Zone::Exile);
    // P0 gained 2 from Swords to Plowshares AND 2 from Barrowgoyf's LKI lifelink!
    assert_eq!(life(&state, P0), life_before_p0 + 4);
    assert_eq!(life(&state, P1), life_before_p1 - 2);

    // 2. Target legality recheck: if target creature leaves before resolution,
    //    Pyrogoyf's trigger fizzles cleanly.
    let mut state = plains();
    put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P1, "Sol Ring", Zone::Graveyard);
    let opp_elf = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    let pyro = put(&mut state, P0, "Pyrogoyf", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, pyro, &[]);
    let mut targeted = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(opp_elf)));
                act(&mut state, Action::ChooseTarget(Target::Object(opp_elf)));
                targeted = true;
            }
            Decision::CastSpellOrPass { .. } if targeted => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(targeted);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(opp_elf, Zone::Exile));
    settled(&mut state);
    assert_eq!(zone(&state, opp_elf), Zone::Exile);

    // 3. Torpor Orb suppresses Pyrogoyf's ETB trigger.
    let mut state = plains();
    put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P1, "Torpor Orb", Zone::Battlefield);
    let pyro = put(&mut state, P0, "Pyrogoyf", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, pyro, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, pyro), Zone::Battlefield);
    assert_eq!(life(&state, P1), 20);
}

// ---- Elite Spellbinder ---------------------------------------------------------

#[test]
fn elite_spellbinder_exiles_optional_nonland_with_permanent_plus_two_tax() {
    // 1. Exiling a nonland card grants owner permanent play permission with +{2}
    //    tax that persists even after Spellbinder dies, and does not tax other
    //    copies cast from hand.
    let mut state = plains();
    let opp_land = put(&mut state, P1, "Mountain", Zone::Hand);
    let opp_bolt_exiled = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let opp_bolt_hand = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let spellbinder = put(&mut state, P0, "Elite Spellbinder", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, spellbinder, &[]);
    let mut offered_optional = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Player(P1)]);
            Action::ChooseTarget(Target::Player(P1))
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            min_targets,
            max_targets,
            ..
        } => {
            assert!(can_finish, "exiling from hand is optional");
            assert_eq!(min_targets, 0);
            assert_eq!(max_targets, 1);
            assert!(legal_targets.contains(&Target::Object(opp_bolt_exiled)));
            assert!(
                !legal_targets.contains(&Target::Object(opp_land)),
                "lands cannot be exiled by Elite Spellbinder"
            );
            offered_optional = true;
            Action::ChooseEffectTarget(Target::Object(opp_bolt_exiled))
        }
        other => first_answer(other),
    });
    assert!(offered_optional);
    assert_eq!(zone(&state, spellbinder), Zone::Battlefield);
    assert_eq!(zone(&state, opp_bolt_exiled), Zone::Exile);
    assert_eq!(zone(&state, opp_bolt_hand), Zone::Hand);
    assert_eq!(zone(&state, opp_land), Zone::Hand);

    // Kill Spellbinder: exiled card stays exiled and remains castable by owner.
    let kill = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, kill, &[Target::Object(spellbinder)]);
    settled(&mut state);
    assert_eq!(zone(&state, spellbinder), Zone::Graveyard);
    assert_eq!(zone(&state, opp_bolt_exiled), Zone::Exile);

    // Advance to P1's turn (proving permission lasts across turns).
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });

    // With {R} alone: hand Bolt IS castable, exiled Bolt is NOT castable ({2}{R} needed).
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(castable(&mut state, opp_bolt_hand));
    assert!(
        !castable(&mut state, opp_bolt_exiled),
        "Spellbinder tax requires {{2}}{{R}}"
    );

    // With {1}{R}: exiled Bolt is STILL not castable.
    pool(&mut state, P1, &[], 1);
    assert!(!castable(&mut state, opp_bolt_exiled));

    // With {2}{R}: exiled Bolt IS castable and consumes all {2}{R}!
    pool(&mut state, P1, &[], 1);
    assert!(castable(&mut state, opp_bolt_exiled));
    cast(&mut state, opp_bolt_exiled, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 17);
    assert_eq!(mana(&state, P1, ManaColor::R), 0);
    assert_eq!(mana(&state, P1, ManaColor::C), 0);
    assert_eq!(zone(&state, opp_bolt_exiled), Zone::Graveyard);

    // 2. Optional decline leaves opponent's hand untouched.
    let mut state = plains();
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    let spellbinder = put(&mut state, P0, "Elite Spellbinder", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, spellbinder, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(zone(&state, opp_ring), Zone::Hand);

    // 3. Opponent with only lands in hand settles cleanly with no exile prompt.
    let mut state = plains();
    let only_land = put(&mut state, P1, "Plains", Zone::Hand);
    let spellbinder = put(&mut state, P0, "Elite Spellbinder", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, spellbinder, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, only_land), Zone::Hand);

    // 4. Opponent with hexproof (Leyline of Sanctity) cannot be targeted.
    let mut state = plains();
    put(&mut state, P1, "Leyline of Sanctity", Zone::Battlefield);
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    let spellbinder = put(&mut state, P0, "Elite Spellbinder", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, spellbinder, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, spellbinder), Zone::Battlefield);
    assert_eq!(zone(&state, opp_ring), Zone::Hand);
}

// ---- Wan Shi Tong, All-Knowing & Spirit Token ----------------------------------

#[test]
fn wan_shi_tong_all_knowing_tuck_owner_chooses_second_or_bottom_and_creates_spirits() {
    // Option 0: target's OWNER (P1) chooses second from top; Wan Shi Tong
    // triggers once and creates two 1/1 colorless Spirit tokens.
    let mut state = plains();
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let wst = put(&mut state, P0, "Wan Shi Tong, All-Knowing", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, wst, &[]);
    let mut owner_prompted = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(opp_ring)));
            Action::ChooseTarget(Target::Object(opp_ring))
        }
        Decision::ChooseEffectOption {
            player,
            option_count,
            ..
        } => {
            assert_eq!(
                player, P1,
                "target's OWNER chooses second from top or bottom"
            );
            assert_eq!(option_count, 2);
            owner_prompted = true;
            Action::ChooseEffectOption(0)
        }
        other => first_answer(other),
    });
    assert!(owner_prompted);
    assert_eq!(zone(&state, opp_ring), Zone::Library);
    assert_eq!(
        state.players[1].library.get(1).copied(),
        Some(opp_ring),
        "option 0 puts target second from top of owner's library"
    );
    let spirits = named(&state, P0, "Wan Shi Tong Spirit Token");
    assert_eq!(
        spirits.len(),
        2,
        "tucking one card creates two Spirit tokens"
    );
    for spirit in spirits {
        assert_eq!(power_toughness(&state, spirit), (1, 1));
        assert!(engine::has_effective_subtype(
            &state,
            spirit,
            Subtype::Spirit
        ));
        assert_eq!(engine::effective_name(&state, spirit), "Spirit Token");
    }

    // Option 1: target's OWNER (P1) chooses bottom of library.
    let mut state = plains();
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let wst = put(&mut state, P0, "Wan Shi Tong, All-Knowing", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, wst, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(opp_ring)),
        Decision::ChooseEffectOption { player: P1, .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(
        state.players[1].library.last().copied(),
        Some(opp_ring),
        "option 1 puts target on the bottom of owner's library"
    );
    assert_eq!(named(&state, P0, "Wan Shi Tong Spirit Token").len(), 2);
}

#[test]
fn wan_shi_tong_all_knowing_batched_library_trigger_and_non_triggers() {
    // 1. Brainstorm puts two cards back into the library in one batch ->
    //    triggers ONCE (2 Spirit tokens, not 4).
    let mut state = plains();
    put(
        &mut state,
        P0,
        "Wan Shi Tong, All-Knowing",
        Zone::Battlefield,
    );
    let brainstorm = put(&mut state, P0, "Brainstorm", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, brainstorm, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            Action::ChooseEffectTarget(legal_targets[0])
        }
        other => first_answer(other),
    });
    assert_eq!(
        named(&state, P0, "Wan Shi Tong Spirit Token").len(),
        2,
        "putting two cards into library simultaneously triggers once"
    );

    // 2. Putting a TOKEN into a library does NOT trigger Wan Shi Tong.
    let mut state = plains();
    let wst = put(&mut state, P0, "Wan Shi Tong, All-Knowing", Zone::Hand);
    let opp_token = token(&mut state, P1, "Wan Shi Tong Spirit Token");
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, wst, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(opp_token)),
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(
        named(&state, P0, "Wan Shi Tong Spirit Token").len(),
        0,
        "tokens entering a library do not trigger 'one or more cards'"
    );

    // 3. Shuffling a library alone does NOT trigger Wan Shi Tong.
    let mut state = plains();
    put(
        &mut state,
        P0,
        "Wan Shi Tong, All-Knowing",
        Zone::Battlefield,
    );
    let delta = put(&mut state, P0, "Polluted Delta", Zone::Battlefield);
    activate(&mut state, delta, 0, &[]);
    settle_first(&mut state);
    assert_eq!(named(&state, P0, "Wan Shi Tong Spirit Token").len(), 0);

    // 4. Wan Shi Tong tucking ITSELF does NOT trigger its Spirit ability (CR 603.6c).
    let mut state = plains();
    let wst = put(&mut state, P0, "Wan Shi Tong, All-Knowing", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, wst, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(wst)));
            Action::ChooseTarget(Target::Object(wst))
        }
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, wst), Zone::Library);
    assert_eq!(
        named(&state, P0, "Wan Shi Tong Spirit Token").len(),
        0,
        "Wan Shi Tong is no longer on the battlefield when it enters the library"
    );
}

#[test]
fn wan_shi_tong_spirit_token_combat_restriction() {
    // 1. Spirit token attacking: cannot be blocked by non-Spirit, CAN be blocked
    //    by a Spirit or a Changeling.
    let mut state = plains();
    let spirit_token = token(&mut state, P0, "Wan Shi Tong Spirit Token");
    let opp_elf = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    let opp_spirit = put(&mut state, P1, "Skyclave Apparition", Zone::Battlefield);
    let opp_changeling = put(&mut state, P1, "Masked Vandal", Zone::Battlefield);
    let blockers = legal_blockers(&mut state, spirit_token);
    assert!(
        !blockers.contains(&opp_elf),
        "non-Spirit cannot block Spirit token"
    );
    assert!(
        blockers.contains(&opp_spirit),
        "Spirit can block Spirit token"
    );
    assert!(
        blockers.contains(&opp_changeling),
        "Changeling counts as Spirit and can block Spirit token"
    );

    // 2. Non-Spirit attacking: Spirit token CANNOT block it.
    let mut state = plains();
    let elf = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let opp_spirit_token = token(&mut state, P1, "Wan Shi Tong Spirit Token");
    let blockers = legal_blockers(&mut state, elf);
    assert!(
        !blockers.contains(&opp_spirit_token),
        "Spirit token cannot block non-Spirit attacker"
    );

    // 3. Spirit or Changeling attacking: Spirit token CAN block it.
    let mut state = plains();
    let spirit_atk = put(&mut state, P0, "Skyclave Apparition", Zone::Battlefield);
    let opp_spirit_token = token(&mut state, P1, "Wan Shi Tong Spirit Token");
    assert!(legal_blockers(&mut state, spirit_atk).contains(&opp_spirit_token));

    let mut state = plains();
    let changeling_atk = put(&mut state, P0, "Masked Vandal", Zone::Battlefield);
    let opp_spirit_token = token(&mut state, P1, "Wan Shi Tong Spirit Token");
    assert!(legal_blockers(&mut state, changeling_atk).contains(&opp_spirit_token));
}

// ---- Wan Shi Tong, Librarian ---------------------------------------------------

#[test]
fn wan_shi_tong_librarian_flash_etb_counters_and_rounded_down_draws() {
    // 1. Flash: castable on opponent's turn.
    let mut state = plains();
    state.active_player = P1;
    state.priority_player = P0;
    let flash_lib = put(&mut state, P0, "Wan Shi Tong, Librarian", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    assert!(castable(&mut state, flash_lib), "Librarian has Flash");

    // 2. Cast with X = 0: 0 counters (1/1), 0 draws.
    let mut state = plains();
    let lib0 = put(&mut state, P0, "Wan Shi Tong, Librarian", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, lib0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, lib0), (1, 1));
    assert_eq!(state.players[0].hand.len(), 0);

    // 3. Cast with X = 3: 3 +1/+1 counters (4/4), draws floor(3/2) = 1 card.
    let mut state = plains();
    let lib3 = put(&mut state, P0, "Wan Shi Tong, Librarian", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 3);
    cast(&mut state, lib3, &[]);
    settle_option(&mut state, 3);
    assert_eq!(power_toughness(&state, lib3), (4, 4));
    assert_eq!(state.objects.get(lib3).counters.plus1_plus1, 3);
    assert_eq!(state.players[0].hand.len(), 1);

    // 4. Cast with X = 4, then killed in response to ETB: still draws 4/2 = 2 cards!
    let mut state = plains();
    let lib4 = put(&mut state, P0, "Wan Shi Tong, Librarian", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 4);
    cast(&mut state, lib4, &[]);
    let mut etb_on_stack = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 5, "options 0..=4");
                act(&mut state, Action::ChooseEffectOption(4));
            }
            Decision::CastSpellOrPass { .. }
                if zone(&state, lib4) == Zone::Battlefield && !state.stack.is_empty() =>
            {
                etb_on_stack = true;
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(etb_on_stack);
    opponent_bolts(&mut state, Target::Object(lib4));
    settled(&mut state);
    assert_eq!(zone(&state, lib4), Zone::Graveyard);
    assert_eq!(
        state.players[0].hand.len(),
        2,
        "ETB still draws X/2 cards after Librarian leaves the battlefield"
    );

    // 5. Flickered with Ephemerate (not cast): X = 0, 0 counters, 0 draws.
    let mut state = plains();
    let lib_flicker = put(&mut state, P0, "Wan Shi Tong, Librarian", Zone::Battlefield);
    let ephem = put(&mut state, P0, "Ephemerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, ephem, &[Target::Object(lib_flicker)]);
    settled(&mut state);
    assert_eq!(zone(&state, lib_flicker), Zone::Battlefield);
    assert_eq!(state.objects.get(lib_flicker).counters.plus1_plus1, 0);
    assert_eq!(state.players[0].hand.len(), 0);
}

#[test]
fn wan_shi_tong_librarian_triggers_on_opponent_searches_and_supports_draw_replacement() {
    // 1. Opponent cracking Polluted Delta triggers Librarian (+1/+1 counter and draw 1).
    let mut state = game(&["Island"; 10]);
    let librarian = put(&mut state, P0, "Wan Shi Tong, Librarian", Zone::Battlefield);
    let opp_delta = put(&mut state, P1, "Polluted Delta", Zone::Battlefield);
    let own_delta = put(&mut state, P0, "Polluted Delta", Zone::Battlefield);

    // P0's OWN search does NOT trigger P0's Librarian.
    activate(&mut state, own_delta, 0, &[]);
    settle_first(&mut state);
    assert_eq!(state.objects.get(librarian).counters.plus1_plus1, 0);
    assert_eq!(state.players[0].hand.len(), 0);

    // P1's Polluted Delta search DOES trigger P0's Librarian.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    let hand_before = state.players[0].hand.len();
    activate(&mut state, opp_delta, 0, &[]);
    settle_first(&mut state);
    assert_eq!(state.objects.get(librarian).counters.plus1_plus1, 1);
    assert_eq!(power_toughness(&state, librarian), (2, 2));
    assert_eq!(state.players[0].hand.len(), hand_before + 1);

    // P1 casting Demonic Tutor also triggers Librarian (even with fail-to-find on empty library).
    state.players[1].library.clear();
    let tutor = put(&mut state, P1, "Demonic Tutor", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B], 1);
    cast(&mut state, tutor, &[]);
    settle_first(&mut state);
    assert_eq!(state.objects.get(librarian).counters.plus1_plus1, 2);
    assert_eq!(state.players[0].hand.len(), hand_before + 2);

    // 2. Resumable draw supports Dredge replacement (Stinkweed Imp) and Narset.
    let mut state = plains();
    let imp = put(&mut state, P0, "Stinkweed Imp", Zone::Graveyard);
    let lib = put(&mut state, P0, "Wan Shi Tong, Librarian", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(&mut state, lib, &[]);
    let mut dredged = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(2),
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.contains(&Target::Object(imp)) => {
            dredged = true;
            Action::ChooseEffectTarget(Target::Object(imp))
        }
        other => first_answer(other),
    });
    assert!(dredged, "Librarian ETB draw must use resumable draw hook");
    assert!(hand_contains(&state, P0, imp));
}

// ---- Thieving Skydiver ---------------------------------------------------------

#[test]
fn thieving_skydiver_kicker_x_forbids_zero_and_steals_artifact_and_attaches_equipment() {
    // 1. With only {1}{U}, kicker is NOT offered (X can't be 0) and Skydiver
    //    enters unkicked with no ETB trigger.
    let mut state = plains();
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let skydiver = put(&mut state, P0, "Thieving Skydiver", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, skydiver, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, skydiver), Zone::Battlefield);
    assert_eq!(state.objects.get(opp_ring).controller, P1);

    // 2. With {2}{U} (max X = 1), ChooseKicker IS offered; kicking auto-selects
    //    X = 1, can target MV <= 1 (Sol Ring) and CANNOT target MV 2 (Jitte).
    let mut state = plains();
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let opp_jitte = put(&mut state, P1, "Umezawa's Jitte", Zone::Battlefield);
    let skydiver = put(&mut state, P0, "Thieving Skydiver", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, skydiver, &[]);
    let mut saw_kicker = false;
    let mut saw_etb_targets = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => {
            saw_kicker = true;
            Action::ChooseKicker(true)
        }
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(opp_ring)));
            assert!(
                !legal_targets.contains(&Target::Object(opp_jitte)),
                "MV 2 artifact is illegal when X = 1"
            );
            saw_etb_targets = true;
            Action::ChooseTarget(Target::Object(opp_ring))
        }
        other => panic!("unexpected {other:?}"),
    });
    assert!(saw_kicker && saw_etb_targets);
    assert_eq!(state.objects.get(opp_ring).controller, P0);
    assert_eq!(state.objects.get(opp_jitte).controller, P1);

    // 3. With {3}{U} (max X = 2), ChooseEffectOption offers exactly 2 options
    //    (option 0 => X=1, option 1 => X=2; option 2 rejected). Choosing X = 2
    //    steals opponent's equipped Umezawa's Jitte, detaches it from opponent's
    //    creature, and attaches it to Skydiver!
    let mut state = plains();
    let opp_elf = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    let opp_jitte = put(&mut state, P1, "Umezawa's Jitte", Zone::Battlefield);
    let elf_zcc = state.objects.get(opp_elf).zone_change_count;
    state.objects.get_mut(opp_jitte).v4.attached_to = Some(mtg_kernel::state::ObjectLinkV4 {
        object: opp_elf,
        zone_change_count: elf_zcc,
    });
    state.objects.get_mut(opp_elf).attachments.push(opp_jitte);
    let skydiver = put(&mut state, P0, "Thieving Skydiver", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, skydiver, &[]);
    assert!(matches!(next(&mut state), Decision::ChooseKicker { .. }));
    act(&mut state, Action::ChooseKicker(true));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 2,
            ..
        }
    ));
    assert!(
        engine::step(&mut state, Action::ChooseEffectOption(2)).is_err(),
        "out-of-range X option must be rejected"
    );
    act(&mut state, Action::ChooseEffectOption(1));
    settle_choosing(&mut state, opp_jitte);
    assert_eq!(state.objects.get(opp_jitte).controller, P0);
    assert!(
        !state.objects.get(opp_elf).attachments.contains(&opp_jitte),
        "Jitte detached from opponent's creature"
    );
    assert!(
        state.objects.get(skydiver).attachments.contains(&opp_jitte),
        "stolen Equipment attaches to Thieving Skydiver"
    );

    // 4. Declining kicker when payable casts unkicked with no ETB trigger.
    let mut state = plains();
    let opp_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let skydiver = put(&mut state, P0, "Thieving Skydiver", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, skydiver, &[]);
    assert!(matches!(next(&mut state), Decision::ChooseKicker { .. }));
    act(&mut state, Action::ChooseKicker(false));
    settled(&mut state);
    assert_eq!(state.objects.get(opp_ring).controller, P1);

    // 5. Skydiver dies in response to ETB: still gains control of Equipment,
    //    without attaching.
    let mut state = plains();
    let opp_jitte = put(&mut state, P1, "Umezawa's Jitte", Zone::Battlefield);
    let skydiver = put(&mut state, P0, "Thieving Skydiver", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, skydiver, &[]);
    let mut etb_on_stack = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseKicker { .. } => act(&mut state, Action::ChooseKicker(true)),
            Decision::ChooseEffectOption { .. } => act(&mut state, Action::ChooseEffectOption(1)),
            Decision::ChooseTargets { .. } => {
                act(&mut state, Action::ChooseTarget(Target::Object(opp_jitte)));
            }
            Decision::CastSpellOrPass { .. }
                if zone(&state, skydiver) == Zone::Battlefield && !state.stack.is_empty() =>
            {
                etb_on_stack = true;
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(etb_on_stack);
    opponent_bolts(&mut state, Target::Object(skydiver));
    settled(&mut state);
    assert_eq!(zone(&state, skydiver), Zone::Graveyard);
    assert_eq!(
        state.objects.get(opp_jitte).controller,
        P0,
        "control change still occurs if Skydiver left before resolution"
    );
}

// ---- Zirda, the Dawnwaker ------------------------------------------------------

#[test]
fn zirda_companion_validation_and_special_action() {
    let zirda = id("Zirda, the Dawnwaker");
    let ids = |names: &[&str]| names.iter().map(|n| id(n)).collect::<Vec<_>>();

    // Valid Zirda deck: lands, mana rocks, Equipment, creatures with activated
    // abilities (tap, cycling, channel, unearth), planeswalkers, and nonpermanents.
    assert_eq!(
        companion_condition_met(
            zirda,
            &ids(&[
                "Plains",
                "Mountain",
                "Plateau",
                "Polluted Delta",
                "Sol Ring",
                "Basalt Monolith",
                "Skullclamp",
                "Umezawa's Jitte",
                "Elvish Mystic",
                "Mother of Runes",
                "Hollow One",
                "Twinshot Sniper",
                "Phyrexian Dragon Engine",
                "Jace, the Mind Sculptor",
                "Lightning Bolt",
                "Demonic Tutor",
            ])
        ),
        Some(true)
    );

    // Every permanent lacking a front-face activated ability is rejected.
    for invalid in [
        "Pyrogoyf",
        "Barrowgoyf",
        "Urza's Saga",
        "Sanctum Lurker",
        "Avatar of Burgeoning Echoes",
        "Unholy Annex // Ritual Chamber",
        "Witch Enchanter",
        "Boggart Trawler",
        "Tamiyo, Inquisitive Student",
        "Lurrus of the Dream-Den",
        "Lutri, the Spellchaser",
        "Torpor Orb",
        "Sylvan Library",
    ] {
        assert_eq!(
            companion_condition_met(zirda, &ids(&["Plains", invalid])),
            Some(false),
            "{invalid} must fail Zirda's companion condition"
        );
    }

    // Companion {3} special action puts Zirda from exile into hand.
    let mut state = plains();
    add_companions(&mut state, [Some(zirda), None]);
    let companion = *state.exile.last().unwrap();
    assert_eq!(zone(&state, companion), Zone::Exile);
    pool(&mut state, P0, &[], 2);
    assert!(
        !activatable(&mut state, companion, 0),
        "{{3}} not yet payable"
    );
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, companion, 0));
    act(&mut state, Action::ActivateAbility(companion, 0));
    assert_eq!(zone(&state, companion), Zone::Hand);
    assert!(hand_contains(&state, P0, companion));
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn zirda_reduces_nonmana_activated_abilities_across_zones_and_skips_mana_abilities() {
    // 1. Battlefield: Basalt Monolith untap ({3}) costs {1} with Zirda.
    let mut state = plains();
    put(&mut state, P0, "Zirda, the Dawnwaker", Zone::Battlefield);
    let monolith = put(&mut state, P0, "Basalt Monolith", Zone::Battlefield);
    state.objects.get_mut(monolith).tapped = true;
    assert!(
        !activatable(&mut state, monolith, 0),
        "cannot reduce below 1 mana"
    );
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, monolith, 0));
    activate(&mut state, monolith, 0, &[]);
    settled(&mut state);
    assert!(!state.objects.get(monolith).tapped);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);

    // 2. Floor of 1 mana: Skullclamp Equip ({1}) stays {1}; Umezawa's Jitte
    //    Equip ({2}) becomes {1}.
    let mut state = plains();
    let zirda = put(&mut state, P0, "Zirda, the Dawnwaker", Zone::Battlefield);
    let clamp = put(&mut state, P0, "Skullclamp", Zone::Battlefield);
    let jitte = put(&mut state, P0, "Umezawa's Jitte", Zone::Battlefield);
    assert!(
        !activatable(&mut state, clamp, 0),
        "{{1}} cannot become {{0}}"
    );
    assert!(
        !activatable(&mut state, jitte, 3),
        "{{2}} cannot become {{0}}"
    );
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, clamp, 0));
    assert!(activatable(&mut state, jitte, 3));
    activate(&mut state, jitte, 3, &[Target::Object(zirda)]);
    settled(&mut state);
    assert!(state.objects.get(zirda).attachments.contains(&jitte));

    // 3. Hand Cycling ({2} -> {1}) and Channel ({1}{R} -> {R}).
    let mut state = plains();
    put(&mut state, P0, "Zirda, the Dawnwaker", Zone::Battlefield);
    let hollow = put(&mut state, P0, "Hollow One", Zone::Hand);
    let sniper = put(&mut state, P0, "Twinshot Sniper", Zone::Hand);
    assert!(!activatable(&mut state, hollow, 0));
    pool(&mut state, P0, &[], 1);
    assert!(
        activatable(&mut state, hollow, 0),
        "Cycling {{2}} reduced to {{1}}"
    );
    activate(&mut state, hollow, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, hollow), Zone::Graveyard);

    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(
        activatable(&mut state, sniper, 0),
        "Channel {{1}}{{R}} reduced to {{R}}"
    );
    activate(&mut state, sniper, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 18);

    // 4. Graveyard Unearth ({3}{R}{R} -> {1}{R}{R}).
    let mut state = plains();
    put(&mut state, P0, "Zirda, the Dawnwaker", Zone::Battlefield);
    let engine_card = put(&mut state, P0, "Phyrexian Dragon Engine", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 0);
    assert!(!activatable(&mut state, engine_card, 0));
    pool(&mut state, P0, &[], 1);
    assert!(
        activatable(&mut state, engine_card, 0),
        "Unearth {{3}}{{R}}{{R}} reduced to {{1}}{{R}}{{R}}"
    );
    activate(&mut state, engine_card, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, engine_card), Zone::Battlefield);

    // 5. X-cost activated ability: Lazotep Quarry ({X}{2}, {T}, Sacrifice a Desert)
    //    targeting a MV 2 creature card in GY needs {4} normally -> {2} with Zirda!
    let mut state = plains();
    put(&mut state, P0, "Zirda, the Dawnwaker", Zone::Battlefield);
    let quarry = put(&mut state, P0, "Lazotep Quarry", Zone::Battlefield);
    let gy_creature = put(&mut state, P0, "Thieving Skydiver", Zone::Graveyard);
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, quarry, 0));
    pool(&mut state, P0, &[], 1);
    assert!(
        activatable(&mut state, quarry, 0),
        "Lazotep Quarry {{X}}{{2}} with X=2 reduced from {{4}} to {{2}}"
    );
    activate(&mut state, quarry, 0, &[Target::Object(gy_creature)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, gy_creature), Zone::Exile);

    // 6. Mana abilities are NOT reduced (Energy Refractor "{2}: Add one mana of
    //    any color" still requires {2}, never {1}), and opponent gets no reduction.
    let mut state = plains();
    put(&mut state, P0, "Zirda, the Dawnwaker", Zone::Battlefield);
    let refractor = put(&mut state, P0, "Energy Refractor", Zone::Battlefield);
    next(&mut state);
    pool(&mut state, P0, &[], 1);
    assert!(
        engine::step(
            &mut state,
            Action::ActivateManaAbilityChoice(refractor, ManaColor::B),
        )
        .is_err(),
        "Zirda must NOT reduce mana ability costs"
    );
    pool(&mut state, P0, &[], 1);
    engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(refractor, ManaColor::B),
    )
    .expect("Energy Refractor still payable with {2}");
    assert_eq!(mana(&state, P0, ManaColor::B), 1);

    let mut state = plains();
    put(&mut state, P1, "Zirda, the Dawnwaker", Zone::Battlefield);
    let opp_monolith = put(&mut state, P0, "Basalt Monolith", Zone::Battlefield);
    state.objects.get_mut(opp_monolith).tapped = true;
    pool(&mut state, P0, &[], 2);
    assert!(
        !activatable(&mut state, opp_monolith, 0),
        "opponent's Zirda does not reduce your abilities"
    );

    // 7. Special actions (Foretell's {2} from hand) are NOT activated abilities
    //    and must NOT be reduced by Zirda (CR 702.143a).
    let mut state = plains();
    put(&mut state, P0, "Zirda, the Dawnwaker", Zone::Battlefield);
    let fireball = put(&mut state, P0, "Delayed Blast Fireball", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    assert!(
        !activatable(&mut state, fireball, 0),
        "Zirda must NOT reduce Foretell's {{2}} special action"
    );
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, fireball, 0));
    act(&mut state, Action::ActivateAbility(fireball, 0));
    assert_eq!(zone(&state, fireball), Zone::Exile);

    // 8. Room door-unlocking special actions (CR 709.5e / 116.2k) are NOT
    //    activated abilities and must NOT be reduced by Zirda.
    let mut state = plains();
    put(&mut state, P0, "Zirda, the Dawnwaker", Zone::Battlefield);
    let room = put(&mut state, P0, "Unholy Annex // Ritual Chamber", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    act(&mut state, Action::CastSpell(room));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Normal),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, room), Zone::Battlefield);
    // Unlocking Ritual Chamber costs {3}{B}{B}, never reduced to {1}{B}{B}.
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 1);
    assert!(
        !activatable(&mut state, room, 1),
        "Zirda must NOT reduce Room door-unlocking special actions"
    );
    pool(&mut state, P0, &[], 2);
    assert!(activatable(&mut state, room, 1));
    act(&mut state, Action::ActivateAbility(room, 1));
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(mana(&state, P0, ManaColor::B), 0);
}

#[test]
fn zirda_activated_ability_prevents_blocking_this_turn_only() {
    let mut state = plains();
    let zirda = put(&mut state, P0, "Zirda, the Dawnwaker", Zone::Battlefield);
    let attacker = put(&mut state, P0, "Monastery Swiftspear", Zone::Battlefield);
    let opp_blocker = put(&mut state, P1, "Skyclave Apparition", Zone::Battlefield);

    // Slot 1 is "{1}, {T}: Target creature can't block this turn."
    // Zirda's own floor keeps {1} at {1}.
    assert!(!activatable(&mut state, zirda, 1));
    pool(&mut state, P0, &[], 1);
    activate(&mut state, zirda, 1, &[Target::Object(opp_blocker)]);
    settled(&mut state);
    assert!(state.objects.get(zirda).tapped);

    let blockers = legal_blockers(&mut state, attacker);
    assert!(
        !blockers.contains(&opp_blocker),
        "targeted creature cannot block this turn"
    );

    // Next turn, the can't-block restriction has expired.
    let turn_before = state.turn;
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1 && s.turn > turn_before
    });
    let blockers_next = legal_blockers(&mut state, attacker);
    assert!(
        blockers_next.contains(&opp_blocker),
        "can't-block expires after the turn ends"
    );
}

// ---- Absorbing Man -------------------------------------------------------------

fn copied_equipment() -> (GameState, ObjectId, ObjectId) {
    let mut state = plains();
    let copy = put(&mut state, P0, "Absorbing Man", Zone::Battlefield);
    let greaves = put(&mut state, P0, "Lightning Greaves", Zone::Battlefield);
    let starting_turn = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > starting_turn
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Object(greaves))),
            _ => None,
        },
    );
    assert_eq!(state.objects.get(copy).card_def, id("Lightning Greaves"));
    assert!(engine::object_has_type(&state, copy, CardType::Creature));
    assert!(engine::has_effective_subtype(
        &state,
        copy,
        Subtype::Equipment
    ));
    (state, copy, greaves)
}

#[test]
fn equipment_creature_can_target_itself_with_equip_and_resolves_after_restore() {
    let (mut state, copy, _) = copied_equipment();
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, copy, 0, &[Target::Object(copy)]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].targets, [Target::Object(copy)]);
    let snapshot = state.snapshot();
    settled(&mut state);
    assert_eq!(state.objects.get(copy).v4.attached_to, None);
    assert!(state.objects.get(copy).attachments.is_empty());
    let resolved = state.snapshot();

    state.restore(&snapshot);
    settled(&mut state);
    assert_eq!(state.snapshot(), resolved);
}

#[test]
fn equipment_creature_snapshot_rejects_a_forged_self_attachment() {
    let (mut state, copy, _) = copied_equipment();
    let live = state.objects.get_mut(copy);
    live.v4.attached_to = Some(ObjectLinkV4 {
        object: copy,
        zone_change_count: live.zone_change_count,
    });
    live.attachments.push(copy);
    let forged = state.snapshot();
    state.restore(&forged);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: engine::UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == copy
    ));
}

#[test]
fn equipment_creature_cannot_attach_to_another_creature_but_ordinary_equip_can() {
    let (mut state, copy, greaves) = copied_equipment();
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    activate(&mut state, copy, 0, &[Target::Object(host)]);
    settled(&mut state);
    assert_eq!(state.objects.get(copy).v4.attached_to, None);
    assert!(state.objects.get(host).attachments.is_empty());

    activate(&mut state, greaves, 0, &[Target::Object(host)]);
    settled(&mut state);
    assert_eq!(
        state
            .objects
            .get(greaves)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(host)
    );
    assert_eq!(state.objects.get(host).attachments, [greaves]);
}

#[test]
fn equipment_creature_losing_equipment_subtype_in_response_cannot_attach() {
    let (mut state, copy, _) = copied_equipment();
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let reconfiguration = put(&mut state, P1, "Swift Reconfiguration", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W], 0);
    activate(&mut state, copy, 0, &[Target::Object(host)]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    if state.priority_player == P0 {
        act(&mut state, Action::Pass);
    }
    assert_eq!(state.priority_player, P1);
    cast(&mut state, reconfiguration, &[Target::Object(copy)]);
    settled(&mut state);

    assert!(engine::has_effective_subtype(
        &state,
        copy,
        Subtype::Vehicle
    ));
    assert!(!engine::has_effective_subtype(
        &state,
        copy,
        Subtype::Equipment
    ));
    assert_eq!(state.objects.get(copy).v4.attached_to, None);
    assert!(state.objects.get(host).attachments.is_empty());
}

#[test]
fn absorbing_man_reconfigure_lion_sash_attaches_and_detaches_on_copy_expiry() {
    let mut state = plains();
    let ab_man = put(&mut state, P0, "Absorbing Man", Zone::Battlefield);
    let sash = put(&mut state, P0, "Lion Sash", Zone::Battlefield);
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Graveyard);

    let turn_start = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > turn_start
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Object(sash))),
            _ => None,
        },
    );
    assert_eq!(state.objects.get(ab_man).card_def, id("Lion Sash"));
    assert!(engine::object_has_type(&state, ab_man, CardType::Creature));

    // Exile a permanent card so the copied Lion Sash has a +1/+1 counter, then
    // Reconfigure {2} onto Elvish Mystic (CR 702.151b).
    pool(&mut state, P0, &[ManaColor::W], 2);
    activate(&mut state, ab_man, 0, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(state.objects.get(ab_man).counters.plus1_plus1, 1);

    activate(&mut state, ab_man, 1, &[Target::Object(host)]);
    settled(&mut state);
    assert_eq!(
        state
            .objects
            .get(ab_man)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(host),
        "copied Lion Sash attaches via Reconfigure (CR 702.151b)"
    );
    assert_eq!(state.objects.get(host).attachments, [ab_man]);
    assert!(
        !engine::object_has_type(&state, ab_man, CardType::Creature),
        "attached Reconfigure Equipment is not a creature"
    );
    assert_eq!(power_toughness(&state, host), (2, 2));

    // Advance to P0's next Upkeep: Absorbing Man's copy expires in Untap,
    // restoring it to a plain Human Villain creature, and CR 704.5n/704.5p
    // SBAs detach it before P0 receives priority in Upkeep.
    let copied_turn = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Upkeep
                && s.turn > copied_turn
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, _| None,
    );
    assert_eq!(state.objects.get(ab_man).card_def, id("Absorbing Man"));
    assert!(engine::object_has_type(&state, ab_man, CardType::Creature));
    assert!(!engine::has_effective_subtype(
        &state,
        ab_man,
        Subtype::Equipment
    ));
    assert_eq!(
        state.objects.get(ab_man).v4.attached_to,
        None,
        "CR 704.5p SBA must detach reverted Absorbing Man"
    );
    assert!(state.objects.get(host).attachments.is_empty());
    assert_eq!(power_toughness(&state, host), (1, 1));
}

#[test]
fn attached_equipment_becoming_creature_or_losing_equipment_subtype_detaches_via_sba() {
    // 1. Karn, the Great Creator (+1) animates attached Lightning Greaves -> SBA detaches.
    let mut state = plains();
    let greaves = put(&mut state, P0, "Lightning Greaves", Zone::Battlefield);
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    activate(&mut state, greaves, 0, &[Target::Object(host)]);
    settled(&mut state);
    assert!(keyword(&state, host, Keywords::SHROUD));
    assert!(keyword(&state, host, Keywords::HASTE));
    pool(&mut state, P0, &[], 4);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    activate(&mut state, karn, 0, &[Target::Object(greaves)]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, greaves, CardType::Creature));
    assert_eq!(
        state.objects.get(greaves).v4.attached_to,
        None,
        "Karn-animated Equipment must detach via CR 704.5p SBA"
    );
    assert!(state.objects.get(host).attachments.is_empty());
    assert!(!keyword(&state, host, Keywords::SHROUD));
    assert!(!keyword(&state, host, Keywords::HASTE));

    // 2. Oko, Thief of Crowns (+1) turns attached Lightning Greaves into a 3/3 Elk -> SBA detaches.
    let mut state = plains();
    let greaves = put(&mut state, P0, "Lightning Greaves", Zone::Battlefield);
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let oko = put(&mut state, P0, "Oko, Thief of Crowns", Zone::Hand);
    activate(&mut state, greaves, 0, &[Target::Object(host)]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, oko, &[]);
    settled(&mut state);
    activate(&mut state, oko, 1, &[Target::Object(greaves)]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, greaves, CardType::Creature));
    assert_eq!(
        state.objects.get(greaves).v4.attached_to,
        None,
        "Oko-Elk'd Equipment must detach via CR 704.5p SBA"
    );
    assert!(state.objects.get(host).attachments.is_empty());
    assert!(!keyword(&state, host, Keywords::SHROUD));

    // 3. Attached Lightning Greaves losing Equipment subtype (Swift Reconfiguration)
    //    while remaining a noncreature Vehicle -> CR 704.5p SBA detaches it.
    let mut state = plains();
    let greaves = put(&mut state, P0, "Lightning Greaves", Zone::Battlefield);
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    activate(&mut state, greaves, 0, &[Target::Object(host)]);
    settled(&mut state);
    assert!(keyword(&state, host, Keywords::SHROUD));
    let reconfig = put(&mut state, P0, "Swift Reconfiguration", Zone::Battlefield);
    let greaves_zcc = state.objects.get(greaves).zone_change_count;
    state.objects.get_mut(reconfig).v4.attached_to = Some(ObjectLinkV4 {
        object: greaves,
        zone_change_count: greaves_zcc,
    });
    state.objects.get_mut(greaves).attachments.push(reconfig);
    assert!(!engine::object_has_type(
        &state,
        greaves,
        CardType::Creature
    ));
    assert!(!engine::has_effective_subtype(
        &state,
        greaves,
        Subtype::Equipment
    ));
    // Defense in depth: even before SBAs run, non-Equipment grants no Shroud.
    assert!(!keyword(&state, host, Keywords::SHROUD));
    // Trigger an SBA checkpoint by resolving an unrelated event.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(bolt, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(
        state.objects.get(greaves).v4.attached_to,
        None,
        "Equipment losing Equipment subtype must detach via CR 704.5p SBA"
    );
    assert!(state.objects.get(host).attachments.is_empty());
    assert!(!keyword(&state, host, Keywords::SHROUD));

    // 4. Unable to Scream attached to an attached Lion Sash -> becomes a 0/2 Toy
    //    creature without Reconfigure -> CR 704.5p SBA detaches it.
    let mut state = plains();
    let sash = put(&mut state, P0, "Lion Sash", Zone::Battlefield);
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    activate(&mut state, sash, 0, &[Target::Object(ring)]);
    settled(&mut state);
    activate(&mut state, sash, 1, &[Target::Object(host)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, host), (2, 2));
    assert!(!engine::object_has_type(&state, sash, CardType::Creature));
    let scream = put(&mut state, P0, "Unable to Scream", Zone::Battlefield);
    let sash_zcc = state.objects.get(sash).zone_change_count;
    state.objects.get_mut(scream).v4.attached_to = Some(ObjectLinkV4 {
        object: sash,
        zone_change_count: sash_zcc,
    });
    state.objects.get_mut(sash).attachments.push(scream);
    assert!(engine::object_has_type(&state, sash, CardType::Creature));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(bolt, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(
        state.objects.get(sash).v4.attached_to,
        None,
        "Unable to Scream on attached Lion Sash must detach via CR 704.5p SBA"
    );
    assert!(state.objects.get(host).attachments.is_empty());
    assert_eq!(power_toughness(&state, host), (1, 1));
}

#[test]
fn forged_non_self_illegal_equipment_attachment_snapshot_is_refused() {
    // 1. Forged snapshot attaching a non-Reconfigure creature Equipment
    //    (Absorbing Man copying Lightning Greaves) to Elvish Mystic.
    let (mut state, copy, _) = copied_equipment();
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let host_zcc = state.objects.get(host).zone_change_count;
    state.objects.get_mut(copy).v4.attached_to = Some(ObjectLinkV4 {
        object: host,
        zone_change_count: host_zcc,
    });
    state.objects.get_mut(host).attachments.push(copy);

    // Defense in depth: even before advancing, illegal creature Equipment
    // must not grant Equipment buffs.
    assert!(
        !keyword(&state, host, Keywords::SHROUD),
        "illegal creature Equipment must not grant Shroud"
    );
    assert!(
        !keyword(&state, host, Keywords::HASTE),
        "illegal creature Equipment must not grant Haste"
    );

    let forged = state.snapshot();
    state.restore(&forged);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: engine::UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == copy
    ));

    // 2. Forged snapshot attaching a plain creature (non-Equipment) to another creature.
    let mut state = plains();
    let a = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let b = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let b_zcc = state.objects.get(b).zone_change_count;
    state.objects.get_mut(a).v4.attached_to = Some(ObjectLinkV4 {
        object: b,
        zone_change_count: b_zcc,
    });
    state.objects.get_mut(b).attachments.push(a);
    let forged = state.snapshot();
    state.restore(&forged);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: engine::UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == a
    ));

    // 3. Forged two-object attachment cycle is rejected by validate_attachment_relations.
    let mut state = plains();
    let eq_a = put(&mut state, P0, "Lightning Greaves", Zone::Battlefield);
    let eq_b = put(&mut state, P0, "Umezawa's Jitte", Zone::Battlefield);
    let a_zcc = state.objects.get(eq_a).zone_change_count;
    let b_zcc = state.objects.get(eq_b).zone_change_count;
    state.objects.get_mut(eq_a).v4.attached_to = Some(ObjectLinkV4 {
        object: eq_b,
        zone_change_count: b_zcc,
    });
    state.objects.get_mut(eq_b).attachments.push(eq_a);
    state.objects.get_mut(eq_b).v4.attached_to = Some(ObjectLinkV4 {
        object: eq_a,
        zone_change_count: a_zcc,
    });
    state.objects.get_mut(eq_a).attachments.push(eq_b);
    assert!(state.validate_attachment_relations().is_err());
}

#[test]
fn impossible_equip_preserves_existing_attachment_cr_701_3b() {
    let mut state = plains();
    let jitte = put(&mut state, P0, "Umezawa's Jitte", Zone::Battlefield);
    let host_a = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let host_b = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let giver = put(&mut state, P0, "Giver of Runes", Zone::Battlefield);

    // Attach Jitte to host_a (Equip {2} is ability index 3).
    pool(&mut state, P0, &[], 6);
    activate(&mut state, jitte, 3, &[Target::Object(host_a)]);
    settled(&mut state);
    assert_eq!(
        state
            .objects
            .get(jitte)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(host_a)
    );

    // Activate Equip targeting host_b; in response Giver of Runes gives host_b
    // protection from colorless so attaching to host_b becomes impossible.
    // Under CR 701.3b, Jitte stays attached to host_a.
    activate(&mut state, jitte, 3, &[Target::Object(host_b)]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    activate(&mut state, giver, 0, &[Target::Object(host_b)]);
    settle_option(&mut state, 5); // colorless
    assert_eq!(
        state
            .objects
            .get(jitte)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(host_a),
        "CR 701.3b: impossible equip must preserve existing attachment"
    );
    assert_eq!(state.objects.get(host_a).attachments, [jitte]);
    assert!(state.objects.get(host_b).attachments.is_empty());

    // Untargeted attach with an existing attachment (CR 701.3b): P1's Umezawa's
    // Jitte is attached to P1's Elvish Mystic. P0 casts kicked Thieving Skydiver
    // targeting Jitte, then gives Skydiver protection from colorless while
    // SkydiverEtbV1 is on the stack. SkydiverEtbV1's target (Jitte) stays legal,
    // P0 gains control of Jitte, can_attach_equipment_to_host rejects Skydiver,
    // and Jitte remains attached to P1's Elvish Mystic.
    let mut state = plains();
    let opp_jitte = put(&mut state, P1, "Umezawa's Jitte", Zone::Battlefield);
    let opp_host = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    state.active_player = P1;
    state.priority_player = P1;
    pool(&mut state, P1, &[], 2);
    activate(&mut state, opp_jitte, 3, &[Target::Object(opp_host)]);
    settled(&mut state);
    assert_eq!(
        state
            .objects
            .get(opp_jitte)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(opp_host)
    );
    state.active_player = P0;
    state.priority_player = P0;
    let giver = put(&mut state, P0, "Giver of Runes", Zone::Battlefield);
    let skydiver = put(&mut state, P0, "Thieving Skydiver", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, skydiver, &[]);
    run_until(
        &mut state,
        |s| zone(s, skydiver) == Zone::Battlefield && !s.stack.is_empty(),
        |_, decision| match decision {
            Decision::ChooseKicker { .. } => Some(Action::ChooseKicker(true)),
            Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
            Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Object(opp_jitte))),
            _ => None,
        },
    );
    activate(&mut state, giver, 0, &[Target::Object(skydiver)]);
    settle_option(&mut state, 5); // protection from colorless
    assert_eq!(state.objects.get(opp_jitte).controller, P0);
    assert_eq!(
        state
            .objects
            .get(opp_jitte)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(opp_host),
        "CR 701.3b: impossible untargeted Skydiver attach must preserve Jitte's existing attachment"
    );
    assert_eq!(state.objects.get(opp_host).attachments, [opp_jitte]);
    assert!(state.objects.get(skydiver).attachments.is_empty());
}

#[test]
fn karn_animated_attached_lion_sash_and_copy_detach_and_lose_buff() {
    // 1. Plain Lion Sash with a +1/+1 counter attached via Reconfigure to Elvish
    //    Mystic, then animated by Karn +1: CR 613.4/613.7 + 704.5n must make it
    //    a creature, unattach it, and remove the +1/+1 host buff.
    let mut state = plains();
    let sash = put(&mut state, P0, "Lion Sash", Zone::Battlefield);
    state.objects.get_mut(sash).counters.plus1_plus1 = 1;
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    pool(&mut state, P0, &[], 6);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    activate(&mut state, sash, 1, &[Target::Object(host)]);
    settled(&mut state);
    assert_eq!(
        state
            .objects
            .get(sash)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(host)
    );
    assert_eq!(
        (
            engine::effective_power(&state, host),
            engine::effective_toughness(&state, host),
        ),
        (2, 2)
    );
    activate(&mut state, karn, 0, &[Target::Object(sash)]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(sash).v4.attached_to,
        None,
        "CR 704.5n: Karn-animated attached Lion Sash must detach"
    );
    assert!(state.objects.get(host).attachments.is_empty());
    assert_eq!(
        (
            engine::effective_power(&state, host),
            engine::effective_toughness(&state, host),
        ),
        (1, 1),
        "host must lose Lion Sash's +1/+1 buff after Karn detaches it"
    );

    // 2. Absorbing Man copying Lion Sash (with a +1/+1 counter) attached via
    //    Reconfigure, then animated by Karn +1: must likewise detach and drop buff.
    let mut state = plains();
    let ab_man = put(&mut state, P0, "Absorbing Man", Zone::Battlefield);
    state.objects.get_mut(ab_man).counters.plus1_plus1 = 1;
    let bf_sash = put(&mut state, P0, "Lion Sash", Zone::Battlefield);
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    let turn_start = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > turn_start
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Object(bf_sash))),
            _ => None,
        },
    );
    assert_eq!(state.objects.get(ab_man).card_def, id("Lion Sash"));
    pool(&mut state, P0, &[], 6);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    activate(&mut state, ab_man, 1, &[Target::Object(host)]);
    settled(&mut state);
    assert_eq!(
        state
            .objects
            .get(ab_man)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(host)
    );
    assert_eq!(power_toughness(&state, host), (2, 2));
    activate(&mut state, karn, 0, &[Target::Object(ab_man)]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(ab_man).v4.attached_to,
        None,
        "CR 704.5n: Karn-animated Absorbing Man copying attached Lion Sash must detach"
    );
    assert!(state.objects.get(host).attachments.is_empty());
    assert_eq!(power_toughness(&state, host), (1, 1));

    // 3. Liquimetal Coating + Karn +1 animating Lurking Skirge, followed by an
    //    opponent's creature dying: Skirge's later type-setting effect makes it
    //    a creature and nothing else (no longer an Artifact or Enchantment).
    let mut state = plains();
    let skirge = put(&mut state, P0, "Lurking Skirge", Zone::Battlefield);
    let coating = put(&mut state, P0, "Liquimetal Coating", Zone::Battlefield);
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let victim = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 4);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    activate(&mut state, coating, 0, &[Target::Object(skirge)]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, skirge, CardType::Artifact));
    activate(&mut state, karn, 0, &[Target::Object(skirge)]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, skirge, CardType::Creature));
    assert!(engine::object_has_type(&state, skirge, CardType::Artifact));
    cast(&mut state, bolt, &[Target::Object(victim)]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, skirge, CardType::Creature));
    assert!(
        !engine::object_has_type(&state, skirge, CardType::Artifact),
        "Lurking Skirge's later type-setting override must clear Artifact"
    );
    assert!(
        !engine::object_has_type(&state, skirge, CardType::Enchantment),
        "Lurking Skirge's later type-setting override must clear Enchantment"
    );
}

#[test]
fn create_token_and_attach_source_respects_equipment_legality() {
    // 1. Karn-animated Cori-Steel Cutter triggers Flurry (CreateTokenAndAttachSource):
    //    creates the 1/1 Monk token without attaching the creature Equipment.
    let mut state = plains();
    let cutter = put(&mut state, P0, "Cori-Steel Cutter", Zone::Battlefield);
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 4);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    activate(&mut state, karn, 0, &[Target::Object(cutter)]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, cutter, CardType::Creature));

    // Second spell this turn triggers Cori-Steel Cutter's Flurry; choose
    // option 0 ("create a 1/1 Monk and attach").
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settle_option(&mut state, 0);
    let monks = named(&state, P0, "Monk Token");
    assert_eq!(monks.len(), 1, "Flurry still creates the 1/1 Monk token");
    assert_eq!(
        state.objects.get(cutter).v4.attached_to,
        None,
        "creature Cori-Steel Cutter must not attach to the Monk token"
    );
    assert!(state.objects.get(monks[0]).attachments.is_empty());

    // 2. Karn-animated Cori-Steel Cutter triggers Flurry, and in response P1
    //    flashes Swift Reconfiguration onto it (turning it into a noncreature
    //    Vehicle without Subtype::Equipment): creates the 1/1 Monk token
    //    without attaching the non-Equipment Vehicle.
    let mut state = plains();
    let cutter = put(&mut state, P0, "Cori-Steel Cutter", Zone::Battlefield);
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let reconfig = put(&mut state, P1, "Swift Reconfiguration", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 4);
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    activate(&mut state, karn, 0, &[Target::Object(cutter)]);
    settled(&mut state);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    if state.priority_player == P0 {
        act(&mut state, Action::Pass);
    }
    assert_eq!(state.priority_player, P1);
    cast(&mut state, reconfig, &[Target::Object(cutter)]);
    settle_option(&mut state, 0);
    let monks = named(&state, P0, "Monk Token");
    assert_eq!(monks.len(), 1, "Flurry still creates the 1/1 Monk token");
    assert!(!engine::object_has_type(&state, cutter, CardType::Creature));
    assert!(!engine::has_effective_subtype(
        &state,
        cutter,
        Subtype::Equipment
    ));
    assert_eq!(
        state.objects.get(cutter).v4.attached_to,
        None,
        "Vehicle Cori-Steel Cutter must not attach to the Monk token"
    );
    assert!(state.objects.get(monks[0]).attachments.is_empty());
}

#[test]
fn formalwear_iron_man_skydiver_and_sword_return_enforce_equipment_legality() {
    // 1. Pre-War Formalwear (FormalwearV1) returning Tourach, Dread Cantor
    //    (MV 2, protection from white): returns Tourach without attaching
    //    white Pre-War Formalwear and without halting (CR 702.16d / 701.3b).
    let mut state = plains();
    let tourach = put(&mut state, P0, "Tourach, Dread Cantor", Zone::Graveyard);
    let formalwear = put(&mut state, P0, "Pre-War Formalwear", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, formalwear, &[]);
    settle_choosing(&mut state, tourach);
    assert_eq!(zone(&state, tourach), Zone::Battlefield);
    assert_eq!(
        state.objects.get(formalwear).v4.attached_to,
        None,
        "white Pre-War Formalwear must not attach to protection-from-white Tourach"
    );
    assert!(state.objects.get(tourach).attachments.is_empty());

    // 2. Iron Man Armor (IronManEquipV1) animates, activates Equip {2}, and in
    //    response P1 flashes Swift Reconfiguration onto it: becomes a noncreature
    //    Vehicle without Subtype::Equipment and must not attach.
    let mut state = plains();
    let armor = put(&mut state, P0, "Iron Man Armor", Zone::Battlefield);
    let host = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let reconfig = put(&mut state, P1, "Swift Reconfiguration", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    pool(&mut state, P1, &[ManaColor::W], 0);
    activate(&mut state, armor, 0, &[]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, armor, CardType::Creature));
    activate(&mut state, armor, 1, &[Target::Object(host)]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    if state.priority_player == P0 {
        act(&mut state, Action::Pass);
    }
    assert_eq!(state.priority_player, P1);
    cast(&mut state, reconfig, &[Target::Object(armor)]);
    settled(&mut state);
    assert!(!engine::object_has_type(&state, armor, CardType::Creature));
    assert!(!engine::has_effective_subtype(
        &state,
        armor,
        Subtype::Equipment
    ));
    assert_eq!(
        state.objects.get(armor).v4.attached_to,
        None,
        "Vehicle Iron Man Armor must not attach via IronManEquipV1"
    );
    assert!(state.objects.get(host).attachments.is_empty());

    // 3a. Thieving Skydiver (SkydiverEtbV1) stealing Lion Sash attaches it per
    //     CR 702.151b, causing Lion Sash to stop being a creature while attached.
    let mut state = plains();
    let opp_sash = put(&mut state, P1, "Lion Sash", Zone::Battlefield);
    let skydiver = put(&mut state, P0, "Thieving Skydiver", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, skydiver, &[]);
    assert!(matches!(next(&mut state), Decision::ChooseKicker { .. }));
    act(&mut state, Action::ChooseKicker(true));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption { .. }
    ));
    act(&mut state, Action::ChooseEffectOption(1)); // X = 2 (MV of Lion Sash)
    settle_choosing(&mut state, opp_sash);
    assert_eq!(state.objects.get(opp_sash).controller, P0);
    assert_eq!(
        state
            .objects
            .get(opp_sash)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(skydiver),
        "CR 702.151b: Thieving Skydiver stealing Lion Sash must attach it"
    );
    assert_eq!(state.objects.get(skydiver).attachments, [opp_sash]);
    assert!(!engine::object_has_type(
        &state,
        opp_sash,
        CardType::Creature
    ));

    // 3b. Thieving Skydiver stealing Lion Sash that lost Equipment via Swift
    //     Reconfiguration in response gains control without attaching it.
    let mut state = plains();
    let opp_sash = put(&mut state, P1, "Lion Sash", Zone::Battlefield);
    let skydiver = put(&mut state, P0, "Thieving Skydiver", Zone::Hand);
    let reconfig = put(&mut state, P1, "Swift Reconfiguration", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, skydiver, &[]);
    run_until(
        &mut state,
        |s| zone(s, skydiver) == Zone::Battlefield && !s.stack.is_empty(),
        |_, decision| match decision {
            Decision::ChooseKicker { .. } => Some(Action::ChooseKicker(true)),
            Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
            Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Object(opp_sash))),
            _ => None,
        },
    );
    if state.priority_player == P0 {
        act(&mut state, Action::Pass);
    }
    assert_eq!(state.priority_player, P1);
    cast(&mut state, reconfig, &[Target::Object(opp_sash)]);
    settled(&mut state);
    assert_eq!(state.objects.get(opp_sash).controller, P0);
    assert!(!engine::has_effective_subtype(
        &state,
        opp_sash,
        Subtype::Equipment
    ));
    assert_eq!(
        state.objects.get(opp_sash).v4.attached_to,
        None,
        "non-Equipment Vehicle Lion Sash must not attach to Thieving Skydiver"
    );
    assert!(state.objects.get(skydiver).attachments.is_empty());

    // 4. Sword of the Meek (SwordReturnV1) returning for a 1/1 creature that
    //    gained protection from colorless in response stays on battlefield unattached.
    let mut state = plains();
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Graveyard);
    let giver = put(&mut state, P0, "Giver of Runes", Zone::Battlefield);
    let mystic = put(&mut state, P0, "Elvish Mystic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, mystic, &[]);
    // Resolve Elvish Mystic so Sword of the Meek's trigger is on the stack.
    run_until(
        &mut state,
        |s| zone(s, mystic) == Zone::Battlefield && !s.stack.is_empty(),
        |_, _| None,
    );
    activate(&mut state, giver, 0, &[Target::Object(mystic)]);
    // Answer Giver of Runes with colorless (5), then Sword of the Meek with return (1).
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption {
            option_count: 6, ..
        } => Action::ChooseEffectOption(5),
        Decision::ChooseEffectOption {
            option_count: 2, ..
        } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, sword), Zone::Battlefield);
    assert_eq!(
        state.objects.get(sword).v4.attached_to,
        None,
        "Sword of the Meek must not attach to a protection-from-colorless creature"
    );
    assert!(state.objects.get(mystic).attachments.is_empty());
}

#[test]
fn absorbing_man_copies_artifact_enchantment_or_land_with_exceptions_and_reverts() {
    // 1. Copies legendary artifact (Mox Opal): name stays "Absorbing Man" (no
    //    Legend rule!), stays Legendary 4/4 Human Villain Artifact Creature with
    //    Vigilance, gains Mox Opal's mana ability, persists on opponent's turn,
    //    and reverts at start of controller's next turn before Main1!
    let mut state = plains();
    let ab_man = put(&mut state, P0, "Absorbing Man", Zone::Battlefield);
    let opal = put(&mut state, P0, "Mox Opal", Zone::Battlefield);
    put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let opp_elf = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    let opp_rancor = put(&mut state, P1, "Rancor", Zone::Battlefield);
    let elf_zcc = state.objects.get(opp_elf).zone_change_count;
    state.objects.get_mut(opp_rancor).v4.attached_to = Some(mtg_kernel::state::ObjectLinkV4 {
        object: opp_elf,
        zone_change_count: elf_zcc,
    });
    state.objects.get_mut(opp_elf).attachments.push(opp_rancor);

    // Advance to P0's next Main1 so Absorbing Man's precombat main trigger fires.
    let mut saw_targets = false;
    let turn_start = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > turn_start
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets {
                legal_targets,
                can_finish,
                ..
            } => {
                assert!(*can_finish, "up to one target allows declining");
                assert!(legal_targets.contains(&Target::Object(opal)));
                assert!(
                    !legal_targets.contains(&Target::Object(opp_rancor)),
                    "Aura enchantments are illegal targets"
                );
                assert!(
                    !legal_targets.contains(&Target::Object(opp_elf)),
                    "plain creatures are illegal targets"
                );
                saw_targets = true;
                Some(Action::ChooseTarget(Target::Object(opal)))
            }
            _ => None,
        },
    );
    assert!(saw_targets);
    // Both Mox Opal and Absorbing Man survive (name stays "Absorbing Man").
    assert_eq!(zone(&state, opal), Zone::Battlefield);
    assert_eq!(zone(&state, ab_man), Zone::Battlefield);
    assert_eq!(engine::effective_name(&state, ab_man), "Absorbing Man");
    assert_eq!(power_toughness(&state, ab_man), (4, 4));
    assert!(keyword(&state, ab_man, Keywords::VIGILANCE));
    assert!(
        engine::effective_keywords(&state, ab_man, Keywords::VIGILANCE).has(Keywords::VIGILANCE),
        "effective_keywords must match has_effective_keyword for Absorbing Man Vigilance"
    );
    assert!(engine::effective_supertypes(&state, ab_man).contains(&Supertype::Legendary));
    assert!(engine::object_has_type(&state, ab_man, CardType::Artifact));
    assert!(engine::object_has_type(&state, ab_man, CardType::Creature));
    assert!(engine::has_effective_subtype(
        &state,
        ab_man,
        Subtype::Human
    ));
    assert!(engine::has_effective_subtype(
        &state,
        ab_man,
        Subtype::Villain
    ));

    // With 3 artifacts (Mox Opal + Sol Ring + Artifact Creature Absorbing Man),
    // Absorbing Man can activate Mox Opal's metalcraft mana ability!
    engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(ab_man, ManaColor::G),
    )
    .expect("Absorbing Man copying Mox Opal taps for mana");
    assert_eq!(mana(&state, P0, ManaColor::G), 1);

    // Persists through opponent's turn.
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert!(
        engine::object_has_type(&state, ab_man, CardType::Artifact),
        "copy lasts until controller's next turn"
    );

    // Expires at start of P0's next turn (before Main1), then Main1 triggers
    // again; declining (FinishEffectSelection) leaves him as plain Absorbing Man!
    let mut triggered_again = false;
    let opp_turn = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > opp_turn
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets {
                can_finish: true, ..
            } => {
                triggered_again = true;
                Some(Action::FinishEffectSelection)
            }
            _ => None,
        },
    );
    assert!(triggered_again);
    assert!(
        !engine::object_has_type(&state, ab_man, CardType::Artifact),
        "copy reverted at start of P0's next turn"
    );
    assert_eq!(power_toughness(&state, ab_man), (4, 4));
    assert!(keyword(&state, ab_man, Keywords::VIGILANCE));
    assert!(
        engine::effective_keywords(&state, ab_man, Keywords::VIGILANCE).has(Keywords::VIGILANCE),
        "effective_keywords must match has_effective_keyword for Absorbing Man Vigilance"
    );

    // 2. Copies non-Aura Enchantment (Goblin Bombardment): gains its activated
    //    ability, and leaving the battlefield reverts it to Absorbing Man.
    let mut state = plains();
    let ab_man = put(&mut state, P0, "Absorbing Man", Zone::Battlefield);
    let bombard = put(&mut state, P0, "Goblin Bombardment", Zone::Battlefield);
    let fodder = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let turn_start = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > turn_start
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(bombard)) =>
            {
                Some(Action::ChooseTarget(Target::Object(bombard)))
            }
            _ => None,
        },
    );
    assert!(engine::object_has_type(
        &state,
        ab_man,
        CardType::Enchantment
    ));
    assert!(engine::object_has_type(&state, ab_man, CardType::Creature));
    assert!(activatable(&mut state, ab_man, 0));
    activate(&mut state, ab_man, 0, &[Target::Player(P1)]);
    settle_choosing(&mut state, fodder);
    assert_eq!(zone(&state, fodder), Zone::Graveyard);
    assert_eq!(life(&state, P1), 19);

    // Exiling Absorbing Man restores its original CardDef in exile.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(ab_man, Zone::Exile));
    assert_eq!(
        CARD_DEFS[usize::from(state.objects.get(ab_man).card_def)].name,
        "Absorbing Man"
    );

    // 3. Copies Land (Volcanic Island): keeps Island + Mountain + Human + Villain
    //    and taps for blue or red mana when not summoning sick.
    let mut state = plains();
    let ab_man = put(&mut state, P0, "Absorbing Man", Zone::Battlefield);
    let volc = put(&mut state, P0, "Volcanic Island", Zone::Battlefield);
    let turn_start = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > turn_start
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(volc)) =>
            {
                Some(Action::ChooseTarget(Target::Object(volc)))
            }
            _ => None,
        },
    );
    assert!(engine::object_has_type(&state, ab_man, CardType::Land));
    assert!(engine::object_has_type(&state, ab_man, CardType::Creature));
    assert!(engine::has_effective_subtype(
        &state,
        ab_man,
        Subtype::Island
    ));
    assert!(engine::has_effective_subtype(
        &state,
        ab_man,
        Subtype::Mountain
    ));
    assert!(engine::has_effective_subtype(
        &state,
        ab_man,
        Subtype::Human
    ));
    assert!(engine::has_effective_subtype(
        &state,
        ab_man,
        Subtype::Villain
    ));
    engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(ab_man, ManaColor::U),
    )
    .expect("Absorbing Man copying Volcanic Island taps for {U}");
    assert_eq!(mana(&state, P0, ManaColor::U), 1);

    // 4. Stacked Absorbing Man -> Thespian's Stage -> Volcanic Island unwinds
    //    e7a before c8c on departure so the physical card arrives as Absorbing Man.
    let mut state = plains();
    let ab_man = put(&mut state, P0, "Absorbing Man", Zone::Battlefield);
    let stage = put(&mut state, P0, "Thespian's Stage", Zone::Battlefield);
    let volc = put(&mut state, P0, "Volcanic Island", Zone::Battlefield);
    let turn_start = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > turn_start
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(stage)) =>
            {
                Some(Action::ChooseTarget(Target::Object(stage)))
            }
            _ => None,
        },
    );
    pool(&mut state, P0, &[], 2);
    activate(&mut state, ab_man, 0, &[Target::Object(volc)]);
    settled(&mut state);
    assert_eq!(
        CARD_DEFS[usize::from(state.objects.get(ab_man).card_def)].name,
        "Volcanic Island"
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(ab_man, Zone::Graveyard),
    );
    assert_eq!(zone(&state, ab_man), Zone::Graveyard);
    assert_eq!(
        CARD_DEFS[usize::from(state.objects.get(ab_man).card_def)].name,
        "Absorbing Man",
        "stacked e7a copy on top of Absorbing Man must unwind to physical Absorbing Man"
    );
}

// ---- Public Sessions -----------------------------------------------------------

#[derive(Default)]
struct C8cEvents {
    casts: [usize; C8C_CARDS.len()],
}

impl C8cEvents {
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
        if let Some(index) = C8C_CARDS.iter().position(|name| *name == card.name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c8c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let jeskai_tempo = deck(&[
        ("Plateau", 8),
        ("Tundra", 8),
        ("Volcanic Island", 4),
        ("Sol Ring", 4),
        ("Mox Sapphire", 4),
        ("Pyrogoyf", 5),
        ("Elite Spellbinder", 6),
        ("Zirda, the Dawnwaker", 5),
        ("Umezawa's Jitte", 4),
        ("Lightning Bolt", 4),
    ]);
    let simic_owls = deck(&[
        ("Tropical Island", 12),
        ("Polluted Delta", 6),
        ("Sol Ring", 4),
        ("Mox Sapphire", 4),
        ("Wan Shi Tong, All-Knowing", 5),
        ("Wan Shi Tong, Librarian", 6),
        ("Thieving Skydiver", 6),
        ("Absorbing Man", 6),
        ("Brainstorm", 4),
    ]);
    let mut events = C8cEvents::default();
    // 2 seats x 3 seeds x 2 policies x (original + replay) = 24 games total.
    for pair in [
        [jeskai_tempo.clone(), simic_owls.clone()],
        [simic_owls, jeskai_tempo],
    ] {
        for seed in 0x4338_6300_u64..0x4338_6303 {
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
    for (name, casts) in C8C_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} was never cast in a public session");
    }
}

#[test]
fn zirda_companion_public_session_both_seats() {
    let zirda_deck = CustomDeckV1 {
        cards: [
            ("Plateau", 12),
            ("Plains", 6),
            ("Mountain", 6),
            ("Sol Ring", 4),
            ("Skullclamp", 4),
            ("Umezawa's Jitte", 4),
            ("Mother of Runes", 4),
            ("Twinshot Sniper", 4),
            ("Lightning Bolt", 6),
        ]
        .into_iter()
        .map(|(name, count)| CustomCardCountV1 {
            name: name.to_owned(),
            count,
        })
        .collect(),
        companion: Some("Zirda, the Dawnwaker".to_owned()),
    };
    let opponent_deck = deck(&[
        ("Tropical Island", 16),
        ("Sol Ring", 4),
        ("Thieving Skydiver", 8),
        ("Absorbing Man", 8),
        ("Wan Shi Tong, Librarian", 8),
    ]);
    let mut companion_uses = 0usize;
    for pair in [
        [zirda_deck.clone(), opponent_deck.clone()],
        [opponent_deck, zirda_deck],
    ] {
        for seed in 0x4338_6390_u64..0x4338_6392 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
                companion_uses += played.companion_actions;
            }
        }
    }
    assert!(
        companion_uses > 0,
        "Zirda companion special action was never used in a public session"
    );
}

/// Without a session stack-size cap, the engine itself imposes no stack cap:
/// with Zirda, the Dawnwaker and Basalt Monolith, stacking untap activations
/// grows the stack well past 600 items without halting.
#[test]
fn zirda_monolith_untap_activations_do_not_halt_without_a_session_stack_cap() {
    let items = 640usize;
    let mut state = plains();
    put(&mut state, P0, "Zirda, the Dawnwaker", Zone::Battlefield);
    let monolith = put(&mut state, P0, "Basalt Monolith", Zone::Battlefield);
    for _ in 0..items {
        pool(&mut state, P0, &[], 1);
        activate(&mut state, monolith, 0, &[]);
    }
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, monolith, 0));
    assert_eq!(state.stack.len(), items);
    assert_eq!(state.engine.halted, None);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
}
