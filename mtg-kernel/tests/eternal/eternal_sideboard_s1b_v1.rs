//! Eternal wave S1b: native rules tests for the Legacy sideboard cards, part
//! 2 (issue #365).
#![allow(dead_code)]
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const S1B_CARDS: [&str; 18] = [
    "Deafening Silence",
    "Debt to the Deathless",
    "Gaddock Teeg",
    "Meddling Mage",
    "Sailors' Bane",
    "Toxic Deluge",
    "Deafening Clarion",
    "Empty the Warrens",
    "Grapeshot",
    "Metallic Rebuke",
    "Paradox Engine",
    "Peer into the Abyss",
    "Shenanigans",
    "Steel Wrecking Ball",
    "Stony Silence",
    "Sudden Demise",
    "Temporary Lockdown",
    "Whipflare",
];

fn islands() -> GameState {
    game(&["Island"; 20])
}

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

/// Settles, choosing `target` whenever a target prompt offers it.
fn settle_target(state: &mut GameState, target: Target) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&target) => {
            Action::ChooseTarget(target)
        }
        Decision::ChooseEffectTargets { legal_targets, .. } if legal_targets.contains(&target) => {
            Action::ChooseEffectTarget(target)
        }
        other => first_answer(other),
    });
}

/// Announces X (the option index) once the cast asks for it.
fn announce_x(state: &mut GameState, x: u16) {
    match next(state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert!(x < option_count, "X {x} not in 0..{option_count}");
            act(state, Action::ChooseEffectOption(x));
        }
        other => panic!("expected the X choice, got {other:?}"),
    }
}


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
        0x4539_4221_0001,
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

/// A tapped dual: enters tapped and taps for either color.
fn tapped_dual(name: &str, colors: [ManaColor; 2]) {
    let mut state = plains();
    let land = put(&mut state, P0, name, Zone::Hand);
    play_land(&mut state, land);
    assert!(state.objects.get(land).tapped, "{name}");
    for color in colors {
        let mut state = plains();
        let land = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(land, color));
        assert_eq!(mana(&state, P0, color), 1, "{name} {color:?}");
    }
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn s1b_cards_are_full_with_their_printed_characteristics() {
    for name in S1B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    assert_eq!(def("Debt to the Deathless").cost.x_count, 1);
    assert_eq!(def("Sudden Demise").cost.x_count, 1);
    assert!(def("Deafening Clarion").mode2.is_some() && def("Deafening Clarion").mode3.is_some());
    assert!(def("Paradox Engine").supertypes.contains(&Supertype::Legendary));
    assert!(def("Metallic Rebuke").alt_cost.is_some());
    assert!(def("Grapeshot").target_spec == TargetSpec::AnyTarget);
}

// ---- Storm spells -----------------------------------------------------------------------

#[test]
fn grapeshot_storm_copies_each_deal_one_damage() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
    let shot = put(&mut state, P0, "Grapeshot", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, shot, &[Target::Player(P1)]);
    settle_target(&mut state, Target::Player(P1));
    assert_eq!(life(&state, P1), 15, "Grapeshot and one storm copy");
}

#[test]
fn empty_the_warrens_makes_two_goblins_per_copy() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    let warrens = put(&mut state, P0, "Empty the Warrens", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, warrens, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Goblin Token").len(), 4);
}

// ---- Life and cards --------------------------------------------------------------------

#[test]
fn debt_to_the_deathless_drains_two_times_x() {
    let mut state = islands();
    let debt = put(&mut state, P0, "Debt to the Deathless", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::W, ManaColor::B, ManaColor::B],
        2,
    );
    act(&mut state, Action::CastSpell(debt));
    announce_x(&mut state, 2);
    settled(&mut state);
    assert_eq!(life(&state, P1), 16);
    assert_eq!(life(&state, P0), 24);
}

#[test]
fn peer_into_the_abyss_draws_half_the_library_and_loses_half_the_life() {
    let mut state = islands();
    let peer = put(&mut state, P0, "Peer into the Abyss", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B, ManaColor::B], 4);
    let library = state.players[0].library.len();
    cast(&mut state, peer, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(state.players[0].library.len(), library - library.div_ceil(2));
    assert_eq!(life(&state, P0), 10, "half of 20");
    // Odd amounts round up each time.
    let mut state = game(&["Island"; 7]);
    state.players[1].life = 7;
    let peer = put(&mut state, P0, "Peer into the Abyss", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B, ManaColor::B], 4);
    cast(&mut state, peer, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].library.len(), 3, "drew four of seven");
    assert_eq!(life(&state, P1), 3, "lost four of seven");
}

