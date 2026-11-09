//! Eternal wave S1b: native rules tests for the Legacy sideboard cards, part
//! 2 (issue #365).
#![allow(dead_code)]
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardDef, CardType, Keywords, Supertype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const S1B_CARDS: [&str; 26] = [
    "Deafening Silence",
    "Debt to the Deathless",
    "Gaddock Teeg",
    "Meddling Mage",
    "Sailors' Bane",
    "Toxic Deluge",
    "Deafening Clarion",
    "Empty the Warrens",
    "Engineered Explosives",
    "Extirpate",
    "Leyline of Sanctity",
    "Liquimetal Coating",
    "Perilous Snare",
    "Pyrokinesis",
    "Stern Dismissal",
    "Tourach, Dread Cantor",
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
fn s1b_cards_keep_their_support_labels_and_printed_characteristics() {
    for name in S1B_CARDS {
        assert_eq!(
            def(name).capability,
            crate::eternal_catalog_v1::expected_capability(name),
            "{name}"
        );
        assert!(!def(name).is_token, "{name}");
    }
    assert_eq!(def("Debt to the Deathless").cost.x_count, 1);
    assert_eq!(def("Sudden Demise").cost.x_count, 1);
    assert!(def("Deafening Clarion").mode2.is_some() && def("Deafening Clarion").mode3.is_some());
    assert!(def("Paradox Engine")
        .supertypes
        .contains(&Supertype::Legendary));
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
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::B],
        4,
    );
    let library = state.players[0].library.len();
    cast(&mut state, peer, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(
        state.players[0].library.len(),
        library - library.div_ceil(2)
    );
    assert_eq!(life(&state, P0), 10, "half of 20");
    // Odd amounts round up each time.
    let mut state = game(&["Island"; 7]);
    state.players[1].life = 7;
    let peer = put(&mut state, P0, "Peer into the Abyss", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::B],
        4,
    );
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
        assert_eq!(
            zone(&state, fury) != Zone::Battlefield,
            damage,
            "mode {mode}"
        );
        assert_eq!(
            state.objects.get(altisaur).damage,
            if damage { 3 } else { 0 }
        );
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
    assert_eq!(
        state.players[0].library.len(),
        library - 2,
        "milled one, drew one"
    );
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
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    pool(&mut state, P0, &[ManaColor::U], 0);
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
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::R, ManaColor::G],
        0,
    );
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
    let silence = put(&mut state, P1, "Deafening Silence", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(silence, Zone::Battlefield),
    );
    let second = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, second));
}

#[test]
fn ordinary_cast_does_not_create_deafening_silence_state() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert!(state
        .eternal_v1
        .as_ref()
        .is_none_or(|eternal| eternal.s1b == Default::default()));
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
        &[
            ManaColor::B,
            ManaColor::B,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
            ManaColor::G,
        ],
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
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseMeddlingMageNameV1 {
            player: P0,
            group: None,
            ..
        })
    ));
    assert_eq!(zone(&state, mage), Zone::Stack);
    act(&mut state, Action::ChooseEffectOption(letter as u16));
    act(&mut state, Action::ChooseEffectOption(name as u16));
    settled(&mut state);
    assert_eq!(zone(&state, mage), Zone::Battlefield);
    state.priority_player = P1;
    let theirs = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let other = put(&mut state, P1, "Lava Dart", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(!castable(&mut state, theirs), "named");
    assert!(castable(&mut state, other), "another spell");
}

fn choose_mage_name(state: &mut GameState, player: PlayerId, name: &str) {
    let groups = mtg_kernel::eternal_sideboard_s1b_v1::nameable_nonland();
    let named = id(name);
    let group = groups
        .iter()
        .position(|entries| entries.contains(&named))
        .unwrap() as u16;
    assert!(matches!(next(state), Decision::ChooseMeddlingMageNameV1 {
        player: chooser, group: None, ..
    } if chooser == player));
    act(state, Action::ChooseEffectOption(group));
    let Decision::ChooseMeddlingMageNameV1 {
        player: chooser,
        group: Some(actual),
        options,
        ..
    } = next(state)
    else {
        panic!("expected the catalog names from the selected group")
    };
    assert_eq!(chooser, player);
    assert_eq!(actual, group);
    let index = options.iter().position(|option| option == name).unwrap();
    act(state, Action::ChooseEffectOption(index as u16));
}

