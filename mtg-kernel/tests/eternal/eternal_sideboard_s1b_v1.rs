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
        "{decision:?}\nvalidation: {:?}\nlast event: {:?}\nstack: {:#?}\npending triggers: {:#?}",
        engine::step(&mut state.clone(), Action::Pass),
        state.engine.event_history.last(),
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
    let groups = mtg_kernel::eternal_sideboard_s1b_v1::nameable_nonland();
    assert!(
        groups.iter().flatten().all(|&named| !def_is_land(named)),
        "nonland names only"
    );
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
    choose_mage_name(&mut state, P0, "Lightning Bolt");
    settled(&mut state);
    assert_eq!(zone(&state, mage), Zone::Battlefield);
    state.priority_player = P1;
    let theirs = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let other = put(&mut state, P1, "Lava Dart", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(!castable(&mut state, theirs), "named");
    assert!(castable(&mut state, other), "another spell");
}

fn choose_mage_name_group(state: &mut GameState, player: PlayerId, name: &str) -> u16 {
    let Decision::ChooseMeddlingMageNameV1 {
        player: chooser,
        group: None,
        options,
        ..
    } = next(state)
    else {
        panic!("expected the catalog name groups")
    };
    assert_eq!(chooser, player);
    let letter = name
        .chars()
        .next()
        .unwrap()
        .to_ascii_uppercase()
        .to_string();
    let group = options.iter().position(|option| option == &letter).unwrap() as u16;
    act(state, Action::ChooseEffectOption(group));
    group
}

fn choose_mage_name(state: &mut GameState, player: PlayerId, name: &str) {
    let group = choose_mage_name_group(state, player, name);
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

/// Wave S1k: Limited schema 5's consumer successor. The policy surface
/// offers the name menu as `choose_meddling_mage_name_v1` actions, letter
/// then name, and schema 5's policy observation lists the entered Mage with
/// its name for both viewers, while the named spell is offered to neither.
/// The observation without the public shroud contract still refuses the
/// pending entry and the named Mage.
#[test]
fn meddling_mage_name_reaches_the_schema_5_session_both_seats() {
    use mtg_kernel::policy_surface_v5::{PolicyDecisionV5, PolicySurfaceV5};
    use mtg_kernel::rl::{
        observe_policy_v5, policy_legal_action_candidates_v5, ActionSemanticV1, PlayerSeatV1,
    };
    use mtg_kernel::surface::SurfaceDecision;

    fn observed(state: &GameState, viewer: PlayerId) -> Result<Vec<(u16, String)>, String> {
        let observation = observe_policy_v5(state, &PolicySurfaceV5::new(), viewer, 0, 0, 0, 1)
            .map_err(|error| error.to_string())?;
        Ok(observation
            .projection
            .meddling_mage_names
            .unwrap_or_default()
            .into_iter()
            .map(|row| (row.source.card_db_id, row.name))
            .collect())
    }
    fn frozen(state: &GameState, viewer: PlayerId) -> String {
        let mut frozen = state.clone();
        frozen.public_shroud_v1 = false;
        observed(&frozen, viewer).expect_err("schema 4 refuses Meddling Mage's entry")
    }

    for chooser in [P0, P1] {
        let opponent = chooser.opponent();
        let mut state = islands();
        state.active_player = chooser;
        state.priority_player = chooser;
        mtg_kernel::rl::enable_public_shroud_v1(&mut state);
        let mage = put(&mut state, chooser, "Meddling Mage", Zone::Hand);
        let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);
        let dart = put(&mut state, opponent, "Lava Dart", Zone::Hand);
        pool(&mut state, chooser, &[ManaColor::W, ManaColor::U], 0);
        cast(&mut state, mage, &[]);
        assert!(settle(&mut state).is_some());
        let mut surface = PolicySurfaceV5::new();
        let mut group = None;
        for text in ["L", "Lightning Bolt"] {
            let decision = surface.next_decision(&mut state).unwrap();
            assert!(
                matches!(&decision, PolicyDecisionV5::Surface(SurfaceDecision::Decision(
                    Decision::ChooseMeddlingMageNameV1 { player, group: actual, .. }
                )) if *player == chooser && *actual == group),
                "{decision:?}"
            );
            // The pending entry is observed without any name; its saved
            // world stays private.
            assert_eq!(observed(&state, chooser), Ok(Vec::new()));
            assert!(frozen(&state, chooser).contains("Meddling Mage named entry"));
            let candidates =
                policy_legal_action_candidates_v5(&decision, &surface, &state).unwrap();
            assert_eq!(
                serde_json::to_value(&candidates[0].record.semantic).unwrap()["action_kind"],
                "choose_meddling_mage_name_v1"
            );
            let mut chosen = None;
            for (index, candidate) in candidates.iter().enumerate() {
                let ActionSemanticV1::ChooseMeddlingMageNameV1 {
                    actor,
                    entry_ordinal,
                    group: actual,
                    option_index,
                    option_count,
                    option_text,
                } = &candidate.record.semantic
                else {
                    panic!("{:?}", candidate.record.semantic)
                };
                assert_eq!(
                    (*actor, *entry_ordinal, *actual),
                    (PlayerSeatV1::from(chooser), 0, group)
                );
                assert_eq!(
                    (usize::from(*option_index), usize::from(*option_count)),
                    (index, candidates.len())
                );
                if option_text == text {
                    chosen = Some((*option_index, candidate.policy_action.clone()));
                }
            }
            let (index, action) = chosen.unwrap_or_else(|| panic!("{text} is not offered"));
            if group.is_none() {
                group = Some(index);
            }
            surface.apply(&mut state, action).unwrap();
        }
        settled(&mut state);
        assert_eq!(zone(&state, mage), Zone::Battlefield);
        let expected = vec![(id("Meddling Mage"), "Lightning Bolt".to_owned())];
        for viewer in [chooser, opponent] {
            assert_eq!(observed(&state, viewer).as_ref(), Ok(&expected));
            assert!(frozen(&state, viewer).contains("Meddling Mage named entry"));
        }
        // The opponent is offered Lava Dart but not the named Bolt.
        state.priority_player = opponent;
        pool(&mut state, opponent, &[ManaColor::R, ManaColor::R], 0);
        let mut surface = PolicySurfaceV5::new();
        let decision = surface.next_decision(&mut state).unwrap();
        let casts: Vec<u32> = policy_legal_action_candidates_v5(&decision, &surface, &state)
            .unwrap()
            .into_iter()
            .filter_map(|candidate| match candidate.record.semantic {
                ActionSemanticV1::CastSpell { source, .. } => Some(source.arena_id),
                _ => None,
            })
            .collect();
        assert!(
            casts.contains(&dart.0) && !casts.contains(&bolt.0),
            "{casts:?}"
        );
    }
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
                if prohibited {
                    "Lightning Bolt"
                } else {
                    "Lava Dart"
                },
                &["Island", "Lightning Bolt", "Plains", "Plains"],
            );
            let bolt = state.players[actor.index()].library[1];
            let party = put(&mut state, actor, "Boarding Party", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::R], 5);
            cast(&mut state, party, &[]);
            let mut offers = 0;
            settle_with(&mut state, |_, decision| match decision {
                Decision::ChooseEffectOption {
                    option_count: 2, ..
                } => {
                    offers += 1;
                    Action::ChooseEffectOption(0)
                }
                Decision::ChooseEffectTargets { legal_targets, .. }
                    if legal_targets == [Target::Object(bolt)] =>
                {
                    Action::ChooseEffectTarget(Target::Object(bolt))
                }
                Decision::ChooseTargets { .. } => {
                    Action::ChooseTarget(Target::Player(actor.opponent()))
                }
                other => first_answer(other),
            });
            assert_eq!(offers, usize::from(!prohibited));
            assert_eq!(zone(&state, party), Zone::Battlefield);
            assert_eq!(
                zone(&state, bolt),
                if prohibited {
                    Zone::Library
                } else {
                    Zone::Graveyard
                }
            );
            assert_eq!(
                life(&state, actor.opponent()),
                if prohibited { 20 } else { 17 }
            );
        }
    }
}

#[test]
fn meddling_mage_filters_paid_bilbo_resolution_cast_both_seats() {
    for actor in [P0, P1] {
        let mut state = paid_meddling_against(actor, "Brotherhood's End", &["Island"; 20]);
        let bilbo = put(
            &mut state,
            actor,
            "Bilbo, Thief in the Night",
            Zone::Battlefield,
        );
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
            assert!(engine::step(
                continued,
                Action::ChooseEffectTarget(Target::Object(forbidden))
            )
            .is_err());
            act(continued, Action::ChooseEffectTarget(Target::Object(ring)));
            settled(continued);
            assert_eq!(zone(continued, forbidden), Zone::Graveyard);
            assert_eq!(zone(continued, ring), Zone::Battlefield);
            assert_eq!(
                continued.players[actor.index()]
                    .mana_pool
                    .iter()
                    .sum::<u8>(),
                2
            );
        }
    }
}

#[test]
fn meddling_mage_names_exact_split_and_adventure_forms_both_seats() {
    for actor in [P0, P1] {
        let chooser = actor.opponent();
        for (name, printing, blocked_color) in [
            ("Fire", "Fire // Ice", ManaColor::R),
            ("Ice", "Fire // Ice", ManaColor::U),
            ("Forktail Sweep", "Fang Dragon", ManaColor::R),
        ] {
            let mut state = islands();
            state.active_player = chooser;
            state.priority_player = chooser;
            let mage = put(&mut state, chooser, "Meddling Mage", Zone::Hand);
            pool(&mut state, chooser, &[ManaColor::W, ManaColor::U], 0);
            cast(&mut state, mage, &[]);
            assert!(settle(&mut state).is_some());
            if name == "Fire" {
                // Recreate the prior checkpoint shape, before any group was chosen.
                let mut legacy = serde_json::to_value(&state).unwrap();
                let checkpoint =
                    &mut legacy["engine"]["linked_aura_operation_v1"]["pending_meddling_mage_v1"];
                let operation = checkpoint["operation_history_index"].as_u64().unwrap() as usize;
                assert!(checkpoint["choice"]["MeddlingMageV1"]["choice"]
                    .as_object_mut()
                    .unwrap()
                    .remove("names_v2")
                    .is_some());
                let journal = legacy["engine"]["event_history"][operation]["MeddlingMageEntryV1"]
                    ["journal"]
                    .as_object_mut()
                    .unwrap();
                let started = journal.remove("StartedWithNamesV2").unwrap();
                journal.insert("Started".into(), started);
                let mut legacy: GameState = serde_json::from_value(legacy).unwrap();
                choose_mage_name(&mut legacy, chooser, "Lightning Bolt");
                settled(&mut legacy);
                let retained = &legacy.eternal_v1.as_ref().unwrap().s1b.mage_names;
                assert_eq!(
                    serde_json::to_value(retained[0].1).unwrap(),
                    serde_json::json!(id("Lightning Bolt"))
                );
                let mut old_hash = std::collections::hash_map::DefaultHasher::new();
                let mut new_hash = std::collections::hash_map::DefaultHasher::new();
                use std::hash::{Hash, Hasher};
                [(retained[0].0, id("Lightning Bolt"))].hash(&mut old_hash);
                retained.hash(&mut new_hash);
                assert_eq!(old_hash.finish(), new_hash.finish());

                let mut land_check = state.clone();
                choose_mage_name_group(&mut land_check, chooser, "Shatterskull Smashing");
                let Decision::ChooseMeddlingMageNameV1 { options, .. } = next(&mut land_check)
                else {
                    panic!("expected the Shatterskull name group")
                };
                assert!(options
                    .iter()
                    .any(|option| option == "Shatterskull Smashing"));
                assert!(!options
                    .iter()
                    .any(|option| option == "Shatterskull, the Hammer Pass"));
            }
            let group = choose_mage_name_group(&mut state, chooser, name);
            let menu = next(&mut state);
            let Decision::ChooseMeddlingMageNameV1 {
                options,
                group: Some(selected),
                ..
            } = &menu
            else {
                panic!("expected names in the chosen group")
            };
            assert_eq!(*selected, group);
            assert!(!options.iter().any(|option| option == "Fire // Ice"));
            let index = options.iter().position(|option| option == name).unwrap() as u16;

            let snapshot = serde_json::to_value(&state).unwrap();
            let mut altered = snapshot.clone();
            assert!(
                altered["engine"]["linked_aura_operation_v1"]["pending_meddling_mage_v1"]["choice"]
                    ["MeddlingMageV1"]["choice"]
                    .as_object_mut()
                    .unwrap()
                    .remove("names_v2")
                    .is_some()
            );
            let mut altered: GameState = serde_json::from_value(altered).unwrap();
            assert!(engine::step(&mut altered, Action::ChooseEffectOption(index)).is_err());
            state = serde_json::from_value(snapshot).unwrap();
            assert_eq!(next(&mut state), menu);
            act(&mut state, Action::ChooseEffectOption(index));
            settled(&mut state);
            assert_eq!(zone(&state, mage), Zone::Battlefield);

            if name == "Ice" {
                let mut altered = serde_json::to_value(&state).unwrap();
                let named = altered["engine"]["event_history"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find_map(|row| {
                        row.get_mut("MeddlingMageEntryV1")?
                            .get_mut("journal")?
                            .get_mut("Named")
                    })
                    .unwrap();
                named["named"]["face"] = serde_json::json!("LeftHalf");
                altered["eternal_v1"]["s1b"]["mage_names"][0][1]["face"] =
                    serde_json::json!("LeftHalf");
                let mut altered: GameState = serde_json::from_value(altered).unwrap();
                assert!(engine::step(&mut altered, Action::Pass).is_err());
            }

            state = serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
            state.active_player = actor;
            state.priority_player = actor;
            state.engine.priority_passes = [false; 2];
            let spell = put(&mut state, actor, printing, Zone::Hand);
            pool(&mut state, actor, &[blocked_color], 1);
            assert!(!castable(&mut state, spell), "only {name} would be payable");
            match name {
                "Fire" => {
                    pool(&mut state, actor, &[ManaColor::U], 0);
                    cast_mode(&mut state, spell, 1, &[Target::Object(mage)]);
                    settled(&mut state);
                    assert!(state.objects.get(mage).tapped);
                    assert_eq!(zone(&state, spell), Zone::Graveyard);
                }
                "Ice" => {
                    pool(&mut state, actor, &[ManaColor::R], 0);
                    cast_mode(&mut state, spell, 0, &[Target::Player(chooser); 2]);
                    settled(&mut state);
                    assert_eq!(life(&state, chooser), 18);
                    assert_eq!(zone(&state, spell), Zone::Graveyard);
                }
                _ => {
                    pool(&mut state, actor, &[ManaColor::R], 4);
                    cast_mode(&mut state, spell, 0, &[]);
                    settled(&mut state);
                    assert_eq!(zone(&state, spell), Zone::Battlefield);
                }
            }
        }
        let mut state = paid_meddling_against(actor, "Luminous Phantom", &["Island"; 20]);
        let disturbed = put(&mut state, actor, "Lunarch Veteran", Zone::Graveyard);
        let normal = put(&mut state, actor, "Lunarch Veteran", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::W], 1);
        assert!(!castable(&mut state, disturbed));
        cast(&mut state, normal, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, normal), Zone::Battlefield);
    }
}

#[test]
fn reanimated_metamorph_chooses_copy_and_mage_name_before_entry_both_seats() {
    use mtg_kernel::event::CommittedEvent;

    for actor in [P0, P1] {
        let opponent = actor.opponent();
        for mixed in [false, true] {
            let mut state = paid_meddling_against(actor, "Lava Dart", &["Island"; 20]);
            let mage = state.players[opponent.index()].battlefield[0];
            let metamorph = put(&mut state, opponent, "Phyrexian Metamorph", Zone::Graveyard);
            let generation = state.objects.get(metamorph).zone_change_count;
            let prior = mixed.then(|| {
                let wall = put(&mut state, opponent, "Wall of Blossoms", Zone::Graveyard);
                let reanimate = put(&mut state, actor, "Reanimate", Zone::Hand);
                pool(&mut state, actor, &[ManaColor::B], 0);
                cast(&mut state, reanimate, &[Target::Object(wall)]);
                assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
                let source = mtg_kernel::state::AbilitySourceContractV4::capture(&state, reanimate);
                let receipt = state
                    .engine
                    .event_history
                    .iter()
                    .find_map(|event| match event {
                        CommittedEvent::PublishedCastIdentityV1 { receipt }
                            if receipt.spell == reanimate =>
                        {
                            Some(*receipt)
                        }
                        _ => None,
                    })
                    .unwrap();
                settled(&mut state);
                assert_eq!(zone(&state, reanimate), Zone::Graveyard);
                assert_eq!(zone(&state, wall), Zone::Battlefield);
                (wall, source, receipt)
            });
            let wall = prior.map(|(wall, _, _)| wall);
            let life_before = life(&state, actor);
            assert_eq!(zone(&state, metamorph), Zone::Graveyard);
            assert_eq!(state.objects.get(metamorph).zone_change_count, generation);
            let reanimate = put(&mut state, actor, "Reanimate", Zone::Hand);
            let mage_history_count = |state: &GameState| {
                state
                    .engine
                    .event_history
                    .iter()
                    .filter(|event| matches!(event, CommittedEvent::MeddlingMageEntryV1 { .. }))
                    .count()
            };
            let history_before_copy = mage_history_count(&state);
            pool(&mut state, actor, &[ManaColor::B], 0);
            cast(&mut state, reanimate, &[Target::Object(metamorph)]);
            let choice = settle(&mut state).expect("native noncast Metamorph copy menu");
            let Decision::ChooseEffectTargets {
                player,
                source,
                min_targets: 0,
                max_targets: 1,
                can_finish: true,
                ref legal_targets,
                ..
            } = choice
            else {
                panic!("unexpected copy menu {choice:?}");
            };
            assert_eq!((player, source), (actor, metamorph));
            assert_eq!(legal_targets.len(), if mixed { 2 } else { 1 });
            assert!(legal_targets.contains(&Target::Object(mage)));
            if let Some(wall) = wall {
                assert!(legal_targets.contains(&Target::Object(wall)));
            }
            assert_eq!(zone(&state, metamorph), Zone::Graveyard);
            assert_eq!(zone(&state, reanimate), Zone::Stack);
            assert_eq!(life(&state, actor), life_before);
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            assert_eq!(mage_history_count(&state), history_before_copy);
            let frozen_hand = state.players[actor.index()].hand.len();

            let mut forged = serde_json::to_value(&state).unwrap();
            let candidate = &mut forged["engine"]["linked_aura_operation_v1"]
                ["pending_meddling_mage_v1"]["choice"]["ReanimateMetamorphV1"]["choice"]
                ["candidates"][0]["zone_change_count"];
            *candidate = serde_json::json!(candidate.as_u64().unwrap() + 1);
            let mut forged: GameState = serde_json::from_value(forged).unwrap();
            let forged_hash = forged.state_hash();
            assert!(engine::step(&mut forged, Action::FinishEffectSelection).is_err());
            assert_eq!(forged.state_hash(), forged_hash);

            let copy_snapshot = serde_json::to_vec(&state).unwrap();
            for selected in std::iter::once(Some(mage))
                .chain(wall.map(Some))
                .chain([None])
            {
                let mut restored: GameState = serde_json::from_slice(&copy_snapshot).unwrap();
                assert_eq!(next(&mut restored), choice);
                let command = selected.map_or(Action::FinishEffectSelection, |object| {
                    Action::ChooseEffectTarget(Target::Object(object))
                });
                act(&mut restored, command);
                if selected == Some(mage) {
                    assert!(matches!(next(&mut restored),
                        Decision::ChooseMeddlingMageNameV1 { player, group: None, .. }
                        if player == actor));
                    assert_eq!(zone(&restored, metamorph), Zone::Graveyard);
                    assert_eq!(zone(&restored, reanimate), Zone::Stack);
                    assert_eq!(life(&restored, actor), life_before);
                    let name_snapshot = serde_json::to_vec(&restored).unwrap();
                    restored = serde_json::from_slice(&name_snapshot).unwrap();
                    choose_mage_name(&mut restored, actor, "Lightning Bolt");
                }
                settled(&mut restored);
                assert_eq!(life(&restored, actor), life_before - 4);
                assert_eq!(zone(&restored, reanimate), Zone::Graveyard);
                assert_eq!(restored.objects.get(metamorph).owner, opponent);
                assert_eq!(
                    restored.players[actor.index()].hand.len(),
                    frozen_hand + usize::from(selected.is_some_and(|object| Some(object) == wall))
                );
                if let Some(donor) = selected {
                    assert_eq!(zone(&restored, metamorph), Zone::Battlefield);
                    assert_eq!(restored.objects.get(metamorph).controller, actor);
                    assert_eq!(
                        restored.objects.get(metamorph).card_def,
                        restored.objects.get(donor).card_def
                    );
                    assert_eq!(
                        restored.objects.get(metamorph).zone_change_count,
                        generation + 1
                    );
                    assert!(!restored.objects.get(metamorph).v4.is_token);
                    assert!(engine::object_has_type(
                        &restored,
                        metamorph,
                        CardType::Artifact
                    ));
                    assert_eq!(
                        restored.eternal_v1.as_ref().unwrap().s1b.mage_names.len(),
                        if donor == mage { 2 } else { 1 }
                    );
                } else {
                    assert_eq!(zone(&restored, metamorph), Zone::Graveyard);
                    assert_eq!(
                        restored.objects.get(metamorph).card_def,
                        id("Phyrexian Metamorph")
                    );
                    assert_eq!(
                        restored.objects.get(metamorph).zone_change_count,
                        generation + 2
                    );
                }
                assert!(restored.engine.event_history.iter().any(|event| matches!(event,
                    CommittedEvent::ZoneChange { object, from: Zone::Graveyard, to: Zone::Battlefield, .. }
                    if *object == metamorph)));
                if let Some((wall, prior_source, prior_cast)) =
                    prior.filter(|(wall, _, _)| selected == Some(*wall))
                {
                    assert_eq!(restored.objects.get(wall).card_def, id("Wall of Blossoms"));
                    let mut wrong_producer = serde_json::to_value(&restored).unwrap();
                    let copy = wrong_producer["engine"]["event_history"]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find_map(|event| {
                            event
                                .get_mut("MeddlingMageEntryV1")?
                                .get_mut("journal")?
                                .get_mut("MetamorphCopied")?
                                .get_mut("copy")
                        })
                        .unwrap();
                    copy["reanimated_by"] = serde_json::to_value(prior_source).unwrap();
                    copy["cast_at"] = serde_json::json!(prior_cast.spell_cast_history_index);
                    copy["stack_item"] = serde_json::to_value(prior_cast.stack_item_id).unwrap();

                    let mut wrong_form = serde_json::to_value(&restored).unwrap();
                    let forms = wrong_form["standard_v1"]["copied_card_defs"]
                        .as_array_mut()
                        .unwrap();
                    let actual =
                        serde_json::json!([metamorph, generation + 1, id("Wall of Blossoms")]);
                    let count = forms.len();
                    forms.retain(|form| *form != actual);
                    assert_eq!(forms.len(), count - 1);
                    for forged in [wrong_producer, wrong_form] {
                        let mut forged: GameState = serde_json::from_value(forged).unwrap();
                        let before = forged.state_hash();
                        assert!(engine::step(&mut forged, Action::Pass).is_err());
                        assert_eq!(forged.state_hash(), before);
                    }
                }
            }
        }
    }
}