// ---- Damage ----------------------------------------------------------------------------

#[test]
fn whipflare_spares_artifact_creatures() {
    let mut state = islands();
    let goblin = token(&mut state, P1, "Goblin Token");
    let fury = put(&mut state, P1, "Fury", Zone::Battlefield);
    let automaton = put(&mut state, P1, "Patchwork Automaton", Zone::Battlefield);
    let flare = put(&mut state, P0, "Whipflare", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, flare, &[]);
    settled(&mut state);
    assert_ne!(zone(&state, goblin), Zone::Battlefield);
    assert_eq!(state.objects.get(fury).damage, 2);
    assert_eq!(state.objects.get(automaton).damage, 0);
}

#[test]
fn sudden_demise_hits_only_creatures_of_the_chosen_color() {
    let mut state = islands();
    let goblin = token(&mut state, P1, "Goblin Token");
    let oracle = put(&mut state, P1, "Thassa's Oracle", Zone::Battlefield);
    let demise = put(&mut state, P0, "Sudden Demise", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    act(&mut state, Action::CastSpell(demise));
    announce_x(&mut state, 2);
    // Options are W, U, B, R, G.
    settle_option(&mut state, 3);
    assert_ne!(zone(&state, goblin), Zone::Battlefield);
    assert_eq!(state.objects.get(oracle).damage, 0);

    let mut state = islands();
    let oracle = put(&mut state, P1, "Thassa's Oracle", Zone::Battlefield);
    let demise = put(&mut state, P0, "Sudden Demise", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    act(&mut state, Action::CastSpell(demise));
    announce_x(&mut state, 2);
    settle_option(&mut state, 1);
    assert_eq!(state.objects.get(oracle).damage, 2, "blue creature");
}

#[test]
fn deafening_clarion_deals_damage_gives_lifelink_or_both() {
    for (mode, damage, lifelink) in [(0_u8, true, false), (1, false, true), (2, true, true)] {
        let mut state = islands();
        let altisaur = put(&mut state, P0, "Annoyed Altisaur", Zone::Battlefield);
        let fury = put(&mut state, P1, "Fury", Zone::Battlefield);
        let clarion = put(&mut state, P0, "Deafening Clarion", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 1);
        cast_mode(&mut state, clarion, mode, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, fury) != Zone::Battlefield, damage, "mode {mode}");
        assert_eq!(state.objects.get(altisaur).damage, if damage { 3 } else { 0 });
        assert_eq!(
            keyword(&state, altisaur, Keywords::LIFELINK),
            lifelink,
            "mode {mode}"
        );
        assert!(!keyword(&state, fury, Keywords::LIFELINK));
    }
}

// ---- Artifacts --------------------------------------------------------------------------

#[test]
fn shenanigans_destroys_an_artifact_and_dredges() {
    let mut state = islands();
    let monolith = put(&mut state, P1, "Grim Monolith", Zone::Battlefield);
    let shenanigans = put(&mut state, P0, "Shenanigans", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, shenanigans, &[Target::Object(monolith)]);
    settled(&mut state);
    assert_eq!(zone(&state, monolith), Zone::Graveyard);
    assert_eq!(zone(&state, shenanigans), Zone::Graveyard);

    // Dredge 1: Bazaar of Baghdad's first draw mills a card and returns it.
    let mut state = islands();
    let bazaar = put(&mut state, P0, "Bazaar of Baghdad", Zone::Battlefield);
    let shenanigans = put(&mut state, P0, "Shenanigans", Zone::Graveyard);
    let library = state.players[0].library.len();
    activate(&mut state, bazaar, 0, &[]);
    let mut dredged = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.contains(&Target::Object(shenanigans)) && dredged == 0 => {
            dredged += 1;
            Action::ChooseEffectTarget(Target::Object(shenanigans))
        }
        Decision::ChooseEffectTargets { .. } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(dredged, 1);
    assert_eq!(state.players[0].library.len(), library - 2, "milled one, drew one");
}

#[test]
fn steel_wrecking_ball_hits_a_creature_and_can_be_discarded_for_an_artifact() {
    let mut state = islands();
    let fury = put(&mut state, P1, "Fury", Zone::Battlefield);
    let ball = put(&mut state, P0, "Steel Wrecking Ball", Zone::Hand);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, ball, &[]);
    settle_choosing(&mut state, fury);
    assert_eq!(zone(&state, ball), Zone::Battlefield);
    assert_ne!(zone(&state, fury), Zone::Battlefield, "5 damage");

    let mut state = islands();
    let monolith = put(&mut state, P1, "Grim Monolith", Zone::Battlefield);
    let ball = put(&mut state, P0, "Steel Wrecking Ball", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    activate(&mut state, ball, 0, &[Target::Object(monolith)]);
    settled(&mut state);
    assert_eq!(zone(&state, ball), Zone::Graveyard);
    assert_eq!(zone(&state, monolith), Zone::Graveyard);
}

#[test]
fn stony_silence_stops_artifact_abilities() {
    let mut state = islands();
    let top = put(&mut state, P0, "Sensei's Divining Top", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, top, 0), "without Stony Silence");
    put(&mut state, P1, "Stony Silence", Zone::Battlefield);
    assert!(!activatable(&mut state, top, 0), "with Stony Silence");
    assert!(!activatable(&mut state, top, 1));
}

