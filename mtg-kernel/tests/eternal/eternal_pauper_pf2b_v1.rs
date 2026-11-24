//! Pauper field wave Pf2b: native rules and public-session tests for the eight
//! cycling/replicate/graveyard-trigger/changeling/suspend/adventure/omen cards
//! (Memory Leak, Gigadrowse, Aurora Eidolon, Sandstorm Eidolon, Rooftop
//! Percher, Rift Sower, Pegasus Guardian // Rescue the Foal + Pegasus Token,
//! Stormshriek Feral // Flush Out).
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::{replicate_cost, EternalStateV1};
use mtg_kernel::eternal_pauper_pf2b_v1::Pf2bStateV1;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const PF2B_CARDS: [&str; 8] = [
    "Memory Leak",
    "Gigadrowse",
    "Aurora Eidolon",
    "Sandstorm Eidolon",
    "Rooftop Percher",
    "Rift Sower",
    "Pegasus Guardian",
    "Stormshriek Feral",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5046_0000_0000 + 0x3262,
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

fn pool(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[ManaColor::C.pool_index()] += colorless;
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
}

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
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

fn keyword(state: &GameState, object: ObjectId, kw: Keywords) -> bool {
    engine::has_effective_keyword(state, object, kw)
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

fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
    }
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
    panic!("did not reach target state");
}

fn to_next_turn(state: &mut GameState) {
    let turn = state.turn;
    let active = state.active_player;
    run_until(
        state,
        |s| (s.turn != turn || s.active_player != active) && s.step == Step::Main1,
        |_, _| None,
    );
}

fn to_next_own_main(state: &mut GameState) {
    let turn = state.turn;
    let owner = state.active_player;
    run_until(
        state,
        |s| s.turn > turn && s.active_player == owner && s.step == Step::Main1,
        |_, _| None,
    );
}

fn deck(rows: &[(&str, u32)]) -> CustomDeckV1 {
    CustomDeckV1 {
        cards: rows
            .iter()
            .map(|(name, count)| CustomCardCountV1 {
                name: (*name).to_owned(),
                count: *count,
            })
            .collect(),
        companion: None,
    }
}

// ---- Catalog & state invariants -----------------------------------------------------

#[test]
fn pf2b_cards_are_full_and_empty_state_hashes_zero_bytes() {
    for name in PF2B_CARDS {
        assert_eq!(
            def(name).capability,
            CardCapability::Full,
            "{name} must be Full"
        );
    }
    assert_eq!(def("Pegasus Token").capability, CardCapability::Full);
    assert!(def("Pegasus Token").is_token);
    assert_eq!(def("Pegasus Token").subtypes, &[Subtype::Pegasus]);
    assert!(def("Pegasus Token").keywords.has(Keywords::FLYING));

    assert!(replicate_cost("Gigadrowse").is_some());
    assert_eq!(def("Gigadrowse").types, &[CardType::Instant]);
    assert_eq!(def("Memory Leak").types, &[CardType::Sorcery]);
    assert_eq!(
        (def("Rift Sower").power, def("Rift Sower").toughness),
        (Some(1), Some(3))
    );
    assert_eq!(def("Rift Sower").subtypes, &[Subtype::Elf, Subtype::Druid]);
    assert!(def("Rooftop Percher").changeling);
    assert!(def("Rooftop Percher").keywords.has(Keywords::FLYING));
    assert!(def("Pegasus Guardian").adventure.is_some());
    assert_eq!(def("Pegasus Guardian").subtypes, &[Subtype::Pegasus]);
    assert!(def("Stormshriek Feral").omen.is_some());
    assert!(def("Stormshriek Feral").keywords.has(Keywords::FLYING));
    assert!(def("Stormshriek Feral").keywords.has(Keywords::HASTE));

    #[derive(Default)]
    struct ByteCounter(usize);
    impl Hasher for ByteCounter {
        fn finish(&self) -> u64 {
            self.0 as u64
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0 += bytes.len();
        }
    }
    let empty = Pf2bStateV1::default();
    let mut counter = ByteCounter::default();
    empty.hash(&mut counter);
    assert_eq!(counter.0, 0);
    assert_eq!(serde_json::to_string(&empty).unwrap(), "{}");
    let eternal_json = serde_json::to_string(&EternalStateV1::default()).unwrap();
    assert!(!eternal_json.contains("\"pf2b\""));
}

// ---- Memory Leak --------------------------------------------------------------------