fn paid_meddling_against(actor: PlayerId, name: &str, library: &[&str]) -> GameState {
    let mut state = game(library);
    let opponent = actor.opponent();
    state.active_player = opponent;
    state.priority_player = opponent;
    let mage = put(&mut state, opponent, "Meddling Mage", Zone::Hand);
    pool(&mut state, opponent, &[ManaColor::W, ManaColor::U], 0);
    cast(&mut state, mage, &[]);
    assert!(settle(&mut state).is_some());
    choose_mage_name(&mut state, opponent, name);
    settled(&mut state);
    state.active_player = actor;
    state.priority_player = actor;
    state.engine.priority_passes = [false; 2];
    state
}

#[test]
fn meddling_mage_filters_cascade_before_the_free_cast_offer_both_seats() {
    for actor in [P0, P1] {
        for prohibited in [true, false] {
            let mut state = paid_meddling_against(
                actor,
                if prohibited { "Lightning Bolt" } else { "Lava Dart" },
                &["Island", "Lightning Bolt", "Plains", "Plains"],
            );
            let bolt = state.players[actor.index()].library[1];
            let party = put(&mut state, actor, "Boarding Party", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::R], 5);
            cast(&mut state, party, &[]);
            let mut offers = 0;
            settle_with(&mut state, |_, decision| match decision {
                Decision::ChooseEffectOption { option_count: 2, .. } => {
                    offers += 1;
                    Action::ChooseEffectOption(0)
                }
                Decision::ChooseEffectTargets { legal_targets, .. }
                    if legal_targets == [Target::Object(bolt)] =>
                {
                    Action::ChooseEffectTarget(Target::Object(bolt))
                }
                Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(actor.opponent())),
                other => first_answer(other),
            });
            assert_eq!(offers, usize::from(!prohibited));
            assert_eq!(zone(&state, party), Zone::Battlefield);
            assert_eq!(zone(&state, bolt), if prohibited { Zone::Library } else { Zone::Graveyard });
            assert_eq!(life(&state, actor.opponent()), if prohibited { 20 } else { 17 });
        }
    }
}

#[test]
fn meddling_mage_filters_paid_bilbo_resolution_cast_both_seats() {
    for actor in [P0, P1] {
        let mut state = paid_meddling_against(actor, "Brotherhood's End", &["Island"; 20]);
        let bilbo = put(&mut state, actor, "Bilbo, Thief in the Night", Zone::Battlefield);
        let ring = put(&mut state, actor, "Ichor Wellspring", Zone::Graveyard);
        let forbidden = put(&mut state, actor, "Brotherhood's End", Zone::Graveyard);
        pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
        act(&mut state, Action::DeclareAttackers(vec![bilbo]));
        pool(&mut state, actor, &[ManaColor::R, ManaColor::R], 1);
        let prompt = settle(&mut state).expect("Bilbo's paid graveyard cast selection");
        assert!(matches!(
            &prompt,
            Decision::ChooseEffectTargets { legal_targets, can_finish: true, .. }
                if legal_targets == &[Target::Object(ring)]
        ));
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(prompt, next(&mut restored));
        for continued in [&mut state, &mut restored] {
            assert!(engine::step(continued, Action::ChooseEffectTarget(Target::Object(forbidden))).is_err());
            act(continued, Action::ChooseEffectTarget(Target::Object(ring)));
            settled(continued);
            assert_eq!(zone(continued, forbidden), Zone::Graveyard);
            assert_eq!(zone(continued, ring), Zone::Battlefield);
            assert_eq!(continued.players[actor.index()].mana_pool.iter().sum::<u8>(), 2);
        }
    }
}

