//! Pauper field wave Pf2a: native rules and public-session tests for the ten
//! existing-mechanics cards (Radiant Grove, Escape Tunnel, Tuktuk Rubblefort,
//! Granger Guildmage, Obsidian Acolyte, Spirit Mascot, Keep Watch, Embrace the
//! Paradox, Thrilling Discovery, Rites of Initiation).
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const PF2A_CARDS: [&str; 10] = [
    "Radiant Grove",
    "Escape Tunnel",
    "Tuktuk Rubblefort",
    "Granger Guildmage",
    "Obsidian Acolyte",
    "Spirit Mascot",
    "Keep Watch",
    "Embrace the Paradox",
    "Thrilling Discovery",
    "Rites of Initiation",
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
        0x5046_0000_0000 + 0x3261,
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

// ---- Catalog ------------------------------------------------------------------------

#[test]
fn pf2a_cards_are_full_with_expected_printed_profiles() {
    for name in PF2A_CARDS {
        assert_eq!(
            def(name).capability,
            CardCapability::Full,
            "{name} must be Full"
        );
    }

    let grove = def("Radiant Grove");
    assert!(grove.is_land);
    assert!(grove.enters_battlefield_tapped);
    assert_eq!(grove.subtypes, &[Subtype::Forest, Subtype::Plains]);
    assert_eq!(grove.produces_mana, &[ManaColor::G, ManaColor::W]);

    let tunnel = def("Escape Tunnel");
    assert!(tunnel.is_land);
    assert!(!tunnel.enters_battlefield_tapped);
    assert_eq!(tunnel.activated_abilities.len(), 2);

    let fort = def("Tuktuk Rubblefort");
    assert_eq!(fort.mana_value, 3);
    assert_eq!((fort.power, fort.toughness), (Some(0), Some(3)));
    assert_eq!(fort.subtypes, &[Subtype::Wall]);
    assert!(fort.keywords.has(Keywords::DEFENDER));
    assert!(fort.keywords.has(Keywords::REACH));

    let guildmage = def("Granger Guildmage");
    assert_eq!(guildmage.mana_value, 1);
    assert_eq!((guildmage.power, guildmage.toughness), (Some(1), Some(1)));
    assert_eq!(guildmage.subtypes, &[Subtype::Human, Subtype::Wizard]);
    assert_eq!(guildmage.activated_abilities.len(), 2);

    let acolyte = def("Obsidian Acolyte");
    assert_eq!(acolyte.mana_value, 2);
    assert_eq!((acolyte.power, acolyte.toughness), (Some(1), Some(1)));
    assert_eq!(acolyte.subtypes, &[Subtype::Human, Subtype::Cleric]);

    let mascot = def("Spirit Mascot");
    assert_eq!(mascot.mana_value, 2);
    assert_eq!((mascot.power, mascot.toughness), (Some(2), Some(2)));
    assert_eq!(mascot.subtypes, &[Subtype::Spirit, Subtype::Ox]);

    assert_eq!(def("Keep Watch").mana_value, 3);
    assert_eq!(def("Keep Watch").types, &[CardType::Instant]);
    assert_eq!(def("Embrace the Paradox").mana_value, 5);
    assert_eq!(def("Embrace the Paradox").types, &[CardType::Instant]);
    assert_eq!(def("Thrilling Discovery").mana_value, 2);
    assert_eq!(def("Thrilling Discovery").types, &[CardType::Sorcery]);
    assert_eq!(def("Rites of Initiation").mana_value, 1);
    assert_eq!(def("Rites of Initiation").types, &[CardType::Instant]);
}

// ---- Radiant Grove ------------------------------------------------------------------

#[test]
fn radiant_grove_enters_tapped_is_forest_plains_and_taps_for_green_or_white() {
    let mut state = plains();
    let grove = put(&mut state, P0, "Radiant Grove", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(grove));
    settled(&mut state);
    assert_eq!(zone(&state, grove), Zone::Battlefield);
    assert!(state.objects.get(grove).tapped, "must enter tapped");
    assert!(engine::has_effective_subtype(
        &state,
        grove,
        Subtype::Forest
    ));
    assert!(engine::has_effective_subtype(
        &state,
        grove,
        Subtype::Plains
    ));

    state.objects.get_mut(grove).tapped = false;
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(grove, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1);

    state.objects.get_mut(grove).tapped = false;
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(grove, ManaColor::W),
    );
    assert_eq!(mana(&state, P0, ManaColor::W), 1);
}