#[test]
fn memory_leak_exiles_nonland_from_hand_or_graveyard_and_cycles() {
    // 1. Reveals hand and exiles a nonland from opponent's hand (lands excluded).
    let mut state = plains();
    let opp_land = put(&mut state, P1, "Plains", Zone::Hand);
    let opp_bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let opp_gy = put(&mut state, P1, "Counterspell", Zone::Graveyard);
    let leak = put(&mut state, P0, "Memory Leak", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, leak, &[Target::Player(P1)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(opp_bolt)));
            assert!(legal_targets.contains(&Target::Object(opp_gy)));
            assert!(
                !legal_targets.contains(&Target::Object(opp_land)),
                "lands must not be selectable"
            );
            Action::ChooseEffectTarget(Target::Object(opp_bolt))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, opp_bolt), Zone::Exile);
    assert_eq!(zone(&state, opp_land), Zone::Hand);
    assert_eq!(zone(&state, opp_gy), Zone::Graveyard);

    // 2. Can exile a nonland from opponent's graveyard even when hand is empty.
    let leak2 = put(&mut state, P0, "Memory Leak", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, leak2, &[Target::Player(P1)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(opp_gy)]);
            Action::ChooseEffectTarget(Target::Object(opp_gy))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, opp_gy), Zone::Exile);

    // 3. No nonland cards in opponent's hand or graveyard -> resolves cleanly.
    let leak3 = put(&mut state, P0, "Memory Leak", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, leak3, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, opp_land), Zone::Hand);

    // 4. Cycling {1} discards Memory Leak and draws 1 card.
    let mut state = plains();
    let leak4 = put(&mut state, P0, "Memory Leak", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, leak4, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, leak4), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1);
}

// ---- Gigadrowse ---------------------------------------------------------------------

#[test]
fn gigadrowse_taps_target_permanent_and_replicates_with_retargeting() {
    // Replicate 2 ({U} base + {U}{U} replicate = 3 blue mana): taps 3 distinct permanents.
    let mut state = plains();
    let p_a = put(&mut state, P1, "Island", Zone::Battlefield);
    let p_b = put(&mut state, P1, "Plains", Zone::Battlefield);
    let p_c = put(&mut state, P1, "K'un-Lun Warrior", Zone::Battlefield);
    let giga = put(&mut state, P0, "Gigadrowse", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::U, ManaColor::U, ManaColor::U],
        0,
    );
    assert!(castable(&mut state, giga));
    act(&mut state, Action::CastSpell(giga));
    let mut announced = false;
    for _ in 0..8 {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(p_a)));
                act(&mut state, Action::ChooseTarget(Target::Object(p_a)));
            }
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 3, "0, 1, or 2 replicate copies");
                act(&mut state, Action::ChooseEffectOption(2));
                announced = true;
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("unexpected cast decision: {other:?}"),
        }
    }
    assert!(announced);
    let mut retargets = vec![Target::Object(p_c), Target::Object(p_b)];
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellCopyRetarget { .. } => Action::ChooseSpellCopyRetarget(true),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(retargets.pop().unwrap()),
        other => first_answer(other),
    });
    assert!(state.objects.get(p_a).tapped);
    assert!(state.objects.get(p_b).tapped);
    assert!(state.objects.get(p_c).tapped);
    assert_eq!(zone(&state, giga), Zone::Graveyard);
}

// ---- Aurora Eidolon -----------------------------------------------------------------

#[test]
fn aurora_eidolon_prevents_three_damage_respects_stomp_and_returns_on_multicolored_cast() {
    // 1. Prevent next 3 damage across multiple packets (1 then 3 -> 0 then 1 dealt).
    let mut state = plains();
    let aurora = put(&mut state, P0, "Aurora Eidolon", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, aurora, 0, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(zone(&state, aurora), Zone::Graveyard);

    // 1 damage from End the Festivities -> completely prevented, 2 shield left.
    state.priority_player = P1;
    let fest = put(&mut state, P1, "End the Festivities", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, fest, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 20);

    // 3 damage from Lightning Bolt -> 2 prevented, 1 dealt!
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 19);

    // 2. Casting a monocolored spell does NOT trigger Aurora Eidolon in graveyard.
    state.priority_player = P0;
    let bolt_own = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt_own, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, aurora), Zone::Graveyard);

    // 3. Casting a multicolored spell triggers Aurora Eidolon -> accept returns to hand!
    let td = put(&mut state, P0, "Thrilling Discovery", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 0);
    cast(&mut state, td, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        other => first_answer(other),
    });
    assert_eq!(
        zone(&state, aurora),
        Zone::Hand,
        "casting multicolored spell returned Aurora Eidolon from graveyard"
    );

    // 4. Stomp ("damage can't be prevented") bypasses Aurora Eidolon's prevention shield.
    let mut state = plains();
    let aurora = put(&mut state, P0, "Aurora Eidolon", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, aurora, 0, &[Target::Player(P0)]);
    settled(&mut state);
    state.priority_player = P1;
    let giant = put(&mut state, P1, "Bonecrusher Giant", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 1);
    cast_mode(&mut state, giant, 1, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(
        life(&state, P0),
        18,
        "Stomp disables damage prevention so Aurora shield cannot prevent it"
    );
}