fn reanimate_copy(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let metamorph = put(
        state,
        actor.opponent(),
        "Phyrexian Metamorph",
        Zone::Graveyard,
    );
    let spell = put(state, actor, "Reanimate", Zone::Hand);
    pool(state, actor, &[ManaColor::B], 0);
    cast(state, spell, &[Target::Object(metamorph)]);
    assert!(
        matches!(settle(state), Some(Decision::ChooseEffectTargets { source, can_finish: true, .. }) if source == metamorph)
    );
    metamorph
}

fn shroud_copy_menu(state: &GameState, viewer: PlayerId) -> serde_json::Value {
    let observation = mtg_kernel::rl::observe_policy_v5(
        state,
        &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
        viewer,
        0,
        0,
        0,
        1,
    )
    .unwrap();
    serde_json::to_value(observation.projection.copy_entry).unwrap()
}

#[test]
fn noncast_metamorph_face_down_and_peacekeeper_halt_only_on_selection() {
    use mtg_kernel::engine::UnsupportedMechanic;
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        state.public_shroud_v1 = true;
        let stone = put(&mut state, actor, "Mind Stone", Zone::Battlefield);
        let keeper = put(&mut state, actor, "Anointed Peacekeeper", Zone::Battlefield);
        let hidden = put(
            &mut state,
            actor.opponent(),
            "Show and Tell",
            Zone::Battlefield,
        );
        state.objects.get_mut(hidden).v4.face_down_v1 = Some(mtg_kernel::state::FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        let metamorph = reanimate_copy(&mut state, actor);
        for viewer in [actor, actor.opponent()] {
            let menu = shroud_copy_menu(&state, viewer);
            let candidates = menu["candidates"].as_array().unwrap();
            assert_eq!(candidates.len(), 3);
            let redacted = candidates
                .iter()
                .find(|row| row["source"] == serde_json::json!(hidden))
                .unwrap();
            assert_eq!(redacted["card_def"], id("Face-down creature"));
            assert_eq!(menu["can_decline"], true);
        }
        let mut forged = serde_json::to_value(&state).unwrap();
        let roster = forged["engine"]["linked_aura_operation_v1"]["pending_meddling_mage_v1"]
            ["choice"]["ReanimateMetamorphV1"]["choice"]["candidates"]
            .as_array_mut()
            .unwrap();
        let face = roster
            .iter_mut()
            .find(|row| row["source"] == serde_json::json!(hidden))
            .unwrap();
        face["card_def"] = id("Show and Tell").into();
        assert_copy_restore_refused(forged, Action::FinishEffectSelection);
        for unsupported in [keeper, hidden] {
            let mut selected = state.clone();
            act(
                &mut selected,
                Action::ChooseEffectTarget(Target::Object(unsupported)),
            );
            assert!(matches!(
                engine::advance_until_decision(&mut selected),
                Decision::Halted {
                    mechanic: UnsupportedMechanic::InvalidEffectContinuation,
                    ..
                }
            ));
            assert_eq!(zone(&selected, metamorph), Zone::Graveyard);
        }
        let mut declined = state.clone();
        act(&mut declined, Action::FinishEffectSelection);
        settled(&mut declined);
        assert_eq!(zone(&declined, metamorph), Zone::Graveyard);
        assert_eq!(
            declined.objects.get(metamorph).card_def,
            id("Phyrexian Metamorph")
        );
        assert_eq!(life(&declined, actor), 16);
        assert!(declined.engine.event_history.iter().any(|event| matches!(event, mtg_kernel::event::CommittedEvent::ZoneChange { object, to: Zone::Battlefield, .. } if *object == metamorph)));

        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(stone)),
        );
        settled(&mut state);
        assert_eq!(life(&state, actor), 16);
        assert_eq!(state.objects.get(metamorph).card_def, id("Mind Stone"));
        assert!(!state.objects.get(metamorph).v4.is_token);
        assert!(shroud_copy_menu(&state, actor).is_null());
        let mana = state.players[actor.index()].mana_pool[ManaColor::C.pool_index()];
        act(&mut state, Action::ActivateManaAbility(metamorph));
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
            mana + 1
        );
        mtg_kernel::event::propose_and_commit(
            &mut state,
            mtg_kernel::event::ProposedEvent::zone_change(metamorph, Zone::Graveyard),
        );
        assert_eq!(
            state.objects.get(metamorph).card_def,
            id("Phyrexian Metamorph")
        );
    }
}

#[test]
fn dress_down_suppresses_the_noncast_copy_menu_in_both_seats() {
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let dress = put(&mut state, actor, "Dress Down", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 1);
        cast(&mut state, dress, &[]);
        settled(&mut state);
        put(&mut state, actor, "Mind Stone", Zone::Battlefield);
        let metamorph = put(
            &mut state,
            actor.opponent(),
            "Phyrexian Metamorph",
            Zone::Graveyard,
        );
        let spell = put(&mut state, actor, "Reanimate", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::B], 0);
        cast(&mut state, spell, &[Target::Object(metamorph)]);
        settled(&mut state);
        assert_eq!(zone(&state, metamorph), Zone::Graveyard);
        assert_eq!(life(&state, actor), 16);
        assert!(mtg_kernel::rl::pending_copy_entry_v1(&state, actor).is_none());
    }
}

#[test]
fn show_and_tell_noncast_metamorph_copies_only_preexisting_mind_stone() {
    for (actor, companion_name) in [
        (P0, "Island"),
        (P1, "Island"),
        (P0, "Wall of Blossoms"),
        (P1, "Wall of Blossoms"),
    ] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        state.public_shroud_v1 = true;
        let stone = put(
            &mut state,
            actor.opponent(),
            "Mind Stone",
            Zone::Battlefield,
        );
        let metamorph = put(&mut state, actor, "Phyrexian Metamorph", Zone::Hand);
        let companion = put(&mut state, actor.opponent(), companion_name, Zone::Hand);
        let show = put(&mut state, actor, "Show and Tell", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 2);
        cast(&mut state, show, &[]);
        assert!(settle(&mut state).is_some());
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(metamorph)),
        );
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(companion)),
        );
        assert!(
            matches!(next(&mut state), Decision::ChooseEffectTargets { legal_targets, .. } if legal_targets == [Target::Object(stone)])
        );
        for viewer in [actor, actor.opponent()] {
            assert_eq!(
                shroud_copy_menu(&state, viewer)["candidates"][0]["card_def"],
                id("Mind Stone")
            );
        }
        let mut forged = serde_json::to_value(&state).unwrap();
        let roster = forged["engine"]["linked_aura_operation_v1"]["pending_meddling_mage_v1"]
            ["choice"]["ShowAndTellV1"]["choice"]["candidates"]
            .as_array_mut()
            .unwrap();
        roster.push(
            serde_json::to_value(mtg_kernel::state::AbilitySourceContractV4::capture(
                &state, companion,
            ))
            .unwrap(),
        );
        assert_copy_restore_refused(
            forged,
            Action::ChooseEffectTarget(Target::Object(companion)),
        );
        let mut declined = state.clone();
        act(&mut declined, Action::FinishEffectSelection);
        settled(&mut declined);
        assert_eq!(zone(&declined, metamorph), Zone::Graveyard);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(stone)),
        );
        settled(&mut state);
        assert_eq!(state.objects.get(metamorph).card_def, id("Mind Stone"));
        assert_eq!(zone(&state, companion), Zone::Battlefield);
        let mana = state.players[actor.index()].mana_pool[ManaColor::C.pool_index()];
        act(&mut state, Action::ActivateManaAbility(metamorph));
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
            mana + 1
        );
        mtg_kernel::event::propose_and_commit(
            &mut state,
            mtg_kernel::event::ProposedEvent::zone_change(metamorph, Zone::Graveyard),
        );
        assert_eq!(zone(&state, metamorph), Zone::Graveyard);
        assert_eq!(
            state.objects.get(metamorph).card_def,
            id("Phyrexian Metamorph")
        );
    }
}

#[test]
fn copied_entry_requirements_are_answered_before_reanimate_moves_the_card() {
    for actor in [P0, P1] {
        for (name, step) in [
            ("Painter's Servant", "color"),
            ("Serra's Emissary", "card_type"),
            ("Pithing Needle", "name"),
            ("Phyrexian Revoker", "name"),
            ("Cavern of Souls", "creature_type"),
            ("Mox Diamond", "discard"),
        ] {
            let mut state = islands();
            state.active_player = actor;
            state.priority_player = actor;
            state.public_shroud_v1 = true;
            let donor = put(&mut state, actor, name, Zone::Battlefield);
            // Resolve the original permanent's own entry choice before the copy is offered.
            settle_first(&mut state);
            if name == "Cavern of Souls" {
                let coating = put(&mut state, actor, "Liquimetal Coating", Zone::Battlefield);
                activate(&mut state, coating, 0, &[Target::Object(donor)]);
                settled(&mut state);
            }
            let land = put(&mut state, actor, "Island", Zone::Hand);
            let metamorph = reanimate_copy(&mut state, actor);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(donor)),
            );
            assert!(
                matches!(next(&mut state), Decision::ChooseEffectOption { player, source, .. } if player == actor && source == metamorph)
            );
            assert_eq!(life(&state, actor), 20);
            assert_eq!(zone(&state, metamorph), Zone::Graveyard);
            assert_eq!(shroud_copy_menu(&state, actor)["step"], step);
            let opponent = shroud_copy_menu(&state, actor.opponent());
            assert!(opponent.get("options").is_none());
            if step == "discard" {
                let options = shroud_copy_menu(&state, actor)["options"]
                    .as_array()
                    .unwrap()
                    .clone();
                assert_eq!(options.len(), 2);
                assert_eq!(options[0], serde_json::json!({"Discard":null}));
                assert_eq!(options[1]["Discard"]["source"], serde_json::json!(land));
                let mut declined = state.clone();
                act(&mut declined, Action::ChooseEffectOption(0));
                settled(&mut declined);
                assert_eq!(zone(&declined, metamorph), Zone::Graveyard);
                assert_eq!(zone(&declined, land), Zone::Hand);
                assert_eq!(life(&declined, actor), 16);
            }
            let index = u16::from(step == "discard");
            act(&mut state, Action::ChooseEffectOption(index));
            settled(&mut state);
            assert_eq!(zone(&state, metamorph), Zone::Battlefield);
            assert_eq!(state.objects.get(metamorph).card_def, id(name));
            assert_eq!(life(&state, actor), 16);
            if step == "color" {
                let mut valid = state.clone();
                act(&mut valid, Action::Pass);
                let mut forged = state.clone();
                forged.objects.get_mut(metamorph).v4.chosen_color = Some(ManaColor::G);
                assert_copy_restore_refused(serde_json::to_value(forged).unwrap(), Action::Pass);
            }
            if step == "discard" {
                assert_eq!(zone(&state, land), Zone::Graveyard);
                let mut forged = serde_json::to_value(&state).unwrap();
                let history = forged["engine"]["event_history"].as_array_mut().unwrap();
                let completed = history
                    .iter_mut()
                    .find_map(|event| {
                        event
                            .get_mut("MeddlingMageEntryV1")?
                            .get_mut("journal")?
                            .get_mut("MetamorphCopied")
                    })
                    .unwrap();
                completed["copy"]["discarded_at"] = 0.into();
                assert_copy_restore_refused(forged, Action::Pass);
            }
        }
    }
}

fn assert_copy_restore_refused(value: serde_json::Value, action: Action) {
    let mut restored: GameState = serde_json::from_value(value).unwrap();
    let before = restored.state_hash();
    assert!(engine::step(&mut restored, action).is_err());
    assert_eq!(before, restored.state_hash());
}

#[test]
fn noncast_metamorph_copies_image_values_and_refuses_forged_frozen_values() {
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        state.public_shroud_v1 = true;
        let strix = put(
            &mut state,
            actor.opponent(),
            "Baleful Strix",
            Zone::Battlefield,
        );
        let image = put(&mut state, actor, "Phantasmal Image", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 1);
        cast(&mut state, image, &[]);
        settle_option(&mut state, 1);
        let metamorph = reanimate_copy(&mut state, actor);
        let snapshot = serde_json::to_value(&state).unwrap();
        for extra in [true, false] {
            let mut changed = snapshot.clone();
            let choice = &mut changed["engine"]["linked_aura_operation_v1"]
                ["pending_meddling_mage_v1"]["choice"]["ReanimateMetamorphV1"]["choice"];
            if extra {
                let candidates = choice["candidates"].as_array_mut().unwrap();
                candidates.push(candidates[0].clone());
            } else {
                choice["values"][image.0.to_string()]["mods"]["illusion_sacrifice"] = false.into();
            }
            assert_copy_restore_refused(changed, Action::FinishEffectSelection);
        }
        let hand = state.players[actor.index()].hand.len();
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(image)),
        );
        settled(&mut state);
        assert_eq!(state.players[actor.index()].hand.len(), hand + 1);
        assert_eq!(state.objects.get(metamorph).card_def, id("Baleful Strix"));
        assert!(engine::has_effective_subtype(
            &state,
            metamorph,
            mtg_kernel::card_def::Subtype::Illusion
        ));
        let original = serde_json::to_value(&state).unwrap();
        for wrong_source in [false, true] {
            let mut changed = original.clone();
            let journal = changed["engine"]["event_history"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find_map(|row| {
                    row.get_mut("MeddlingMageEntryV1")?
                        .get_mut("journal")?
                        .get_mut("MetamorphCopied")
                })
                .unwrap();
            if wrong_source {
                journal["copy"]["copied"]["source"] = serde_json::json!(strix);
            } else {
                journal["copy"]["values"]["mods"]["illusion_sacrifice"] = false.into();
            }
            assert_copy_restore_refused(changed, Action::Pass);
        }
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        cast(&mut state, bolt, &[Target::Object(metamorph)]);
        next(&mut state);
        assert_eq!(
            state.stack.len(),
            2,
            "Illusion sacrifice trigger above the bolt"
        );
        settled(&mut state);
        assert_eq!(zone(&state, metamorph), Zone::Graveyard);
        assert_eq!(
            state.objects.get(metamorph).card_def,
            id("Phyrexian Metamorph")
        );
    }
}

#[test]
fn noncast_metamorph_copies_token_blueprints_and_prototype_characteristics() {
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let goblin = token(&mut state, actor.opponent(), "Goblin Token");
        let metamorph = reanimate_copy(&mut state, actor);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(goblin)),
        );
        settled(&mut state);
        assert!(!state.objects.get(metamorph).v4.is_token);
        assert_eq!(state.objects.get(metamorph).card_def, id("Goblin Token"));
        mtg_kernel::event::propose_and_commit(
            &mut state,
            mtg_kernel::event::ProposedEvent::zone_change(metamorph, Zone::Graveyard),
        );
        assert_eq!(
            state.objects.get(metamorph).card_def,
            id("Phyrexian Metamorph")
        );

        let golem = put(&mut state, actor, "Boulderbranch Golem", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::G], 3);
        cast(&mut state, golem, &[]);
        if let Decision::ChooseCastMode { .. } = next(&mut state) {
            act(&mut state, Action::ChooseCastMode(CastMode::Alternative));
        }
        settled(&mut state);
        let second = reanimate_copy(&mut state, actor);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(golem)),
        );
        settled(&mut state);
        assert_eq!(power_toughness(&state, second), (3, 3));
        assert_eq!(engine::object_mana_value(&state, second), 4);
        assert_eq!(
            engine::object_color_mask(&state, second),
            engine::object_color_mask(&state, golem)
        );
    }
}

#[test]
fn noncast_saga_copy_gets_its_entry_lore_and_first_chapter() {
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let saga = put(
            &mut state,
            actor.opponent(),
            "Fable of the Mirror-Breaker",
            Zone::Battlefield,
        );
        let coating = put(&mut state, actor, "Liquimetal Coating", Zone::Battlefield);
        activate(&mut state, coating, 0, &[Target::Object(saga)]);
        settled(&mut state);
        let metamorph = reanimate_copy(&mut state, actor);
        act(&mut state, Action::ChooseEffectTarget(Target::Object(saga)));
        settled(&mut state);
        assert_eq!(state.objects.get(metamorph).counters.lore, 1);
        assert!(state.players[actor.index()]
            .battlefield
            .iter()
            .any(|id| state.objects.get(*id).v4.is_token
                && engine::has_effective_subtype(
                    &state,
                    *id,
                    mtg_kernel::card_def::Subtype::Goblin
                )));
    }
}