// ---- Escape Tunnel ------------------------------------------------------------------

#[test]
fn escape_tunnel_fetches_basic_land_tapped_and_makes_small_creature_unblockable() {
    // Mode 1: search library for a basic land onto the battlefield tapped.
    let mut state = game(&["Lightning Bolt"; 10]);
    let tunnel = put(&mut state, P0, "Escape Tunnel", Zone::Hand);
    let island = put(&mut state, P0, "Island", Zone::Library);
    next(&mut state);
    act(&mut state, Action::PlayLand(tunnel));
    settled(&mut state);
    assert!(!state.objects.get(tunnel).tapped, "enters untapped");

    activate(&mut state, tunnel, 0, &[]);
    assert_eq!(zone(&state, tunnel), Zone::Graveyard);
    settle_with(&mut state, |_, d| match d {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(island)) =>
        {
            Action::ChooseEffectTarget(Target::Object(island))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, island), Zone::Battlefield);
    assert!(state.objects.get(island).tapped);

    // Mode 2: target creature with power <= 2 can't be blocked this turn; power >= 3 excluded.
    let mut state = plains();
    let tunnel = put(&mut state, P0, "Escape Tunnel", Zone::Battlefield);
    let small = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield); // 2/2
    let big = put(&mut state, P0, "Primordial Pachyderm", Zone::Battlefield); // 4/4
    let blocker = put(&mut state, P1, "K'un-Lun Warrior", Zone::Battlefield);
    assert!(activatable(&mut state, tunnel, 1));
    act(&mut state, Action::ActivateAbility(tunnel, 1));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(small)));
            assert!(!legal_targets.contains(&Target::Object(big)));
            act(&mut state, Action::ChooseTarget(Target::Object(small)));
        }
        other => panic!("expected target prompt, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, tunnel), Zone::Graveyard);

    // Attack with small: P1's blocker cannot legally block it.
    run_until(
        &mut state,
        |s| s.step == Step::Main2,
        |_, decision| match decision {
            Decision::DeclareAttackers { eligible, .. } if eligible.contains(&small) => {
                Some(Action::DeclareAttackers(vec![small]))
            }
            Decision::DeclareBlockers { legal_blockers, .. } => {
                assert!(
                    legal_blockers
                        .iter()
                        .all(|(atk, blockers)| !(*atk == small && blockers.contains(&blocker))),
                    "unblockable creature must not be blockable: {legal_blockers:?}"
                );
                Some(Action::DeclareBlockers(Vec::new()))
            }
            _ => None,
        },
    );
    assert_eq!(life(&state, P1), 18);
}

// ---- Tuktuk Rubblefort --------------------------------------------------------------

#[test]
fn tuktuk_rubblefort_defends_reaches_and_grants_haste_to_controlled_creatures() {
    let mut state = plains();
    let fort = put(&mut state, P0, "Tuktuk Rubblefort", Zone::Battlefield);
    let bear = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield);
    state.objects.get_mut(bear).summoning_sick = true;
    let opp_bear = put(&mut state, P1, "K'un-Lun Warrior", Zone::Battlefield);
    state.objects.get_mut(opp_bear).summoning_sick = true;

    assert!(keyword(&state, fort, Keywords::DEFENDER));
    assert!(keyword(&state, fort, Keywords::REACH));
    assert!(keyword(&state, bear, Keywords::HASTE));
    assert!(
        !keyword(&state, opp_bear, Keywords::HASTE),
        "only grants haste to creatures you control"
    );

    // Summoning-sick creature can attack; defender Rubblefort cannot.
    run_until(
        &mut state,
        |s| s.step == Step::Main2,
        |_, decision| match decision {
            Decision::DeclareAttackers { eligible, .. } => {
                assert!(eligible.contains(&bear));
                assert!(!eligible.contains(&fort));
                Some(Action::DeclareAttackers(vec![bear]))
            }
            _ => None,
        },
    );
    assert_eq!(life(&state, P1), 18);

    // Dress Down suppresses Rubblefort's haste grant.
    let dress = put(&mut state, P1, "Dress Down", Zone::Battlefield);
    assert!(!keyword(&state, bear, Keywords::HASTE));
    state.objects.get_mut(dress).zone = Zone::Graveyard;
    state.players[P1.index()]
        .battlefield
        .retain(|&o| o != dress);
    assert!(keyword(&state, bear, Keywords::HASTE));
}

// ---- Granger Guildmage --------------------------------------------------------------