// ---- Sandstorm Eidolon --------------------------------------------------------------

#[test]
fn sandstorm_eidolon_stops_blocking_and_returns_on_multicolored_cast() {
    let mut state = plains();
    let sandstorm = put(&mut state, P0, "Sandstorm Eidolon", Zone::Battlefield);
    let attacker = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield);
    let blocker = put(&mut state, P1, "K'un-Lun Warrior", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, sandstorm, 0, &[Target::Object(blocker)]);
    settled(&mut state);
    assert_eq!(zone(&state, sandstorm), Zone::Graveyard);

    run_until(
        &mut state,
        |s| s.step == Step::Main2,
        |_, decision| match decision {
            Decision::DeclareAttackers { .. } => Some(Action::DeclareAttackers(vec![attacker])),
            Decision::DeclareBlockers { legal_blockers, .. } => {
                assert!(
                    legal_blockers
                        .iter()
                        .all(|(_, blockers)| !blockers.contains(&blocker)),
                    "target creature must not be able to block this turn: {legal_blockers:?}"
                );
                Some(Action::DeclareBlockers(Vec::new()))
            }
            _ => None,
        },
    );
    assert_eq!(life(&state, P1), 18);

    // Decline multicolored return -> stays in graveyard; accept -> returns to hand.
    let inside = put(&mut state, P0, "Inside Out", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, inside, &[Target::Object(attacker)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, sandstorm), Zone::Graveyard);

    let inside2 = put(&mut state, P0, "Inside Out", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, inside2, &[Target::Object(attacker)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, sandstorm), Zone::Hand);
}

// ---- Rooftop Percher ----------------------------------------------------------------

#[test]
fn rooftop_percher_is_changeling_flyer_and_exiles_up_to_two_and_gains_life() {
    // 1. Changeling subtypes, flying, 2 targets from both graveyards + 3 life.
    let mut state = plains();
    let gy0 = put(&mut state, P0, "Plains", Zone::Graveyard);
    let gy1 = put(&mut state, P1, "Island", Zone::Graveyard);
    let percher = put(&mut state, P0, "Rooftop Percher", Zone::Hand);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, percher, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            if legal_targets.contains(&Target::Object(gy0)) {
                Action::ChooseTarget(Target::Object(gy0))
            } else {
                Action::ChooseTarget(Target::Object(gy1))
            }
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, percher), Zone::Battlefield);
    assert!(keyword(&state, percher, Keywords::FLYING));
    assert!(engine::has_effective_subtype(&state, percher, Subtype::Elf));
    assert!(engine::has_effective_subtype(
        &state,
        percher,
        Subtype::Pegasus
    ));
    assert!(engine::has_effective_subtype(
        &state,
        percher,
        Subtype::Dragon
    ));
    assert_eq!(zone(&state, gy0), Zone::Exile);
    assert_eq!(zone(&state, gy1), Zone::Exile);
    assert_eq!(life(&state, P0), 23);

    // 2. 0 targets chosen -> still gains 3 life.
    let mut state = plains();
    let _gy0 = put(&mut state, P0, "Plains", Zone::Graveyard);
    let percher = put(&mut state, P0, "Rooftop Percher", Zone::Hand);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, percher, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(life(&state, P0), 23);
    assert_eq!(state.players[0].graveyard.len(), 1);

    // 3. Full fizzle (only target leaves graveyard before resolution) -> NO life gained.
    let mut state = plains();
    let gy = put(&mut state, P1, "Plains", Zone::Graveyard);
    let spellbomb = put(&mut state, P0, "Nihil Spellbomb", Zone::Battlefield);
    let percher = put(&mut state, P0, "Rooftop Percher", Zone::Hand);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, percher, &[]);
    // Resolve Percher onto the battlefield, then pick `gy` as its sole ETB target.
    let mut picked = false;
    run_until(
        &mut state,
        |s| zone(s, percher) == Zone::Battlefield && s.stack.len() == 1 && s.priority_player == P0,
        |_, decision| match decision {
            Decision::ChooseTargets {
                legal_targets,
                can_finish,
                ..
            } => {
                if !picked && legal_targets.contains(&Target::Object(gy)) {
                    picked = true;
                    Some(Action::ChooseTarget(Target::Object(gy)))
                } else if *can_finish {
                    Some(Action::FinishEffectSelection)
                } else {
                    None
                }
            }
            _ => None,
        },
    );
    assert!(picked, "Rooftop Percher ETB must have targeted gy");
    // Exile P1's graveyard with Nihil Spellbomb before Rooftop Percher's trigger resolves.
    activate(&mut state, spellbomb, 0, &[Target::Player(P1)]);
    settle_with(&mut state, |_, d| first_answer(d));
    assert_eq!(zone(&state, gy), Zone::Exile);
    assert_eq!(
        life(&state, P0),
        20,
        "fizzled Rooftop Percher ETB must not gain 3 life"
    );
}