#[test]
fn noncast_copy_of_daybound_creature_keeps_the_chosen_face() {
    use mtg_kernel::state::DayNightV1;
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let donor = put(
            &mut state,
            actor.opponent(),
            "Outland Liberator",
            Zone::Battlefield,
        );
        let metamorph = reanimate_copy(&mut state, actor);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(donor)),
        );
        settled(&mut state);
        assert_eq!(state.day_night_v1, Some(DayNightV1::Day));
        pass_until(&mut state, |state| {
            state.day_night_v1 == Some(DayNightV1::Night) && state.step == Step::Main1
        });
        assert_eq!(state.objects.get(donor).v4.face_index, 1);
        assert_eq!(state.objects.get(metamorph).v4.face_index, 0);
        let mut restored: GameState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        act(&mut restored, Action::Pass);
    }
}

#[test]
fn noncast_metamorph_copies_transformed_faces_before_containment_priest() {
    use mtg_kernel::event::{propose_and_commit, ProposedEvent};
    for actor in [P0, P1] {
        for (name, creature_back) in [
            ("Fable of the Mirror-Breaker", true),
            ("Ojer Axonil, Deepest Might", false),
        ] {
            let mut state = islands();
            state.active_player = actor;
            state.priority_player = actor;
            let donor = put(&mut state, actor.opponent(), name, Zone::Battlefield);
            propose_and_commit(&mut state, ProposedEvent::transform_in_place(donor, 1));
            if !creature_back {
                let coating = put(&mut state, actor, "Liquimetal Coating", Zone::Battlefield);
                activate(&mut state, coating, 0, &[Target::Object(donor)]);
                settled(&mut state);
            }
            for priest in [false, true] {
                let mut world = state.clone();
                if priest {
                    put(&mut world, actor, "Containment Priest", Zone::Battlefield);
                }
                let metamorph = reanimate_copy(&mut world, actor);
                act(
                    &mut world,
                    Action::ChooseEffectTarget(Target::Object(donor)),
                );
                settled(&mut world);
                if priest && creature_back {
                    assert_eq!(zone(&world, metamorph), Zone::Exile);
                    assert!(world.engine.event_history.iter().all(|event| !matches!(
                        event,
                        mtg_kernel::event::CommittedEvent::MeddlingMageEntryV1 { .. }
                    )));
                } else {
                    assert_eq!(zone(&world, metamorph), Zone::Battlefield);
                    assert_eq!(world.objects.get(metamorph).v4.face_index, 1);
                    assert_eq!(
                        world.objects.get(metamorph).name,
                        world.objects.get(donor).name
                    );
                    assert_eq!(
                        engine::object_has_type(&world, metamorph, CardType::Creature),
                        creature_back
                    );
                    propose_and_commit(
                        &mut world,
                        ProposedEvent::zone_change(metamorph, Zone::Graveyard),
                    );
                }
                assert_eq!(
                    world.objects.get(metamorph).card_def,
                    id("Phyrexian Metamorph")
                );
                assert_eq!(world.objects.get(metamorph).v4.face_index, 0);
            }
        }
    }
}

#[test]
fn noncast_metamorph_copies_a_cast_metamorph_and_absorbing_man() {
    for actor in [P0, P1] {
        let other = actor.opponent();
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let stone = put(&mut state, actor, "Mind Stone", Zone::Battlefield);
        let first = put(&mut state, actor, "Phyrexian Metamorph", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 3);
        cast(&mut state, first, &[]);
        settle_choosing(&mut state, stone);
        let second = reanimate_copy(&mut state, actor);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(first)),
        );
        settled(&mut state);
        assert_eq!(state.objects.get(second).card_def, id("Mind Stone"));
        assert!(engine::object_has_type(&state, second, CardType::Artifact));
        let donor = put(&mut state, other, "Absorbing Man", Zone::Battlefield);
        let turn = state.turn;
        run_until(
            &mut state,
            |s| {
                s.active_player == other
                    && s.step == Step::Main1
                    && s.turn > turn
                    && s.stack.is_empty()
                    && s.engine.pending_triggers.is_empty()
            },
            |_, decision| match decision {
                Decision::ChooseTargets { legal_targets, .. }
                    if legal_targets.contains(&Target::Object(stone)) =>
                {
                    Some(Action::ChooseTarget(Target::Object(stone)))
                }
                _ => None,
            },
        );
        assert_eq!(state.objects.get(donor).card_def, id("Mind Stone"));
        pass_until(&mut state, |s| {
            s.active_player == actor && s.step == Step::Main1
        });
        let third = reanimate_copy(&mut state, actor);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(donor)),
        );
        settled(&mut state);
        assert_eq!(engine::effective_name(&state, third), "Absorbing Man");
        assert_eq!(power_toughness(&state, third), (4, 4));
        assert!(engine::object_has_type(&state, third, CardType::Creature));
        assert!(keyword(&state, third, Keywords::VIGILANCE));
    }
}

#[test]
fn reanimated_metamorph_chooses_painter_color_before_lethal_life_loss_both_seats() {
    for actor in [P0, P1] {
        let mut state = paid_meddling_against(actor, "Lava Dart", &["Island"; 20]);
        let painter = put(&mut state, actor, "Painter's Servant", Zone::Hand);
        pool(&mut state, actor, &[], 2);
        cast(&mut state, painter, &[]);
        settle_option(&mut state, 1);
        assert_eq!(zone(&state, painter), Zone::Battlefield);
        state.players[actor.index()].life = 4;
        let metamorph = put(
            &mut state,
            actor.opponent(),
            "Phyrexian Metamorph",
            Zone::Graveyard,
        );
        let reanimate = put(&mut state, actor, "Reanimate", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::B], 0);
        cast(&mut state, reanimate, &[Target::Object(metamorph)]);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { legal_targets, .. }) if legal_targets.contains(&Target::Object(painter)))
        );
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(painter)),
        );
        assert!(
            matches!(next(&mut state), Decision::ChooseEffectOption { player, source, option_count: 5 } if player == actor && source == metamorph)
        );
        assert_eq!(zone(&state, metamorph), Zone::Graveyard);
        assert_eq!(life(&state, actor), 4);
        let mut forged = serde_json::to_value(&state).unwrap();
        let pending = &mut forged["engine"]["linked_aura_operation_v1"]["pending_meddling_mage_v1"];
        let requirement = pending["choice"]["CopyEntryOptionV1"]["choice"].clone();
        pending["answers"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"CopyEntryOptionV1":{"choice":requirement,"index":0}}));
        assert_copy_restore_refused(forged, Action::ChooseEffectOption(2));
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        act(&mut restored, Action::ChooseEffectOption(2));
        assert_eq!(life(&restored, actor), 0);
        assert_eq!(zone(&restored, metamorph), Zone::Battlefield);
        assert_eq!(
            restored.objects.get(metamorph).v4.chosen_color,
            Some(ManaColor::B)
        );
    }
}

#[test]
fn meddling_mage_name_precedes_paid_metamorph_copy_entry_both_seats() {
    use mtg_kernel::card_def::CardType;
    use mtg_kernel::event::CommittedEvent;

    for actor in [P0, P1] {
        let mut state = paid_meddling_against(actor, "Lava Dart", &["Island"; 20]);
        let donor = state.players[actor.opponent().index()].battlefield[0];
        let metamorph = put(&mut state, actor, "Phyrexian Metamorph", Zone::Hand);
        let mut suppressed = state.clone();
        let dress = put(&mut suppressed, actor, "Dress Down", Zone::Hand);
        pool(&mut suppressed, actor, &[ManaColor::U, ManaColor::U], 4);
        cast(&mut suppressed, dress, &[]);
        settled(&mut suppressed);
        assert_eq!(zone(&suppressed, dress), Zone::Battlefield);
        cast(&mut suppressed, metamorph, &[]);
        settled(&mut suppressed);
        assert_eq!(zone(&suppressed, metamorph), Zone::Graveyard);
        assert_eq!(
            suppressed.objects.get(metamorph).card_def,
            id("Phyrexian Metamorph")
        );
        assert!(suppressed
            .eternal_v1
            .as_ref()
            .unwrap()
            .vintage
            .copies
            .is_empty());
        assert_eq!(
            suppressed.eternal_v1.as_ref().unwrap().s1b.mage_names.len(),
            1
        );

        pool(&mut state, actor, &[ManaColor::U], 3);
        cast(&mut state, metamorph, &[]);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, legal_targets, .. })
            if player == actor && legal_targets.contains(&Target::Object(donor)))
        );
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(donor)),
        );
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseMeddlingMageNameV1 { player, group: None, .. })
            if player == actor)
        );
        let physical_generation = state.objects.get(metamorph).zone_change_count;
        assert_eq!(zone(&state, metamorph), Zone::Stack);
        assert_eq!(
            state.objects.get(metamorph).card_def,
            id("Phyrexian Metamorph")
        );
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        assert!(!state.engine.event_history.iter().any(|event| matches!(event,
            CommittedEvent::ZoneChange { object, to: Zone::Battlefield, .. } if *object == metamorph)));

        let snapshot = serde_json::to_vec(&state).unwrap();
        state = serde_json::from_slice(&snapshot).unwrap();
        choose_mage_name(&mut state, actor, "Lightning Bolt");
        settled(&mut state);
        assert_eq!(zone(&state, metamorph), Zone::Battlefield);
        assert_eq!(state.objects.get(metamorph).card_def, id("Meddling Mage"));
        assert_eq!(
            state.objects.get(metamorph).zone_change_count,
            physical_generation + 1
        );
        assert!(engine::object_has_type(
            &state,
            metamorph,
            CardType::Artifact
        ));
        assert!(engine::object_has_type(
            &state,
            metamorph,
            CardType::Creature
        ));
        assert!(!state.objects.get(metamorph).v4.is_token);
        assert_eq!(engine::effective_power(&state, metamorph), 2);

        let mut forged = serde_json::to_value(&state).unwrap();
        let mut edits = 0;
        for event in forged["engine"]["event_history"].as_array_mut().unwrap() {
            let Some(journal) = event
                .get_mut("MeddlingMageEntryV1")
                .and_then(|row| row.get_mut("journal"))
            else {
                continue;
            };
            let copy = if journal.get("MetamorphCopied").is_some() {
                journal["MetamorphCopied"].get_mut("copy")
            } else {
                journal
                    .get_mut("Chosen")
                    .and_then(|row| row["choice"].get_mut("metamorph_copy"))
            };
            if let Some(copy) = copy {
                let original = copy["stack_item"].as_u64().unwrap();
                copy["stack_item"] = serde_json::json!(original + 1);
                edits += 1;
            }
        }
        assert_eq!(edits, 2);
        let mut forged: GameState = serde_json::from_value(forged).unwrap();
        assert!(engine::step(&mut forged, Action::Pass).is_err());

        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let disenchant = put(&mut state, actor, "Disenchant", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R, ManaColor::W], 1);
        assert!(!castable(&mut state, bolt));
        cast(&mut state, disenchant, &[Target::Object(metamorph)]);
        settled(&mut state);
        assert_eq!(zone(&state, metamorph), Zone::Graveyard);
        assert_eq!(
            state.objects.get(metamorph).card_def,
            id("Phyrexian Metamorph")
        );
        assert!(state.eternal_v1.as_ref().unwrap().vintage.copies.is_empty());
        assert!(castable(&mut state, bolt));
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
        let bilbo = put(
            &mut state,
            actor,
            "Bilbo, Thief in the Night",
            Zone::Battlefield,
        );
        let dragon = put(&mut state, actor, "Fang Dragon", Zone::Graveyard);
        pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
        act(&mut state, Action::DeclareAttackers(vec![bilbo]));
        pool(&mut state, actor, &[ManaColor::R], 0);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { legal_targets, .. })
                if legal_targets == [Target::Object(dragon)]
        ));
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(dragon)),
        );
        settled(&mut state);
        assert_eq!(zone(&state, dragon), Zone::Exile);
        assert!(state.objects.get(dragon).v4.on_adventure);
        assert!(state.players[actor.index()]
            .mana_pool
            .iter()
            .all(|&count| count == 0));
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
        let named = altered["engine"]["event_history"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find_map(|row| {
                row.get_mut("MeddlingMageEntryV1")?
                    .get_mut("journal")?
                    .get_mut("Named")
            })
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
    assert!(!state.engine.event_history.iter().any(|event| matches!(
        event,
        mtg_kernel::event::CommittedEvent::MeddlingMageEntryV1 { .. }
    )));
    let projection = mtg_kernel::rl::observe_policy_v5(
        &state,
        &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
        P1,
        0,
        0,
        0,
        1,
    )
    .unwrap();
    assert!(matches!(
        projection.projection.surface.engine_context.pending_effect.as_ref()
            .and_then(|pending| pending.choice.as_ref()),
        Some(mtg_kernel::rl::PendingEffectChoiceSemanticV4::Targets {
            selected_targets, legal_targets, ..
        }) if selected_targets.is_empty() && legal_targets.is_empty()
    ));
    act(&mut state, Action::ChooseEffectTarget(Target::Object(mage)));
    assert_eq!(zone(&state, mage), Zone::Hand);
    choose_mage_name(&mut state, P0, "Lightning Bolt");
    settled(&mut state);
    assert_eq!(zone(&state, mage), Zone::Battlefield);
    assert!(state.objects.get(vial).tapped);
}

fn vial_metamorph_opening(actor: PlayerId, hidden: &str) -> GameState {
    use mtg_kernel::event::{self, ProposedEvent};

    let deck = |opening: &[&str]| {
        let mut ids = vec![id("Island"); 60];
        for (slot, name) in ids.iter_mut().zip(opening) {
            *slot = id(name);
        }
        ids
    };
    let own = deck(&["Aether Vial", hidden, "Delver of Secrets"]);
    let other = deck(&["Meddling Mage", "Wall of Blossoms", "Echoing Truth"]);
    let (zero, one) = if actor == P0 {
        (&own, &other)
    } else {
        (&other, &own)
    };
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        zero,
        one,
        mtg_kernel::rl::card_name,
        196740,
        actor,
    );
    for _ in 0..7 {
        for player in [P0, P1] {
            event::propose_and_commit(&mut state, ProposedEvent::draw(player));
        }
    }
    mtg_kernel::eternal_opening_v1::enable_eternal_opening_v1(&mut state).unwrap();
    state
}

fn frozen_opening_refusal(state: &GameState, viewer: PlayerId) -> String {
    mtg_kernel::rl::observe_policy_v5(
        state,
        &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
        viewer,
        0,
        0,
        0,
        1,
    )
    .expect_err("public native opening profile refuses frozen V5")
    .to_string()
}