#[test]
fn meddling_mage_named_front_leaves_adventure_castable_both_seats() {
    for actor in [P0, P1] {
        let mut state = paid_meddling_against(actor, "Fang Dragon", &["Island"; 20]);
        let dragon = put(&mut state, actor, "Fang Dragon", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R, ManaColor::R], 5);
        cast(&mut state, dragon, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, dragon), Zone::Exile);
        assert!(state.objects.get(dragon).v4.on_adventure);
        pool(&mut state, actor, &[ManaColor::R, ManaColor::R], 5);
        assert!(!castable(&mut state, dragon));

        let mut state = paid_meddling_against(actor, "Fang Dragon", &["Island"; 20]);
        let bilbo = put(&mut state, actor, "Bilbo, Thief in the Night", Zone::Battlefield);
        let dragon = put(&mut state, actor, "Fang Dragon", Zone::Graveyard);
        pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
        act(&mut state, Action::DeclareAttackers(vec![bilbo]));
        pool(&mut state, actor, &[ManaColor::R], 0);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { legal_targets, .. })
                if legal_targets == [Target::Object(dragon)]
        ));
        act(&mut state, Action::ChooseEffectTarget(Target::Object(dragon)));
        settled(&mut state);
        assert_eq!(zone(&state, dragon), Zone::Exile);
        assert!(state.objects.get(dragon).v4.on_adventure);
        assert!(state.players[actor.index()].mana_pool.iter().all(|&count| count == 0));
    }
}

#[test]
fn meddling_mage_reanimation_pauses_before_entry_and_life_loss_both_seats() {
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let owner = actor.opponent();
        let mage = put(&mut state, owner, "Meddling Mage", Zone::Graveyard);
        let reanimate = put(&mut state, actor, "Reanimate", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::B], 0);
        cast(&mut state, reanimate, &[Target::Object(mage)]);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseMeddlingMageNameV1 {
            player, group: None, ..
        }) if player == actor)
        );
        assert_eq!(zone(&state, mage), Zone::Graveyard);
        assert_eq!(zone(&state, reanimate), Zone::Stack);
        assert_eq!(life(&state, actor), 20);
        let snapshot = serde_json::to_vec(&state).unwrap();
        let mut forged: serde_json::Value = serde_json::from_slice(&snapshot).unwrap();
        forged["engine"]["linked_aura_operation_v1"]["pending_meddling_mage_v1"] =
            serde_json::Value::Null;
        let mut forged: GameState = serde_json::from_value(forged).unwrap();
        assert!(engine::step(&mut forged, Action::ChooseEffectOption(0)).is_err());
        state = serde_json::from_slice(&snapshot).unwrap();
        choose_mage_name(&mut state, actor, "Lightning Bolt");
        settled(&mut state);
        assert_eq!(zone(&state, mage), Zone::Battlefield);
        assert_eq!(state.objects.get(mage).owner, owner);
        assert_eq!(state.objects.get(mage).controller, actor);
        assert_eq!(life(&state, actor), 18);
        let mut altered = serde_json::to_value(&state).unwrap();
        let named = altered["engine"]["event_history"].as_array_mut().unwrap().iter_mut()
            .find_map(|row| row.get_mut("MeddlingMageEntryV1")?
                .get_mut("journal")?.get_mut("Named"))
            .unwrap();
        let dart_id = serde_json::json!(id("Lava Dart"));
        named["named"] = dart_id.clone();
        altered["eternal_v1"]["s1b"]["mage_names"][0][1] = dart_id;
        let mut altered: GameState = serde_json::from_value(altered).unwrap();
        assert!(engine::step(&mut altered, Action::Pass).is_err());

        let blink = put(&mut state, actor, "Ephemerate", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::W], 0);
        let before_blink = state.objects.get(mage).zone_change_count;
        cast(&mut state, blink, &[Target::Object(mage)]);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseMeddlingMageNameV1 {
            player, group: None, ..
        }) if player == owner)
        );
        assert_eq!(zone(&state, mage), Zone::Exile);
        choose_mage_name(&mut state, owner, "Lava Dart");
        settled(&mut state);
        assert_eq!(state.objects.get(mage).controller, owner);
        assert_eq!(state.objects.get(mage).zone_change_count, before_blink + 2);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let dart = put(&mut state, actor, "Lava Dart", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        state.priority_player = actor;
        assert!(
            castable(&mut state, bolt),
            "the earlier incarnation's name expired"
        );
        assert!(
            !castable(&mut state, dart),
            "the new incarnation chose a new name"
        );
    }
}