// ---- Rift Sower ---------------------------------------------------------------------

#[test]
fn rift_sower_suspends_two_casts_free_with_haste_and_adds_any_color() {
    let mut state = plains();
    let sower = put(&mut state, P0, "Rift Sower", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    // Suspend 2--{G} is a special action (ability 0 from hand).
    activate(&mut state, sower, 0, &[]);
    assert!(state.stack.is_empty(), "suspending does not use the stack");
    assert_eq!(zone(&state, sower), Zone::Exile);
    assert_eq!(state.objects.get(sower).v4.time_counters_v1, 2);

    // Advance to P0's next turn -> 1 time counter left.
    to_next_own_main(&mut state);
    assert_eq!(zone(&state, sower), Zone::Exile);
    assert_eq!(state.objects.get(sower).v4.time_counters_v1, 1);

    // Advance to P0's second upkeep -> last counter removed, casts free with haste!
    to_next_own_main(&mut state);
    assert_eq!(zone(&state, sower), Zone::Battlefield);
    assert_eq!(power_toughness(&state, sower), (1, 3));
    assert!(
        keyword(&state, sower, Keywords::HASTE),
        "cast from suspend grants haste (CR 702.62a)"
    );

    // Can tap immediately for any color (e.g. Blue).
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(sower, ManaColor::U),
    );
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
}

// ---- Pegasus Guardian // Rescue the Foal --------------------------------------------

#[test]
fn pegasus_guardian_adventures_flickers_and_makes_pegasus_token_on_revolt_end_step() {
    let mut state = plains();
    let watcher = put(&mut state, P0, "Mulldrifter", Zone::Battlefield);
    let guardian = put(&mut state, P0, "Pegasus Guardian", Zone::Hand);

    // Cast Rescue the Foal ({1}{W}, mode 1) flickering Mulldrifter.
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast_mode(&mut state, guardian, 1, &[Target::Object(watcher)]);
    settled(&mut state);
    assert_eq!(zone(&state, watcher), Zone::Battlefield);
    assert_eq!(state.objects.get(watcher).zone_change_count, 2);
    assert_eq!(
        state.players[0].hand.len(),
        2,
        "flickered Mulldrifter re-triggered ETB draw 2"
    );
    assert_eq!(zone(&state, guardian), Zone::Exile);
    assert!(state.objects.get(guardian).v4.on_adventure);

    // Cast Pegasus Guardian ({5}{W}) from exile on adventure in the same turn.
    pool(&mut state, P0, &[ManaColor::W], 5);
    cast(&mut state, guardian, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, guardian), Zone::Battlefield);
    assert!(keyword(&state, guardian, Keywords::FLYING));
    assert_eq!(power_toughness(&state, guardian), (3, 3));

    // At P0's end step, Mulldrifter left the battlefield this turn -> 1/1 flying Pegasus token!
    to_next_turn(&mut state);
    let tokens = named(&state, P0, "Pegasus Token");
    assert_eq!(
        tokens.len(),
        1,
        "revolt end step must create a Pegasus token"
    );
    assert_eq!(power_toughness(&state, tokens[0]), (1, 1));
    assert!(keyword(&state, tokens[0], Keywords::FLYING));
    assert!(engine::has_effective_subtype(
        &state,
        tokens[0],
        Subtype::Pegasus
    ));

    // Next P0 turn with NO permanent leaving -> no second Pegasus token created.
    to_next_turn(&mut state); // P0 Main1
    to_next_turn(&mut state); // P1 Main1 (after P0's end step)
    assert_eq!(
        named(&state, P0, "Pegasus Token").len(),
        1,
        "no permanent left on P0's second turn"
    );
}