#[test]
fn granger_guildmage_pings_any_target_and_controller_or_grants_first_strike() {
    // Ability 0: {R}, {T}: 1 damage to any target and 1 damage to you.
    let mut state = plains();
    let mage = put(&mut state, P0, "Granger Guildmage", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, mage, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 19);
    assert_eq!(life(&state, P0), 19);

    // Fizzled ping deals NO self-damage.
    let mut state = plains();
    let mage = put(&mut state, P0, "Granger Guildmage", Zone::Battlefield);
    let target = put(&mut state, P1, "K'un-Lun Warrior", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 0);
    activate(&mut state, mage, 0, &[Target::Object(target)]);
    cast(&mut state, bolt, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(zone(&state, target), Zone::Graveyard);
    assert_eq!(
        life(&state, P0),
        20,
        "fizzled ping must not damage controller"
    );

    // Ability 1: {W}, {T}: target creature gains first strike until EOT.
    let mut state = plains();
    let mage = put(&mut state, P0, "Granger Guildmage", Zone::Battlefield);
    let target = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield);
    assert!(!keyword(&state, target, Keywords::FIRST_STRIKE));
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, mage, 1, &[Target::Object(target)]);
    settled(&mut state);
    assert!(keyword(&state, target, Keywords::FIRST_STRIKE));
    to_next_turn(&mut state);
    assert!(!keyword(&state, target, Keywords::FIRST_STRIKE));
}

// ---- Obsidian Acolyte ---------------------------------------------------------------

#[test]
fn obsidian_acolyte_has_and_grants_protection_from_black_until_eot() {
    let mut state = plains();
    let acolyte = put(&mut state, P0, "Obsidian Acolyte", Zone::Battlefield);
    let target = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield);
    let kill = put(&mut state, P1, "Cast Down", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::B], 1);

    // Innate protection from black: Cast Down can target warrior, not acolyte.
    act(&mut state, Action::CastSpell(kill));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(target)));
            assert!(
                !legal_targets.contains(&Target::Object(acolyte)),
                "acolyte has innate protection from black"
            );
            act(&mut state, Action::ChooseTarget(Target::Object(target)));
        }
        other => panic!("expected target prompt, got {other:?}"),
    }
    // P1 passes priority -> P0 responds with Obsidian Acolyte's {W} granting pro-black.
    match next(&mut state) {
        Decision::CastSpellOrPass { player: P1, .. } => act(&mut state, Action::Pass),
        other => panic!("expected P1 priority, got {other:?}"),
    }
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, acolyte, 0, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, target),
        Zone::Battlefield,
        "granted protection from black fizzled Cast Down"
    );

    // Protection expires at cleanup.
    to_next_turn(&mut state);
    let kill2 = put(&mut state, P1, "Cast Down", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B], 1);
    cast(&mut state, kill2, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(zone(&state, target), Zone::Graveyard);
}

// ---- Spirit Mascot ------------------------------------------------------------------

#[test]
fn spirit_mascot_triggers_once_per_graveyard_departure_event_and_only_for_own_graveyard() {
    // 1. Two cards leaving own graveyard simultaneously -> ONE +1/+1 counter.
    let mut state = plains();
    let mascot = put(&mut state, P0, "Spirit Mascot", Zone::Battlefield);
    let own_a = put(&mut state, P0, "Plains", Zone::Graveyard);
    let own_b = put(&mut state, P0, "Plains", Zone::Graveyard);
    let opp_card = put(&mut state, P1, "Plains", Zone::Graveyard);
    let percher = put(&mut state, P0, "Rooftop Percher", Zone::Hand);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, percher, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            if legal_targets.contains(&Target::Object(own_a)) {
                Action::ChooseTarget(Target::Object(own_a))
            } else if legal_targets.contains(&Target::Object(own_b)) {
                Action::ChooseTarget(Target::Object(own_b))
            } else if can_finish {
                Action::FinishEffectSelection
            } else {
                Action::ChooseTarget(legal_targets[0])
            }
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, own_a), Zone::Exile);
    assert_eq!(zone(&state, own_b), Zone::Exile);
    assert_eq!(
        state.objects.get(mascot).counters.plus1_plus1,
        1,
        "one or more cards leaving simultaneously triggers only once"
    );
    assert_eq!(power_toughness(&state, mascot), (3, 3));

    // 2. Card leaving opponent's graveyard does NOT trigger P0's Spirit Mascot.
    let percher2 = put(&mut state, P0, "Rooftop Percher", Zone::Hand);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, percher2, &[]);
    let mut picked_opp = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            if !picked_opp && legal_targets.contains(&Target::Object(opp_card)) {
                picked_opp = true;
                Action::ChooseTarget(Target::Object(opp_card))
            } else if can_finish {
                Action::FinishEffectSelection
            } else {
                Action::ChooseTarget(legal_targets[0])
            }
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, opp_card), Zone::Exile);
    assert_eq!(
        state.objects.get(mascot).counters.plus1_plus1,
        1,
        "opponent graveyard departure must not trigger Spirit Mascot"
    );
}