#[test]
fn meddling_mage_names_before_aether_vial_puts_it_onto_the_battlefield() {
    let mut state = islands();
    let vial = put(&mut state, P0, "Aether Vial", Zone::Battlefield);
    state.objects.get_mut(vial).counters.charge = 2;
    let mage = put(&mut state, P0, "Meddling Mage", Zone::Hand);
    activate(&mut state, vial, 0, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { player: P0, legal_targets, .. })
            if legal_targets == [Target::Object(mage)]
    ));
    act(&mut state, Action::ChooseEffectTarget(Target::Object(mage)));
    assert_eq!(zone(&state, mage), Zone::Hand);
    choose_mage_name(&mut state, P0, "Lightning Bolt");
    settled(&mut state);
    assert_eq!(zone(&state, mage), Zone::Battlefield);
    assert!(state.objects.get(vial).tapped);
}

#[test]
fn meddling_mage_simultaneous_exhume_names_follow_apnap_before_either_entry() {
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let other = actor.opponent();
        let first = put(&mut state, actor, "Meddling Mage", Zone::Graveyard);
        let second = put(&mut state, other, "Meddling Mage", Zone::Graveyard);
        let spell = put(&mut state, actor, "Exhume", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::B], 1);
        cast(&mut state, spell, &[]);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseMeddlingMageNameV1 {
            player, entry_ordinal: 0, ..
        }) if player == actor)
        );
        choose_mage_name(&mut state, actor, "Lightning Bolt");
        assert_eq!(zone(&state, first), Zone::Graveyard);
        assert_eq!(zone(&state, second), Zone::Graveyard);
        assert!(
            matches!(next(&mut state), Decision::ChooseMeddlingMageNameV1 {
            player, entry_ordinal: 1, ..
        } if player == other)
        );
        choose_mage_name(&mut state, other, "Lava Dart");
        settled(&mut state);
        assert_eq!(zone(&state, first), Zone::Battlefield);
        assert_eq!(zone(&state, second), Zone::Battlefield);
    }
}

#[test]
fn meddling_mage_token_copy_names_independently_before_allocation() {
    let mut state = islands();
    let mage = put(&mut state, P0, "Meddling Mage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 0);
    cast(&mut state, mage, &[]);
    assert!(settle(&mut state).is_some());
    choose_mage_name(&mut state, P0, "Lightning Bolt");
    settled(&mut state);
    let reflection = put(
        &mut state,
        P0,
        "Fable of the Mirror-Breaker",
        Zone::Battlefield,
    );
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::transform_in_place(reflection, 1),
    );
    pool(&mut state, P0, &[], 1);
    activate(&mut state, reflection, 0, &[Target::Object(mage)]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseMeddlingMageNameV1 { player: P0, .. })
    ));
    assert_eq!(named(&state, P0, "Meddling Mage"), [mage]);
    choose_mage_name(&mut state, P0, "Lava Dart");
    settled(&mut state);
    let copies = named(&state, P0, "Meddling Mage");
    assert_eq!(copies.len(), 2);
    assert!(copies
        .iter()
        .any(|&object| state.objects.get(object).v4.is_token));
    let dart = put(&mut state, P1, "Lava Dart", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    state.priority_player = P1;
    assert!(!castable(&mut state, dart));
}

#[test]
fn meddling_mage_entering_under_dress_down_never_chose_a_name() {
    let mut state = islands();
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    let first = put(&mut state, P0, "Meddling Mage", Zone::Hand);
    let mage = put(&mut state, P0, "Meddling Mage", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::W, ManaColor::U, ManaColor::U, ManaColor::U],
        1,
    );
    cast(&mut state, first, &[]);
    assert!(settle(&mut state).is_some());
    choose_mage_name(&mut state, P0, "Lava Dart");
    settled(&mut state);
    cast(&mut state, dress, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, dress), Zone::Battlefield);
    cast(&mut state, mage, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mage), Zone::Battlefield);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(dress, Zone::Graveyard),
    );
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    state.priority_player = P1;
    assert!(castable(&mut state, bolt));
    state.eternal_v1.as_mut().unwrap().s1b.mage_names.push((
        mtg_kernel::state::ObjectLinkV4 {
            object: mage,
            zone_change_count: state.objects.get(mage).zone_change_count,
        },
        id("Lightning Bolt"),
    ));
    let mut restored: GameState = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(engine::step(&mut restored, Action::Pass).is_err());
}