// ---- Stormshriek Feral // Flush Out -------------------------------------------------

#[test]
fn stormshriek_feral_omends_into_library_and_creature_has_flying_haste_firebreathing() {
    // 1. Cast Omen Flush Out ({1}{R}, mode 1) with a card in hand -> discard 1, draw 2, shuffle into library.
    let mut state = plains();
    let discard_me = put(&mut state, P0, "Mountain", Zone::Hand);
    let feral = put(&mut state, P0, "Stormshriek Feral", Zone::Hand);
    let lib_before = state.players[0].library.len();
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_mode(&mut state, feral, 1, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(discard_me)) =>
        {
            Action::ChooseEffectTarget(Target::Object(discard_me))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, discard_me), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 2, "drew 2 after discarding");
    assert_eq!(
        zone(&state, feral),
        Zone::Library,
        "Omen shuffles into owner's library on resolution"
    );
    assert_eq!(state.players[0].library.len(), lib_before - 2 + 1);

    // 2. Empty hand on Flush Out resolution -> draws 0, still shuffles into library.
    let mut state = plains();
    let feral = put(&mut state, P0, "Stormshriek Feral", Zone::Hand);
    let lib_before = state.players[0].library.len();
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_mode(&mut state, feral, 1, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 0);
    assert_eq!(zone(&state, feral), Zone::Library);
    assert_eq!(state.players[0].library.len(), lib_before + 1);

    // 3. Countered Flush Out goes to graveyard (not library).
    let mut state = plains();
    let feral = put(&mut state, P0, "Stormshriek Feral", Zone::Hand);
    let cs = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_mode(&mut state, feral, 1, &[]);
    act(&mut state, Action::Pass);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, cs, &[Target::Object(feral)]);
    settled(&mut state);
    assert_eq!(zone(&state, feral), Zone::Graveyard);

    // 4. Creature half ({4}{R}, mode 0): 3/3 Flying Haste + {1}{R} firebreathing.
    let mut state = plains();
    let feral = put(&mut state, P0, "Stormshriek Feral", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 4);
    cast_mode(&mut state, feral, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, feral), Zone::Battlefield);
    assert!(keyword(&state, feral, Keywords::FLYING));
    assert!(keyword(&state, feral, Keywords::HASTE));
    assert_eq!(power_toughness(&state, feral), (3, 3));
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 2);
    activate(&mut state, feral, 0, &[]);
    settled(&mut state);
    activate(&mut state, feral, 0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, feral), (5, 3));
    to_next_turn(&mut state);
    assert_eq!(power_toughness(&state, feral), (3, 3));
}

// ---- Public sessions ----------------------------------------------------------------

#[derive(Default)]
struct Pf2bEvents {
    plays: [usize; PF2B_CARDS.len()],
}

impl Pf2bEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        if !matches!(
            semantic["action_kind"].as_str(),
            Some("cast_spell" | "activate_ability")
        ) {
            return;
        }
        let Some(card_id) = semantic["source"]["card_db_id"]
            .as_u64()
            .and_then(|id| usize::try_from(id).ok())
        else {
            return;
        };
        let name = CARD_DEFS[card_id].name;
        if let Some(index) = PF2B_CARDS.iter().position(|&card| card == name) {
            self.plays[index] += 1;
        }
    }
}

#[test]
fn pf2b_decks_finish_and_replay_through_public_sessions_both_seats() {
    let ub_ramp = deck(&[
        ("Swamp", 8),
        ("Island", 8),
        ("Forest", 6),
        ("Memory Leak", 4),
        ("Gigadrowse", 4),
        ("Rift Sower", 4),
        ("Rooftop Percher", 4),
        ("Llanowar Elves", 2),
    ]);
    let boros = deck(&[
        ("Plains", 10),
        ("Mountain", 10),
        ("Aurora Eidolon", 4),
        ("Sandstorm Eidolon", 4),
        ("Pegasus Guardian", 4),
        ("Stormshriek Feral", 4),
        ("Thrilling Discovery", 4),
    ]);
    let mut events = Pf2bEvents::default();
    for pair in [[ub_ramp.clone(), boros.clone()], [boros, ub_ramp]] {
        for seed in 0x5F2B_u64..0x5F2E {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played = play_observed(pair.clone(), None, seed, policy, &mut |d, a| {
                    events.observe(d, a)
                });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, plays) in PF2B_CARDS.into_iter().zip(events.plays) {
        assert!(
            plays > 0,
            "{name} was never cast or activated through a public session"
        );
    }
}