#[test]
fn paid_vial_metamorph_uses_private_hand_and_prospective_copy_both_seats() {
    use mtg_kernel::event::CommittedEvent;

    for actor in [P0, P1] {
        let opponent = actor.opponent();
        for (bounce_vial, hidden, extended) in [
            (false, "Phyrexian Metamorph", false),
            (false, "Phyrexian Metamorph", true),
            (true, "Phyrexian Metamorph", false),
            (false, "Lodestone Golem", false),
            (false, "Island", false),
        ] {
            let mut state = vial_metamorph_opening(actor, hidden);
            let refusal = frozen_opening_refusal(&state, opponent);
            assert!(refusal.contains("Eternal opening actions"), "{refusal}");
            let hand = |state: &GameState, player: PlayerId, name| {
                state.players[player.index()]
                    .hand
                    .iter()
                    .copied()
                    .find(|object| state.objects.get(*object).card_def == id(name))
                    .unwrap()
            };
            let vial = hand(&state, actor, "Aether Vial");
            let selected = hand(&state, actor, hidden);
            let wrong_value = hand(&state, actor, "Delver of Secrets");
            let generation = state.objects.get(selected).zone_change_count;
            let wall = hand(&state, opponent, "Wall of Blossoms");
            let mage = bounce_vial.then(|| hand(&state, opponent, "Meddling Mage"));
            let bounce = hand(&state, opponent, "Echoing Truth");
            for player in [actor, opponent] {
                assert!(matches!(next(&mut state), Decision::ChooseLondonMulligan {
                    player: chooser, .. } if chooser == player));
                act(&mut state, Action::ChooseLondonMulligan { mulligan: false });
            }
            for player in [actor, opponent] {
                assert!(
                    matches!(next(&mut state), Decision::ChooseEternalOpeningV1 {
                    player: chooser, legal_cards } if chooser == player && legal_cards.is_empty())
                );
                act(&mut state, Action::FinishEffectSelection);
            }
            pass_until(&mut state, |state| {
                state.active_player == actor && state.step == Step::Main1
            });
            pool(&mut state, actor, &[], 1);
            cast(&mut state, vial, &[]);
            settled(&mut state);

            // Keep an earlier real paid Tap on the same Vial incarnation.
            activate(&mut state, vial, 0, &[]);
            settled(&mut state);
            assert!(state.objects.get(vial).tapped);
            let prior_paid_at = state.engine.event_history.len() - 1;
            assert!(matches!(
                state.engine.event_history[prior_paid_at],
                CommittedEvent::MeddlingMageEntryV1 { .. }
            ));
            assert!(matches!(state.engine.event_history[prior_paid_at - 1],
                CommittedEvent::Tap { object } if object == vial));

            pass_until(&mut state, |state| {
                state.active_player == opponent && state.step == Step::Main1
            });
            pool(&mut state, opponent, &[ManaColor::G], 1);
            cast(&mut state, wall, &[]);
            settled(&mut state);
            if let Some(mage) = mage {
                pool(&mut state, opponent, &[ManaColor::W, ManaColor::U], 0);
                cast(&mut state, mage, &[]);
                assert!(settle(&mut state).is_some());
                choose_mage_name(&mut state, opponent, "Lava Dart");
                settled(&mut state);
            }
            run_until(
                &mut state,
                |state| {
                    state.active_player == actor
                        && state.step == Step::Main1
                        && state.objects.get(vial).counters.charge == 4
                },
                |state, decision| match decision {
                    Decision::ChooseEffectOption {
                        player,
                        option_count: 2,
                        ..
                    } if *player == actor && state.step == Step::Upkeep => {
                        Some(Action::ChooseEffectOption(1))
                    }
                    Decision::Discard { count, choices, .. } => {
                        let islands = choices
                            .iter()
                            .copied()
                            .filter(|&object| {
                                object != selected
                                    && state.objects.get(object).card_def == id("Island")
                            })
                            .take(*count as usize)
                            .collect::<Vec<_>>();
                        assert_eq!(islands.len(), *count as usize);
                        Some(Action::Discard(islands))
                    }
                    _ => None,
                },
            );
            assert_eq!(zone(&state, selected), Zone::Hand);
            assert_eq!(zone(&state, wrong_value), Zone::Hand);
            assert!(!state.objects.get(vial).tapped);
            let stone =
                extended.then(|| put(&mut state, opponent, "Mind Stone", Zone::Battlefield));
            let vial_generation = state.objects.get(vial).zone_change_count;
            activate(&mut state, vial, 0, &[]);
            assert!(matches!(next(&mut state),
                Decision::CastSpellOrPass { player, .. } if player == actor));
            assert!(state.objects.get(vial).tapped);
            assert_eq!(state.stack.last().unwrap().source, vial);
            assert_eq!(frozen_opening_refusal(&state, opponent), refusal);
            if hidden == "Phyrexian Metamorph" {
                let mut missing_marker = serde_json::to_value(&state).unwrap();
                let mut missing_receipt = missing_marker.clone();
                let row = missing_marker["stack"]
                    .as_array_mut()
                    .unwrap()
                    .last_mut()
                    .unwrap();
                let at = row["v4"]
                    .as_object_mut()
                    .unwrap()
                    .remove("vial_metamorph_paid_v1")
                    .unwrap()
                    .as_u64()
                    .unwrap() as usize;
                assert!(at > prior_paid_at);
                let history = missing_receipt["engine"]["event_history"]
                    .as_array_mut()
                    .unwrap();
                assert!(history[at].get("MeddlingMageEntryV1").is_some());
                history.remove(at);
                for missing in [missing_marker, missing_receipt] {
                    let mut missing: GameState = serde_json::from_value(missing).unwrap();
                    let before = missing.state_hash();
                    assert!(engine::step(&mut missing, Action::Pass).is_err());
                    assert_eq!(missing.state_hash(), before);
                }
            }
            if bounce_vial {
                assert_eq!(zone(&state, bounce), Zone::Hand);
                pool(&mut state, opponent, &[ManaColor::U], 1);
                act(&mut state, Action::Pass);
                cast(&mut state, bounce, &[Target::Object(vial)]);
                pass_until(&mut state, |state| zone(state, vial) == Zone::Hand);
                assert_eq!(state.stack.last().unwrap().source, vial);
                assert_eq!(
                    state.counter_lki_for(vial, vial_generation).unwrap().charge,
                    4
                );
            }
            let hand_choice = settle(&mut state);
            assert_eq!(frozen_opening_refusal(&state, opponent), refusal);
            if hidden == "Island" {
                assert!(hand_choice.is_none());
                assert_eq!(zone(&state, selected), Zone::Hand);
                continue;
            }
            let hand_choice = hand_choice.expect("Vial's paid four-charge hand choice");
            assert!(matches!(&hand_choice, Decision::ChooseEffectTargets {
                player, source, legal_targets, min_targets: 0, max_targets: 1,
                can_finish: true, ..
            } if *player == actor && *source == vial && legal_targets == &[Target::Object(selected)]));
            assert_eq!(
                zone(&state, vial),
                if bounce_vial {
                    Zone::Hand
                } else {
                    Zone::Battlefield
                }
            );
            if bounce_vial {
                assert_eq!(state.objects.get(vial).counters.charge, 0);
            }
            if hidden == "Lodestone Golem" {
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(selected)),
                );
                settled(&mut state);
                assert_eq!(zone(&state, selected), Zone::Battlefield);
                assert_eq!(frozen_opening_refusal(&state, opponent), refusal);
                continue;
            }
            let metamorph = selected;

            let snapshot = serde_json::to_vec(&state).unwrap();
            let mut declined: GameState = serde_json::from_slice(&snapshot).unwrap();
            assert_eq!(next(&mut declined), hand_choice);
            let before = declined.state_hash();
            assert!(engine::step(
                &mut declined,
                Action::ChooseEffectTarget(Target::Object(wrong_value))
            )
            .is_err());
            assert_eq!(declined.state_hash(), before);
            act(&mut declined, Action::FinishEffectSelection);
            settled(&mut declined);
            assert_eq!(zone(&declined, metamorph), Zone::Hand);
            assert_eq!(frozen_opening_refusal(&declined, opponent), refusal);

            let mut wrong_payment = serde_json::to_value(&state).unwrap();
            let origin = &mut wrong_payment["engine"]["pending_effect"]["choice"]["SelectTargets"]
                ["purpose"]["EternalV1"]["choice"]["kind"]["E8aV1"]["VialHandToBattlefieldV1"]
                ["paid_at"];
            assert!(origin.as_u64().unwrap() > prior_paid_at as u64);
            *origin = serde_json::json!(prior_paid_at);
            let mut wrong_payment: GameState = serde_json::from_value(wrong_payment).unwrap();
            assert!(engine::step(
                &mut wrong_payment,
                Action::ChooseEffectTarget(Target::Object(metamorph))
            )
            .is_err());

            state = serde_json::from_slice(&snapshot).unwrap();
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(metamorph)),
            );
            let copy_choice = next(&mut state);
            let other_copy = mage.unwrap_or(vial);
            for viewer in [actor, opponent] {
                let menu = mtg_kernel::rl::pending_copy_entry_v1(&state, viewer).unwrap();
                assert_eq!(menu.player, actor);
                assert!(menu.can_decline);
                let mut actual: Vec<_> = menu.candidates.iter().map(|c| c.source).collect();
                let mut expected = vec![wall, other_copy];
                expected.extend(stone);
                actual.sort();
                expected.sort();
                assert_eq!(actual, expected);
            }
            assert!(matches!(&copy_choice, Decision::ChooseEffectTargets {
                player, source, legal_targets, can_finish: true, ..
            } if *player == actor && *source == vial && legal_targets.len() == 2 + usize::from(extended)
                && legal_targets.contains(&Target::Object(wall))
                && legal_targets.contains(&Target::Object(other_copy))));
            assert_eq!(zone(&state, metamorph), Zone::Hand);
            assert_eq!(state.objects.get(metamorph).zone_change_count, generation);
            assert_eq!(frozen_opening_refusal(&state, opponent), refusal);

            let mut wrong_incarnation = serde_json::to_value(&state).unwrap();
            let physical = &mut wrong_incarnation["engine"]["linked_aura_operation_v1"]
                ["pending_meddling_mage_v1"]["choice"]["VialMetamorphV1"]["choice"]["physical"]
                ["zone_change_count"];
            *physical = serde_json::json!(generation + 1);
            let mut wrong_incarnation: GameState =
                serde_json::from_value(wrong_incarnation).unwrap();
            assert!(engine::step(&mut wrong_incarnation, Action::FinishEffectSelection).is_err());

            let copy_snapshot = serde_json::to_vec(&state).unwrap();
            for donor in [None, Some(wall), Some(other_copy)]
                .into_iter()
                .chain(stone.map(Some))
            {
                let mut restored: GameState = serde_json::from_slice(&copy_snapshot).unwrap();
                assert_eq!(next(&mut restored), copy_choice);
                act(
                    &mut restored,
                    donor.map_or(Action::FinishEffectSelection, |donor| {
                        Action::ChooseEffectTarget(Target::Object(donor))
                    }),
                );
                if donor.is_some() && donor == mage {
                    assert!(
                        matches!(next(&mut restored), Decision::ChooseMeddlingMageNameV1 {
                        player, group: None, .. } if player == actor)
                    );
                    assert_eq!(zone(&restored, metamorph), Zone::Hand);
                    assert_eq!(frozen_opening_refusal(&restored, opponent), refusal);
                    restored =
                        serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
                    choose_mage_name(&mut restored, actor, "Lightning Bolt");
                }
                let hand_before = restored.players[actor.index()].hand.len();
                settled(&mut restored);
                if let Some(donor) = donor {
                    let entered = restored.objects.get(metamorph);
                    assert_eq!(entered.zone, Zone::Battlefield);
                    assert_eq!(entered.card_def, restored.objects.get(donor).card_def);
                    assert_eq!(entered.controller, actor);
                    assert_eq!(entered.zone_change_count, generation + 1);
                    assert_eq!(entered.counters.charge, 0);
                    assert!(engine::object_has_type(
                        &restored,
                        metamorph,
                        CardType::Artifact
                    ));
                    if donor == wall {
                        // The actual Wall trigger draws after entry.
                        assert_eq!(restored.players[actor.index()].hand.len(), hand_before + 1);
                    }

                    let mut wrong_source = serde_json::to_value(&restored).unwrap();
                    let copy = wrong_source["engine"]["event_history"]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find_map(|event| {
                            event
                                .get_mut("MeddlingMageEntryV1")?
                                .get_mut("journal")?
                                .get_mut("MetamorphCopied")?
                                .get_mut("copy")
                        })
                        .unwrap();
                    assert!(copy.get("cast_at").is_none());
                    copy["vial"]["paid_at"] = serde_json::json!(prior_paid_at);
                    let mut wrong_source: GameState = serde_json::from_value(wrong_source).unwrap();
                    assert!(engine::step(&mut wrong_source, Action::Pass).is_err());
                } else {
                    assert_eq!(zone(&restored, metamorph), Zone::Graveyard);
                    assert_eq!(
                        restored.objects.get(metamorph).card_def,
                        id("Phyrexian Metamorph")
                    );
                }
                assert!(restored.engine.event_history.iter().any(|event| matches!(event,
                    CommittedEvent::ZoneChange { object, from: Zone::Hand, to: Zone::Battlefield, .. }
                    if *object == metamorph)));
            }
        }
    }
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
        &[
            ManaColor::W,
            ManaColor::W,
            ManaColor::U,
            ManaColor::U,
            ManaColor::U,
        ],
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
        mtg_kernel::eternal_sideboard_s1b_v1::MeddlingMageNameV1::Front(id("Lightning Bolt")),
    ));
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
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
                put(
                    &mut state,
                    actor.opponent(),
                    "Containment Priest",
                    Zone::Battlefield,
                );
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
fn phyrexian_metamorph_entering_as_engineered_explosives_has_sunburst_for_its_ability() {
    // 702.44a: a Metamorph cast for {3}{U} that enters as a copy of an
    // Engineered Explosives enters with one charge counter, and its
    // sacrifice destroys mana value 1, not the original's 0.
    let mut state = islands();
    let star = put(&mut state, P1, "Chromatic Star", Zone::Battlefield);
    let oracle = put(&mut state, P1, "Thassa's Oracle", Zone::Battlefield);
    let ee = put(&mut state, P0, "Engineered Explosives", Zone::Battlefield);
    let metamorph = put(&mut state, P0, "Phyrexian Metamorph", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, metamorph, &[]);
    settle_choosing(&mut state, ee);
    assert_eq!(zone(&state, metamorph), Zone::Battlefield);
    assert_eq!(
        state.objects.get(metamorph).card_def,
        id("Engineered Explosives")
    );
    assert_eq!(state.objects.get(metamorph).counters.charge, 1);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, metamorph, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, metamorph), Zone::Graveyard);
    assert_eq!(zone(&state, star), Zone::Graveyard, "mana value 1");
    assert_eq!(zone(&state, oracle), Zone::Battlefield, "mana value 2");
    assert_eq!(zone(&state, ee), Zone::Battlefield, "mana value 0");
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

#[test]
fn paid_show_and_tell_metamorph_owns_private_choices_and_simultaneous_entry() {
    use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};

    for active in [P0, P1] {
        let other = active.opponent();
        for (active_copies, other_copies) in [(true, false), (false, true), (true, true)] {
            let mut state = paid_meddling_against(active, "Lava Dart", &["Island"; 20]);
            let donor = state.players[other.index()].battlefield[0];
            let show = put(&mut state, active, "Show and Tell", Zone::Hand);
            let mut put_entrant = |player, copies| {
                put(
                    &mut state,
                    player,
                    if copies {
                        "Phyrexian Metamorph"
                    } else {
                        "Meddling Mage"
                    },
                    Zone::Hand,
                )
            };
            let first = put_entrant(active, active_copies);
            let second = put_entrant(other, other_copies);
            let secret = put(&mut state, active, "Street Wraith", Zone::Hand);
            let other_secret = put(&mut state, other, "Wall of Blossoms", Zone::Hand);
            pool(&mut state, active, &[ManaColor::U], 2);
            cast(&mut state, show, &[]);

            assert!(matches!(settle(&mut state),
                Some(Decision::ChooseEffectTargets { player, source, legal_targets, .. })
                if player == active && source == show
                    && legal_targets == [Target::Object(first), Target::Object(secret)]));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(first)),
            );
            let second_choice = next(&mut state);
            assert!(matches!(&second_choice,
                Decision::ChooseEffectTargets { player, source, legal_targets, .. }
                if *player == other && *source == show
                    && *legal_targets == [Target::Object(second), Target::Object(other_secret)]));
            assert!(state.known_hand_cards(other, active).is_empty());
            assert!(state.known_hand_cards(active, other).is_empty());
            assert_eq!(zone(&state, first), Zone::Hand);
            assert_eq!(zone(&state, second), Zone::Hand);
            let snapshot = serde_json::to_vec(&state).unwrap();
            state = serde_json::from_slice(&snapshot).unwrap();
            assert_eq!(next(&mut state), second_choice);

            for substitute_valid_answer in [false, true] {
                let mut rebound = serde_json::to_value(&state).unwrap();
                let answer = &mut rebound["engine"]["linked_aura_operation_v1"]
                    ["pending_meddling_mage_v1"]["answers"][0]["ShowAndTellV1"];
                if substitute_valid_answer {
                    answer["selected"] = answer["choice"]["candidates"][1].clone();
                } else {
                    answer["choice"]["player"] = serde_json::to_value(other).unwrap();
                }
                let mut rebound: GameState = serde_json::from_value(rebound).unwrap();
                let before = rebound.state_hash();
                assert!(engine::step(&mut rebound, Action::FinishEffectSelection).is_err());
                assert_eq!(rebound.state_hash(), before);
            }

            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(second)),
            );
            for (player, entrant, copies) in [
                (active, first, active_copies),
                (other, second, other_copies),
            ] {
                if !copies {
                    continue;
                }
                let choice = next(&mut state);
                assert!(matches!(&choice,
                    Decision::ChooseEffectTargets { player: chooser, source, legal_targets,
                        min_targets: 0, max_targets: 1, can_finish: true, .. }
                    if *chooser == player && *source == entrant
                        && *legal_targets == [Target::Object(donor)]));
                assert_eq!(
                    state
                        .known_hand_cards(other, active)
                        .iter()
                        .map(|entry| entry.object)
                        .collect::<Vec<_>>(),
                    [first]
                );
                assert_eq!(
                    state
                        .known_hand_cards(active, other)
                        .iter()
                        .map(|entry| entry.object)
                        .collect::<Vec<_>>(),
                    [second]
                );
                assert_eq!(zone(&state, first), Zone::Hand);
                assert_eq!(zone(&state, second), Zone::Hand);
                assert_eq!(zone(&state, show), Zone::Stack);
                let before = state.state_hash();
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(entrant, Zone::Battlefield),
                );
                assert_eq!(
                    state.state_hash(),
                    before,
                    "raw entry cannot take the waiting copy"
                );
                let sibling = if player == active { second } else { first };
                assert!(engine::step(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(sibling))
                )
                .is_err());
                assert_eq!(
                    state.state_hash(),
                    before,
                    "another entrant is outside the copy menu"
                );
                state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                assert_eq!(next(&mut state), choice);
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(donor)),
                );
            }

            for (ordinal, (player, name)) in [(active, "Lightning Bolt"), (other, "Echoing Truth")]
                .into_iter()
                .enumerate()
            {
                assert!(matches!(next(&mut state),
                    Decision::ChooseMeddlingMageNameV1 { player: chooser, entry_ordinal, group: None, .. }
                    if chooser == player && entry_ordinal as usize == ordinal));
                assert_eq!(zone(&state, first), Zone::Hand);
                assert_eq!(zone(&state, second), Zone::Hand);
                state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                choose_mage_name(&mut state, player, name);
            }
            settled(&mut state);
            let first_entry = state
                .engine
                .event_history
                .iter()
                .position(|event| {
                    matches!(event,
                CommittedEvent::ZoneChange { object, from: Zone::Hand, to: Zone::Battlefield, .. }
                if *object == first)
                })
                .unwrap();
            let second_entry = state
                .engine
                .event_history
                .iter()
                .position(|event| {
                    matches!(event,
                CommittedEvent::ZoneChange { object, from: Zone::Hand, to: Zone::Battlefield, .. }
                if *object == second)
                })
                .unwrap();
            let departure = state
                .engine
                .event_history
                .iter()
                .position(|event| {
                    matches!(event,
                CommittedEvent::ZoneChange { object, from: Zone::Stack, to: Zone::Graveyard, .. }
                if *object == show)
                })
                .unwrap();
            assert!(first_entry < second_entry && second_entry < departure);
            assert_eq!(state.eternal_v1.as_ref().unwrap().s1b.mage_names.len(), 3);
            for (player, entrant, copied) in [
                (active, first, active_copies),
                (other, second, other_copies),
            ] {
                assert_eq!(zone(&state, entrant), Zone::Battlefield);
                assert_eq!(state.objects.get(entrant).controller, player);
                assert_eq!(state.objects.get(entrant).card_def, id("Meddling Mage"));
                assert_eq!(
                    engine::object_has_type(&state, entrant, CardType::Artifact),
                    copied
                );
            }
            let denied = put(&mut state, active, "Lightning Bolt", Zone::Hand);
            pool(&mut state, active, &[ManaColor::R], 0);
            assert!(!castable(&mut state, denied));

            let mut rebound = serde_json::to_value(&state).unwrap();
            let copy = rebound["engine"]["event_history"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find_map(|event| {
                    event
                        .get_mut("MeddlingMageEntryV1")?
                        .get_mut("journal")?
                        .get_mut("MetamorphCopied")?
                        .get_mut("copy")
                })
                .unwrap();
            copy["show_and_tell"]["producer"]["zone_change_count"] = serde_json::json!(999);
            let mut rebound: GameState = serde_json::from_value(rebound).unwrap();
            let before = rebound.state_hash();
            assert!(engine::step(&mut rebound, Action::Pass).is_err());
            assert_eq!(rebound.state_hash(), before);

            state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            let copied = if active_copies { first } else { second };
            // Stern Dismissal targets a creature an opponent controls.
            let caster = state.objects.get(copied).controller.opponent();
            if state.priority_player != caster {
                act(&mut state, Action::Pass);
            }
            let bounce = put(&mut state, caster, "Stern Dismissal", Zone::Hand);
            pool(&mut state, caster, &[ManaColor::U], 0);
            cast(&mut state, bounce, &[Target::Object(copied)]);
            settled(&mut state);
            assert_eq!(zone(&state, copied), Zone::Hand);
            assert_eq!(
                state.objects.get(copied).card_def,
                id("Phyrexian Metamorph")
            );
        }
    }
}

#[test]
fn paid_show_and_tell_metamorph_priest_replacement_uses_pre_entry_battlefield() {
    use mtg_kernel::event::CommittedEvent;

    for metamorph_owner in [P0, P1] {
        for priest_already_present in [false, true] {
            let mut state = paid_meddling_against(P0, "Lava Dart", &["Island"; 20]);
            let donor = state.players[P1.index()].battlefield[0];
            let priest_owner = metamorph_owner.opponent();
            let priest = put(&mut state, priest_owner, "Containment Priest", Zone::Hand);
            let companion = if priest_already_present {
                if priest_owner == P1 {
                    state.active_player = P1;
                    state.priority_player = P1;
                }
                pool(&mut state, priest_owner, &[ManaColor::W], 1);
                cast(&mut state, priest, &[]);
                settled(&mut state);
                state.active_player = P0;
                state.priority_player = P0;
                state.engine.priority_passes = [false; 2];
                put(&mut state, priest_owner, "Island", Zone::Hand)
            } else {
                priest
            };
            let metamorph = put(
                &mut state,
                metamorph_owner,
                "Phyrexian Metamorph",
                Zone::Hand,
            );
            let show = put(&mut state, P0, "Show and Tell", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U], 2);
            cast(&mut state, show, &[]);
            let history_start = state.engine.event_history.len();
            for player in [P0, P1] {
                let card = if player == metamorph_owner {
                    metamorph
                } else {
                    companion
                };
                assert!(matches!(settle(&mut state),
                    Some(Decision::ChooseEffectTargets { player: chooser, source, .. })
                    if chooser == player && source == show));
                act(&mut state, Action::ChooseEffectTarget(Target::Object(card)));
            }
            assert!(matches!(next(&mut state),
                Decision::ChooseEffectTargets { player, source, legal_targets, .. }
                if player == metamorph_owner && source == metamorph
                    && legal_targets.contains(&Target::Object(donor))
                    && legal_targets.contains(&Target::Object(priest)) == priest_already_present));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(donor)),
            );
            if !priest_already_present {
                assert_eq!(zone(&state, priest), Zone::Hand);
                assert_eq!(zone(&state, metamorph), Zone::Hand);
                choose_mage_name(&mut state, metamorph_owner, "Lightning Bolt");
            }
            settled(&mut state);
            assert_eq!(zone(&state, priest), Zone::Battlefield);
            assert_eq!(zone(&state, companion), Zone::Battlefield);
            if priest_already_present {
                assert_eq!(zone(&state, metamorph), Zone::Exile);
                assert_eq!(
                    state.objects.get(metamorph).card_def,
                    id("Phyrexian Metamorph")
                );
                assert!(!state.engine.event_history[history_start..]
                    .iter()
                    .any(|event| { matches!(event, CommittedEvent::MeddlingMageEntryV1 { .. }) }));
                assert_eq!(state.eternal_v1.as_ref().unwrap().s1b.mage_names.len(), 1);
            } else {
                assert_eq!(zone(&state, metamorph), Zone::Battlefield);
                assert_eq!(state.objects.get(metamorph).card_def, id("Meddling Mage"));
                assert_eq!(state.eternal_v1.as_ref().unwrap().s1b.mage_names.len(), 2);
            }
        }
    }
}