#[test]
fn meddling_mage_priest_replacement_does_not_offer_a_name() {
    let mut state = islands();
    put(&mut state, P1, "Containment Priest", Zone::Battlefield);
    let mage = put(&mut state, P1, "Meddling Mage", Zone::Graveyard);
    let reanimate = put(&mut state, P0, "Reanimate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, reanimate, &[Target::Object(mage)]);
    settled(&mut state);
    assert_eq!(zone(&state, mage), Zone::Exile);
    assert_eq!(life(&state, P0), 18);
}

#[test]
fn meddling_mage_manifest_does_not_expose_hidden_printing_as_a_name_choice() {
    for actor in [P0, P1] {
        for (hidden, priest) in [
            ("Meddling Mage", false),
            ("Lightning Bolt", false),
            ("Meddling Mage", true),
            ("Lightning Bolt", true),
        ] {
            let mut state = islands();
            state.active_player = actor;
            state.priority_player = actor;
            if priest {
                put(&mut state, actor.opponent(), "Containment Priest", Zone::Battlefield);
            }
            let oculus = put(&mut state, actor, "Abhorrent Oculus", Zone::Hand);
            for _ in 0..6 {
                put(&mut state, actor, "Island", Zone::Graveyard);
            }
            let chosen = put(&mut state, actor, hidden, Zone::Library);
            pool(&mut state, actor, &[ManaColor::U], 2);
            cast(&mut state, oculus, &[]);
            settle_with(&mut state, |_, decision| first_answer(decision));
            run_until(
                &mut state,
                |state| state.active_player == actor.opponent() && state.step == Step::Main1,
                |_, decision| match decision {
                    Decision::ChooseEffectTargets {
                        player,
                        legal_targets,
                        ..
                    } if *player == actor && legal_targets.contains(&Target::Object(chosen)) => {
                        Some(Action::ChooseEffectTarget(Target::Object(chosen)))
                    }
                    Decision::ChooseMeddlingMageNameV1 { .. } => {
                        panic!("the face-down entry cannot expose a printed name ability")
                    }
                    _ => None,
                },
            );
            let live = state.objects.get(chosen);
            if priest {
                assert_eq!(live.zone, Zone::Exile);
                assert!(live.v4.face_down_v1.is_none());
                continue;
            }
            assert_eq!(live.zone, Zone::Battlefield);
            assert_eq!(live.v4.face_down_v1.unwrap().lookers, 1 << actor.index());
            assert_eq!(live.v4.ward_generic, 0);
            assert_eq!(power_toughness(&state, chosen), (2, 2));
        }
    }
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

// ---- Stage 3 ------------------------------------------------------------------------------

#[test]
fn pyrokinesis_divides_four_damage_and_can_be_cast_by_exiling_a_red_card() {
    let mut state = islands();
    let fury = put(&mut state, P1, "Fury", Zone::Battlefield);
    let goblin = token(&mut state, P1, "Goblin Token");
    let pyro = put(&mut state, P0, "Pyrokinesis", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    cast(&mut state, pyro, &[]);
    let mut picks = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseCostTargets { .. } => Action::ChooseCostTarget(bolt),
        Decision::ChooseTargets { legal_targets, .. } => {
            picks += 1;
            assert_eq!(legal_targets.len(), 2, "creatures only");
            // Three to Fury, one to the Goblin.
            Action::ChooseTarget(Target::Object(if picks <= 3 { fury } else { goblin }))
        }
        other => first_answer(other),
    });
    assert_eq!(picks, 4);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_ne!(zone(&state, fury), Zone::Battlefield);
    assert_ne!(zone(&state, goblin), Zone::Battlefield);
}

#[test]
fn stern_dismissal_returns_an_opponents_creature_or_enchantment_only() {
    let mut state = islands();
    let mine = put(&mut state, P0, "Fury", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Pestilence", Zone::Battlefield);
    let creature = put(&mut state, P1, "Annoyed Altisaur", Zone::Battlefield);
    let artifact = put(&mut state, P1, "Grim Monolith", Zone::Battlefield);
    let dismissal = put(&mut state, P0, "Stern Dismissal", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    let legal = cast_targets(&state, dismissal);
    assert!(legal.contains(&Target::Object(theirs)));
    assert!(legal.contains(&Target::Object(creature)));
    assert!(!legal.contains(&Target::Object(mine)), "your own");
    assert!(!legal.contains(&Target::Object(artifact)), "an artifact");
    cast(&mut state, dismissal, &[Target::Object(theirs)]);
    settled(&mut state);
    assert!(hand_contains(&state, P1, theirs));
}

#[test]
fn extirpate_requires_graveyard_matches_and_allows_any_hidden_subset() {
    use mtg_kernel::eternal_sideboard_s1a_v1::surgical_search_look_v1;
    use mtg_kernel::eternal_sideboard_s1b_v1::extirpate_search_look_v1;
    use mtg_kernel::rl::{legal_action_candidates_v1, ActionSemanticV1, PlayerSeatV1, TargetRefV1};
    use mtg_kernel::state::ObservationNoteKindV1;
    use mtg_kernel::surface::{SurfaceAction, SurfaceDecision};

    for actor in [P0, P1] {
        let owner = actor.opponent();
        let mut state = game(&["Island"; 4]);
        state.active_player = actor;
        state.enable_hand_knowledge_by_name_v2();
        let target = put(&mut state, owner, "Lightning Bolt", Zone::Graveyard);
        let graveyard = put(&mut state, owner, "Lightning Bolt", Zone::Graveyard);
        let hand = put(&mut state, owner, "Lightning Bolt", Zone::Hand);
        let library = put(&mut state, owner, "Lightning Bolt", Zone::Library);
        let other_hand = put(&mut state, owner, "Fireblast", Zone::Hand);
        let other_graveyard = put(&mut state, owner, "Mountain", Zone::Graveyard);
        let own_match = put(&mut state, actor, "Lightning Bolt", Zone::Graveyard);
        let spell = put(&mut state, actor, "Extirpate", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::B], 0);
        pool(&mut state, owner, &[ManaColor::R], 0);
        state.priority_player = owner;
        assert!(castable(&mut state, hand), "the response is payable");
        state.priority_player = actor;
        let legal = cast_targets(&state, spell);
        assert!(legal.contains(&Target::Object(target)));
        assert!(
            !legal.contains(&Target::Object(other_graveyard)),
            "basic land"
        );
        cast(&mut state, spell, &[Target::Object(target)]);
        assert!(matches!(
            next(&mut state),
            Decision::CastSpellOrPass { player, .. } if player == actor
        ));
        assert_eq!(zone(&state, spell), Zone::Stack);
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        act(&mut state, Action::Pass);
        assert!(
            matches!(
                next(&mut state),
                Decision::CastSpellOrPass { player, castable_spells, .. }
                    if player == owner && !castable_spells.contains(&hand)
            ),
            "split second"
        );

        let prompt = settle(&mut state).expect("native Extirpate search");
        assert!(matches!(
            &prompt,
            Decision::ChooseEffectTargets {
                player, legal_targets, min_targets: 0, max_targets: 2, can_finish: true, ..
            } if *player == actor
                && legal_targets.as_slice() == [Target::Object(hand), Target::Object(library)]
        ));
        assert_eq!(zone(&state, target), Zone::Graveyard);
        assert_eq!(zone(&state, graveyard), Zone::Graveyard);
        let look = extirpate_search_look_v1(&state, actor).unwrap().unwrap();
        assert_eq!(look.owner, PlayerSeatV1::from(owner));
        assert_eq!(look.hand_printings, ["Fireblast", "Lightning Bolt"]);
        assert_eq!(
            look.library_printings,
            ["Island", "Island", "Island", "Island", "Lightning Bolt"]
        );
        assert!(extirpate_search_look_v1(&state, owner).unwrap().is_none());
        assert!(surgical_search_look_v1(&state, actor).unwrap().is_none());

        let rows = legal_action_candidates_v1(&SurfaceDecision::Decision(prompt), &state).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(matches!(
            &rows[0].record.semantic,
            ActionSemanticV1::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
                if object.arena_id == hand.0 && object.card_db_id == id("Lightning Bolt")
        ));
        assert_eq!(
            rows[1].surface_action,
            SurfaceAction::Action(Action::ChooseEffectTarget(Target::Object(library)))
        );
        assert_eq!(
            rows[2].surface_action,
            SurfaceAction::Action(Action::FinishEffectSelection)
        );

        let check_outcome = |branch: &mut GameState, expected_hand, expected_library| {
            settled(branch);
            for card in [target, graveyard] {
                assert_eq!(zone(branch, card), Zone::Exile);
            }
            assert_eq!(zone(branch, hand), expected_hand);
            assert_eq!(zone(branch, library), expected_library);
            assert_eq!(zone(branch, other_hand), Zone::Hand);
            assert_eq!(zone(branch, other_graveyard), Zone::Graveyard);
            assert_eq!(zone(branch, own_match), Zone::Graveyard);
            assert_eq!(zone(branch, spell), Zone::Graveyard);
            assert!(extirpate_search_look_v1(branch, actor).unwrap().is_none());
            assert!(branch.observation_journal.notes().iter().any(|note| matches!(
                note.kind,
                ObservationNoteKindV1::LibraryRandomized { owner: shuffled } if shuffled == owner
            )));
        };
        let mut zero = state.clone();
        act(&mut zero, Action::FinishEffectSelection);
        check_outcome(&mut zero, Zone::Hand, Zone::Library);
        assert!(zero
            .known_hand_cards(actor, owner)
            .iter()
            .any(|entry| entry.object == other_hand));
        assert!(zero.known_hand_cards(owner, actor).is_empty());

        choose_target(&mut state, Target::Object(hand));
        let mut subset = state.clone();
        act(&mut subset, Action::FinishEffectSelection);
        check_outcome(&mut subset, Zone::Exile, Zone::Library);

        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(next(&mut state), next(&mut restored));
        for changed in [graveyard, library] {
            let mut stale = restored.clone();
            stale.objects.get_mut(changed).zone_change_count += 1;
            assert!(mtg_kernel::effect::validate_pending_effect_choice(&stale).is_err());
        }
        for branch in [&mut state, &mut restored] {
            choose_target(branch, Target::Object(library));
            check_outcome(branch, Zone::Exile, Zone::Exile);
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

#[test]
fn extirpate_hides_the_search_look_and_matching_width_from_the_nonchooser() {
    use mtg_kernel::eternal_sideboard_s1b_v1::extirpate_search_look_v1;
    use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
    use mtg_kernel::rl::{observe_policy_v5, PendingEffectChoiceSemanticV4};

    for actor in [P0, P1] {
        let viewer = actor.opponent();
        let observe = |hidden, width| {
            let mut state = game(&["Island"; 4]);
            state.active_player = actor;
            state.priority_player = actor;
            let target = put(&mut state, actor, "Lightning Bolt", Zone::Graveyard);
            put(&mut state, actor, hidden, Zone::Library);
            let spell = put(&mut state, actor, "Extirpate", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::B], 0);
            cast(&mut state, spell, &[Target::Object(target)]);
            assert!(matches!(
                settle(&mut state),
                Some(Decision::ChooseEffectTargets { max_targets, can_finish: true, .. })
                    if max_targets == width
            ));
            let look = extirpate_search_look_v1(&state, actor).unwrap().unwrap();
            assert!(look.library_printings.iter().any(|name| name == hidden));
            assert!(extirpate_search_look_v1(&state, viewer).unwrap().is_none());
            let observation =
                observe_policy_v5(&state, &PolicySurfaceV5::new(), viewer, 0, 0, 0, 1).unwrap();
            assert!(matches!(
                observation.projection.surface.engine_context.pending_effect.as_ref()
                    .and_then(|pending| pending.choice.as_ref()),
                Some(PendingEffectChoiceSemanticV4::Targets {
                    selected_targets, legal_targets, min_targets: 0, max_targets: 0, ..
                }) if selected_targets.is_empty() && legal_targets.is_empty()
            ));
            if width == 0 {
                act(&mut state, Action::FinishEffectSelection);
                settled(&mut state);
                assert_eq!(zone(&state, target), Zone::Exile);
            }
            observation
        };
        assert_eq!(observe("Lightning Bolt", 1), observe("Forest", 0));
    }
}

#[test]
fn tourach_grows_when_an_opponent_discards_and_makes_them_discard_when_kicked() {
    let mut state = islands();
    let tourach = put(&mut state, P0, "Tourach, Dread Cantor", Zone::Hand);
    for _ in 0..4 {
        put(&mut state, P1, "Island", Zone::Hand);
    }
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::B, ManaColor::B],
        0,
    );
    act(&mut state, Action::CastSpell(tourach));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => Action::ChooseKicker(true),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, tourach), Zone::Battlefield);
    assert_eq!(state.players[1].hand.len(), 2, "two discarded at random");
    assert_eq!(power_toughness(&state, tourach), (4, 3), "two counters");
    assert!(def("Tourach, Dread Cantor").kicker_cost.is_some());
}