// ---- Keep Watch ---------------------------------------------------------------------

#[test]
fn keep_watch_draws_for_each_attacking_creature_including_zero() {
    // 0 attackers -> draws 0.
    let mut state = plains();
    let kw = put(&mut state, P0, "Keep Watch", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, kw, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 0);

    // 2 attacking creatures -> draws 2.
    let mut state = plains();
    let a = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield);
    let b = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield);
    let kw = put(&mut state, P0, "Keep Watch", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Battlefield);
    let mut cast_kw = false;
    run_until(
        &mut state,
        |s| s.step == Step::Main2,
        |s, decision| match decision {
            Decision::DeclareAttackers { .. } => Some(Action::DeclareAttackers(vec![a, b])),
            Decision::CastSpellOrPass {
                player: P0,
                castable_spells,
                ..
            } if !cast_kw && s.step == Step::DeclareAttackers && castable_spells.contains(&kw) => {
                cast_kw = true;
                Some(Action::CastSpell(kw))
            }
            _ => None,
        },
    );
    assert!(cast_kw, "Keep Watch must be cast during combat");
    assert_eq!(state.players[0].hand.len(), 2);
}

// ---- Embrace the Paradox ------------------------------------------------------------

#[test]
fn embrace_the_paradox_draws_three_and_optionally_puts_land_tapped() {
    // 1. Accept: puts a newly drawn land onto the battlefield tapped.
    let mut state = game(&["Forest", "Lightning Bolt", "Lightning Bolt", "Plains"]);
    let embrace = put(&mut state, P0, "Embrace the Paradox", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 3);
    cast(&mut state, embrace, &[]);
    let mut put_land = None;
    settle_with(&mut state, |s, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            let chosen = legal_targets[0];
            if let Target::Object(obj) = chosen {
                assert!(
                    CARD_DEFS[s.objects.get(obj).card_def as usize].is_land,
                    "only land cards may be offered"
                );
                put_land = Some(obj);
            }
            Action::ChooseEffectTarget(chosen)
        }
        other => first_answer(other),
    });
    let land = put_land.expect("offered land card from hand");
    assert_eq!(zone(&state, land), Zone::Battlefield);
    assert!(state.objects.get(land).tapped, "land enters tapped");
    assert_eq!(state.players[0].hand.len(), 2);

    // 2. Decline: keeps all 3 drawn cards in hand.
    let mut state = game(&["Forest", "Lightning Bolt", "Lightning Bolt", "Plains"]);
    let embrace = put(&mut state, P0, "Embrace the Paradox", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 3);
    cast(&mut state, embrace, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(state.players[0].hand.len(), 3);
    assert!(state.players[0].battlefield.is_empty());

    // 3. No lands in hand after drawing 3 -> resolves cleanly with no land prompt.
    let mut state = game(&["Lightning Bolt"; 10]);
    let embrace = put(&mut state, P0, "Embrace the Paradox", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 3);
    cast(&mut state, embrace, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 3);
}

// ---- Thrilling Discovery ------------------------------------------------------------