#[test]
fn noncast_metamorph_copy_of_delver_and_temple_of_power_cannot_transform_in_place() {
    use mtg_kernel::event::{propose_and_commit, ProposedEvent};

    // CR 701.28a: a single-faced physical card copying Delver of Secrets cannot
    // transform in place via EffectOp::TransformSourceInPlace, and the live
    // game + snapshot restore stay valid.
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let donor = put(
            &mut state,
            actor.opponent(),
            "Delver of Secrets",
            Zone::Battlefield,
        );
        let metamorph = reanimate_copy(&mut state, actor);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(donor)),
        );
        settled(&mut state);
        assert_eq!(
            state.objects.get(metamorph).card_def,
            id("Delver of Secrets")
        );
        assert_eq!(state.objects.get(metamorph).v4.face_index, 0);

        // Place an instant on top of actor's library and advance to actor's next upkeep.
        put(&mut state, actor, "Lightning Bolt", Zone::Library);
        let turn = state.turn;
        let mut revealed = false;
        run_until(
            &mut state,
            |s| {
                s.active_player == actor
                    && s.turn > turn
                    && s.step == Step::Main1
                    && s.stack.is_empty()
                    && s.engine.pending_triggers.is_empty()
            },
            |_, decision| match decision {
                Decision::ChooseEffectBoolean { source, .. } if *source == metamorph => {
                    revealed = true;
                    Some(Action::ChooseEffectBoolean(true))
                }
                _ => None,
            },
        );
        assert!(
            revealed,
            "copied Delver upkeep trigger must resolve and reveal"
        );
        assert!(state.engine.halted.is_none());
        assert_eq!(state.objects.get(metamorph).v4.face_index, 0);
        let mut restored: GameState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        act(&mut restored, Action::Pass);
    }

    // CR 701.28a: a single-faced physical card copying Temple of Power
    // (face_index == 1) cannot transform in place via transform_resolving_source.
    for actor in [P0, P1] {
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        let donor = put(
            &mut state,
            actor.opponent(),
            "Ojer Axonil, Deepest Might",
            Zone::Battlefield,
        );
        propose_and_commit(&mut state, ProposedEvent::transform_in_place(donor, 1));
        let coating = put(&mut state, actor, "Liquimetal Coating", Zone::Battlefield);
        activate(&mut state, coating, 0, &[Target::Object(donor)]);
        settled(&mut state);

        let metamorph = reanimate_copy(&mut state, actor);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(donor)),
        );
        settled(&mut state);
        assert_eq!(state.objects.get(metamorph).v4.face_index, 1);
        assert_eq!(state.objects.get(metamorph).name, "Temple of Power");

        // Deal 6 noncombat red damage to opponent so Temple of Power's transform
        // ability is activatable.
        for _ in 0..2 {
            let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::R], 0);
            cast(&mut state, bolt, &[Target::Player(actor.opponent())]);
            settled(&mut state);
        }
        pool(&mut state, actor, &[ManaColor::R], 2);
        activate(&mut state, metamorph, 0, &[]);
        settled(&mut state);
        assert!(state.engine.halted.is_none());
        assert_eq!(state.objects.get(metamorph).v4.face_index, 1);
        assert_eq!(state.objects.get(metamorph).name, "Temple of Power");
        let mut restored: GameState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        act(&mut restored, Action::Pass);
    }
}