#[test]
fn metallic_rebuke_improvises_and_counters_unless_three_are_paid() {
    let mut state = islands();
    let stars: Vec<ObjectId> = (0..2)
        .map(|_| put(&mut state, P0, "Chromatic Star", Zone::Battlefield))
        .collect();
    let rebuke = put(&mut state, P0, "Metallic Rebuke", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    state.priority_player = P0;
    cast(&mut state, rebuke, &[Target::Object(bolt)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            Action::ChooseCastMode(*options.last().expect("a cast mode"))
        }
        other => first_answer(other),
    });
    assert!(stars.iter().all(|&star| state.objects.get(star).tapped));
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "countered");
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn paradox_engine_untaps_nonland_permanents_whenever_a_spell_is_cast() {
    let mut state = islands();
    put(&mut state, P0, "Paradox Engine", Zone::Battlefield);
    let star = put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    let fury = put(&mut state, P0, "Fury", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    for object in [star, fury, island] {
        state.objects.get_mut(object).tapped = true;
    }
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert!(!state.objects.get(star).tapped);
    assert!(!state.objects.get(fury).tapped);
    assert!(state.objects.get(island).tapped, "lands stay tapped");
}

// ---- Exile ------------------------------------------------------------------------------

#[test]
fn temporary_lockdown_exiles_small_nonland_permanents_until_it_leaves() {
    let mut state = islands();
    let star = put(&mut state, P1, "Chromatic Star", Zone::Battlefield);
    let fury = put(&mut state, P1, "Fury", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let own = put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    let lockdown = put(&mut state, P0, "Temporary Lockdown", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 1);
    cast(&mut state, lockdown, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, star), Zone::Exile);
    assert_eq!(zone(&state, own), Zone::Exile, "each player's");
    assert_eq!(zone(&state, fury), Zone::Battlefield, "mana value 5");
    assert_eq!(zone(&state, island), Zone::Battlefield, "lands stay");
    assert_eq!(zone(&state, lockdown), Zone::Battlefield);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(lockdown, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, star), Zone::Battlefield, "returned");
    assert_eq!(zone(&state, own), Zone::Battlefield);
}

// ---- Cast limits and prohibitions ----------------------------------------------------

#[test]
fn deafening_silence_allows_one_noncreature_spell_per_player_each_turn() {
    let mut state = islands();
    put(&mut state, P1, "Deafening Silence", Zone::Battlefield);
    let first = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let second = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let creature = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R, ManaColor::G], 0);
    cast(&mut state, first, &[Target::Player(P1)]);
    settled(&mut state);
    assert!(!castable(&mut state, second), "a second noncreature spell");
    assert!(castable(&mut state, creature), "creature spells are free");
    // The opponent has cast none this turn.
    state.priority_player = P1;
    let theirs = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(castable(&mut state, theirs));
}