#[test]
fn thrilling_discovery_gains_life_and_handles_accept_decline_and_short_hand() {
    // 1. Accept discard 2 -> +2 life, -2 hand, +3 draw.
    let mut state = plains();
    let d1 = put(&mut state, P0, "Mountain", Zone::Hand);
    let d2 = put(&mut state, P0, "Plains", Zone::Hand);
    let td = put(&mut state, P0, "Thrilling Discovery", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 0);
    cast(&mut state, td, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2);
            Action::ChooseEffectOption(0)
        }
        Decision::Discard { count, .. } => {
            assert_eq!(count, 2);
            Action::Discard(vec![d1, d2])
        }
        other => first_answer(other),
    });
    assert_eq!(life(&state, P0), 22);
    assert_eq!(zone(&state, d1), Zone::Graveyard);
    assert_eq!(zone(&state, d2), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 3);

    // 2. Decline discard 2 -> +2 life, keep cards, draw 0.
    let mut state = plains();
    let d1 = put(&mut state, P0, "Mountain", Zone::Hand);
    let d2 = put(&mut state, P0, "Plains", Zone::Hand);
    let td = put(&mut state, P0, "Thrilling Discovery", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 0);
    cast(&mut state, td, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(life(&state, P0), 22);
    assert_eq!(zone(&state, d1), Zone::Hand);
    assert_eq!(zone(&state, d2), Zone::Hand);
    assert_eq!(state.players[0].hand.len(), 2);

    // 3. Only 1 card left in hand -> gains 2 life, skips choice and draws 0.
    let mut state = plains();
    let only = put(&mut state, P0, "Mountain", Zone::Hand);
    let td = put(&mut state, P0, "Thrilling Discovery", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 0);
    cast(&mut state, td, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 22);
    assert_eq!(zone(&state, only), Zone::Hand);
    assert_eq!(state.players[0].hand.len(), 1);
}

// ---- Rites of Initiation ------------------------------------------------------------

#[test]
fn rites_of_initiation_discards_chosen_count_at_random_and_pumps_controlled_creatures() {
    // 1. Choose 2 with 3 cards in hand -> discards 2 at random, +2/+0 to own creatures only.
    let mut state = plains();
    let own = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield);
    let opp = put(&mut state, P1, "K'un-Lun Warrior", Zone::Battlefield);
    put(&mut state, P0, "Mountain", Zone::Hand);
    put(&mut state, P0, "Plains", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Hand);
    let rites = put(&mut state, P0, "Rites of Initiation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, rites, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 4, "0, 1, 2, or 3 cards");
            Action::ChooseEffectOption(2)
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(power_toughness(&state, own), (4, 2));
    assert_eq!(power_toughness(&state, opp), (2, 2));
    to_next_turn(&mut state);
    assert_eq!(power_toughness(&state, own), (2, 2));

    // 2. Choose 0 -> no discard, no pump.
    let mut state = plains();
    let own = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield);
    put(&mut state, P0, "Mountain", Zone::Hand);
    let rites = put(&mut state, P0, "Rites of Initiation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, rites, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        other => first_answer(other),
    });
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(power_toughness(&state, own), (2, 2));

    // 3. Empty hand -> resolves directly with no choice prompt.
    let mut state = plains();
    let own = put(&mut state, P0, "K'un-Lun Warrior", Zone::Battlefield);
    let rites = put(&mut state, P0, "Rites of Initiation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, rites, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, own), (2, 2));
}

// ---- Public sessions ----------------------------------------------------------------

#[derive(Default)]
struct Pf2aEvents {
    plays: [usize; PF2A_CARDS.len()],
}

impl Pf2aEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        if !matches!(
            semantic["action_kind"].as_str(),
            Some("cast_spell" | "play_land")
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
        if let Some(index) = PF2A_CARDS.iter().position(|&card| card == name) {
            self.plays[index] += 1;
        }
    }
}

#[test]
fn pf2a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let naya = deck(&[
        ("Radiant Grove", 4),
        ("Escape Tunnel", 4),
        ("Mountain", 6),
        ("Plains", 4),
        ("Forest", 4),
        ("Granger Guildmage", 4),
        ("Obsidian Acolyte", 4),
        ("Spirit Mascot", 4),
        ("Tuktuk Rubblefort", 4),
        ("Thrilling Discovery", 4),
        ("Rites of Initiation", 4),
    ]);
    let simic = deck(&[
        ("Island", 10),
        ("Forest", 8),
        ("Escape Tunnel", 4),
        ("Keep Watch", 4),
        ("Embrace the Paradox", 4),
        ("Llanowar Elves", 4),
        ("Elvish Mystic", 4),
        ("Augur of Bolas", 4),
        ("Primordial Pachyderm", 2),
    ]);
    let mut events = Pf2aEvents::default();
    for pair in [[naya.clone(), simic.clone()], [simic, naya]] {
        for seed in 0x5F2A_u64..0x5F2D {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played = play_observed(pair.clone(), None, seed, policy, &mut |d, a| {
                    events.observe(d, a)
                });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, plays) in PF2A_CARDS.into_iter().zip(events.plays) {
        assert!(
            plays > 0,
            "{name} was never cast or played through a public session"
        );
    }
}
