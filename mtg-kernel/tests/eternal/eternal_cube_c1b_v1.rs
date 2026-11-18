//! Vintage Cube wave C1b: native rules and public-session tests for the
//! black and green instants and sorceries.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{in_parallel, play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C1B_CARDS: [&str; 16] = [
    "Damnation",
    "Entomb",
    "Hymn to Tourach",
    "Imperial Seal",
    "Infernal Grasp",
    "Mind Twist",
    "Night's Whisper",
    "Mutagenic Growth",
    "Bloodchief's Thirst",
    "Bone Shards",
    "Chain of Smog",
    "Collective Brutality",
    "Natural Order",
    "Pest Infestation",
    "Tamiyo's Safekeeping",
    "Cathar Commando",
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
        0x4355_4245_0000 + 0x3162,
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
fn c1b_cards_are_full() {
    for name in C1B_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c1b_cards_have_their_printed_characteristics() {
    let black = Pip::Colored(ManaColor::B);
    for (name, generic, x_count, pips, card_type) in [
        ("Damnation", 2, 0, vec![black, black], CardType::Sorcery),
        ("Entomb", 0, 0, vec![black], CardType::Instant),
        (
            "Hymn to Tourach",
            0,
            0,
            vec![black, black],
            CardType::Sorcery,
        ),
        ("Imperial Seal", 0, 0, vec![black], CardType::Sorcery),
        ("Infernal Grasp", 1, 0, vec![black], CardType::Instant),
        ("Mind Twist", 0, 1, vec![black], CardType::Sorcery),
        ("Night's Whisper", 1, 0, vec![black], CardType::Sorcery),
        (
            "Mutagenic Growth",
            0,
            0,
            vec![Pip::Phyrexian(ManaColor::G)],
            CardType::Instant,
        ),
    ] {
        let card = def(name);
        assert_eq!(card.cost.generic, generic, "{name}");
        assert_eq!(card.cost.x_count, x_count, "{name}");
        assert_eq!(card.cost.pips, pips.as_slice(), "{name}");
        assert_eq!(card.types, &[card_type], "{name}");
        assert!(card.subtypes.is_empty(), "{name}");
        assert!(!card.is_token, "{name}");
    }
    assert_eq!(def("Mutagenic Growth").colors, &[ManaColor::G]);
    for name in C1B_CARDS[..8]
        .iter()
        .filter(|name| **name != "Mutagenic Growth")
    {
        assert_eq!(def(name).colors, &[ManaColor::B], "{name}");
    }
}

#[test]
fn c1b_second_half_has_its_printed_characteristics() {
    let black = Pip::Colored(ManaColor::B);
    let green = Pip::Colored(ManaColor::G);
    for (name, generic, x_count, pips, card_type, color) in [
        (
            "Bloodchief's Thirst",
            0,
            0,
            vec![black],
            CardType::Sorcery,
            ManaColor::B,
        ),
        (
            "Bone Shards",
            0,
            0,
            vec![black],
            CardType::Sorcery,
            ManaColor::B,
        ),
        (
            "Chain of Smog",
            1,
            0,
            vec![black],
            CardType::Sorcery,
            ManaColor::B,
        ),
        (
            "Collective Brutality",
            1,
            0,
            vec![black],
            CardType::Sorcery,
            ManaColor::B,
        ),
        (
            "Natural Order",
            2,
            0,
            vec![green, green],
            CardType::Sorcery,
            ManaColor::G,
        ),
        (
            "Pest Infestation",
            0,
            2,
            vec![green],
            CardType::Sorcery,
            ManaColor::G,
        ),
        (
            "Tamiyo's Safekeeping",
            0,
            0,
            vec![green],
            CardType::Instant,
            ManaColor::G,
        ),
    ] {
        let card = def(name);
        assert_eq!(card.cost.generic, generic, "{name}");
        assert_eq!(card.cost.x_count, x_count, "{name}");
        assert_eq!(card.cost.pips, pips.as_slice(), "{name}");
        assert_eq!(card.types, &[card_type], "{name}");
        assert_eq!(card.colors, &[color], "{name}");
        assert!(card.subtypes.is_empty(), "{name}");
        assert!(!card.is_token, "{name}");
    }
    assert!(def("Bloodchief's Thirst").kicker_cost.is_some());
    // Its two additional costs are its two modes (#715), not an alternative cost.
    let shards = def("Bone Shards");
    assert!(shards.additional_cost.is_some());
    assert!(shards.alt_cost.is_none());
    assert!(shards.mode2.is_some() && shards.mode3.is_none());
    assert_eq!(
        shards.mode2.as_ref().map(|mode| mode.target_spec),
        Some(shards.target_spec)
    );
    assert!(def("Natural Order").additional_cost.is_some());
    let brutality = def("Collective Brutality");
    assert!(brutality.additional_cost.is_some());
    assert!(brutality.mode2.is_some() && brutality.mode3.is_some());
    assert_eq!(brutality.additional_modes.len(), 4, "seven modes in all");
    let pest = def("Pest Token");
    assert!(pest.is_token);
    assert_eq!((pest.power, pest.toughness), (Some(1), Some(1)));
}

fn swamps() -> GameState {
    game(&["Swamp"; 20])
}

/// With P0's spell on the stack, P0 passes and P1 answers with a Lightning
/// Bolt at `target`, which resolves first.
fn opponent_responds_with_bolt(state: &mut GameState, target: Target) -> ObjectId {
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    next(state);
    act(state, Action::Pass);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    cast(state, bolt, &[target]);
    bolt
}

// ---- Damnation -------------------------------------------------------------------

#[test]
fn damnation_destroys_every_creature_even_a_regenerating_or_token_one() {
    let mut state = swamps();
    let troll = put(&mut state, P0, "Golgari Grave-Troll", Zone::Battlefield);
    state.objects.get_mut(troll).counters.plus1_plus1 = 2;
    // A regeneration shield first: "They can't be regenerated."
    pool(&mut state, P0, &[], 1);
    activate(&mut state, troll, 0, &[]);
    settled(&mut state);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let spawn = token(&mut state, P1, "Thraben Inspector");
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let damnation = put(&mut state, P0, "Damnation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    cast(&mut state, damnation, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, troll), Zone::Graveyard, "can't be regenerated");
    assert_eq!(zone(&state, mine), Zone::Graveyard);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert!(!state.players[1].battlefield.contains(&spawn));
    assert_eq!(zone(&state, ring), Zone::Battlefield, "only creatures die");
    assert_eq!(zone(&state, damnation), Zone::Graveyard);
}

#[test]
fn damnation_leaves_an_indestructible_creature_and_works_on_an_empty_board() {
    let mut state = swamps();
    let lage = token(&mut state, P1, "Marit Lage Token");
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let damnation = put(&mut state, P0, "Damnation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    cast(&mut state, damnation, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, lage), Zone::Battlefield, "indestructible");
    assert_eq!(zone(&state, elves), Zone::Graveyard);

    let mut state = swamps();
    let damnation = put(&mut state, P0, "Damnation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    cast(&mut state, damnation, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, damnation), Zone::Graveyard);
}

// ---- Entomb ----------------------------------------------------------------------

#[test]
fn entomb_puts_the_chosen_card_into_the_graveyard_at_instant_speed() {
    let mut state = swamps();
    let wanted = put(&mut state, P0, "Dark Ritual", Zone::Library);
    let library = state.players[0].library.len();
    let entomb = put(&mut state, P0, "Entomb", Zone::Hand);
    // In response to an opponent's spell.
    opponent_bolts(&mut state, Target::Player(P0));
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, entomb, &[]);
    settle_choosing(&mut state, wanted);
    assert_eq!(zone(&state, wanted), Zone::Graveyard);
    assert_eq!(state.players[0].library.len(), library - 1);
    assert!(state.players[0].graveyard.contains(&wanted));
    assert_eq!(life(&state, P0), 17, "the bolt still resolved");
    assert_eq!(zone(&state, entomb), Zone::Graveyard);
}

/// The search prompt of a resolving "search your library for a card": a
/// nonempty library always has a card to find (701.23d), so it cannot finish
/// before one is chosen.
fn assert_must_find_a_card(state: &mut GameState) {
    match settle(state) {
        Some(Decision::ChooseEffectTargets {
            player,
            min_targets,
            can_finish,
            legal_targets,
            ..
        }) => {
            assert_eq!(player, P0);
            assert_eq!(min_targets, 1);
            assert!(!can_finish, "a search for a card must find one");
            assert_eq!(legal_targets.len(), state.players[0].library.len());
        }
        other => panic!("expected the search, got {other:?}"),
    }
    assert!(
        engine::step(state, Action::FinishEffectSelection).is_err(),
        "finishing with nothing found is refused"
    );
}

#[test]
fn entomb_must_find_a_card_in_a_nonempty_library_and_finds_nothing_in_an_empty_one() {
    let mut state = swamps();
    let library = state.players[0].library.len();
    let entomb = put(&mut state, P0, "Entomb", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, entomb, &[]);
    assert_must_find_a_card(&mut state);
    settle_first(&mut state);
    assert_eq!(state.players[0].library.len(), library - 1);
    assert_eq!(
        state.players[0].graveyard.len(),
        2,
        "Entomb and the found card"
    );

    // An empty library has nothing to find.
    let mut state = game(&[]);
    let entomb = put(&mut state, P0, "Entomb", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, entomb, &[]);
    settle_first(&mut state);
    assert_eq!(
        state.players[0].graveyard,
        vec![entomb],
        "only Entomb itself"
    );
}

// ---- Hymn to Tourach and Mind Twist ----------------------------------------------

const HAND: [&str; 5] = [
    "Island",
    "Swamp",
    "Dark Ritual",
    "Llanowar Elves",
    "Sol Ring",
];

fn stock_hand(state: &mut GameState, player: PlayerId, names: &[&str]) -> Vec<ObjectId> {
    names
        .iter()
        .map(|name| put(state, player, name, Zone::Hand))
        .collect()
}

/// The cards of `before` now in `player`'s graveyard.
fn discarded(state: &GameState, player: PlayerId, before: &[ObjectId]) -> Vec<ObjectId> {
    before
        .iter()
        .copied()
        .filter(|&card| zone(state, card) == Zone::Graveyard)
        .inspect(|card| assert!(state.players[player.index()].graveyard.contains(card)))
        .collect()
}

#[test]
fn hymn_to_tourach_discards_two_cards_at_random_from_the_target_player() {
    let mut state = swamps();
    let cards = stock_hand(&mut state, P1, &HAND);
    let own = put(&mut state, P0, "Island", Zone::Hand);
    let hymn = put(&mut state, P0, "Hymn to Tourach", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    let before = state.clone();
    cast(&mut state, hymn, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].hand.len(), 3);
    assert_eq!(discarded(&state, P1, &cards).len(), 2);
    assert!(
        hand_contains(&state, P0, own),
        "the caster discards nothing"
    );
    assert_eq!(state.players[0].hand.len(), 1);

    // The same position and seed discard the same two cards.
    let mut again = before;
    cast(&mut again, hymn, &[Target::Player(P1)]);
    settled(&mut again);
    assert_eq!(discarded(&state, P1, &cards), discarded(&again, P1, &cards));
}

#[test]
fn hymn_to_tourach_takes_a_short_hand_whole_and_an_empty_hand_does_nothing() {
    let mut state = swamps();
    let cards = stock_hand(&mut state, P1, &HAND[..1]);
    let hymn = put(&mut state, P0, "Hymn to Tourach", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    cast(&mut state, hymn, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, cards[0]), Zone::Graveyard);
    assert!(state.players[1].hand.is_empty());

    let mut state = swamps();
    let hymn = put(&mut state, P0, "Hymn to Tourach", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    cast(&mut state, hymn, &[Target::Player(P1)]);
    settled(&mut state);
    assert!(state.players[1].graveyard.is_empty());
    assert_eq!(zone(&state, hymn), Zone::Graveyard);
}

#[test]
fn hymn_to_tourach_may_target_its_caster_and_fizzles_against_a_hexproof_player() {
    let mut state = swamps();
    let cards = stock_hand(&mut state, P0, &HAND[..3]);
    let hymn = put(&mut state, P0, "Hymn to Tourach", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    cast(&mut state, hymn, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(discarded(&state, P0, &cards).len(), 2);

    // The target becomes illegal before resolution (608.2b).
    let mut state = swamps();
    let cards = stock_hand(&mut state, P1, &HAND);
    let hymn = put(&mut state, P0, "Hymn to Tourach", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    cast(&mut state, hymn, &[Target::Player(P1)]);
    // The cast completes, then the target becomes illegal.
    next(&mut state);
    put(&mut state, P1, "Leyline of Sanctity", Zone::Battlefield);
    settled(&mut state);
    assert_eq!(state.players[1].hand.len(), HAND.len());
    assert!(discarded(&state, P1, &cards).is_empty());
    assert_eq!(zone(&state, hymn), Zone::Graveyard);
}

/// Announces Mind Twist at `target` with X = `x`, leaving it on the stack.
fn announce_mind_twist(state: &mut GameState, twist: ObjectId, target: PlayerId, x: u16) {
    assert!(castable(state, twist));
    act(state, Action::CastSpell(twist));
    for _ in 0..10 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Player(target)));
                act(state, Action::ChooseTarget(Target::Player(target)));
            }
            Decision::ChooseEffectOption { option_count, .. } => {
                assert!(option_count > x, "X = {x} of {option_count} options");
                act(state, Action::ChooseEffectOption(x));
            }
            Decision::CastSpellOrPass { .. } => return,
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    panic!("Mind Twist was never announced");
}

#[test]
fn mind_twist_discards_x_cards_at_random_for_every_x() {
    // X = 0 pays only {B} and discards nothing.
    let mut state = swamps();
    let cards = stock_hand(&mut state, P1, &HAND);
    let twist = put(&mut state, P0, "Mind Twist", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 3);
    announce_mind_twist(&mut state, twist, P1, 0);
    settled(&mut state);
    assert!(discarded(&state, P1, &cards).is_empty());
    assert_eq!(zone(&state, twist), Zone::Graveyard);
    assert_eq!(mana(&state, P0, ManaColor::C), 3, "X = 0 spent no generic");

    // X = 2 spends two more mana and discards two.
    let mut state = swamps();
    let cards = stock_hand(&mut state, P1, &HAND);
    let twist = put(&mut state, P0, "Mind Twist", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 3);
    announce_mind_twist(&mut state, twist, P1, 2);
    settled(&mut state);
    assert_eq!(discarded(&state, P1, &cards).len(), 2);
    assert_eq!(state.players[1].hand.len(), 3);
    assert_eq!(mana(&state, P0, ManaColor::C), 1);

    // X larger than the hand discards the whole hand.
    let mut state = swamps();
    let cards = stock_hand(&mut state, P1, &HAND[..2]);
    let twist = put(&mut state, P0, "Mind Twist", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 4);
    announce_mind_twist(&mut state, twist, P1, 4);
    settled(&mut state);
    assert_eq!(discarded(&state, P1, &cards).len(), 2);
    assert!(state.players[1].hand.is_empty());

    // X = 5 empties a five-card hand.
    let mut state = swamps();
    let cards = stock_hand(&mut state, P1, &HAND);
    let twist = put(&mut state, P0, "Mind Twist", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 5);
    announce_mind_twist(&mut state, twist, P1, 5);
    settled(&mut state);
    assert_eq!(discarded(&state, P1, &cards).len(), 5);
}

#[test]
fn mind_twist_is_a_sorcery_and_fizzles_when_its_target_becomes_illegal() {
    let mut state = swamps();
    let twist = put(&mut state, P0, "Mind Twist", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    opponent_bolts(&mut state, Target::Player(P0));
    assert!(
        !castable(&mut state, twist),
        "a sorcery, with a spell on the stack"
    );

    let mut state = swamps();
    let cards = stock_hand(&mut state, P1, &HAND);
    let twist = put(&mut state, P0, "Mind Twist", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    announce_mind_twist(&mut state, twist, P1, 2);
    put(&mut state, P1, "Leyline of Sanctity", Zone::Battlefield);
    settled(&mut state);
    assert!(discarded(&state, P1, &cards).is_empty());
    assert_eq!(zone(&state, twist), Zone::Graveyard);
}

// ---- Imperial Seal ---------------------------------------------------------------

#[test]
fn imperial_seal_puts_the_chosen_card_on_top_and_costs_two_life() {
    let mut state = swamps();
    let wanted = put(&mut state, P0, "Dark Ritual", Zone::Library);
    // Buried under ten other cards first.
    state.players[0].library.retain(|&card| card != wanted);
    state.players[0].library.insert(10, wanted);
    let library = state.players[0].library.len();
    let seal = put(&mut state, P0, "Imperial Seal", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, seal, &[]);
    settle_choosing(&mut state, wanted);
    assert_eq!(state.players[0].library[0], wanted);
    assert_eq!(state.players[0].library.len(), library);
    assert_eq!(life(&state, P0), 18);
    assert!(state
        .known_library_cards(P0, P0)
        .iter()
        .any(|entry| entry.object == wanted && entry.position == 0));
    assert!(!state
        .known_library_cards(P1, P0)
        .iter()
        .any(|entry| entry.object == wanted));
}

#[test]
fn imperial_seal_must_find_a_card_costs_life_with_nothing_to_find_and_is_a_sorcery() {
    let mut state = swamps();
    let seal = put(&mut state, P0, "Imperial Seal", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, seal, &[]);
    assert_must_find_a_card(&mut state);
    settle_first(&mut state);
    assert_eq!(life(&state, P0), 18);
    assert_eq!(zone(&state, seal), Zone::Graveyard);

    // An empty library has nothing to find; the life is still paid.
    let mut state = game(&[]);
    let seal = put(&mut state, P0, "Imperial Seal", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, seal, &[]);
    settle_first(&mut state);
    assert_eq!(life(&state, P0), 18);
    assert_eq!(zone(&state, seal), Zone::Graveyard);

    let mut state = swamps();
    let seal = put(&mut state, P0, "Imperial Seal", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    opponent_bolts(&mut state, Target::Player(P1));
    assert!(
        !castable(&mut state, seal),
        "a sorcery, with a spell on the stack"
    );
}

// ---- Infernal Grasp --------------------------------------------------------------

#[test]
fn infernal_grasp_destroys_a_creature_and_costs_two_life() {
    let mut state = swamps();
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let grasp = put(&mut state, P0, "Infernal Grasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    let legal = cast_targets(&state, grasp);
    assert!(legal.contains(&Target::Object(theirs)));
    assert!(legal.contains(&Target::Object(mine)));
    assert!(!legal.contains(&Target::Player(P1)), "only creatures");
    cast(&mut state, grasp, &[Target::Object(theirs)]);
    settled(&mut state);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(life(&state, P0), 18);
    assert_eq!(life(&state, P1), 20);
}

#[test]
fn infernal_grasp_still_costs_life_against_an_indestructible_creature() {
    let mut state = swamps();
    let lage = token(&mut state, P1, "Marit Lage Token");
    let grasp = put(&mut state, P0, "Infernal Grasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, grasp, &[Target::Object(lage)]);
    settled(&mut state);
    assert_eq!(zone(&state, lage), Zone::Battlefield);
    assert_eq!(life(&state, P0), 18);
}

#[test]
fn infernal_grasp_fizzles_without_its_life_loss_when_the_target_is_gone() {
    let mut state = swamps();
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let grasp = put(&mut state, P0, "Infernal Grasp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, grasp, &[Target::Object(theirs)]);
    opponent_responds_with_bolt(&mut state, Target::Object(theirs));
    settled(&mut state);
    assert_eq!(zone(&state, theirs), Zone::Graveyard, "the bolt killed it");
    assert_eq!(zone(&state, grasp), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20, "a fizzled spell loses no life");
}

// ---- Night's Whisper -------------------------------------------------------------

#[test]
fn nights_whisper_draws_two_cards_and_loses_two_life() {
    let mut state = game(&["Island", "Swamp", "Dark Ritual", "Plains"]);
    let top = state.players[0].library[0];
    let second = state.players[0].library[1];
    let whisper = put(&mut state, P0, "Night's Whisper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, whisper, &[]);
    settled(&mut state);
    assert!(hand_contains(&state, P0, top));
    assert!(hand_contains(&state, P0, second));
    assert_eq!(state.players[0].hand.len(), 2);
    assert_eq!(state.players[0].library.len(), 2);
    assert_eq!(life(&state, P0), 18);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(zone(&state, whisper), Zone::Graveyard);
}

#[test]
fn nights_whisper_draws_are_replaceable_by_dredge() {
    let mut state = swamps();
    let thug = put(&mut state, P0, "Golgari Thug", Zone::Graveyard);
    let library = state.players[0].library.len();
    let whisper = put(&mut state, P0, "Night's Whisper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, whisper, &[]);
    let mut dredged = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.contains(&Target::Object(thug)) && dredged == 0 => {
            dredged += 1;
            Action::ChooseEffectTarget(Target::Object(thug))
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if legal_targets.contains(&Target::Object(thug)) => Action::FinishEffectSelection,
        Decision::ChooseEffectTargets {
            selected_count: 1, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(dredged, 1);
    assert!(
        hand_contains(&state, P0, thug),
        "dredged back instead of a draw"
    );
    // Four cards milled for dredge 4, one drawn normally.
    assert_eq!(state.players[0].library.len(), library - 4 - 1);
    assert_eq!(state.players[0].hand.len(), 2);
    assert_eq!(life(&state, P0), 18);
}

// ---- Mutagenic Growth ------------------------------------------------------------

#[test]
fn mutagenic_growth_gives_plus_two_plus_two_until_end_of_turn_paid_with_green() {
    let mut state = swamps();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let growth = put(&mut state, P0, "Mutagenic Growth", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    assert_eq!(power_toughness(&state, inspector), (1, 2));
    cast(&mut state, growth, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, inspector), (3, 4));
    assert_eq!(life(&state, P0), 20, "paid with green mana, not life");
    assert_eq!(zone(&state, growth), Zone::Graveyard);
    // It lasts only until end of turn.
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(power_toughness(&state, inspector), (1, 2));
}

#[test]
fn mutagenic_growth_can_be_cast_for_two_life_and_saves_a_creature_from_a_bolt() {
    let mut state = swamps();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let growth = put(&mut state, P0, "Mutagenic Growth", Zone::Hand);
    // No mana at all: {G/P} is paid with 2 life.
    opponent_bolts(&mut state, Target::Object(inspector));
    cast(&mut state, growth, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 18, "two life for the Phyrexian symbol");
    assert_eq!(zone(&state, inspector), Zone::Battlefield);
    assert_eq!(state.objects.get(inspector).damage, 3);
    assert_eq!(power_toughness(&state, inspector), (3, 4));
}

#[test]
fn mutagenic_growth_fizzles_when_its_target_dies_first() {
    let mut state = swamps();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let growth = put(&mut state, P0, "Mutagenic Growth", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, growth, &[Target::Object(inspector)]);
    opponent_responds_with_bolt(&mut state, Target::Object(inspector));
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(zone(&state, growth), Zone::Graveyard);
}

// ---- Wave C1b, second half: shared helpers ----------------------------------------

fn forests() -> GameState {
    game(&["Forest"; 20])
}

/// What a test answers when a spell asks its caster or a resolving effect
/// asks a player. Every field is optional; an unanswered prompt takes its
/// first legal answer.
#[derive(Default, Clone)]
struct Plan {
    /// Spell targets, matched to each prompt by legality.
    targets: Vec<Target>,
    /// Resolution selections (searches, revealed cards, a copy's target).
    picks: Vec<Target>,
    kicked: bool,
    alternative: bool,
    mode: Option<u8>,
    x: Option<u16>,
    sacrifice: Option<ObjectId>,
    discard: Vec<ObjectId>,
}

/// The first of `wanted` that `legal` offers, removed from `wanted`.
fn take_legal(wanted: &mut Vec<Target>, legal: &[Target]) -> Option<Target> {
    let index = wanted.iter().position(|target| legal.contains(target))?;
    Some(wanted.remove(index))
}

/// Settles after an announced cast, answering every prompt from `plan`, and
/// returns the kind of each prompt asked, in order.
fn run_plan(state: &mut GameState, plan: &Plan) -> Vec<String> {
    let mut targets = plan.targets.clone();
    let mut picks = plan.picks.clone();
    let mut log = Vec::new();
    settle_with(state, |_, decision| {
        log.push(
            format!("{decision:?}")
                .split(|c: char| !c.is_alphanumeric())
                .next()
                .unwrap_or_default()
                .to_owned(),
        );
        match decision {
            Decision::ChooseKicker { .. } => Action::ChooseKicker(plan.kicked),
            Decision::ChooseCastMode { options, .. } => {
                let wanted = if plan.alternative {
                    engine::CastMode::Alternative
                } else {
                    engine::CastMode::Normal
                };
                assert!(options.contains(&wanted), "{wanted:?} not in {options:?}");
                Action::ChooseCastMode(wanted)
            }
            Decision::ChooseSpellMode { legal_modes, .. } => {
                let mode = plan.mode.expect("a mode to choose");
                assert!(legal_modes.contains(&mode), "{mode} not in {legal_modes:?}");
                Action::ChooseSpellMode(mode)
            }
            Decision::ChooseTargets {
                legal_targets,
                can_finish,
                ..
            } => match take_legal(&mut targets, &legal_targets) {
                Some(target) => Action::ChooseTarget(target),
                None if can_finish => Action::FinishEffectSelection,
                None => Action::ChooseTarget(legal_targets[0]),
            },
            Decision::ChooseEffectTargets {
                selected_count,
                legal_targets,
                can_finish,
                ..
            } => {
                if selected_count > 0 && can_finish {
                    Action::FinishEffectSelection
                } else if let Some(pick) = take_legal(&mut picks, &legal_targets) {
                    Action::ChooseEffectTarget(pick)
                } else if can_finish {
                    Action::FinishEffectSelection
                } else {
                    Action::ChooseEffectTarget(legal_targets[0])
                }
            }
            Decision::ChooseEffectOption { .. } => match plan.x {
                Some(x) => {
                    let minimum = plan
                        .targets
                        .iter()
                        .filter(|target| matches!(target, Target::Object(_)))
                        .count() as u16;
                    Action::ChooseEffectOption(x - minimum)
                }
                None => Action::ChooseEffectOption(0),
            },
            Decision::ChooseCostTargets { candidates, .. } => match plan.sacrifice {
                Some(object) => {
                    assert!(
                        candidates.contains(&object),
                        "{object:?} not in {candidates:?}"
                    );
                    Action::ChooseCostTarget(object)
                }
                None => Action::ChooseCostTarget(candidates[0]),
            },
            Decision::Discard { count, choices, .. } => {
                let chosen: Vec<ObjectId> = plan
                    .discard
                    .iter()
                    .copied()
                    .filter(|object| choices.contains(object))
                    .take(count as usize)
                    .collect();
                if chosen.len() == count as usize {
                    Action::Discard(chosen)
                } else {
                    Action::Discard(choices.into_iter().take(count as usize).collect())
                }
            }
            other => first_answer(other),
        }
    });
    log
}

/// Announces `spell` and plays out `plan`.
fn cast_plan(state: &mut GameState, spell: ObjectId, plan: &Plan) -> Vec<String> {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    run_plan(state, plan)
}

/// The targets `spell` may choose first, answering a kicker prompt with
/// `kicked`, and whether the kicker was offered.
fn kicker_targets(state: &GameState, spell: ObjectId, kicked: bool) -> (Vec<Target>, bool) {
    let mut probe = state.clone();
    assert!(castable(&mut probe, spell));
    act(&mut probe, Action::CastSpell(spell));
    let mut offered = false;
    for _ in 0..5 {
        match next(&mut probe) {
            Decision::ChooseKicker { .. } => {
                offered = true;
                act(&mut probe, Action::ChooseKicker(kicked));
            }
            Decision::ChooseTargets { legal_targets, .. } => return (legal_targets, offered),
            other => panic!("expected targets, got {other:?}"),
        }
    }
    panic!("no target prompt");
}

/// Sends `object` to its owner's graveyard, as if something removed it.
fn remove(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

fn tokens_named(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    named(state, player, name)
}

// ---- Bloodchief's Thirst ---------------------------------------------------------

#[test]
fn bloodchiefs_thirst_unkicked_destroys_a_creature_with_mana_value_two_or_less() {
    let mut state = swamps();
    let small = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let big = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let thirst = put(&mut state, P0, "Bloodchief's Thirst", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    // With only {B}, the kicker is not offered and only the small one is legal.
    let (legal, offered) = kicker_targets(&state, thirst, false);
    assert!(!offered, "{{2}}{{B}} more is unaffordable");
    assert!(legal.contains(&Target::Object(small)));
    assert!(!legal.contains(&Target::Object(big)), "mana value 3");
    assert!(!legal.contains(&Target::Player(P1)));
    cast_plan(
        &mut state,
        thirst,
        &Plan {
            targets: vec![Target::Object(small)],
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, small), Zone::Graveyard);
    assert_eq!(zone(&state, big), Zone::Battlefield);
    assert_eq!(zone(&state, thirst), Zone::Graveyard);
}

#[test]
fn bloodchiefs_thirst_cannot_be_cast_unkicked_with_only_a_big_creature_to_target() {
    let mut state = swamps();
    put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let thirst = put(&mut state, P0, "Bloodchief's Thirst", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    assert!(!castable(&mut state, thirst));
    // Kicked ({2}{B} more), the same creature is a legal target.
    pool(&mut state, P0, &[ManaColor::B], 2);
    assert!(castable(&mut state, thirst));
}

#[test]
fn bloodchiefs_thirst_kicked_destroys_any_creature_instead() {
    let mut state = swamps();
    let small = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let big = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let thirst = put(&mut state, P0, "Bloodchief's Thirst", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    let (legal, offered) = kicker_targets(&state, thirst, true);
    assert!(offered);
    assert!(legal.contains(&Target::Object(small)));
    assert!(legal.contains(&Target::Object(big)));
    let (declined, _) = kicker_targets(&state, thirst, false);
    assert!(
        !declined.contains(&Target::Object(big)),
        "declined: back to 2 or less"
    );
    cast_plan(
        &mut state,
        thirst,
        &Plan {
            targets: vec![Target::Object(big)],
            kicked: true,
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, big), Zone::Graveyard);
    assert_eq!(zone(&state, small), Zone::Battlefield);
    assert_eq!(
        mana(&state, P0, ManaColor::C),
        0,
        "{{2}}{{B}} more was paid"
    );
}

#[test]
fn bloodchiefs_thirst_hits_planeswalkers_by_mana_value_too() {
    let mut state = game(&["Forest"; 20]);
    let wrenn = put(&mut state, P0, "Wrenn and Six", Zone::Hand);
    let dack = put(&mut state, P0, "Dack Fayden", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
    cast(&mut state, wrenn, &[]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    cast(&mut state, dack, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, wrenn), Zone::Battlefield);
    assert_eq!(zone(&state, dack), Zone::Battlefield);
    let thirst = put(&mut state, P0, "Bloodchief's Thirst", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    let (legal, _) = kicker_targets(&state, thirst, false);
    assert!(legal.contains(&Target::Object(wrenn)), "mana value 2");
    assert!(!legal.contains(&Target::Object(dack)), "mana value 3");
    cast_plan(
        &mut state,
        thirst,
        &Plan {
            targets: vec![Target::Object(wrenn)],
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, wrenn), Zone::Graveyard);

    // Kicked, the mana value 3 planeswalker dies too.
    let thirst = put(&mut state, P0, "Bloodchief's Thirst", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    cast_plan(
        &mut state,
        thirst,
        &Plan {
            targets: vec![Target::Object(dack)],
            kicked: true,
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, dack), Zone::Graveyard);
}

#[test]
fn bloodchiefs_thirst_fizzles_when_its_target_is_gone_and_is_a_sorcery() {
    let mut state = swamps();
    let small = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let thirst = put(&mut state, P0, "Bloodchief's Thirst", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, thirst, &[Target::Object(small)]);
    opponent_responds_with_bolt(&mut state, Target::Object(small));
    settled(&mut state);
    assert_eq!(zone(&state, small), Zone::Graveyard, "the bolt killed it");
    assert_eq!(zone(&state, thirst), Zone::Graveyard);

    let mut state = swamps();
    put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let thirst = put(&mut state, P0, "Bloodchief's Thirst", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    opponent_bolts(&mut state, Target::Player(P0));
    assert!(
        !castable(&mut state, thirst),
        "a sorcery, with a spell on the stack"
    );
}

// ---- Bone Shards -----------------------------------------------------------------

#[test]
fn bone_shards_discards_a_card_and_destroys_a_creature() {
    let mut state = swamps();
    let big = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let spare = put(&mut state, P0, "Island", Zone::Hand);
    let other = put(&mut state, P0, "Sol Ring", Zone::Hand);
    let shards = put(&mut state, P0, "Bone Shards", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast_plan(
        &mut state,
        shards,
        &Plan {
            targets: vec![Target::Object(big)],
            discard: vec![spare],
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, big), Zone::Graveyard, "any mana value");
    assert_eq!(zone(&state, spare), Zone::Graveyard, "the chosen discard");
    assert_eq!(zone(&state, other), Zone::Hand);
    assert_eq!(zone(&state, shards), Zone::Graveyard);
}

#[test]
fn bone_shards_may_sacrifice_a_creature_instead_of_discarding() {
    let mut state = swamps();
    let big = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let keep = put(&mut state, P0, "Island", Zone::Hand);
    let shards = put(&mut state, P0, "Bone Shards", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    // Both costs are payable, so the caster is asked which one (mode 0 is
    // the sacrifice, mode 1 the discard), before targets and payment.
    let log = cast_plan(
        &mut state,
        shards,
        &Plan {
            targets: vec![Target::Object(big)],
            mode: Some(0),
            sacrifice: Some(elves),
            ..Plan::default()
        },
    );
    assert_eq!(log.first().map(String::as_str), Some("ChooseSpellMode"));
    assert!(!log.contains(&"ChooseCastMode".to_owned()), "{log:?}");
    assert!(
        !log.contains(&"Discard".to_owned()),
        "the sacrifice is paid instead of the discard"
    );
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, keep), Zone::Hand);
    assert_eq!(zone(&state, big), Zone::Graveyard);

    // The other answer discards and keeps the creature. With two other
    // cards in hand the discard is a choice of its own.
    let mut state = swamps();
    let big = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let spare = put(&mut state, P0, "Island", Zone::Hand);
    let other = put(&mut state, P0, "Sol Ring", Zone::Hand);
    let shards = put(&mut state, P0, "Bone Shards", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    let log = cast_plan(
        &mut state,
        shards,
        &Plan {
            targets: vec![Target::Object(big)],
            mode: Some(1),
            discard: vec![spare],
            ..Plan::default()
        },
    );
    assert!(log.contains(&"Discard".to_owned()), "{log:?}");
    assert!(!log.contains(&"ChooseCostTargets".to_owned()), "{log:?}");
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, spare), Zone::Graveyard);
    assert_eq!(zone(&state, other), Zone::Hand);
    assert_eq!(zone(&state, big), Zone::Graveyard);

    // With no other card, only the sacrifice is payable.
    let mut state = swamps();
    let big = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let shards = put(&mut state, P0, "Bone Shards", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast_plan(
        &mut state,
        shards,
        &Plan {
            targets: vec![Target::Object(big)],
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, big), Zone::Graveyard);

    // With neither a creature nor another card, it can't be cast.
    let mut state = swamps();
    put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let shards = put(&mut state, P0, "Bone Shards", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    assert!(!castable(&mut state, shards));
}

/// Gives `card` (in P0's graveyard) flashback with the Flashback sorcery,
/// which leaves P0's hand empty unless the test added other cards.
fn grant_flashback(state: &mut GameState, card: ObjectId) {
    let flashback = put(state, P0, "Flashback", Zone::Hand);
    pool(state, P0, &[ManaColor::R], 0);
    cast(state, flashback, &[Target::Object(card)]);
    settled(state);
    assert_eq!(zone(state, flashback), Zone::Graveyard);
}

/// #715: a cast for another alternative cost still chooses between the two
/// additional costs. With flashback, no card in hand and a creature, the
/// cast is legal by sacrificing the creature.
#[test]
fn bone_shards_cast_by_a_granted_flashback_may_sacrifice_a_creature() {
    let mut state = swamps();
    let big = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let shards = put(&mut state, P0, "Bone Shards", Zone::Graveyard);
    grant_flashback(&mut state, shards);
    assert!(state.players[0].hand.is_empty(), "nothing to discard");
    assert!(!castable(&mut state, shards), "{{B}} is not in the pool");
    pool(&mut state, P0, &[ManaColor::B], 0);
    let log = cast_plan(
        &mut state,
        shards,
        &Plan {
            targets: vec![Target::Object(big)],
            sacrifice: Some(elves),
            ..Plan::default()
        },
    );
    assert!(
        !log.contains(&"ChooseSpellMode".to_owned()),
        "only the sacrifice is payable, so nothing is asked: {log:?}"
    );
    assert!(!log.contains(&"Discard".to_owned()), "{log:?}");
    assert_eq!(zone(&state, elves), Zone::Graveyard, "sacrificed");
    assert_eq!(zone(&state, big), Zone::Graveyard, "destroyed");
    assert_eq!(zone(&state, shards), Zone::Exile, "flashback exiles it");
}

#[test]
fn bone_shards_cast_by_a_granted_flashback_chooses_either_cost() {
    for mode in [0, 1] {
        let mut state = swamps();
        let big = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
        let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
        let shards = put(&mut state, P0, "Bone Shards", Zone::Graveyard);
        grant_flashback(&mut state, shards);
        let spare = put(&mut state, P0, "Island", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::B], 0);
        let log = cast_plan(
            &mut state,
            shards,
            &Plan {
                targets: vec![Target::Object(big)],
                mode: Some(mode),
                sacrifice: Some(elves),
                discard: vec![spare],
                ..Plan::default()
            },
        );
        assert!(log.contains(&"ChooseSpellMode".to_owned()), "{log:?}");
        let sacrificed = mode == 0;
        assert_eq!(zone(&state, elves) == Zone::Graveyard, sacrificed, "{mode}");
        assert_eq!(
            zone(&state, spare) == Zone::Graveyard,
            !sacrificed,
            "{mode}"
        );
        assert_eq!(zone(&state, big), Zone::Graveyard);
        assert_eq!(zone(&state, shards), Zone::Exile);
    }

    // By flashback with neither a creature nor a card in hand, no cost is
    // payable and the card is not castable.
    let mut state = swamps();
    put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let shards = put(&mut state, P0, "Bone Shards", Zone::Graveyard);
    grant_flashback(&mut state, shards);
    pool(&mut state, P0, &[ManaColor::B], 0);
    assert!(!castable(&mut state, shards));
}

/// #715: a cast without paying the mana cost (Omniscience, no mana in the
/// pool) still chooses between the two additional costs and pays only the
/// chosen one.
#[test]
fn bone_shards_cast_by_omniscience_chooses_either_cost() {
    for mode in [0, 1] {
        let mut state = swamps();
        put(&mut state, P0, "Omniscience", Zone::Battlefield);
        let big = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
        let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
        let spare = put(&mut state, P0, "Island", Zone::Hand);
        let shards = put(&mut state, P0, "Bone Shards", Zone::Hand);
        let log = cast_plan(
            &mut state,
            shards,
            &Plan {
                targets: vec![Target::Object(big)],
                mode: Some(mode),
                sacrifice: Some(elves),
                discard: vec![spare],
                ..Plan::default()
            },
        );
        assert!(log.contains(&"ChooseSpellMode".to_owned()), "{log:?}");
        assert!(
            !log.contains(&"ChooseEffectOption".to_owned()),
            "no mana, so paying the mana cost is not offered: {log:?}"
        );
        let sacrificed = mode == 0;
        assert_eq!(zone(&state, elves) == Zone::Graveyard, sacrificed, "{mode}");
        assert_eq!(
            zone(&state, spare) == Zone::Graveyard,
            !sacrificed,
            "{mode}"
        );
        assert_eq!(zone(&state, big), Zone::Graveyard);
        assert_eq!(zone(&state, shards), Zone::Graveyard);
    }
}

/// #715: with {B} in the pool Omniscience also offers paying the mana cost.
/// Declining it (the cast stays free) still chooses either additional cost,
/// and the {B} stays in the pool.
#[test]
fn bone_shards_cast_free_by_omniscience_with_mana_available_chooses_either_cost() {
    for mode in [0, 1] {
        let mut state = swamps();
        put(&mut state, P0, "Omniscience", Zone::Battlefield);
        let big = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
        let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
        let spare = put(&mut state, P0, "Island", Zone::Hand);
        let shards = put(&mut state, P0, "Bone Shards", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::B], 0);
        assert!(castable(&mut state, shards));
        act(&mut state, Action::CastSpell(shards));
        let mut asked = Vec::new();
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 2);
                asked.push("free");
                // Option 0 pays the mana cost, option 1 casts it free.
                Action::ChooseEffectOption(1)
            }
            Decision::ChooseSpellMode { legal_modes, .. } => {
                assert_eq!(legal_modes, vec![0, 1]);
                asked.push("mode");
                Action::ChooseSpellMode(mode)
            }
            Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(big)),
            Decision::ChooseCostTargets { candidates, .. } => {
                assert!(candidates.contains(&elves), "{candidates:?}");
                Action::ChooseCostTarget(elves)
            }
            Decision::Discard { choices, .. } => {
                assert!(choices.contains(&spare), "{choices:?}");
                Action::Discard(vec![spare])
            }
            other => first_answer(other),
        });
        assert!(
            asked.contains(&"free") && asked.contains(&"mode"),
            "{asked:?}"
        );
        let sacrificed = mode == 0;
        assert_eq!(zone(&state, elves) == Zone::Graveyard, sacrificed, "{mode}");
        assert_eq!(
            zone(&state, spare) == Zone::Graveyard,
            !sacrificed,
            "{mode}"
        );
        assert_eq!(zone(&state, big), Zone::Graveyard);
        assert_eq!(mana(&state, P0, ManaColor::B), 1, "cast free: {mode}");
    }
}

/// #715: a copy of Bone Shards cast in its second mode (Chandra, Hope's
/// Beacon copies it) pays no cost and resolves with the card's target rules:
/// one card is discarded for two destroyed creatures.
#[test]
fn a_copy_of_bone_shards_cast_by_discarding_pays_no_cost() {
    let mut state = swamps();
    // A real entry, so Chandra has her loyalty.
    let chandra = put(&mut state, P0, "Chandra, Hope's Beacon", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(chandra, Zone::Battlefield),
    );
    let first = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let second = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let spare = put(&mut state, P0, "Island", Zone::Hand);
    let other = put(&mut state, P0, "Sol Ring", Zone::Hand);
    let shards = put(&mut state, P0, "Bone Shards", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    let log = cast_plan(
        &mut state,
        shards,
        &Plan {
            targets: vec![Target::Object(first)],
            picks: vec![Target::Object(second)],
            mode: Some(1),
            discard: vec![spare],
            ..Plan::default()
        },
    );
    assert_eq!(log.first().map(String::as_str), Some("ChooseSpellMode"));
    assert_eq!(
        log.iter().filter(|kind| *kind == "Discard").count(),
        1,
        "{log:?}"
    );
    assert!(!log.contains(&"ChooseCostTargets".to_owned()), "{log:?}");
    let copies = state
        .objects
        .iter()
        .filter_map(|(_, object)| object.spell_copy_origin.map(|origin| origin.parent))
        .collect::<Vec<_>>();
    assert_eq!(copies, vec![shards], "Chandra copied it once");
    assert_eq!(zone(&state, first), Zone::Graveyard, "the first target");
    assert_eq!(zone(&state, second), Zone::Graveyard, "the copy's target");
    assert_eq!(zone(&state, spare), Zone::Graveyard, "the one discard");
    assert_eq!(zone(&state, other), Zone::Hand, "the copy discards nothing");
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, chandra), Zone::Battlefield);
}

#[test]
fn bone_shards_destroys_a_planeswalker_and_still_pays_when_it_fizzles() {
    let mut state = swamps();
    let wrenn = put(&mut state, P0, "Wrenn and Six", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
    cast(&mut state, wrenn, &[]);
    settled(&mut state);
    let spare = put(&mut state, P0, "Island", Zone::Hand);
    let shards = put(&mut state, P0, "Bone Shards", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast_plan(
        &mut state,
        shards,
        &Plan {
            targets: vec![Target::Object(wrenn)],
            discard: vec![spare],
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, wrenn), Zone::Graveyard);

    let mut state = swamps();
    let small = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let spare = put(&mut state, P0, "Island", Zone::Hand);
    let shards = put(&mut state, P0, "Bone Shards", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, shards, &[Target::Object(small)]);
    opponent_responds_with_bolt(&mut state, Target::Object(small));
    settle_first(&mut state);
    assert_eq!(zone(&state, small), Zone::Graveyard, "the bolt killed it");
    assert_eq!(
        zone(&state, spare),
        Zone::Graveyard,
        "the cost was still paid"
    );
    assert_eq!(zone(&state, shards), Zone::Graveyard);
}

// ---- Chain of Smog ---------------------------------------------------------------

#[test]
fn chain_of_smog_makes_the_target_discard_two_of_their_choice_and_may_decline_the_copy() {
    let mut state = swamps();
    let cards = stock_hand(&mut state, P1, &HAND[..4]);
    let smog = put(&mut state, P0, "Chain of Smog", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, smog, &[Target::Player(P1)]);
    let wanted = [cards[2], cards[3]];
    let mut discard_prompts = 0;
    let mut copy_offers = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            selected_count,
            min_targets: 2,
            can_finish,
            ..
        } => {
            assert_eq!(player, P1, "the target chooses");
            let _ = selected_count;
            if can_finish {
                return Action::FinishEffectSelection;
            }
            discard_prompts += 1;
            let card = wanted
                .iter()
                .find(|card| legal_targets.contains(&Target::Object(**card)))
                .unwrap();
            Action::ChooseEffectTarget(Target::Object(*card))
        }
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            selected_count: 0,
            can_finish: true,
            ..
        } => {
            copy_offers += 1;
            assert_eq!(player, P1, "the target may copy");
            assert!(legal_targets.contains(&Target::Player(P0)));
            assert!(legal_targets.contains(&Target::Player(P1)));
            Action::FinishEffectSelection
        }
        other => first_answer(other),
    });
    assert_eq!(discard_prompts, 2, "one prompt for each card");
    assert_eq!(copy_offers, 1);
    assert_eq!(zone(&state, cards[2]), Zone::Graveyard);
    assert_eq!(zone(&state, cards[3]), Zone::Graveyard);
    assert_eq!(zone(&state, cards[0]), Zone::Hand);
    assert_eq!(zone(&state, cards[1]), Zone::Hand);
    assert_eq!(state.players[1].hand.len(), 2);
    assert!(state.stack.is_empty(), "no copy was made");
    assert_eq!(zone(&state, smog), Zone::Graveyard);
}

#[test]
fn chain_of_smog_copies_bounce_between_the_players_with_new_targets() {
    let mut state = swamps();
    stock_hand(&mut state, P1, &HAND);
    stock_hand(&mut state, P0, &HAND[..3]);
    let smog = put(&mut state, P0, "Chain of Smog", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, smog, &[Target::Player(P1)]);
    let mut offers = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            min_targets: 2,
            ..
        } => {
            if can_finish {
                Action::FinishEffectSelection
            } else {
                Action::ChooseEffectTarget(legal_targets[0])
            }
        }
        Decision::ChooseEffectTargets {
            player,
            selected_count,
            ..
        } => {
            if selected_count > 0 {
                return Action::FinishEffectSelection;
            }
            offers.push(player);
            match offers.len() {
                // P1 copies it at P0, who copies it back at P1.
                1 => Action::ChooseEffectTarget(Target::Player(P0)),
                2 => Action::ChooseEffectTarget(Target::Player(P1)),
                _ => Action::FinishEffectSelection,
            }
        }
        other => first_answer(other),
    });
    assert_eq!(offers, vec![P1, P0, P1]);
    // P1 discarded for the original and for the copy P0 made; P0 for P1's.
    assert_eq!(state.players[1].hand.len(), HAND.len() - 4);
    assert_eq!(state.players[0].hand.len(), 3 - 2);
    assert!(state.stack.is_empty());
}

#[test]
fn chain_of_smog_discards_a_short_hand_whole_and_may_target_its_caster() {
    // Two or fewer cards are discarded without a choice, and the copy is
    // still offered.
    for count in [1, 2] {
        let mut state = swamps();
        let cards = stock_hand(&mut state, P1, &HAND[..count]);
        let smog = put(&mut state, P0, "Chain of Smog", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::B], 1);
        cast(&mut state, smog, &[Target::Player(P1)]);
        let mut offered = 0;
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets {
                selected_count: 0,
                min_targets: 0,
                ..
            } => {
                offered += 1;
                Action::FinishEffectSelection
            }
            other => panic!("a short hand is discarded unasked: {other:?}"),
        });
        for card in cards {
            assert_eq!(zone(&state, card), Zone::Graveyard);
        }
        assert!(state.players[1].hand.is_empty());
        assert_eq!(
            offered, 1,
            "the copy is still offered after a short discard"
        );
    }

    // An empty hand discards nothing, and the copy is offered.
    let mut state = swamps();
    let smog = put(&mut state, P0, "Chain of Smog", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, smog, &[Target::Player(P1)]);
    settle_first(&mut state);
    assert!(state.players[1].graveyard.is_empty());
    assert_eq!(zone(&state, smog), Zone::Graveyard);

    // The caster may target themself, discarding two of their own.
    let mut state = swamps();
    let cards = stock_hand(&mut state, P0, &HAND[..3]);
    let smog = put(&mut state, P0, "Chain of Smog", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, smog, &[Target::Player(P0)]);
    settle_first(&mut state);
    assert_eq!(
        cards
            .iter()
            .filter(|&&card| zone(&state, card) == Zone::Graveyard)
            .count(),
        2
    );
}

#[test]
fn chain_of_smog_fizzles_against_a_player_who_can_no_longer_be_targeted() {
    let mut state = swamps();
    let cards = stock_hand(&mut state, P1, &HAND);
    let smog = put(&mut state, P0, "Chain of Smog", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, smog, &[Target::Player(P1)]);
    next(&mut state);
    put(&mut state, P1, "Leyline of Sanctity", Zone::Battlefield);
    settled(&mut state);
    assert_eq!(state.players[1].hand.len(), HAND.len());
    assert!(discarded(&state, P1, &cards).is_empty());
    assert_eq!(zone(&state, smog), Zone::Graveyard);
}

// ---- Collective Brutality --------------------------------------------------------

const BRUTALITY_REVEAL: u8 = 0;
const BRUTALITY_SHRINK: u8 = 1;
const BRUTALITY_REVEAL_SHRINK: u8 = 2;
const BRUTALITY_DRAIN: u8 = 3;
const BRUTALITY_REVEAL_DRAIN: u8 = 4;
const BRUTALITY_SHRINK_DRAIN: u8 = 5;
const BRUTALITY_ALL: u8 = 6;

/// P0 holds Collective Brutality with `spare` other cards and mana for it.
fn brutality_table(spare: &[&str]) -> (GameState, ObjectId, Vec<ObjectId>) {
    let mut state = swamps();
    let spares = spare
        .iter()
        .map(|name| put(&mut state, P0, name, Zone::Hand))
        .collect();
    let brutality = put(&mut state, P0, "Collective Brutality", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    (state, brutality, spares)
}

#[test]
fn collective_brutality_reveals_a_hand_and_discards_the_instant_or_sorcery_you_choose() {
    let (mut state, brutality, _) = brutality_table(&[]);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let ritual = put(&mut state, P1, "Dark Ritual", Zone::Hand);
    let island = put(&mut state, P1, "Island", Zone::Hand);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    let mut offered = Vec::new();
    act(&mut state, Action::CastSpell(brutality));
    let plan = Plan {
        mode: Some(BRUTALITY_REVEAL),
        targets: vec![Target::Player(P1)],
        ..Plan::default()
    };
    // Watch the choice of card: only instants and sorceries, chosen by the caster.
    let mut targets = plan.targets.clone();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { .. } => Action::ChooseSpellMode(BRUTALITY_REVEAL),
        Decision::ChooseTargets { legal_targets, .. } => {
            Action::ChooseTarget(take_legal(&mut targets, &legal_targets).unwrap())
        }
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert_eq!(player, P0, "you choose");
            offered = legal_targets.clone();
            Action::ChooseEffectTarget(Target::Object(ritual))
        }
        other => first_answer(other),
    });
    offered.sort_by_key(|target| format!("{target:?}"));
    let mut expected = vec![Target::Object(bolt), Target::Object(ritual)];
    expected.sort_by_key(|target| format!("{target:?}"));
    assert_eq!(offered, expected, "instants and sorceries only");
    assert_eq!(zone(&state, ritual), Zone::Graveyard);
    assert!(state.players[1].graveyard.contains(&ritual));
    assert_eq!(zone(&state, bolt), Zone::Hand);
    assert_eq!(zone(&state, island), Zone::Hand);
    assert_eq!(zone(&state, ring), Zone::Hand);
}

#[test]
fn collective_brutality_reveal_does_nothing_without_an_instant_or_sorcery() {
    let (mut state, brutality, _) = brutality_table(&[]);
    stock_hand(&mut state, P1, &["Island", "Sol Ring", "Llanowar Elves"]);
    act(&mut state, Action::CastSpell(brutality));
    let log = run_plan(
        &mut state,
        &Plan {
            mode: Some(BRUTALITY_REVEAL),
            targets: vec![Target::Player(P1)],
            ..Plan::default()
        },
    );
    assert!(!log.contains(&"ChooseEffectTargets".to_owned()), "{log:?}");
    assert_eq!(state.players[1].hand.len(), 3);
    assert_eq!(zone(&state, brutality), Zone::Graveyard);
}

#[test]
fn collective_brutality_shrinks_a_creature_until_end_of_turn() {
    let (mut state, brutality, _) = brutality_table(&[]);
    let big = put(&mut state, P1, "Sowing Mycospawn", Zone::Battlefield);
    let small = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert_eq!(power_toughness(&state, big), (3, 3));
    act(&mut state, Action::CastSpell(brutality));
    run_plan(
        &mut state,
        &Plan {
            mode: Some(BRUTALITY_SHRINK),
            targets: vec![Target::Object(big)],
            ..Plan::default()
        },
    );
    assert_eq!(power_toughness(&state, big), (1, 1));
    assert_eq!(zone(&state, small), Zone::Battlefield);
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(power_toughness(&state, big), (3, 3), "until end of turn");

    // -2/-2 kills a small creature.
    let (mut state, brutality, _) = brutality_table(&[]);
    let small = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    act(&mut state, Action::CastSpell(brutality));
    run_plan(
        &mut state,
        &Plan {
            mode: Some(BRUTALITY_SHRINK),
            targets: vec![Target::Object(small)],
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, small), Zone::Graveyard);
}

#[test]
fn collective_brutality_drains_two_life() {
    let (mut state, brutality, _) = brutality_table(&[]);
    act(&mut state, Action::CastSpell(brutality));
    run_plan(
        &mut state,
        &Plan {
            mode: Some(BRUTALITY_DRAIN),
            targets: vec![Target::Player(P1)],
            ..Plan::default()
        },
    );
    assert_eq!(life(&state, P1), 18);
    assert_eq!(life(&state, P0), 22);
}

#[test]
fn collective_brutality_escalates_one_discard_for_each_extra_mode() {
    // Two modes, one discard: reveal and shrink.
    let (mut state, brutality, spares) = brutality_table(&["Island"]);
    let ritual = put(&mut state, P1, "Dark Ritual", Zone::Hand);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    act(&mut state, Action::CastSpell(brutality));
    run_plan(
        &mut state,
        &Plan {
            mode: Some(BRUTALITY_REVEAL_SHRINK),
            targets: vec![Target::Player(P1), Target::Object(elves)],
            picks: vec![Target::Object(ritual)],
            discard: spares.clone(),
            ..Plan::default()
        },
    );
    assert_eq!(
        zone(&state, spares[0]),
        Zone::Graveyard,
        "the escalate discard"
    );
    assert_eq!(zone(&state, ritual), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Graveyard);

    // Two modes, one discard: reveal and drain.
    let (mut state, brutality, spares) = brutality_table(&["Island"]);
    let ritual = put(&mut state, P1, "Dark Ritual", Zone::Hand);
    act(&mut state, Action::CastSpell(brutality));
    run_plan(
        &mut state,
        &Plan {
            mode: Some(BRUTALITY_REVEAL_DRAIN),
            targets: vec![Target::Player(P1), Target::Player(P1)],
            picks: vec![Target::Object(ritual)],
            discard: spares.clone(),
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, spares[0]), Zone::Graveyard);
    assert_eq!(zone(&state, ritual), Zone::Graveyard);
    assert_eq!((life(&state, P1), life(&state, P0)), (18, 22));

    // Two modes, one discard: shrink and drain.
    let (mut state, brutality, spares) = brutality_table(&["Island"]);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    act(&mut state, Action::CastSpell(brutality));
    run_plan(
        &mut state,
        &Plan {
            mode: Some(BRUTALITY_SHRINK_DRAIN),
            targets: vec![Target::Object(elves), Target::Player(P1)],
            discard: spares.clone(),
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, spares[0]), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!((life(&state, P1), life(&state, P0)), (18, 22));

    // Three modes, two discards.
    let (mut state, brutality, spares) = brutality_table(&["Island", "Swamp"]);
    let ritual = put(&mut state, P1, "Dark Ritual", Zone::Hand);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    act(&mut state, Action::CastSpell(brutality));
    run_plan(
        &mut state,
        &Plan {
            mode: Some(BRUTALITY_ALL),
            targets: vec![
                Target::Player(P1),
                Target::Object(elves),
                Target::Player(P1),
            ],
            picks: vec![Target::Object(ritual)],
            discard: spares.clone(),
            ..Plan::default()
        },
    );
    for spare in spares {
        assert_eq!(zone(&state, spare), Zone::Graveyard);
    }
    assert_eq!(zone(&state, ritual), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!((life(&state, P1), life(&state, P0)), (18, 22));
    assert_eq!(state.players[0].hand.len(), 0);
}

#[test]
fn collective_brutality_offers_only_the_modes_whose_discards_it_can_pay() {
    let modes_with = |spares: &[&str]| -> Vec<u8> {
        let (mut state, brutality, _) = brutality_table(spares);
        put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
        assert!(castable(&mut state, brutality));
        act(&mut state, Action::CastSpell(brutality));
        match next(&mut state) {
            Decision::ChooseSpellMode { legal_modes, .. } => legal_modes,
            other => panic!("expected a mode choice, got {other:?}"),
        }
    };
    assert_eq!(
        modes_with(&[]),
        vec![BRUTALITY_REVEAL, BRUTALITY_SHRINK, BRUTALITY_DRAIN]
    );
    assert_eq!(
        modes_with(&["Island"]),
        vec![
            BRUTALITY_REVEAL,
            BRUTALITY_SHRINK,
            BRUTALITY_REVEAL_SHRINK,
            BRUTALITY_DRAIN,
            BRUTALITY_REVEAL_DRAIN,
            BRUTALITY_SHRINK_DRAIN
        ]
    );
    assert_eq!(modes_with(&["Island", "Swamp"]).len(), 7);
}

#[test]
fn collective_brutality_runs_the_other_modes_when_one_target_is_gone() {
    let (mut state, brutality, spares) = brutality_table(&["Island"]);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    act(&mut state, Action::CastSpell(brutality));
    let mut targets = vec![Target::Object(elves), Target::Player(P1)];
    // Announce, then remove the creature with the spell on the stack.
    loop {
        match next(&mut state) {
            Decision::ChooseSpellMode { .. } => {
                act(&mut state, Action::ChooseSpellMode(BRUTALITY_SHRINK_DRAIN))
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = take_legal(&mut targets, &legal_targets).unwrap();
                act(&mut state, Action::ChooseTarget(target));
            }
            Decision::Discard { count, choices, .. } => {
                act(
                    &mut state,
                    Action::Discard(choices.into_iter().take(count as usize).collect()),
                );
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("{other:?}"),
        }
    }
    remove(&mut state, elves);
    settled(&mut state);
    assert_eq!(zone(&state, spares[0]), Zone::Graveyard);
    assert_eq!(
        (life(&state, P1), life(&state, P0)),
        (18, 22),
        "the drain still happened"
    );
    assert_eq!(zone(&state, brutality), Zone::Graveyard);
}

// ---- Natural Order ---------------------------------------------------------------

/// Natural Order's table: Llanowar Elves for the sacrifice, and a library
/// with a green creature, a black creature and a green noncreature.
fn natural_order_table() -> (GameState, ObjectId, ObjectId, Vec<ObjectId>) {
    let mut state = forests();
    let library = [
        put(&mut state, P0, "Ravenous Baloth", Zone::Library),
        put(&mut state, P0, "Hypnotic Specter", Zone::Library),
        put(&mut state, P0, "Mutagenic Growth", Zone::Library),
    ]
    .to_vec();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let order = put(&mut state, P0, "Natural Order", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    (state, order, elves, library)
}

#[test]
fn natural_order_sacrifices_a_green_creature_and_puts_a_green_creature_onto_the_battlefield() {
    let (mut state, order, elves, library) = natural_order_table();
    let before = state.players[0].library.len();
    act(&mut state, Action::CastSpell(order));
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![elves], "only green creatures");
            Action::ChooseCostTarget(elves)
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            offered = legal_targets.clone();
            Action::ChooseEffectTarget(Target::Object(library[0]))
        }
        other => first_answer(other),
    });
    assert_eq!(
        offered,
        vec![Target::Object(library[0])],
        "only green creature cards"
    );
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, library[0]), Zone::Battlefield);
    assert_eq!(state.objects.get(library[0]).controller, P0);
    assert_eq!(state.players[0].library.len(), before - 1);
    assert_eq!(zone(&state, library[1]), Zone::Library);
    assert_eq!(zone(&state, order), Zone::Graveyard);
}

#[test]
fn natural_order_needs_a_green_creature_to_sacrifice() {
    let (mut state, order, elves, _) = natural_order_table();
    remove(&mut state, elves);
    assert!(!castable(&mut state, order), "no creature at all");
    let black = put(&mut state, P0, "Hypnotic Specter", Zone::Battlefield);
    assert!(
        !castable(&mut state, order),
        "a black creature is not green"
    );
    let _ = black;
}

#[test]
fn natural_order_may_find_nothing_but_still_sacrifices() {
    let (mut state, order, elves, library) = natural_order_table();
    act(&mut state, Action::CastSpell(order));
    run_plan(&mut state, &Plan::default());
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(
        zone(&state, library[0]),
        Zone::Library,
        "declined to find it"
    );
    assert_eq!(zone(&state, order), Zone::Graveyard);
}

// ---- Pest Infestation ------------------------------------------------------------

fn pests(state: &GameState, player: PlayerId) -> Vec<ObjectId> {
    tokens_named(state, player, "Pest Token")
}

#[test]
fn pest_infestation_destroys_up_to_x_targets_and_creates_twice_x_pests() {
    let mut state = forests();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let rip = put(&mut state, P1, "Exploration", Zone::Battlefield);
    let other = put(&mut state, P1, "Mox Emerald", Zone::Battlefield);
    let infestation = put(&mut state, P0, "Pest Infestation", Zone::Hand);
    // X = 2: {2}{2}{G}.
    pool(&mut state, P0, &[ManaColor::G], 4);
    act(&mut state, Action::CastSpell(infestation));
    run_plan(
        &mut state,
        &Plan {
            targets: vec![Target::Object(ring), Target::Object(rip)],
            x: Some(2),
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, rip), Zone::Graveyard);
    assert_eq!(
        zone(&state, other),
        Zone::Battlefield,
        "only the chosen ones"
    );
    let made = pests(&state, P0);
    assert_eq!(made.len(), 4, "twice X");
    let printed = def("Pest Token");
    assert_eq!(printed.colors, &[ManaColor::B, ManaColor::G]);
    assert_eq!(printed.subtypes, &[Subtype::Pest]);
    assert_eq!(printed.types, &[CardType::Creature]);
    for pest in made {
        assert_eq!(power_toughness(&state, pest), (1, 1));
        assert!(state.objects.get(pest).v4.is_token);
    }
    assert!(pests(&state, P1).is_empty());
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn pest_infestation_destroys_its_targets_as_one_batch() {
    // Sol Ring and Rest in Peace are destroyed together, and Rest in Peace's
    // replacement effect exiles both, whichever is named first: the batch
    // is one event, and the replacement exists immediately before it.
    for rest_first in [false, true] {
        let mut state = forests();
        let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
        let rest = put(&mut state, P1, "Rest in Peace", Zone::Battlefield);
        let infestation = put(&mut state, P0, "Pest Infestation", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::G], 4);
        act(&mut state, Action::CastSpell(infestation));
        let mut targets = vec![Target::Object(ring), Target::Object(rest)];
        if rest_first {
            targets.reverse();
        }
        run_plan(
            &mut state,
            &Plan {
                targets,
                x: Some(2),
                ..Plan::default()
            },
        );
        assert_eq!(zone(&state, ring), Zone::Exile, "rest_first={rest_first}");
        assert_eq!(zone(&state, rest), Zone::Exile, "rest_first={rest_first}");
        assert_eq!(pests(&state, P0).len(), 4);
    }
}

#[test]
fn pest_infestation_may_destroy_fewer_than_x_and_may_be_cast_for_zero() {
    // X = 3 with one target: six Pests.
    let mut state = forests();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let infestation = put(&mut state, P0, "Pest Infestation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 6);
    act(&mut state, Action::CastSpell(infestation));
    run_plan(
        &mut state,
        &Plan {
            targets: vec![Target::Object(ring)],
            x: Some(3),
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(pests(&state, P0).len(), 6);

    // X = 3 with no target: six Pests and nothing destroyed.
    let mut state = forests();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let infestation = put(&mut state, P0, "Pest Infestation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 6);
    act(&mut state, Action::CastSpell(infestation));
    run_plan(
        &mut state,
        &Plan {
            x: Some(3),
            ..Plan::default()
        },
    );
    assert_eq!(zone(&state, ring), Zone::Battlefield);
    assert_eq!(pests(&state, P0).len(), 6);

    // X = 0 costs only {G} and does nothing.
    let mut state = forests();
    let infestation = put(&mut state, P0, "Pest Infestation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    act(&mut state, Action::CastSpell(infestation));
    run_plan(&mut state, &Plan::default());
    assert!(pests(&state, P0).is_empty());
    assert_eq!(zone(&state, infestation), Zone::Graveyard);
}

#[test]
fn pest_infestation_offers_no_more_targets_than_the_x_it_can_pay_for() {
    let mut state = forests();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let rip = put(&mut state, P1, "Exploration", Zone::Battlefield);
    let infestation = put(&mut state, P0, "Pest Infestation", Zone::Hand);
    // {G} and two more: X can be at most 1.
    pool(&mut state, P0, &[ManaColor::G], 2);
    act(&mut state, Action::CastSpell(infestation));
    let first = match next(&mut state) {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(can_finish, "up to X: zero targets is fine");
            assert!(legal_targets.contains(&Target::Object(ring)));
            assert!(legal_targets.contains(&Target::Object(rip)));
            legal_targets[0]
        }
        other => panic!("{other:?}"),
    };
    act(&mut state, Action::ChooseTarget(first));
    if let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) {
        assert!(
            legal_targets.is_empty(),
            "a second target needs X = 2: {legal_targets:?}"
        );
    }
}

#[test]
fn pest_tokens_gain_their_controller_a_life_when_they_die() {
    let mut state = forests();
    let infestation = put(&mut state, P0, "Pest Infestation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 4);
    act(&mut state, Action::CastSpell(infestation));
    run_plan(
        &mut state,
        &Plan {
            x: Some(2),
            ..Plan::default()
        },
    );
    let made = pests(&state, P0);
    assert_eq!(made.len(), 4);
    assert_eq!(life(&state, P0), 20);
    // A bolt kills one: one life.
    opponent_bolts(&mut state, Target::Object(made[0]));
    settled(&mut state);
    assert_eq!(life(&state, P0), 21);
    assert_eq!(pests(&state, P0).len(), 3);
    // Damnation kills the other three: three life.
    let damnation = put(&mut state, P0, "Damnation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    cast(&mut state, damnation, &[]);
    settled(&mut state);
    assert!(pests(&state, P0).is_empty());
    assert_eq!(life(&state, P0), 24);
}

#[test]
fn pest_infestation_still_resolves_when_only_some_targets_are_gone_and_fizzles_when_all_are() {
    let mut state = forests();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let rip = put(&mut state, P1, "Exploration", Zone::Battlefield);
    let infestation = put(&mut state, P0, "Pest Infestation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 4);
    act(&mut state, Action::CastSpell(infestation));
    let mut targets = vec![Target::Object(ring), Target::Object(rip)];
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                match take_legal(&mut targets, &legal_targets) {
                    Some(target) => act(&mut state, Action::ChooseTarget(target)),
                    None => act(&mut state, Action::FinishEffectSelection),
                }
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("{other:?}"),
        }
    }
    remove(&mut state, ring);
    settled(&mut state);
    assert_eq!(
        zone(&state, rip),
        Zone::Graveyard,
        "the other target was destroyed"
    );
    assert_eq!(
        pests(&state, P0).len(),
        4,
        "the tokens do not need a target"
    );

    let mut state = forests();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let infestation = put(&mut state, P0, "Pest Infestation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    act(&mut state, Action::CastSpell(infestation));
    let mut targets = vec![Target::Object(ring)];
    loop {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                match take_legal(&mut targets, &legal_targets) {
                    Some(target) => act(&mut state, Action::ChooseTarget(target)),
                    None => act(&mut state, Action::FinishEffectSelection),
                }
            }
            Decision::ChooseEffectOption { .. } => act(&mut state, Action::ChooseEffectOption(0)),
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("{other:?}"),
        }
    }
    remove(&mut state, ring);
    settled(&mut state);
    assert!(
        pests(&state, P0).is_empty(),
        "every target illegal: no effect"
    );
    assert_eq!(zone(&state, infestation), Zone::Graveyard);
}

// ---- Tamiyo's Safekeeping --------------------------------------------------------

#[test]
fn tamiyos_safekeeping_grants_hexproof_and_indestructible_until_end_of_turn_and_gains_two() {
    let mut state = swamps();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let safekeeping = put(&mut state, P0, "Tamiyo's Safekeeping", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    assert!(!keyword(&state, elves, Keywords::HEXPROOF));
    cast(&mut state, safekeeping, &[Target::Object(elves)]);
    settled(&mut state);
    assert!(keyword(&state, elves, Keywords::HEXPROOF));
    assert!(keyword(&state, elves, Keywords::INDESTRUCTIBLE));
    assert_eq!(life(&state, P0), 22);
    assert_eq!(zone(&state, safekeeping), Zone::Graveyard);
    // Indestructible: Damnation leaves it, and kills the other creature.
    let damnation = put(&mut state, P0, "Damnation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 2);
    cast(&mut state, damnation, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    // It lasts only until end of turn.
    pass_until(&mut state, |state| state.active_player == P1);
    assert!(!keyword(&state, elves, Keywords::HEXPROOF));
    assert!(!keyword(&state, elves, Keywords::INDESTRUCTIBLE));
}

#[test]
fn tamiyos_safekeeping_targets_only_your_own_permanents() {
    let mut state = swamps();
    let mine = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let land = put(&mut state, P0, "Swamp", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let safekeeping = put(&mut state, P0, "Tamiyo's Safekeeping", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    let legal = cast_targets(&state, safekeeping);
    assert!(legal.contains(&Target::Object(mine)));
    assert!(legal.contains(&Target::Object(land)));
    assert!(!legal.contains(&Target::Object(theirs)));
    assert!(!legal.contains(&Target::Player(P0)));
}

#[test]
fn tamiyos_safekeeping_counters_removal_by_making_it_illegal() {
    let mut state = swamps();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let grasp = put(&mut state, P1, "Infernal Grasp", Zone::Hand);
    let safekeeping = put(&mut state, P0, "Tamiyo's Safekeeping", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::B], 1);
    cast(&mut state, grasp, &[Target::Object(elves)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, safekeeping, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield, "the Grasp fizzled");
    assert_eq!(life(&state, P1), 20, "a fizzled Grasp costs no life");
    assert_eq!(life(&state, P0), 22);
}

#[test]
fn tamiyos_safekeeping_fizzles_without_gaining_life_when_its_target_is_gone() {
    let mut state = swamps();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let safekeeping = put(&mut state, P0, "Tamiyo's Safekeeping", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, safekeeping, &[Target::Object(elves)]);
    opponent_responds_with_bolt(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20, "no life gained");
}

// ---- Cathar Commando -------------------------------------------------------------

#[test]
fn cathar_commando_has_flash_and_may_be_cast_in_response() {
    let commando = def("Cathar Commando");
    assert_eq!(commando.capability, CardCapability::Full);
    assert_eq!(commando.types, &[CardType::Creature]);
    assert_eq!(commando.subtypes, &[Subtype::Human, Subtype::Soldier]);
    assert_eq!(commando.colors, &[ManaColor::W]);
    assert_eq!((commando.power, commando.toughness), (Some(3), Some(1)));
    assert_eq!(commando.cost.generic, 1);
    assert_eq!(commando.cost.pips, [Pip::Colored(ManaColor::W)].as_slice());
    assert!(commando.keywords.has(Keywords::FLASH));

    let mut state = swamps();
    let card = put(&mut state, P0, "Cathar Commando", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    opponent_bolts(&mut state, Target::Player(P0));
    cast(&mut state, card, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, card), Zone::Battlefield);
    assert_eq!(life(&state, P0), 17, "the bolt resolved after it");
    assert!(keyword(&state, card, Keywords::FLASH));
}

#[test]
fn cathar_commando_sacrifices_itself_to_destroy_an_artifact_or_an_enchantment() {
    for (name, kind) in [("Sol Ring", "artifact"), ("Exploration", "enchantment")] {
        let mut state = swamps();
        let commando = put(&mut state, P0, "Cathar Commando", Zone::Battlefield);
        let target = put(&mut state, P1, name, Zone::Battlefield);
        assert!(
            !activatable(&mut state, commando, 0),
            "{kind}: {{1}} is unpaid"
        );
        pool(&mut state, P0, &[], 1);
        activate(&mut state, commando, 0, &[Target::Object(target)]);
        settled(&mut state);
        assert_eq!(zone(&state, target), Zone::Graveyard, "{kind}");
        assert_eq!(
            zone(&state, commando),
            Zone::Graveyard,
            "sacrificed as a cost"
        );
        assert_eq!(mana(&state, P0, ManaColor::C), 0);
    }
}

#[test]
fn cathar_commando_targets_either_players_artifacts_and_enchantments_only() {
    let mut state = swamps();
    let commando = put(&mut state, P0, "Cathar Commando", Zone::Battlefield);
    let mine = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Exploration", Zone::Battlefield);
    let creature = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let land = put(&mut state, P1, "Swamp", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, commando, 0));
    act(&mut state, Action::ActivateAbility(commando, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(mine)));
            assert!(legal_targets.contains(&Target::Object(theirs)));
            assert!(!legal_targets.contains(&Target::Object(creature)));
            assert!(!legal_targets.contains(&Target::Object(land)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

#[test]
fn cathar_commando_ability_fizzles_when_its_target_is_gone() {
    let mut state = swamps();
    let commando = put(&mut state, P0, "Cathar Commando", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, commando, 0, &[Target::Object(ring)]);
    next(&mut state);
    remove(&mut state, ring);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, commando), Zone::Graveyard, "the cost was paid");
}

// ---- Public sessions -------------------------------------------------------------

#[derive(Default)]
struct C1bEvents {
    casts: std::collections::BTreeMap<&'static str, usize>,
}

impl C1bEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let Some(card_id) = semantic["source"]["card_db_id"].as_u64() else {
            return;
        };
        if semantic["action_kind"].as_str() != Some("cast_spell") {
            return;
        }
        let name = CARD_DEFS[card_id as usize].name;
        if let Some(card) = C1B_CARDS.into_iter().find(|card| *card == name) {
            *self.casts.entry(card).or_default() += 1;
        }
    }
}

#[test]
fn c1b_decks_finish_and_replay_through_public_sessions_both_seats() {
    // Black discard and removal, with the second half's black spells.
    let black_discard = deck(&[
        ("Swamp", 14),
        ("Forest", 4),
        ("Dark Ritual", 3),
        ("Hypnotic Specter", 3),
        ("Llanowar Elves", 3),
        ("Damnation", 1),
        ("Entomb", 1),
        ("Hymn to Tourach", 3),
        ("Imperial Seal", 1),
        ("Infernal Grasp", 3),
        ("Mind Twist", 2),
        ("Night's Whisper", 3),
        ("Mutagenic Growth", 2),
        ("Bloodchief's Thirst", 3),
        ("Bone Shards", 3),
        ("Chain of Smog", 3),
        ("Collective Brutality", 4),
    ]);
    // Green and black: Natural Order, Pest Infestation and Safekeeping.
    let black_green = deck(&[
        ("Swamp", 11),
        ("Forest", 8),
        ("Llanowar Elves", 8),
        ("Hypnotic Specter", 2),
        ("Natural Order", 4),
        ("Pest Infestation", 4),
        ("Tamiyo's Safekeeping", 4),
        ("Mutagenic Growth", 2),
        ("Infernal Grasp", 2),
        ("Night's Whisper", 2),
        ("Bloodchief's Thirst", 2),
        ("Bone Shards", 2),
        ("Chain of Smog", 2),
        ("Collective Brutality", 2),
        ("Sol Ring", 2),
        ("Hymn to Tourach", 2),
        ("Mind Twist", 1),
    ]);
    // White and black: Cathar Commando and the black spells.
    let white_black = deck(&[
        ("Plains", 8),
        ("Swamp", 11),
        ("Cathar Commando", 4),
        ("Thraben Inspector", 4),
        ("Sol Ring", 2),
        ("Damnation", 1),
        ("Entomb", 1),
        ("Hymn to Tourach", 2),
        ("Imperial Seal", 1),
        ("Infernal Grasp", 3),
        ("Mind Twist", 2),
        ("Night's Whisper", 3),
        ("Bloodchief's Thirst", 3),
        ("Bone Shards", 3),
        ("Chain of Smog", 3),
        ("Collective Brutality", 3),
        ("Dark Ritual", 3),
        ("Hypnotic Specter", 3),
    ]);

    // Chain of Smog's copy is free, so a policy that always accepts it (the
    // ranked one) copies forever: the rules allow that loop, and only the
    // random policy, which declines sooner or later, plays the card here.
    let without_smog = |deck: &CustomDeckV1| CustomDeckV1 {
        cards: deck
            .cards
            .iter()
            .map(|row| {
                if row.name == "Chain of Smog" {
                    CustomCardCountV1 {
                        name: "Swamp".to_owned(),
                        count: row.count,
                    }
                } else {
                    row.clone()
                }
            })
            .collect(),
        companion: None,
    };
    let mut totals = C1bEvents::default();
    let mut games = Vec::new();
    for (pair, first_seed) in [
        (
            [black_discard.clone(), black_green.clone()],
            0x4331_4200_u64,
        ),
        ([black_green.clone(), black_discard.clone()], 0x4331_4210),
        ([black_green.clone(), white_black.clone()], 0x4331_4220),
        ([white_black.clone(), black_green], 0x4331_4230),
        ([white_black.clone(), black_discard.clone()], 0x4331_4240),
        ([black_discard, white_black], 0x4331_4250),
    ] {
        for seed in first_seed..first_seed + 1 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let pair = if matches!(policy, Policy::Ranked) {
                    [without_smog(&pair[0]), without_smog(&pair[1])]
                } else {
                    pair.clone()
                };
                games.push((pair, seed, policy));
            }
        }
    }
    // The games run on worker threads (`in_parallel`); the totals add their
    // casts in this loop's order.
    let casts = in_parallel(&games, |(pair, seed, policy)| {
        let mut events = C1bEvents::default();
        let game = play_observed(
            pair.clone(),
            None,
            *seed,
            *policy,
            &mut |decision, action| events.observe(decision, action),
        );
        let replay = play_observed(pair.clone(), None, *seed, *policy, &mut |_, _| {});
        assert_eq!(
            game.transcript_sha256, replay.transcript_sha256,
            "seed={seed} ranked={}",
            matches!(policy, Policy::Ranked)
        );
        events.casts
    });
    for game_casts in casts {
        for (card, count) in game_casts {
            *totals.casts.entry(card).or_default() += count;
        }
    }
    for card in C1B_CARDS {
        assert!(
            totals.casts.get(card).copied().unwrap_or(0) > 0,
            "{card} never cast"
        );
    }
}