#[test]
fn leyline_of_sanctity_gives_its_controller_hexproof_and_may_open_the_game() {
    let mut state = islands();
    put(&mut state, P0, "Leyline of Sanctity", Zone::Battlefield);
    state.priority_player = P1;
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    let legal = cast_targets(&state, bolt);
    assert!(!legal.contains(&Target::Player(P0)), "hexproof");
    assert!(legal.contains(&Target::Player(P1)));

    // Opening hand: offered as the first upkeep begins.
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4c45_594c,
    );
    let starter = state.starting_player;
    let leyline = put(&mut state, starter, "Leyline of Sanctity", Zone::Hand);
    let mut offered = false;
    for _ in 0..100 {
        match next(&mut state) {
            Decision::ChooseEffectOption { .. } => {
                offered = true;
                act(&mut state, Action::ChooseEffectOption(0));
                break;
            }
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => act(&mut state, first_answer(other)),
        }
    }
    assert!(offered, "the opening-hand offer");
    settled(&mut state);
    assert_eq!(zone(&state, leyline), Zone::Battlefield);
}

#[test]
fn perilous_snare_exiles_until_it_leaves_and_grows_a_creature_at_max_speed() {
    let mut state = islands();
    let theirs = put(&mut state, P1, "Fury", Zone::Battlefield);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let snare = put(&mut state, P0, "Perilous Snare", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, snare, &[]);
    settle_choosing(&mut state, theirs);
    assert_eq!(zone(&state, snare), Zone::Battlefield);
    assert_eq!(zone(&state, theirs), Zone::Exile);
    assert_eq!(
        state.speed_v1.map(|speed| speed.speeds[0]),
        Some(1),
        "start your engines"
    );
    // Below max speed the counter ability is not offered.
    assert!(!activatable(&mut state, snare, 0));
    state.speed_v1.as_mut().expect("speed").speeds[0] = 4;
    state.objects.get_mut(snare).summoning_sick = false;
    activate(&mut state, snare, 0, &[Target::Object(mine)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, mine), (2, 2));
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::zone_change(snare, Zone::Graveyard),
    );
    // Fury's own entry trigger asks for its damage.
    settle_first(&mut state);
    assert_eq!(zone(&state, theirs), Zone::Battlefield, "returned");
}