#[test]
fn noncast_mox_diamond_copy_refuses_discard_bypass_and_unrelated_hand_moves() {
    let mut state = islands();
    state.active_player = P0;
    state.priority_player = P0;
    let mox = put(&mut state, P0, "Mox Diamond", Zone::Battlefield);
    settle_first(&mut state);
    let land = put(&mut state, P0, "Island", Zone::Hand);
    let metamorph = reanimate_copy(&mut state, P0);
    act(&mut state, Action::ChooseEffectTarget(Target::Object(mox)));
    act(&mut state, Action::ChooseEffectOption(1));
    settled(&mut state);
    assert_eq!(zone(&state, metamorph), Zone::Battlefield);
    assert_eq!(zone(&state, land), Zone::Graveyard);

    // Forgery A: coherently bypass Mox Diamond's land discard by removing the
    // discard event and its CopyDiscardPaid receipt, restoring the land to
    // hand, shifting prepared_at and entry_history_index down by 2, and
    // clearing entry_answer + discarded_at in both NoncastCopyPrepared and
    // MetamorphCopied.
    {
        let mut state_a = state.clone();
        state_a.players[0].graveyard.retain(|&id| id != land);
        state_a.players[0].hand.push(land);
        let obj = state_a.objects.get_mut(land);
        obj.zone = Zone::Hand;
        obj.zone_change_count -= 1;

        let mut forged = serde_json::to_value(&state_a).unwrap();
        let history = forged["engine"]["event_history"].as_array_mut().unwrap();
        let discard_idx = history
            .iter()
            .position(|ev| {
                ev.get("ZoneChange").is_some_and(|z| {
                    z["object"] == serde_json::json!(land)
                        && z["from"] == "Hand"
                        && z["to"] == "Graveyard"
                })
            })
            .unwrap();
        assert!(
            history[discard_idx + 1]
                .get("CopyEntryProvenanceV1")
                .and_then(|e| e.get("journal"))
                .and_then(|j| j.get("CopyDiscardPaid"))
                .is_some(),
            "expected CopyDiscardPaid immediately after Mox Diamond land discard"
        );
        history.remove(discard_idx + 1);
        history.remove(discard_idx);
        for ev in history.iter_mut() {
            if let Some(journal) = ev
                .get_mut("MeddlingMageEntryV1")
                .and_then(|e| e.get_mut("journal"))
            {
                if let Some(prep) = journal.get_mut("NoncastCopyPrepared") {
                    let old_p = prep["copy"]["prepared_at"].as_u64().unwrap();
                    prep["copy"]["prepared_at"] = serde_json::json!(old_p - 2);
                    prep["copy"].as_object_mut().unwrap().remove("entry_answer");
                    prep["copy"].as_object_mut().unwrap().remove("discarded_at");
                }
                if let Some(copied) = journal.get_mut("MetamorphCopied") {
                    let old_e = copied["entry_history_index"].as_u64().unwrap();
                    copied["entry_history_index"] = serde_json::json!(old_e - 2);
                    let old_p = copied["copy"]["prepared_at"].as_u64().unwrap();
                    copied["copy"]["prepared_at"] = serde_json::json!(old_p - 2);
                    copied["copy"]
                        .as_object_mut()
                        .unwrap()
                        .remove("entry_answer");
                    copied["copy"]
                        .as_object_mut()
                        .unwrap()
                        .remove("discarded_at");
                }
            }
        }
        assert_copy_restore_refused(forged, Action::Pass);
    }

    // Forgery B: unrelated Hand -> Library move (Brainstorm while Reanimate is
    // on the stack) must be refused as discarded_at.
    {
        let mut bs = islands();
        bs.active_player = P0;
        bs.priority_player = P0;
        let mox = put(&mut bs, P0, "Mox Diamond", Zone::Battlefield);
        settle_first(&mut bs);
        let l1 = put(&mut bs, P0, "Island", Zone::Hand);
        let _l2 = put(&mut bs, P0, "Mountain", Zone::Hand);
        let l1_contract = mtg_kernel::state::AbilitySourceContractV4::capture(&bs, l1);
        let metamorph = put(&mut bs, P0, "Phyrexian Metamorph", Zone::Graveyard);
        let reanimate = put(&mut bs, P0, "Reanimate", Zone::Hand);
        let brainstorm = put(&mut bs, P0, "Brainstorm", Zone::Hand);
        pool(&mut bs, P0, &[ManaColor::B, ManaColor::U], 0);
        cast(&mut bs, reanimate, &[Target::Object(metamorph)]);
        next(&mut bs);
        cast(&mut bs, brainstorm, &[]);
        // Resolve Brainstorm, putting l1 from Hand onto Library.
        let mut put_l1 = false;
        run_until(
            &mut bs,
            |s| s.stack.len() == 1 && s.engine.pending_effect.is_none(),
            |_, decision| match decision {
                Decision::ChooseEffectTargets { legal_targets, .. } => {
                    if !put_l1 && legal_targets.contains(&Target::Object(l1)) {
                        put_l1 = true;
                        Some(Action::ChooseEffectTarget(Target::Object(l1)))
                    } else {
                        Some(Action::ChooseEffectTarget(legal_targets[0]))
                    }
                }
                _ => None,
            },
        );
        assert!(put_l1);
        let brainstorm_move = bs
            .engine
            .event_history
            .iter()
            .rposition(|ev| {
                matches!(
                    ev,
                    mtg_kernel::event::CommittedEvent::ZoneChange {
                        object,
                        from: Zone::Hand,
                        to: Zone::Library,
                        ..
                    } if *object == l1
                )
            })
            .unwrap();
        // Now let Reanimate resolve, copying Mox Diamond and discarding a land.
        assert!(matches!(
            settle(&mut bs),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        act(&mut bs, Action::ChooseEffectTarget(Target::Object(mox)));
        act(&mut bs, Action::ChooseEffectOption(1));
        settled(&mut bs);
        assert_eq!(zone(&bs, metamorph), Zone::Battlefield);

        let mut forged = serde_json::to_value(&bs).unwrap();
        for ev in forged["engine"]["event_history"].as_array_mut().unwrap() {
            if let Some(journal) = ev
                .get_mut("MeddlingMageEntryV1")
                .and_then(|e| e.get_mut("journal"))
            {
                for key in ["NoncastCopyPrepared", "MetamorphCopied"] {
                    if let Some(entry) = journal.get_mut(key) {
                        entry["copy"]["entry_answer"] = serde_json::json!({"Discard": l1_contract});
                        entry["copy"]["discarded_at"] = serde_json::json!(brainstorm_move);
                    }
                }
            }
        }
        assert_copy_restore_refused(forged, Action::Pass);
    }

    // Forgery C (Test isolation): reusing the same discarded_at across two
    // copies in the SAME Show and Tell window with NO second land discard
    // ZoneChange in history (P0 copies Mox Diamond discarding l0; P1 copies
    // Mind Stone, then P1 is coherently forged to Mox Diamond reusing l0).
    // Also check that a genuine dual-Mox Show and Tell snapshot restores cleanly.
    {
        let mut two = islands();
        two.active_player = P0;
        two.priority_player = P0;
        let mox = put(&mut two, P0, "Mox Diamond", Zone::Battlefield);
        settle_first(&mut two);
        let _l0 = put(&mut two, P0, "Island", Zone::Hand);
        let _l1 = put(&mut two, P1, "Mountain", Zone::Hand);
        let p0_mm = put(&mut two, P0, "Phyrexian Metamorph", Zone::Hand);
        let p1_mm = put(&mut two, P1, "Phyrexian Metamorph", Zone::Hand);
        let show = put(&mut two, P0, "Show and Tell", Zone::Hand);
        pool(&mut two, P0, &[ManaColor::U], 2);
        cast(&mut two, show, &[]);
        assert!(settle(&mut two).is_some());
        act(&mut two, Action::ChooseEffectTarget(Target::Object(p0_mm)));
        act(&mut two, Action::ChooseEffectTarget(Target::Object(p1_mm)));
        act(&mut two, Action::ChooseEffectTarget(Target::Object(mox)));
        act(&mut two, Action::ChooseEffectOption(1));
        act(&mut two, Action::ChooseEffectTarget(Target::Object(mox)));
        act(&mut two, Action::ChooseEffectOption(1));
        settled(&mut two);
        assert_eq!(zone(&two, p0_mm), Zone::Battlefield);
        assert_eq!(zone(&two, p1_mm), Zone::Battlefield);
        // Genuine dual-Mox Show and Tell snapshot must restore cleanly.
        let mut restored: GameState =
            serde_json::from_value(serde_json::to_value(&two).unwrap()).unwrap();
        act(&mut restored, Action::Pass);

        // Isolated duplicate-discard forgery: P0 copies Mox Diamond (discarding
        // l0) and P1 copies Mind Stone (zero second land discard in history),
        // then P1's copy is rewritten to Mox Diamond reusing l0's discarded_at.
        let mut dup = islands();
        dup.active_player = P0;
        dup.priority_player = P0;
        let mox = put(&mut dup, P0, "Mox Diamond", Zone::Battlefield);
        settle_first(&mut dup);
        let stone = put(&mut dup, P0, "Mind Stone", Zone::Battlefield);
        let _l0 = put(&mut dup, P0, "Island", Zone::Hand);
        let p0_mm = put(&mut dup, P0, "Phyrexian Metamorph", Zone::Hand);
        let p1_mm = put(&mut dup, P1, "Phyrexian Metamorph", Zone::Hand);
        let show = put(&mut dup, P0, "Show and Tell", Zone::Hand);
        pool(&mut dup, P0, &[ManaColor::U], 2);
        cast(&mut dup, show, &[]);
        assert!(settle(&mut dup).is_some());
        act(&mut dup, Action::ChooseEffectTarget(Target::Object(p0_mm)));
        act(&mut dup, Action::ChooseEffectTarget(Target::Object(p1_mm)));
        act(&mut dup, Action::ChooseEffectTarget(Target::Object(mox)));
        act(&mut dup, Action::ChooseEffectOption(1));
        act(&mut dup, Action::ChooseEffectTarget(Target::Object(stone)));
        settled(&mut dup);
        assert_eq!(zone(&dup, p0_mm), Zone::Battlefield);
        assert_eq!(zone(&dup, p1_mm), Zone::Battlefield);

        let mox_contract = mtg_kernel::state::AbilitySourceContractV4::capture(&dup, mox);
        let mox_id = id("Mox Diamond");
        let stone_id = id("Mind Stone");
        dup.objects.get_mut(p1_mm).card_def = mox_id;
        dup.objects.get_mut(p1_mm).name = "Mox Diamond".to_string();
        let mut forged = serde_json::to_value(&dup).unwrap();
        if let Some(defs) = forged["standard_v1"]["copied_card_defs"].as_array_mut() {
            for row in defs.iter_mut() {
                if row[0] == serde_json::json!(p1_mm) && row[2] == serde_json::json!(stone_id) {
                    row[2] = serde_json::json!(mox_id);
                }
            }
        }
        let history = forged["engine"]["event_history"].as_array_mut().unwrap();
        let mut first_discard = None;
        let mut first_answer = None;
        let mut seen_chosen = 0;
        let mut seen_copied = 0;
        for ev in history.iter_mut() {
            let Some(journal) = ev
                .get_mut("MeddlingMageEntryV1")
                .and_then(|e| e.get_mut("journal"))
            else {
                continue;
            };
            if let Some(chosen) = journal.get_mut("ShowAndTellMetamorphChosen") {
                seen_chosen += 1;
                if seen_chosen == 1 {
                    first_discard = Some(chosen["copy"]["discarded_at"].clone());
                    first_answer = Some(chosen["copy"]["entry_answer"].clone());
                } else {
                    chosen["copy"]["copied"] = serde_json::to_value(mox_contract).unwrap();
                    chosen["copy"]["discarded_at"] = first_discard.clone().unwrap();
                    chosen["copy"]["entry_answer"] = first_answer.clone().unwrap();
                }
            }
            if let Some(copied) = journal.get_mut("MetamorphCopied") {
                seen_copied += 1;
                if seen_copied == 2 {
                    copied["entrant"]["card_def"] = serde_json::json!(mox_id);
                    copied["copy"]["copied"] = serde_json::to_value(mox_contract).unwrap();
                    copied["copy"]["discarded_at"] = first_discard.clone().unwrap();
                    copied["copy"]["entry_answer"] = first_answer.clone().unwrap();
                }
            }
        }
        assert_eq!(seen_chosen, 2);
        assert_eq!(seen_copied, 2);
        assert_copy_restore_refused(forged, Action::Pass);
    }

    // Forgery D1 (Test isolation): Putrid Imp activated-ability Hand ->
    // Graveyard land discard while Reanimate is on the stack, followed by
    // Reanimate copying Mind Stone (ZERO intervening events between imp_discard
    // and prepared_at, and NO genuine second-land Mox payment in history).
    // Coherently forging Mind Stone -> Mox Diamond pointing at imp_discard
    // isolates CopyDiscardPaid provenance without any blacklisted ZoneChange.
    {
        let mut imp_state = islands();
        imp_state.active_player = P0;
        imp_state.priority_player = P0;
        let mox = put(&mut imp_state, P0, "Mox Diamond", Zone::Battlefield);
        settle_first(&mut imp_state);
        let stone = put(&mut imp_state, P0, "Mind Stone", Zone::Battlefield);
        let imp = put(&mut imp_state, P0, "Putrid Imp", Zone::Battlefield);
        let l1 = put(&mut imp_state, P0, "Island", Zone::Hand);
        let l1_contract = mtg_kernel::state::AbilitySourceContractV4::capture(&imp_state, l1);
        let metamorph = put(&mut imp_state, P0, "Phyrexian Metamorph", Zone::Graveyard);
        let reanimate = put(&mut imp_state, P0, "Reanimate", Zone::Hand);
        pool(&mut imp_state, P0, &[ManaColor::B], 0);
        cast(&mut imp_state, reanimate, &[Target::Object(metamorph)]);
        next(&mut imp_state);

        // Activate Putrid Imp while Reanimate is on the stack, discarding l1.
        activate(&mut imp_state, imp, 0, &[]);
        run_until(
            &mut imp_state,
            |s| {
                s.stack.len() == 1
                    && s.engine.pending_discard.is_none()
                    && zone(s, l1) == Zone::Graveyard
            },
            |_, decision| match decision {
                Decision::Discard { .. } => Some(Action::Discard(vec![l1])),
                _ => None,
            },
        );
        assert_eq!(zone(&imp_state, l1), Zone::Graveyard);
        let imp_discard = imp_state
            .engine
            .event_history
            .iter()
            .rposition(|ev| {
                matches!(
                    ev,
                    mtg_kernel::event::CommittedEvent::ZoneChange {
                        object,
                        from: Zone::Hand,
                        to: Zone::Graveyard,
                        ..
                    } if *object == l1
                )
            })
            .unwrap();

        // Let Reanimate resolve copying Mind Stone so NO second land discard
        // exists in event_history, placing NoncastCopyPrepared directly at
        // imp_discard + 1.
        assert!(matches!(
            settle(&mut imp_state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        act(
            &mut imp_state,
            Action::ChooseEffectTarget(Target::Object(stone)),
        );
        settled(&mut imp_state);
        assert_eq!(zone(&imp_state, metamorph), Zone::Battlefield);
        assert!(matches!(
            imp_state.engine.event_history.get(imp_discard + 1),
            Some(mtg_kernel::event::CommittedEvent::MeddlingMageEntryV1 { .. })
        ));

        let mox_contract = mtg_kernel::state::AbilitySourceContractV4::capture(&imp_state, mox);
        let mox_id = id("Mox Diamond");
        let stone_id = id("Mind Stone");
        imp_state.objects.get_mut(metamorph).card_def = mox_id;
        imp_state.objects.get_mut(metamorph).name = "Mox Diamond".to_string();
        let mut forged = serde_json::to_value(&imp_state).unwrap();
        if let Some(defs) = forged["standard_v1"]["copied_card_defs"].as_array_mut() {
            for row in defs.iter_mut() {
                if row[0] == serde_json::json!(metamorph) && row[2] == serde_json::json!(stone_id) {
                    row[2] = serde_json::json!(mox_id);
                }
            }
        }
        for ev in forged["engine"]["event_history"].as_array_mut().unwrap() {
            if let Some(journal) = ev
                .get_mut("MeddlingMageEntryV1")
                .and_then(|e| e.get_mut("journal"))
            {
                if let Some(prep) = journal.get_mut("NoncastCopyPrepared") {
                    prep["copy"]["copied"] = serde_json::to_value(mox_contract).unwrap();
                    prep["copy"]["entry_answer"] = serde_json::json!({"Discard": l1_contract});
                    prep["copy"]["discarded_at"] = serde_json::json!(imp_discard);
                }
                if let Some(copied) = journal.get_mut("MetamorphCopied") {
                    copied["entrant"]["card_def"] = serde_json::json!(mox_id);
                    copied["copy"]["copied"] = serde_json::to_value(mox_contract).unwrap();
                    copied["copy"]["entry_answer"] = serde_json::json!({"Discard": l1_contract});
                    copied["copy"]["discarded_at"] = serde_json::json!(imp_discard);
                }
            }
        }
        assert_copy_restore_refused(forged.clone(), Action::Pass);

        // Forgery D1b (R4 Blocking Finding 4): inserting a forged
        // `CopyDiscardPaid` receipt immediately before `NoncastCopyPrepared`
        // pointing at Putrid Imp's discard must STILL be refused by same-turn
        // hand-discard conservation (Putrid Imp's activation + Mox Diamond
        // copy-entry require 2 hand discards, while only 1 occurred).
        let mut forged_with_receipt = forged;
        let history = forged_with_receipt["engine"]["event_history"]
            .as_array_mut()
            .unwrap();
        let prep_idx = history
            .iter()
            .position(|ev| {
                ev.get("MeddlingMageEntryV1")
                    .and_then(|e| e.get("journal"))
                    .and_then(|j| j.get("NoncastCopyPrepared"))
                    .is_some()
            })
            .unwrap();
        let (stack_item, physical) = {
            let copy =
                &history[prep_idx]["MeddlingMageEntryV1"]["journal"]["NoncastCopyPrepared"]["copy"];
            (copy["stack_item"].clone(), copy["physical"].clone())
        };
        history.insert(
            prep_idx,
            serde_json::json!({
                "CopyEntryProvenanceV1": {
                    "journal": {
                        "CopyDiscardPaid": {
                            "stack_item": stack_item,
                            "physical": physical,
                            "land": l1_contract,
                            "discard_at": imp_discard,
                        }
                    }
                }
            }),
        );
        for ev in history.iter_mut() {
            if let Some(journal) = ev
                .get_mut("MeddlingMageEntryV1")
                .and_then(|e| e.get_mut("journal"))
            {
                if let Some(prep) = journal.get_mut("NoncastCopyPrepared") {
                    let old_p = prep["copy"]["prepared_at"].as_u64().unwrap();
                    prep["copy"]["prepared_at"] = serde_json::json!(old_p + 1);
                }
                if let Some(copied) = journal.get_mut("MetamorphCopied") {
                    let old_e = copied["entry_history_index"].as_u64().unwrap();
                    copied["entry_history_index"] = serde_json::json!(old_e + 1);
                    let old_p = copied["copy"]["prepared_at"].as_u64().unwrap();
                    copied["copy"]["prepared_at"] = serde_json::json!(old_p + 1);
                }
            }
        }
        assert_copy_restore_refused(forged_with_receipt, Action::Pass);
    }

    // Positive control D1c: Putrid Imp activated earlier discarding one land
    // AND Mox Diamond copy legitimately discarding a second land must restore
    // cleanly without false rejection.
    {
        let mut both_state = islands();
        both_state.active_player = P0;
        both_state.priority_player = P0;
        let mox = put(&mut both_state, P0, "Mox Diamond", Zone::Battlefield);
        settle_first(&mut both_state);
        let imp = put(&mut both_state, P0, "Putrid Imp", Zone::Battlefield);
        let l1 = put(&mut both_state, P0, "Island", Zone::Hand);
        let l2 = put(&mut both_state, P0, "Island", Zone::Hand);
        let metamorph = put(&mut both_state, P0, "Phyrexian Metamorph", Zone::Graveyard);
        let reanimate = put(&mut both_state, P0, "Reanimate", Zone::Hand);
        pool(&mut both_state, P0, &[ManaColor::B], 0);
        cast(&mut both_state, reanimate, &[Target::Object(metamorph)]);
        next(&mut both_state);

        activate(&mut both_state, imp, 0, &[]);
        run_until(
            &mut both_state,
            |s| {
                s.stack.len() == 1
                    && s.engine.pending_discard.is_none()
                    && zone(s, l1) == Zone::Graveyard
            },
            |_, decision| match decision {
                Decision::Discard { .. } => Some(Action::Discard(vec![l1])),
                _ => None,
            },
        );
        assert!(matches!(
            settle(&mut both_state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        act(
            &mut both_state,
            Action::ChooseEffectTarget(Target::Object(mox)),
        );
        assert!(matches!(
            next(&mut both_state),
            Decision::ChooseEffectOption { .. }
        ));
        act(&mut both_state, Action::ChooseEffectOption(1));
        settled(&mut both_state);
        assert_eq!(zone(&both_state, l1), Zone::Graveyard);
        assert_eq!(zone(&both_state, l2), Zone::Graveyard);
        assert_eq!(zone(&both_state, metamorph), Zone::Battlefield);
        assert_eq!(
            both_state.objects.get(metamorph).card_def,
            id("Mox Diamond")
        );

        let mut restored: GameState =
            serde_json::from_value(serde_json::to_value(&both_state).unwrap()).unwrap();
        act(&mut restored, Action::Pass);
    }

    // Forgery D2 (Test isolation): inserting a non-blacklisted event (LifeGain,
    // which r2's event blacklist allowed) between a genuine Mox Diamond land
    // discard and its CopyDiscardPaid receipt isolates r3's companion-only
    // adjacency whitelist.
    {
        let mut forged = serde_json::to_value(&state).unwrap();
        let history = forged["engine"]["event_history"].as_array_mut().unwrap();
        let discard_idx = history
            .iter()
            .position(|ev| {
                ev.get("ZoneChange").is_some_and(|z| {
                    z["object"] == serde_json::json!(land)
                        && z["from"] == "Hand"
                        && z["to"] == "Graveyard"
                })
            })
            .unwrap();
        history.insert(
            discard_idx + 1,
            serde_json::json!({"LifeGain": {"player": 0, "amount": 1}}),
        );
        for ev in history.iter_mut() {
            if let Some(journal) = ev
                .get_mut("MeddlingMageEntryV1")
                .and_then(|e| e.get_mut("journal"))
            {
                if let Some(prep) = journal.get_mut("NoncastCopyPrepared") {
                    let old_p = prep["copy"]["prepared_at"].as_u64().unwrap();
                    prep["copy"]["prepared_at"] = serde_json::json!(old_p + 1);
                }
                if let Some(copied) = journal.get_mut("MetamorphCopied") {
                    let old_e = copied["entry_history_index"].as_u64().unwrap();
                    copied["entry_history_index"] = serde_json::json!(old_e + 1);
                    let old_p = copied["copy"]["prepared_at"].as_u64().unwrap();
                    copied["copy"]["prepared_at"] = serde_json::json!(old_p + 1);
                }
            }
        }
        assert_copy_restore_refused(forged, Action::Pass);
    }
}

#[test]
fn noncast_metamorph_completed_snapshot_refuses_coherent_forgery() {
    fn mutate_both_receipts(
        value: &mut serde_json::Value,
        mut f: impl FnMut(&mut serde_json::Value, bool),
    ) {
        for ev in value["engine"]["event_history"].as_array_mut().unwrap() {
            let Some(journal) = ev
                .get_mut("MeddlingMageEntryV1")
                .and_then(|e| e.get_mut("journal"))
            else {
                continue;
            };
            if let Some(prep) = journal.get_mut("NoncastCopyPrepared") {
                f(prep, false);
            }
            if let Some(copied) = journal.get_mut("MetamorphCopied") {
                f(copied, true);
            }
        }
    }

    // 1. Coherently stripping Phantasmal Image's illusion_sacrifice by removing
    // `values` from both receipts and removing all `c8b.copy_mods` rows (so
    // `current_copy_mods` is `None == expected_mods`, genuinely exercising
    // donor provenance rather than r1's `None != Some(empty)` mismatch) must be
    // refused (Finding 5).
    {
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let _strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
        let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U], 1);
        cast(&mut state, image, &[]);
        settle_option(&mut state, 1);
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(image)),
        );
        settled(&mut state);
        assert_eq!(zone(&state, metamorph), Zone::Battlefield);

        let mut forged = serde_json::to_value(&state).unwrap();
        mutate_both_receipts(&mut forged, |entry, _| {
            entry["copy"].as_object_mut().unwrap().remove("values");
        });
        forged["eternal_v1"]["c8b"]["copy_mods"] = serde_json::json!([]);
        assert_copy_restore_refused(forged, Action::Pass);
    }

    // 1b (Blocking Finding 4): unused Saheeli, Sublime Artificer in library
    // must NOT allow coherently stripping Phantasmal Image's illusion_sacrifice
    // via `added_artifact: true` without an actual Saheeli cover on the donor.
    {
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let _unused_saheeli = put(&mut state, P0, "Saheeli, Sublime Artificer", Zone::Library);
        let _strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
        let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U], 1);
        cast(&mut state, image, &[]);
        settle_option(&mut state, 1);
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(image)),
        );
        settled(&mut state);
        assert_eq!(zone(&state, metamorph), Zone::Battlefield);

        let mut forged = serde_json::to_value(&state).unwrap();
        let forged_mods = serde_json::json!({
            "added_artifact": true,
        });
        mutate_both_receipts(&mut forged, |entry, _| {
            entry["copy"]["values"]["mods"] = forged_mods.clone();
        });
        for row in forged["eternal_v1"]["c8b"]["copy_mods"]
            .as_array_mut()
            .unwrap()
        {
            row[1] = forged_mods.clone();
        }
        assert_copy_restore_refused(forged, Action::Pass);
    }

    // 2. Coherently forging values/c8b.copy_mods (arbitrary base_pt, Absorbing
    // Man mods, or prototyped), fabricating entry_answer, or rewriting donor to
    // Anointed Peacekeeper / Omniscience on a Mind Stone copy must be refused.
    {
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
        let peacekeeper = put(&mut state, P1, "Anointed Peacekeeper", Zone::Battlefield);
        let omniscience = put(&mut state, P1, "Omniscience", Zone::Battlefield);
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(stone)),
        );
        settled(&mut state);
        let metamorph_link = serde_json::json!({
            "object": metamorph,
            "zone_change_count": state.objects.get(metamorph).zone_change_count,
        });

        // 2a: Arbitrary 99/99 copy_mods injection on Mind Stone.
        {
            let mut forged = serde_json::to_value(&state).unwrap();
            let mods = serde_json::json!({
                "base_pt": [99, 99],
                "added_creature": true,
                "added_vigilance": true,
            });
            let vals = serde_json::json!({
                "card_def": id("Mind Stone"),
                "face_index": 0,
                "mods": mods,
                "copied_end_step_sacrifices_v1": 0,
                "copied_nightmare_v1": false,
                "prototyped": false,
            });
            mutate_both_receipts(&mut forged, |entry, _| {
                entry["copy"]["values"] = vals.clone();
            });
            forged["eternal_v1"]["c8b"]["copy_mods"] = serde_json::json!([[metamorph_link, mods]]);
            assert_copy_restore_refused(forged, Action::Pass);
        }

        // 2b: Structurally valid Absorbing Man mods injected onto plain Mind Stone.
        {
            let mut forged = serde_json::to_value(&state).unwrap();
            let mut sub_ids = vec![
                mtg_kernel::card_def::Subtype::Human.stable_id(),
                mtg_kernel::card_def::Subtype::Villain.stable_id(),
            ];
            sub_ids.sort_unstable();
            let mods = serde_json::json!({
                "added_subtypes": sub_ids,
                "override_name": "Absorbing Man",
                "base_pt": [4, 4],
                "added_creature": true,
                "added_legendary": true,
                "added_vigilance": true,
            });
            let vals = serde_json::json!({
                "card_def": id("Mind Stone"),
                "face_index": 0,
                "mods": mods,
                "copied_end_step_sacrifices_v1": 0,
                "copied_nightmare_v1": false,
                "prototyped": false,
            });
            mutate_both_receipts(&mut forged, |entry, _| {
                entry["copy"]["values"] = vals.clone();
            });
            forged["eternal_v1"]["c8b"]["copy_mods"] = serde_json::json!([[metamorph_link, mods]]);
            assert_copy_restore_refused(forged, Action::Pass);
        }

        // 2c: Forging prototyped: true on non-prototype Mind Stone.
        {
            let mut forged = serde_json::to_value(&state).unwrap();
            let vals = serde_json::json!({
                "card_def": id("Mind Stone"),
                "face_index": 0,
                "mods": {},
                "copied_end_step_sacrifices_v1": 0,
                "copied_nightmare_v1": false,
                "prototyped": true,
            });
            mutate_both_receipts(&mut forged, |entry, _| {
                entry["copy"]["values"] = vals.clone();
            });
            forged["eternal_v1"]["pf2c"]["prototyped_permanents"] =
                serde_json::json!([metamorph_link]);
            assert_copy_restore_refused(forged, Action::Pass);
        }

        // 2d: Fabricating Serra's Emissary entry_answer on Mind Stone.
        {
            let mut forged = serde_json::to_value(&state).unwrap();
            mutate_both_receipts(&mut forged, |entry, _| {
                entry["copy"]["entry_answer"] = serde_json::json!({"ProtectionType": 0});
            });
            forged["eternal_v1"]["e5a"]["emissary_types"] =
                serde_json::json!([[metamorph_link, 0]]);
            assert_copy_restore_refused(forged, Action::Pass);
        }

        // 2e: Coherently rewriting donor from Mind Stone to unsupported
        // Anointed Peacekeeper or non-artifact/non-creature Omniscience.
        for (bad_obj, bad_name) in [
            (peacekeeper, "Anointed Peacekeeper"),
            (omniscience, "Omniscience"),
        ] {
            let mut state_e = state.clone();
            let bad_contract =
                mtg_kernel::state::AbilitySourceContractV4::capture(&state_e, bad_obj);
            let bad_id = id(bad_name);
            state_e.objects.get_mut(metamorph).card_def = bad_id;
            state_e.objects.get_mut(metamorph).name = bad_name.to_string();
            let mut forged = serde_json::to_value(&state_e).unwrap();
            mutate_both_receipts(&mut forged, |entry, is_copied| {
                entry["copy"]["copied"] = serde_json::to_value(bad_contract).unwrap();
                if is_copied {
                    entry["entrant"]["card_def"] = serde_json::json!(bad_id);
                }
            });
            if let Some(defs) = forged["standard_v1"]["copied_card_defs"].as_array_mut() {
                for row in defs.iter_mut() {
                    if row[0] == serde_json::json!(metamorph)
                        && row[2] == serde_json::json!(id("Mind Stone"))
                    {
                        row[2] = serde_json::json!(bad_id);
                    }
                }
            }
            assert_copy_restore_refused(forged, Action::Pass);
        }
    }

    // 3. Coherent entry_answer domain/omission forgery on Serra's Emissary and
    // Phyrexian Revoker.
    {
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let emissary = put(&mut state, P0, "Serra's Emissary", Zone::Battlefield);
        settle_first(&mut state);
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(emissary)),
        );
        act(&mut state, Action::ChooseEffectOption(0));
        settled(&mut state);

        // Out-of-range ProtectionType(99).
        let mut out_of_range = serde_json::to_value(&state).unwrap();
        mutate_both_receipts(&mut out_of_range, |entry, _| {
            entry["copy"]["entry_answer"] = serde_json::json!({"ProtectionType": 99});
        });
        for row in out_of_range["eternal_v1"]["e5a"]["emissary_types"]
            .as_array_mut()
            .unwrap()
        {
            if row[0]["object"] == serde_json::json!(metamorph) {
                row[1] = serde_json::json!(99);
            }
        }
        assert_copy_restore_refused(out_of_range, Action::Pass);

        // Omitting required entry_answer completely.
        let mut omitted = serde_json::to_value(&state).unwrap();
        mutate_both_receipts(&mut omitted, |entry, _| {
            entry["copy"]
                .as_object_mut()
                .unwrap()
                .remove("entry_answer");
        });
        omitted["eternal_v1"]["e5a"]["emissary_types"]
            .as_array_mut()
            .unwrap()
            .retain(|row| row[0]["object"] != serde_json::json!(metamorph));
        assert_copy_restore_refused(omitted, Action::Pass);
    }

    {
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let revoker = put(&mut state, P0, "Phyrexian Revoker", Zone::Battlefield);
        settle_first(&mut state);
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(revoker)),
        );
        act(&mut state, Action::ChooseEffectOption(0));
        settled(&mut state);

        // Naming a land card (Island) for Phyrexian Revoker.
        let mut land_name = serde_json::to_value(&state).unwrap();
        let island_id = id("Island");
        mutate_both_receipts(&mut land_name, |entry, _| {
            entry["copy"]["entry_answer"] = serde_json::json!({"RevokerName": island_id});
        });
        for row in land_name["eternal_v1"]["e7b"]["revoker_names"]
            .as_array_mut()
            .unwrap()
        {
            if row[0]["object"] == serde_json::json!(metamorph) {
                row[1] = serde_json::json!(island_id);
            }
        }
        assert_copy_restore_refused(land_name, Action::Pass);
    }

    // 4 (Blocking Finding 2): Reanimate Metamorph copying front-face Delver of
    // Secrets, then let the donor Delver legitimately transform to its back
    // face on a later upkeep. Coherently forging NoncastCopyPrepared,
    // MetamorphCopied, and live Metamorph to face_index: 1 must be refused
    // because the donor's reconstructed face as of entry was 0.
    {
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let donor = put(&mut state, P1, "Delver of Secrets", Zone::Battlefield);
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(donor)),
        );
        settled(&mut state);
        assert_eq!(state.objects.get(metamorph).v4.face_index, 0);

        // Place an instant on top of P1's library so donor Delver transforms on
        // P1's upkeep.
        put(&mut state, P1, "Lightning Bolt", Zone::Library);
        let mut donor_revealed = false;
        run_until(
            &mut state,
            |s| {
                s.active_player == P1
                    && s.step == Step::Main1
                    && s.stack.is_empty()
                    && s.engine.pending_triggers.is_empty()
            },
            |_, decision| match decision {
                Decision::ChooseEffectBoolean { source, .. } if *source == donor => {
                    donor_revealed = true;
                    Some(Action::ChooseEffectBoolean(true))
                }
                _ => None,
            },
        );
        assert!(donor_revealed);
        assert_eq!(state.objects.get(donor).v4.face_index, 1);
        assert_eq!(state.objects.get(metamorph).v4.face_index, 0);

        // Genuine post-transform snapshot must restore cleanly.
        let mut restored: GameState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        act(&mut restored, Action::Pass);

        // Forge copy to back face (Insectile Aberration, face_index: 1).
        let mut forged_state = state.clone();
        let delver_id = id("Delver of Secrets");
        let back = mtg_kernel::card_def::CARD_DEFS[usize::from(delver_id)]
            .transform_face
            .as_ref()
            .unwrap();
        let live_mm = forged_state.objects.get_mut(metamorph);
        live_mm.v4.face_index = 1;
        live_mm.name = back.name.to_string();
        live_mm.v4.effective_color_mask = mtg_kernel::card_def::mana_colors_mask(back.colors);
        live_mm.v4.effective_subtype_ids = back.subtypes.iter().map(|s| s.stable_id()).collect();
        live_mm.v4.effective_subtype_ids.sort_unstable();
        live_mm.v4.effective_subtype_ids.dedup();
        let mut forged = serde_json::to_value(&forged_state).unwrap();
        let back_vals = serde_json::json!({
            "card_def": delver_id,
            "face_index": 1,
            "mods": {},
            "copied_end_step_sacrifices_v1": 0,
            "copied_nightmare_v1": false,
            "prototyped": false,
        });
        mutate_both_receipts(&mut forged, |entry, _| {
            entry["copy"]["values"] = back_vals.clone();
        });
        assert_copy_restore_refused(forged, Action::Pass);
    }

    // 5 (Blocking Finding 3): unused Liquimetal Coating or Mycosynth Lattice in
    // a library must NOT excuse coherently forging Omniscience as a copy donor.
    for unused_granter in ["Liquimetal Coating", "Mycosynth Lattice"] {
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let _unused = put(&mut state, P0, unused_granter, Zone::Library);
        let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
        let omniscience = put(&mut state, P1, "Omniscience", Zone::Battlefield);
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(stone)),
        );
        settled(&mut state);

        let mut state_f = state.clone();
        let omni_contract =
            mtg_kernel::state::AbilitySourceContractV4::capture(&state_f, omniscience);
        let omni_id = id("Omniscience");
        state_f.objects.get_mut(metamorph).card_def = omni_id;
        state_f.objects.get_mut(metamorph).name = "Omniscience".to_string();
        let mut forged = serde_json::to_value(&state_f).unwrap();
        mutate_both_receipts(&mut forged, |entry, is_copied| {
            entry["copy"]["copied"] = serde_json::to_value(omni_contract).unwrap();
            if is_copied {
                entry["entrant"]["card_def"] = serde_json::json!(omni_id);
            }
        });
        if let Some(defs) = forged["standard_v1"]["copied_card_defs"].as_array_mut() {
            for row in defs.iter_mut() {
                if row[0] == serde_json::json!(metamorph)
                    && row[2] == serde_json::json!(id("Mind Stone"))
                {
                    row[2] = serde_json::json!(omni_id);
                }
            }
        }
        assert_copy_restore_refused(forged, Action::Pass);
    }

    // 5b (Positive check for Finding 3): an actual Liquimetal Coating
    // activation targeting Omniscience on Turn 1 must keep restoring cleanly
    // on Turn 2 after cleanup expires Liquimetal Coating's artifact grant.
    {
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let coating = put(&mut state, P0, "Liquimetal Coating", Zone::Battlefield);
        let omniscience = put(&mut state, P1, "Omniscience", Zone::Battlefield);
        activate(&mut state, coating, 0, &[Target::Object(omniscience)]);
        settled(&mut state);
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(omniscience)),
        );
        settled(&mut state);
        assert_eq!(state.objects.get(metamorph).card_def, id("Omniscience"));
        // Advance to P1's Main1 so cleanup expires Liquimetal Coating.
        run_until(
            &mut state,
            |s| {
                s.active_player == P1
                    && s.step == Step::Main1
                    && s.stack.is_empty()
                    && s.engine.pending_triggers.is_empty()
            },
            |_, _| None,
        );
        assert!(!engine::object_has_type(
            &state,
            omniscience,
            CardType::Artifact
        ));
        let mut restored: GameState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        act(&mut restored, Action::Pass);
    }

    // 6 (R3 Blocking Finding 1): never-animated Mishra's Factory must be
    // refused both after cleanup and after donor departure; a genuinely
    // animated Mishra's Factory must restore cleanly after both.
    {
        let advance_to_p1_main1 = |state: &mut GameState| {
            run_until(
                state,
                |s| {
                    s.active_player == P1
                        && s.step == Step::Main1
                        && s.stack.is_empty()
                        && s.engine.pending_triggers.is_empty()
                },
                |_, _| None,
            );
        };
        let forge_to_factory = |base: &GameState,
                                metamorph: mtg_kernel::ids::ObjectId,
                                factory_contract: mtg_kernel::state::AbilitySourceContractV4|
         -> serde_json::Value {
            let mut state_f = base.clone();
            let factory_id = id("Mishra's Factory");
            let stone_id = id("Mind Stone");
            state_f.objects.get_mut(metamorph).card_def = factory_id;
            state_f.objects.get_mut(metamorph).name = "Mishra's Factory".to_string();
            let mut forged = serde_json::to_value(&state_f).unwrap();
            mutate_both_receipts(&mut forged, |entry, is_copied| {
                entry["copy"]["copied"] = serde_json::to_value(factory_contract).unwrap();
                if is_copied {
                    entry["entrant"]["card_def"] = serde_json::json!(factory_id);
                }
            });
            if let Some(defs) = forged["standard_v1"]["copied_card_defs"].as_array_mut() {
                for row in defs.iter_mut() {
                    if row[0] == serde_json::json!(metamorph)
                        && row[2] == serde_json::json!(stone_id)
                    {
                        row[2] = serde_json::json!(factory_id);
                    }
                }
            }
            forged
        };

        let insert_forged_land_animated =
            |forged: &mut serde_json::Value,
             factory_contract: mtg_kernel::state::AbilitySourceContractV4,
             stack_item: u64,
             animation_timestamp: u64| {
                let history = forged["engine"]["event_history"].as_array_mut().unwrap();
                let prep_idx = history
                    .iter()
                    .position(|ev| {
                        ev.get("MeddlingMageEntryV1")
                            .and_then(|e| e.get("journal"))
                            .and_then(|j| j.get("NoncastCopyPrepared"))
                            .is_some()
                    })
                    .unwrap();
                history.insert(
                    prep_idx,
                    serde_json::json!({
                        "CopyEntryProvenanceV1": {
                            "journal": {
                                "LandAnimated": {
                                    "host": {
                                        "object": factory_contract.source,
                                        "zone_change_count": factory_contract.zone_change_count,
                                    },
                                    "card_def": factory_contract.card_def,
                                    "until_end_of_turn": true,
                                    "stack_item": stack_item,
                                    "animation_timestamp": animation_timestamp,
                                }
                            }
                        }
                    }),
                );
                for ev in history.iter_mut() {
                    if let Some(journal) = ev
                        .get_mut("MeddlingMageEntryV1")
                        .and_then(|e| e.get_mut("journal"))
                    {
                        if let Some(prep) = journal.get_mut("NoncastCopyPrepared") {
                            let old_p = prep["copy"]["prepared_at"].as_u64().unwrap();
                            prep["copy"]["prepared_at"] = serde_json::json!(old_p + 1);
                        }
                        if let Some(copied) = journal.get_mut("MetamorphCopied") {
                            let old_e = copied["entry_history_index"].as_u64().unwrap();
                            copied["entry_history_index"] = serde_json::json!(old_e + 1);
                            let old_p = copied["copy"]["prepared_at"].as_u64().unwrap();
                            copied["copy"]["prepared_at"] = serde_json::json!(old_p + 1);
                        }
                    }
                }
            };

        // 6a: Never-animated Mishra's Factory after cleanup (both without a
        // receipt and with a forged LandAnimated receipt).
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let factory = put(&mut state, P0, "Mishra's Factory", Zone::Battlefield);
        let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
        let factory_contract = mtg_kernel::state::AbilitySourceContractV4::capture(&state, factory);
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(stone)),
        );
        settled(&mut state);
        advance_to_p1_main1(&mut state);
        assert!(!engine::object_has_type(
            &state,
            factory,
            CardType::Creature
        ));
        assert!(!engine::object_has_type(
            &state,
            factory,
            CardType::Artifact
        ));
        let forged_cleanup = forge_to_factory(&state, metamorph, factory_contract);
        assert_copy_restore_refused(forged_cleanup.clone(), Action::Pass);
        for (bad_item, bad_ts) in [(1, 0), (2, 0), (1, 1)] {
            let mut with_receipt = forged_cleanup.clone();
            insert_forged_land_animated(&mut with_receipt, factory_contract, bad_item, bad_ts);
            assert_copy_restore_refused(with_receipt, Action::Pass);
        }

        // 6b: Never-animated Mishra's Factory after donor departure (both
        // without a receipt and with a forged LandAnimated receipt).
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let factory = put(&mut state, P0, "Mishra's Factory", Zone::Battlefield);
        let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
        let wasteland = put(&mut state, P0, "Wasteland", Zone::Battlefield);
        let factory_contract = mtg_kernel::state::AbilitySourceContractV4::capture(&state, factory);
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(stone)),
        );
        settled(&mut state);
        activate(&mut state, wasteland, 0, &[Target::Object(factory)]);
        settled(&mut state);
        assert_eq!(zone(&state, factory), Zone::Graveyard);
        let forged_departed = forge_to_factory(&state, metamorph, factory_contract);
        assert_copy_restore_refused(forged_departed.clone(), Action::Pass);
        for (bad_item, bad_ts) in [(1, 0), (2, 0), (3, 0)] {
            let mut with_receipt = forged_departed.clone();
            insert_forged_land_animated(&mut with_receipt, factory_contract, bad_item, bad_ts);
            assert_copy_restore_refused(with_receipt, Action::Pass);
        }

        // 6c: Positive genuinely animated Mishra's Factory, after cleanup and
        // after donor departure.
        let mut state = islands();
        state.active_player = P0;
        state.priority_player = P0;
        let factory = put(&mut state, P0, "Mishra's Factory", Zone::Battlefield);
        let wasteland = put(&mut state, P0, "Wasteland", Zone::Battlefield);
        pool(&mut state, P0, &[], 1);
        activate(&mut state, factory, 0, &[]);
        settled(&mut state);
        assert!(engine::object_has_type(&state, factory, CardType::Creature));
        let metamorph = reanimate_copy(&mut state, P0);
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(factory)),
        );
        settled(&mut state);
        assert_eq!(
            state.objects.get(metamorph).card_def,
            id("Mishra's Factory")
        );

        // After cleanup on P1's Main1.
        advance_to_p1_main1(&mut state);
        assert!(!engine::object_has_type(
            &state,
            factory,
            CardType::Creature
        ));
        let mut restored_cleanup: GameState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        act(&mut restored_cleanup, Action::Pass);

        // After donor departure (Wasteland destroys Factory).
        act(&mut state, Action::Pass);
        activate(&mut state, wasteland, 0, &[Target::Object(factory)]);
        settled(&mut state);
        assert_eq!(zone(&state, factory), Zone::Graveyard);
        let mut restored_departed: GameState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        act(&mut restored_departed, Action::Pass);
    }

    // 7 (R3 Blocking Finding 2): Saheeli, Sublime Artificer coverage must be
    // bound to slot 0 (host), exact copied values, and resolution before entry.
    {
        let advance_to_p1_main1 = |state: &mut GameState| {
            run_until(
                state,
                |s| {
                    s.active_player == P1
                        && s.step == Step::Main1
                        && s.stack.is_empty()
                        && s.engine.pending_triggers.is_empty()
                },
                |_, _| None,
            );
        };
        let setup_saheeli_image = || {
            let mut state = islands();
            state.active_player = P0;
            state.priority_player = P0;
            let _strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
            let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
            let saheeli = put(&mut state, P0, "Saheeli, Sublime Artificer", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
            cast(&mut state, saheeli, &[]);
            settled(&mut state);
            let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U], 1);
            cast(&mut state, image, &[]);
            settle_option(&mut state, 1);
            assert_eq!(state.objects.get(image).card_def, id("Baleful Strix"));
            (state, saheeli, image, stone)
        };

        let activate_saheeli_stifling_image =
            |state: &mut GameState,
             saheeli: mtg_kernel::ids::ObjectId,
             slot0: mtg_kernel::ids::ObjectId,
             slot1: mtg_kernel::ids::ObjectId| {
                activate(
                    state,
                    saheeli,
                    0,
                    &[Target::Object(slot0), Target::Object(slot1)],
                );
                next(state);
                let sac_trigger = state.stack.last().unwrap().v4.stack_item_id;
                let stifle = put(state, P0, "Stifle", Zone::Hand);
                pool(state, P0, &[ManaColor::U], 0);
                cast(state, stifle, &[Target::StackItem(sac_trigger)]);
                settled(state);
            };

        // 7a: Negative post-entry Saheeli host forgery (plus genuine post-entry
        // restore). Image copies Strix; Metamorph copies Image; AFTER entry
        // Saheeli targets [Image (slot 0), Mind Stone (slot 1)] (Stifle counters
        // Image's sacrifice trigger so Image gets a genuine live saheeli_copies
        // record). Forging Metamorph's frozen mods to `{ added_artifact: true }`
        // must be refused.
        {
            let (mut state, saheeli, image, stone) = setup_saheeli_image();
            let metamorph = reanimate_copy(&mut state, P0);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(image)),
            );
            settled(&mut state);
            activate_saheeli_stifling_image(&mut state, saheeli, image, stone);
            assert_eq!(zone(&state, image), Zone::Battlefield);
            assert_eq!(state.objects.get(image).card_def, id("Mind Stone"));

            // Genuine post-entry Saheeli snapshot must restore cleanly.
            let mut genuine: GameState =
                serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
            act(&mut genuine, Action::Pass);

            // Forge earlier Metamorph receipt + live copy_mods/subtypes to
            // strip illusion_sacrifice and Illusion subtype.
            let mut forged_state = state.clone();
            let illusion_id = mtg_kernel::card_def::Subtype::Illusion.stable_id();
            forged_state
                .objects
                .get_mut(metamorph)
                .v4
                .effective_subtype_ids
                .retain(|&s| s != illusion_id);
            let mut forged = serde_json::to_value(&forged_state).unwrap();
            let forged_mods = serde_json::json!({ "added_artifact": true });
            mutate_both_receipts(&mut forged, |entry, _| {
                entry["copy"]["values"]["mods"] = forged_mods.clone();
            });
            for row in forged["eternal_v1"]["c8b"]["copy_mods"]
                .as_array_mut()
                .unwrap()
            {
                if row[0]["object"] == serde_json::json!(metamorph) {
                    row[1] = forged_mods.clone();
                }
            }
            assert_copy_restore_refused(forged, Action::Pass);
        }

        // 7b: Negative model-only Saheeli after cleanup. BEFORE entry, Saheeli
        // targets [Mind Stone (slot 0), Image (slot 1)] so Image is ONLY the
        // model, never the host. After cleanup, stripping Image's
        // illusion_sacrifice must be refused.
        {
            let (mut state, saheeli, image, stone) = setup_saheeli_image();
            activate_saheeli_stifling_image(&mut state, saheeli, stone, image);
            assert_eq!(zone(&state, image), Zone::Battlefield);
            assert_eq!(state.objects.get(stone).card_def, id("Baleful Strix"));
            let metamorph = reanimate_copy(&mut state, P0);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(image)),
            );
            settled(&mut state);
            advance_to_p1_main1(&mut state);

            let mut forged_state = state.clone();
            let illusion_id = mtg_kernel::card_def::Subtype::Illusion.stable_id();
            forged_state
                .objects
                .get_mut(metamorph)
                .v4
                .effective_subtype_ids
                .retain(|&s| s != illusion_id);
            let mut forged = serde_json::to_value(&forged_state).unwrap();
            let forged_mods = serde_json::json!({ "added_artifact": true });
            mutate_both_receipts(&mut forged, |entry, _| {
                entry["copy"]["values"]["mods"] = forged_mods.clone();
            });
            for row in forged["eternal_v1"]["c8b"]["copy_mods"]
                .as_array_mut()
                .unwrap()
            {
                if row[0]["object"] == serde_json::json!(metamorph) {
                    row[1] = forged_mods.clone();
                }
            }
            assert_copy_restore_refused(forged, Action::Pass);
        }

        // 7c: Positive genuine Saheeli host before AND after cleanup. BEFORE
        // entry, Saheeli targets [Image (slot 0), Mind Stone (slot 1)].
        // Metamorph copies the Saheeli-covered Image (becoming Mind Stone with
        // `added_artifact: true` and no illusion_sacrifice). Restores cleanly
        // both before cleanup and after cleanup unwinds Image back to Strix.
        {
            let (mut state, saheeli, image, stone) = setup_saheeli_image();
            activate_saheeli_stifling_image(&mut state, saheeli, image, stone);
            assert_eq!(zone(&state, image), Zone::Battlefield);
            assert_eq!(state.objects.get(image).card_def, id("Mind Stone"));
            let metamorph = reanimate_copy(&mut state, P0);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(image)),
            );
            settled(&mut state);
            assert_eq!(state.objects.get(metamorph).card_def, id("Mind Stone"));

            let mut before_cleanup: GameState =
                serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
            act(&mut before_cleanup, Action::Pass);

            advance_to_p1_main1(&mut state);
            assert_eq!(state.objects.get(image).card_def, id("Baleful Strix"));
            assert_eq!(state.objects.get(metamorph).card_def, id("Mind Stone"));
            let mut after_cleanup: GameState =
                serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
            act(&mut after_cleanup, Action::Pass);
        }

        // 7d (R4 Blocking Finding 2): duplicating OR moving a post-entry
        // Saheeli activation's Targeted pair and SaheeliApplied receipt before
        // Metamorph preparation (with a second ordinary Baleful Strix as model)
        // must be refused.
        {
            let (mut state, saheeli, image, _stone) = setup_saheeli_image();
            let strix2 = put(&mut state, P0, "Baleful Strix", Zone::Battlefield);
            let metamorph = reanimate_copy(&mut state, P0);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(image)),
            );
            settled(&mut state);
            activate_saheeli_stifling_image(&mut state, saheeli, image, strix2);
            assert_eq!(zone(&state, image), Zone::Battlefield);
            assert_eq!(state.objects.get(image).card_def, id("Baleful Strix"));

            let mut forged_base = state.clone();
            let illusion_id = mtg_kernel::card_def::Subtype::Illusion.stable_id();
            forged_base
                .objects
                .get_mut(metamorph)
                .v4
                .effective_subtype_ids
                .retain(|&s| s != illusion_id);
            let mut base_json = serde_json::to_value(&forged_base).unwrap();
            let forged_mods = serde_json::json!({ "added_artifact": true });
            mutate_both_receipts(&mut base_json, |entry, _| {
                entry["copy"]["values"]["mods"] = forged_mods.clone();
            });
            for row in base_json["eternal_v1"]["c8b"]["copy_mods"]
                .as_array_mut()
                .unwrap()
            {
                if row[0]["object"] == serde_json::json!(metamorph) {
                    row[1] = forged_mods.clone();
                }
            }

            let (slot0_idx, slot1_idx, saheeli_idx) = {
                let history = base_json["engine"]["event_history"].as_array().unwrap();
                let s_idx = history
                    .iter()
                    .rposition(|ev| {
                        ev.get("CopyEntryProvenanceV1")
                            .and_then(|e| e.get("journal"))
                            .and_then(|j| j.get("SaheeliApplied"))
                            .is_some()
                    })
                    .unwrap();
                let s_item = history[s_idx]["CopyEntryProvenanceV1"]["journal"]["SaheeliApplied"]
                    ["stack_item"]
                    .clone();
                let t_indices: Vec<usize> = history
                    .iter()
                    .enumerate()
                    .filter_map(|(i, ev)| {
                        ev.get("Targeted")
                            .filter(|t| t["targeting_stack_item"] == s_item)
                            .map(|_| i)
                    })
                    .collect();
                assert_eq!(t_indices.len(), 2);
                (t_indices[0], t_indices[1], s_idx)
            };

            // Variant A: duplicate the two Targeted events + SaheeliApplied
            // right before NoncastCopyPrepared (exact Kimi r4 Finding 2 attack).
            {
                let mut dup = base_json.clone();
                let history = dup["engine"]["event_history"].as_array_mut().unwrap();
                let t0 = history[slot0_idx].clone();
                let t1 = history[slot1_idx].clone();
                let sa = history[saheeli_idx].clone();
                let prep_idx = history
                    .iter()
                    .position(|ev| {
                        ev.get("MeddlingMageEntryV1")
                            .and_then(|e| e.get("journal"))
                            .and_then(|j| j.get("NoncastCopyPrepared"))
                            .is_some()
                    })
                    .unwrap();
                history.insert(prep_idx, sa);
                history.insert(prep_idx, t1);
                history.insert(prep_idx, t0);
                for ev in history.iter_mut() {
                    if let Some(journal) = ev
                        .get_mut("MeddlingMageEntryV1")
                        .and_then(|e| e.get_mut("journal"))
                    {
                        if let Some(prep) = journal.get_mut("NoncastCopyPrepared") {
                            let old_p = prep["copy"]["prepared_at"].as_u64().unwrap();
                            prep["copy"]["prepared_at"] = serde_json::json!(old_p + 3);
                        }
                        if let Some(copied) = journal.get_mut("MetamorphCopied") {
                            let old_e = copied["entry_history_index"].as_u64().unwrap();
                            copied["entry_history_index"] = serde_json::json!(old_e + 3);
                            let old_p = copied["copy"]["prepared_at"].as_u64().unwrap();
                            copied["copy"]["prepared_at"] = serde_json::json!(old_p + 3);
                        }
                    }
                }
                assert_copy_restore_refused(dup, Action::Pass);
            }

            // Variant B: move (removing the post-entry originals) the two
            // Targeted events + SaheeliApplied right before NoncastCopyPrepared.
            {
                let mut moved = base_json.clone();
                let history = moved["engine"]["event_history"].as_array_mut().unwrap();
                let sa = history.remove(saheeli_idx);
                let t1 = history.remove(slot1_idx);
                let t0 = history.remove(slot0_idx);
                let prep_idx = history
                    .iter()
                    .position(|ev| {
                        ev.get("MeddlingMageEntryV1")
                            .and_then(|e| e.get("journal"))
                            .and_then(|j| j.get("NoncastCopyPrepared"))
                            .is_some()
                    })
                    .unwrap();
                history.insert(prep_idx, sa);
                history.insert(prep_idx, t1);
                history.insert(prep_idx, t0);
                for ev in history.iter_mut() {
                    if let Some(journal) = ev
                        .get_mut("MeddlingMageEntryV1")
                        .and_then(|e| e.get_mut("journal"))
                    {
                        if let Some(prep) = journal.get_mut("NoncastCopyPrepared") {
                            let old_p = prep["copy"]["prepared_at"].as_u64().unwrap();
                            prep["copy"]["prepared_at"] = serde_json::json!(old_p + 3);
                        }
                        if let Some(copied) = journal.get_mut("MetamorphCopied") {
                            let old_e = copied["entry_history_index"].as_u64().unwrap();
                            copied["entry_history_index"] = serde_json::json!(old_e + 3);
                            let old_p = copied["copy"]["prepared_at"].as_u64().unwrap();
                            copied["copy"]["prepared_at"] = serde_json::json!(old_p + 3);
                        }
                    }
                }
                assert_copy_restore_refused(moved, Action::Pass);
            }
        }
    }

    // 8 (R3 Blocking Finding 3): unrelated battlefield Phantasmal Image must
    // NOT authenticate forged Illusion mods on a departed ordinary Servo token;
    // a token that actually copied Phantasmal Image (via Vaultborn Tyrant) must
    // restore cleanly after both the original Image and the token depart.
    {
        // 8a: Negative ordinary Servo token departed with unrelated Image.
        {
            let mut state = islands();
            state.active_player = P0;
            state.priority_player = P0;
            let _strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
            let saheeli = put(&mut state, P0, "Saheeli, Sublime Artificer", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
            cast(&mut state, saheeli, &[]);
            settled(&mut state);
            // Cast a noncreature spell to create a genuine ordinary Servo Token.
            let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
            pool(&mut state, P0, &[], 1);
            cast(&mut state, ring, &[]);
            settled(&mut state);
            let servo = state.players[0]
                .battlefield
                .iter()
                .copied()
                .find(|&o| state.objects.get(o).card_def == id("Servo Token"))
                .expect("ordinary Servo Token created by Saheeli");
            // Put an unrelated Phantasmal Image copying Baleful Strix on play.
            let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U], 1);
            cast(&mut state, image, &[]);
            settle_option(&mut state, 1);

            let metamorph = reanimate_copy(&mut state, P0);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(servo)),
            );
            settled(&mut state);
            assert_eq!(state.objects.get(metamorph).card_def, id("Servo Token"));

            // Kill the ordinary Servo token so donor_same_incarnation is false.
            let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::R], 0);
            cast(&mut state, bolt, &[Target::Object(servo)]);
            settled(&mut state);
            assert_ne!(zone(&state, servo), Zone::Battlefield);

            // Forge Phantasmal Image mods onto the departed Servo token copy.
            let mut forged_state = state.clone();
            let illusion_id = mtg_kernel::card_def::Subtype::Illusion.stable_id();
            let live_mm = forged_state.objects.get_mut(metamorph);
            live_mm.v4.effective_subtype_ids.push(illusion_id);
            live_mm.v4.effective_subtype_ids.sort_unstable();
            live_mm.v4.effective_subtype_ids.dedup();
            let metamorph_link = serde_json::json!({
                "object": metamorph,
                "zone_change_count": forged_state.objects.get(metamorph).zone_change_count,
            });
            let mut forged = serde_json::to_value(&forged_state).unwrap();
            let forged_mods = serde_json::json!({
                "added_subtypes": [illusion_id],
                "illusion_sacrifice": true,
            });
            let forged_vals = serde_json::json!({
                "card_def": id("Servo Token"),
                "face_index": 0,
                "mods": forged_mods,
                "copied_end_step_sacrifices_v1": 0,
                "copied_nightmare_v1": false,
                "prototyped": false,
            });
            mutate_both_receipts(&mut forged, |entry, _| {
                entry["copy"]["values"] = forged_vals.clone();
            });
            forged["eternal_v1"]["c8b"]["copy_mods"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!([metamorph_link, forged_mods]));
            assert_copy_restore_refused(forged, Action::Pass);
        }

        // 8b: Positive token that genuinely copied a Phantasmal Image (via
        // Vaultborn Tyrant death trigger), restoring cleanly after both the
        // original Phantasmal Image and the token donor have departed.
        {
            let mut state = islands();
            state.active_player = P0;
            state.priority_player = P0;
            let _orig_tyrant = put(&mut state, P1, "Vaultborn Tyrant", Zone::Battlefield);
            let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U], 1);
            cast(&mut state, image, &[]);
            settle_option(&mut state, 1);
            assert_eq!(state.objects.get(image).card_def, id("Vaultborn Tyrant"));

            // Destroy Phantasmal Image with Wrath of God so Vaultborn Tyrant's
            // dies trigger creates an artifact Illusion token copy.
            let wrath = put(&mut state, P0, "Wrath of God", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
            cast(&mut state, wrath, &[]);
            settled(&mut state);
            assert_eq!(zone(&state, image), Zone::Graveyard);
            let token_donor = state.players[0]
                .battlefield
                .iter()
                .copied()
                .find(|&o| {
                    state.objects.get(o).v4.is_token
                        && state.objects.get(o).card_def == id("Vaultborn Tyrant")
                })
                .expect("Vaultborn Tyrant artifact Illusion token copy");

            // Reanimate Metamorph copying the Vaultborn Tyrant Illusion token.
            let metamorph = reanimate_copy(&mut state, P0);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(token_donor)),
            );
            settled(&mut state);
            assert_eq!(
                state.objects.get(metamorph).card_def,
                id("Vaultborn Tyrant")
            );

            // Target the token donor so its Illusion sacrifice trigger removes
            // it from the battlefield (both original Image and token departed).
            let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::R], 0);
            cast(&mut state, bolt, &[Target::Object(token_donor)]);
            settled(&mut state);
            assert_ne!(zone(&state, token_donor), Zone::Battlefield);
            assert_eq!(zone(&state, metamorph), Zone::Battlefield);

            let mut restored: GameState =
                serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
            act(&mut restored, Action::Pass);
        }

        // 8c (R4 Blocking Finding 3): inserting a forged `TokenCopyCreated`
        // receipt naming a real Springheart Nantuko as producer and a real
        // Phantasmal Image as donor onto an ordinary Saheeli Servo Token must
        // be refused.
        {
            let mut state = islands();
            state.active_player = P0;
            state.priority_player = P0;
            let _strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
            let saheeli = put(&mut state, P0, "Saheeli, Sublime Artificer", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
            cast(&mut state, saheeli, &[]);
            settled(&mut state);
            let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U], 1);
            cast(&mut state, image, &[]);
            settle_option(&mut state, 1);
            assert_eq!(state.objects.get(image).card_def, id("Baleful Strix"));
            let nantuko = put(&mut state, P0, "Springheart Nantuko", Zone::Battlefield);
            // Cast Sol Ring so Saheeli creates an ordinary Servo Token while
            // Springheart Nantuko and Phantasmal Image are already on play.
            let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
            pool(&mut state, P0, &[], 1);
            cast(&mut state, ring, &[]);
            settled(&mut state);
            let servo = state.players[0]
                .battlefield
                .iter()
                .copied()
                .find(|&o| state.objects.get(o).card_def == id("Servo Token"))
                .expect("ordinary Servo Token created by Saheeli");
            let nantuko_contract =
                mtg_kernel::state::AbilitySourceContractV4::capture(&state, nantuko);
            let image_link = serde_json::json!({
                "object": image,
                "zone_change_count": state.objects.get(image).zone_change_count,
            });
            let servo_create_at = state
                .engine
                .event_history
                .iter()
                .position(|ev| {
                    matches!(
                        ev,
                        mtg_kernel::event::CommittedEvent::CreateToken { object, .. }
                            if *object == servo
                    )
                })
                .unwrap();

            let metamorph = reanimate_copy(&mut state, P0);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(servo)),
            );
            settled(&mut state);
            assert_eq!(state.objects.get(metamorph).card_def, id("Servo Token"));

            let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::R], 0);
            cast(&mut state, bolt, &[Target::Object(servo)]);
            settled(&mut state);
            assert_ne!(zone(&state, servo), Zone::Battlefield);

            let mut forged_state = state.clone();
            let illusion_id = mtg_kernel::card_def::Subtype::Illusion.stable_id();
            let live_mm = forged_state.objects.get_mut(metamorph);
            live_mm.v4.effective_subtype_ids.push(illusion_id);
            live_mm.v4.effective_subtype_ids.sort_unstable();
            live_mm.v4.effective_subtype_ids.dedup();
            let metamorph_link = serde_json::json!({
                "object": metamorph,
                "zone_change_count": forged_state.objects.get(metamorph).zone_change_count,
            });
            let mut forged = serde_json::to_value(&forged_state).unwrap();
            let forged_mods = serde_json::json!({
                "added_subtypes": [illusion_id],
                "illusion_sacrifice": true,
            });
            let forged_vals = serde_json::json!({
                "card_def": id("Servo Token"),
                "face_index": 0,
                "mods": forged_mods,
                "copied_end_step_sacrifices_v1": 0,
                "copied_nightmare_v1": false,
                "prototyped": false,
            });
            mutate_both_receipts(&mut forged, |entry, _| {
                entry["copy"]["values"] = forged_vals.clone();
            });
            forged["eternal_v1"]["c8b"]["copy_mods"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!([metamorph_link, forged_mods]));
            let history = forged["engine"]["event_history"].as_array_mut().unwrap();
            let prep_idx = history
                .iter()
                .position(|ev| {
                    ev.get("MeddlingMageEntryV1")
                        .and_then(|e| e.get("journal"))
                        .and_then(|j| j.get("NoncastCopyPrepared"))
                        .is_some()
                })
                .unwrap();
            history.insert(
                prep_idx,
                serde_json::json!({
                    "CopyEntryProvenanceV1": {
                        "journal": {
                            "TokenCopyCreated": {
                                "token": { "object": servo, "zone_change_count": 0 },
                                "create_at": servo_create_at,
                                "producer": nantuko_contract,
                                "donor": image_link,
                                "values": forged_vals,
                            }
                        }
                    }
                }),
            );
            for ev in history.iter_mut() {
                if let Some(journal) = ev
                    .get_mut("MeddlingMageEntryV1")
                    .and_then(|e| e.get_mut("journal"))
                {
                    if let Some(prep) = journal.get_mut("NoncastCopyPrepared") {
                        let old_p = prep["copy"]["prepared_at"].as_u64().unwrap();
                        prep["copy"]["prepared_at"] = serde_json::json!(old_p + 1);
                    }
                    if let Some(copied) = journal.get_mut("MetamorphCopied") {
                        let old_e = copied["entry_history_index"].as_u64().unwrap();
                        copied["entry_history_index"] = serde_json::json!(old_e + 1);
                        let old_p = copied["copy"]["prepared_at"].as_u64().unwrap();
                        copied["copy"]["prepared_at"] = serde_json::json!(old_p + 1);
                    }
                }
            }
            assert_copy_restore_refused(forged, Action::Pass);
        }

        // 8d: Positive control for Springheart Nantuko bestowed onto a
        // Phantasmal Image copying Baleful Strix, creating an Illusion token
        // copy via Landfall; Metamorph copies that token copy and restores
        // cleanly after the token copy departs.
        {
            let mut state = islands();
            state.active_player = P0;
            state.priority_player = P0;
            let _orig_strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
            let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U], 1);
            cast(&mut state, image, &[]);
            settle_option(&mut state, 1);
            assert_eq!(state.objects.get(image).card_def, id("Baleful Strix"));

            // Bestow Springheart Nantuko onto Phantasmal Image, Stifling
            // Image's Illusion sacrifice trigger.
            let nantuko = put(&mut state, P0, "Springheart Nantuko", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::G], 1);
            next(&mut state);
            act(&mut state, Action::CastSpell(nantuko));
            assert!(matches!(next(&mut state), Decision::ChooseSpellMode { .. }));
            act(&mut state, Action::ChooseSpellMode(1));
            assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
            act(&mut state, Action::ChooseTarget(Target::Object(image)));
            next(&mut state);
            let sac_trigger = state.stack.last().unwrap().v4.stack_item_id;
            let stifle = put(&mut state, P0, "Stifle", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U], 0);
            cast(&mut state, stifle, &[Target::StackItem(sac_trigger)]);
            settled(&mut state);
            assert_eq!(zone(&state, nantuko), Zone::Battlefield);
            assert_eq!(zone(&state, image), Zone::Battlefield);

            // Trigger Landfall and pay {1}{G} (option 0) to copy Phantasmal Image.
            pool(&mut state, P0, &[ManaColor::G], 1);
            let forest = put(&mut state, P0, "Forest", Zone::Hand);
            next(&mut state);
            act(&mut state, Action::PlayLand(forest));
            settle_option(&mut state, 0);
            let token_donor = state.players[0]
                .battlefield
                .iter()
                .copied()
                .find(|&o| {
                    state.objects.get(o).v4.is_token
                        && state.objects.get(o).card_def == id("Baleful Strix")
                })
                .expect("Springheart Nantuko Illusion Baleful Strix token copy");

            let metamorph = reanimate_copy(&mut state, P0);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(token_donor)),
            );
            settled(&mut state);
            assert_eq!(state.objects.get(metamorph).card_def, id("Baleful Strix"));

            // Depart the token donor and verify the completed snapshot restores.
            let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::R], 0);
            cast(&mut state, bolt, &[Target::Object(token_donor)]);
            settled(&mut state);
            assert_ne!(zone(&state, token_donor), Zone::Battlefield);
            assert_eq!(zone(&state, metamorph), Zone::Battlefield);

            let mut restored: GameState =
                serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
            act(&mut restored, Action::Pass);
        }
    }
}