#[test]
fn deafening_silence_counts_spells_cast_before_it_entered() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    put(&mut state, P1, "Deafening Silence", Zone::Battlefield);
    let second = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, second));
}

#[test]
fn gaddock_teeg_stops_expensive_and_x_noncreature_spells() {
    let mut state = islands();
    put(&mut state, P1, "Gaddock Teeg", Zone::Battlefield);
    let deluge = put(&mut state, P0, "Peer into the Abyss", Zone::Hand);
    let demise = put(&mut state, P0, "Sudden Demise", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let altisaur = put(&mut state, P0, "Annoyed Altisaur", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::B, ManaColor::R, ManaColor::G],
        8,
    );
    assert!(!castable(&mut state, deluge), "mana value 7");
    assert!(!castable(&mut state, demise), "{{X}} in its cost");
    assert!(castable(&mut state, bolt), "mana value 1");
    assert!(castable(&mut state, altisaur), "creature, mana value 5");
}

#[test]
fn meddling_mage_names_a_card_that_then_cannot_be_cast() {
    let mut state = islands();
    let mage = put(&mut state, P0, "Meddling Mage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 0);
    let bolt_id = id("Lightning Bolt");
    let groups = mtg_kernel::eternal_sideboard_s1b_v1::nameable_nonland();
    assert!(
        groups.iter().flatten().all(|&named| !def_is_land(named)),
        "nonland names only"
    );
    let letter = groups
        .iter()
        .position(|group| group.contains(&bolt_id))
        .expect("Lightning Bolt is nameable");
    let name = groups[letter]
        .iter()
        .position(|&named| named == bolt_id)
        .unwrap();
    cast(&mut state, mage, &[]);
    let mut answers = vec![letter as u16, name as u16];
    answers.reverse();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => {
            Action::ChooseEffectOption(answers.pop().expect("two name choices"))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, mage), Zone::Battlefield);
    state.priority_player = P1;
    let theirs = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let other = put(&mut state, P1, "Fireblast", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(!castable(&mut state, theirs), "named");
    assert!(castable(&mut state, other), "another spell");
}

fn def_is_land(named: u16) -> bool {
    CARD_DEFS[named as usize].types.contains(&CardType::Land)
}

// ---- Costs ---------------------------------------------------------------------------------

#[test]
fn sailors_bane_costs_one_less_per_instant_sorcery_and_adventure_card() {
    let bane_pool = |extra: u8| {
        let mut state = islands();
        for name in ["Lightning Bolt", "Whipflare", "Brazen Borrower"] {
            put(&mut state, P0, name, Zone::Graveyard);
        }
        put(&mut state, P0, "Grapeshot", Zone::Exile);
        // An opponent's card and a creature card do not count.
        put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
        put(&mut state, P0, "Fury", Zone::Graveyard);
        let bane = put(&mut state, P0, "Sailors' Bane", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U, ManaColor::U], extra);
        (state, bane)
    };
    // Nine less four cards: {3}{U}{U}.
    let (mut state, bane) = bane_pool(3);
    assert!(castable(&mut state, bane));
    let (mut state, bane) = bane_pool(2);
    assert!(!castable(&mut state, bane));
    assert!(def("Sailors' Bane").ward_cost.is_some());
}

#[test]
fn toxic_deluge_pays_x_life_and_shrinks_every_creature() {
    let mut state = islands();
    let goblin = token(&mut state, P1, "Goblin Token");
    let fury = put(&mut state, P1, "Fury", Zone::Battlefield);
    let mine = put(&mut state, P0, "Annoyed Altisaur", Zone::Battlefield);
    let deluge = put(&mut state, P0, "Toxic Deluge", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    act(&mut state, Action::CastSpell(deluge));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 21, "X from 0 to the 20 life");
            act(&mut state, Action::ChooseEffectOption(2));
        }
        other => panic!("expected the X choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(life(&state, P0), 18);
    assert_ne!(zone(&state, goblin), Zone::Battlefield);
    assert_eq!(power_toughness(&state, fury), (1, 1));
    assert_eq!(power_toughness(&state, mine), (4, 3));
}