#[test]
fn engineered_explosives_enters_with_a_counter_per_color_and_destroys_that_mana_value() {
    let mut state = islands();
    let oracle = put(&mut state, P1, "Thassa's Oracle", Zone::Battlefield);
    let fury = put(&mut state, P1, "Fury", Zone::Battlefield);
    let star = put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    let ee = put(&mut state, P0, "Engineered Explosives", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
    act(&mut state, Action::CastSpell(ee));
    announce_x(&mut state, 2);
    settled(&mut state);
    assert_eq!(zone(&state, ee), Zone::Battlefield);
    assert_eq!(state.objects.get(ee).counters.charge, 2, "two colors spent");
    pool(&mut state, P0, &[], 2);
    activate(&mut state, ee, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ee), Zone::Graveyard);
    assert_eq!(zone(&state, oracle), Zone::Graveyard, "mana value 2");
    assert_eq!(zone(&state, fury), Zone::Battlefield, "mana value 5");
    assert_eq!(zone(&state, star), Zone::Battlefield, "mana value 1");
}

#[test]
fn liquimetal_coating_makes_a_permanent_an_artifact_for_the_turn() {
    let mut state = islands();
    let fury = put(&mut state, P1, "Fury", Zone::Battlefield);
    let coating = put(&mut state, P0, "Liquimetal Coating", Zone::Battlefield);
    state.objects.get_mut(coating).summoning_sick = false;
    assert!(!engine::object_has_type(&state, fury, CardType::Artifact));
    activate(&mut state, coating, 0, &[Target::Object(fury)]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, fury, CardType::Artifact));
    assert!(engine::object_has_type(&state, fury, CardType::Creature));
    // Stony Silence now stops its (nonexistent) abilities, and Whipflare
    // spares it as an artifact creature.
    let flare = put(&mut state, P0, "Whipflare", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, flare, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(fury).damage, 0);
}