#[test]
fn noncast_metamorph_refuses_mdfc_bolt_land_back_face() {
    use mtg_kernel::event::{propose_and_commit, ProposedEvent};

    let mut state = islands();
    state.active_player = P0;
    state.priority_player = P0;
    let mdfc = put(&mut state, P1, "Sink into Stupor", Zone::Battlefield);
    propose_and_commit(&mut state, ProposedEvent::transform_in_place(mdfc, 1));
    assert_eq!(state.objects.get(mdfc).name, "Soporific Springs");
    let coating = put(&mut state, P0, "Liquimetal Coating", Zone::Battlefield);
    activate(&mut state, coating, 0, &[Target::Object(mdfc)]);
    settled(&mut state);

    let metamorph = reanimate_copy(&mut state, P0);
    act(&mut state, Action::ChooseEffectTarget(Target::Object(mdfc)));
    let _ = engine::advance_until_decision(&mut state);
    assert!(
        state.engine.halted.is_some(),
        "copying an MDFC bolt-land back face must refuse rather than entering untapped for free"
    );
    assert_eq!(zone(&state, metamorph), Zone::Graveyard);
}

#[test]
fn show_and_tell_dual_metamorph_defers_mox_discard_until_both_choices_complete() {
    let mut state = islands();
    state.active_player = P0;
    state.priority_player = P0;
    state.public_shroud_v1 = true;
    let mox = put(&mut state, P0, "Mox Diamond", Zone::Battlefield);
    settle_first(&mut state);
    let needle = put(&mut state, P1, "Pithing Needle", Zone::Battlefield);
    settle_first(&mut state);

    let taiga = put(&mut state, P0, "Taiga", Zone::Hand);
    let p0_metamorph = put(&mut state, P0, "Phyrexian Metamorph", Zone::Hand);
    let p1_metamorph = put(&mut state, P1, "Phyrexian Metamorph", Zone::Hand);
    let show = put(&mut state, P0, "Show and Tell", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, show, &[]);

    assert!(settle(&mut state).is_some());
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(p0_metamorph)),
    );
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(p1_metamorph)),
    );
    // P0 chooses to copy Mox Diamond and selects Taiga to discard.
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectTargets { player, source, .. }
            if player == P0 && source == p0_metamorph
    ));
    act(&mut state, Action::ChooseEffectTarget(Target::Object(mox)));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption { player, source, .. }
            if player == P0 && source == p0_metamorph
    ));
    act(&mut state, Action::ChooseEffectOption(1));

    // CR 614.12: P0's Taiga must still be in P0's hand while P1 chooses what to
    // copy and answers Pithing Needle's name menu.
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectTargets { player, source, .. }
            if player == P1 && source == p1_metamorph
    ));
    assert_eq!(zone(&state, taiga), Zone::Hand);
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(needle)),
    );
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption { player, source, .. }
            if player == P1 && source == p1_metamorph
    ));
    assert_eq!(zone(&state, taiga), Zone::Hand);
    let needle_menu = shroud_copy_menu(&state, P1);
    let option_text = needle_menu["option_text"].as_array().unwrap();
    assert!(
        !option_text.iter().any(|name| name == "Taiga"),
        "P1 must not see P0's deferred Mox Diamond discard in Pithing Needle's menu"
    );
    act(&mut state, Action::ChooseEffectOption(0));
    settled(&mut state);

    assert_eq!(zone(&state, taiga), Zone::Graveyard);
    assert_eq!(zone(&state, p0_metamorph), Zone::Battlefield);
    assert_eq!(zone(&state, p1_metamorph), Zone::Battlefield);
    assert_eq!(state.objects.get(p0_metamorph).card_def, id("Mox Diamond"));
    assert_eq!(
        state.objects.get(p1_metamorph).card_def,
        id("Pithing Needle")
    );
    let mut restored: GameState =
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    act(&mut restored, Action::Pass);
}
