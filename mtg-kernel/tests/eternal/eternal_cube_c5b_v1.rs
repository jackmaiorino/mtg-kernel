//! Vintage Cube wave C5b: native rules and public-session tests for the
//! token makers and mana creatures.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C5B_CARDS: [&str; 6] = [
    "Worldspine Wurm",
    "Myr Battlesphere",
    "Mole Man, Moloid Master",
    "Torsten, Founder of Benalia",
    "Titania, Protector of Argoth",
    "Otharri, Suns' Glory",
];

const C5B_TOKENS: [&str; 4] = [
    "Green Wurm Token",
    "Myr Token",
    "Moloid Token",
    "Green Elemental Token",
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
        0x4355_4245_0000 + 0x3562,
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
fn c5b_cards_are_full() {
    for name in C5B_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c5b_tokens_are_full() {
    for name in C5B_TOKENS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_token, "{name}");
    }
}

// ---- Helpers of this wave ------------------------------------------------------------

fn tap(state: &mut GameState, object: ObjectId) {
    state.objects.get_mut(object).tapped = true;
}

fn is_tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
}

fn hand_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

fn library_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].library.len()
}

fn to_graveyard(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

fn to_exile(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
}

fn card_name(state: &GameState, object: ObjectId) -> &'static str {
    CARD_DEFS[state.objects.get(object).card_def as usize].name
}

/// The names of `player`'s library, top first.
fn library_names(state: &GameState, player: PlayerId) -> Vec<&'static str> {
    state.players[player.index()]
        .library
        .iter()
        .map(|&object| card_name(state, object))
        .collect()
}

/// The names of `player`'s hand, sorted.
fn hand_names(state: &GameState, player: PlayerId) -> Vec<&'static str> {
    sorted(
        state.players[player.index()]
            .hand
            .iter()
            .map(|&object| card_name(state, object))
            .collect(),
    )
}

fn sorted(mut names: Vec<&'static str>) -> Vec<&'static str> {
    names.sort_unstable();
    names
}

/// Plays on until a triggered ability is on the stack with its targets
/// chosen and its controller holds priority.
fn until_trigger_on_stack(state: &mut GameState) {
    for _ in 0..50 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state
                    .stack
                    .iter()
                    .any(|item| item.kind == StackItemKind::TriggeredAbility) =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => act(state, first_answer(other)),
        }
    }
    panic!("the trigger never reached the stack");
}

/// Settles, answering a selection by choosing the offered objects named in
/// `names` (one at a time, in the order offered) and then finishing.
fn settle_picking(state: &mut GameState, names: &[&str]) {
    settle_with(state, |state, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            let wanted = legal_targets.iter().copied().find(|target| match target {
                Target::Object(object) => names.contains(&card_name(state, *object)),
                _ => false,
            });
            match wanted {
                Some(target) => Action::ChooseEffectTarget(target),
                None => Action::FinishEffectSelection,
            }
        }
        other => first_answer(other),
    });
}

/// An attack answer that picks the first `count` offered objects of a
/// selection, then finishes.
fn choose_first(count: usize) -> impl FnMut(&GameState, &Decision) -> Option<Action> {
    move |_, decision| match decision {
        Decision::ChooseEffectTargets {
            selected_count,
            legal_targets,
            ..
        } => {
            if usize::from(*selected_count) < count {
                Some(Action::ChooseEffectTarget(legal_targets[0]))
            } else {
                Some(Action::FinishEffectSelection)
            }
        }
        _ => None,
    }
}

// ---- Characteristics -----------------------------------------------------------------

#[test]
fn c5b_creatures_have_their_printed_characteristics() {
    let wurm = def("Worldspine Wurm");
    assert_eq!(wurm.types, &[CardType::Creature]);
    assert_eq!(wurm.subtypes, &[Subtype::Wurm]);
    assert!(wurm.supertypes.is_empty());
    assert_eq!((wurm.cost.generic, wurm.cost.pips.len()), (8, 3));
    assert_eq!(wurm.mana_value, 11);
    assert_eq!(wurm.colors, &[ManaColor::G]);
    assert_eq!((wurm.power, wurm.toughness), (Some(15), Some(15)));
    assert!(wurm.keywords.has(Keywords::TRAMPLE));

    let sphere = def("Myr Battlesphere");
    assert_eq!(sphere.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(sphere.subtypes, &[Subtype::Myr, Subtype::Construct]);
    assert_eq!((sphere.cost.generic, sphere.cost.pips.len()), (7, 0));
    assert_eq!(sphere.mana_value, 7);
    assert!(sphere.colors.is_empty());
    assert_eq!((sphere.power, sphere.toughness), (Some(4), Some(7)));

    let mole_man = def("Mole Man, Moloid Master");
    assert_eq!(mole_man.types, &[CardType::Creature]);
    assert_eq!(mole_man.subtypes, &[Subtype::Human, Subtype::Villain]);
    assert_eq!(mole_man.supertypes, &[Supertype::Legendary]);
    assert_eq!((mole_man.cost.generic, mole_man.cost.pips.len()), (2, 1));
    assert_eq!(mole_man.colors, &[ManaColor::G]);
    assert_eq!((mole_man.power, mole_man.toughness), (Some(1), Some(1)));

    let torsten = def("Torsten, Founder of Benalia");
    assert_eq!(torsten.types, &[CardType::Creature]);
    assert_eq!(torsten.subtypes, &[Subtype::Human, Subtype::Soldier]);
    assert_eq!(torsten.supertypes, &[Supertype::Legendary]);
    assert_eq!((torsten.cost.generic, torsten.cost.pips.len()), (5, 2));
    assert_eq!(torsten.mana_value, 7);
    assert_eq!(torsten.colors.len(), 2);
    assert!(torsten.colors.contains(&ManaColor::G) && torsten.colors.contains(&ManaColor::W));
    assert_eq!((torsten.power, torsten.toughness), (Some(7), Some(7)));

    let titania = def("Titania, Protector of Argoth");
    assert_eq!(titania.types, &[CardType::Creature]);
    assert_eq!(titania.subtypes, &[Subtype::Elemental]);
    assert_eq!(titania.supertypes, &[Supertype::Legendary]);
    assert_eq!((titania.cost.generic, titania.cost.pips.len()), (3, 2));
    assert_eq!(titania.colors, &[ManaColor::G]);
    assert_eq!((titania.power, titania.toughness), (Some(5), Some(3)));

    let otharri = def("Otharri, Suns' Glory");
    assert_eq!(otharri.types, &[CardType::Creature]);
    assert_eq!(otharri.subtypes, &[Subtype::Phoenix]);
    assert_eq!(otharri.supertypes, &[Supertype::Legendary]);
    assert_eq!((otharri.cost.generic, otharri.cost.pips.len()), (3, 2));
    assert_eq!(otharri.mana_value, 5);
    assert_eq!(otharri.colors.len(), 2);
    assert!(otharri.colors.contains(&ManaColor::R) && otharri.colors.contains(&ManaColor::W));
    assert_eq!((otharri.power, otharri.toughness), (Some(3), Some(3)));
    assert!(otharri.keywords.has(Keywords::FLYING));
    assert!(otharri.keywords.has(Keywords::LIFELINK));
    assert!(otharri.keywords.has(Keywords::HASTE));
    assert_eq!(otharri.activated_abilities.len(), 1, "the graveyard return");

    // Their printed triggered abilities.
    let trigger_count = |name: &str| mtg_kernel::trigger::triggers_for(id(name)).len();
    assert_eq!(trigger_count("Worldspine Wurm"), 2, "dies and shuffle");
    assert_eq!(trigger_count("Myr Battlesphere"), 2, "enters and attacks");
    assert_eq!(trigger_count("Mole Man, Moloid Master"), 1, "landfall");
    assert_eq!(trigger_count("Torsten, Founder of Benalia"), 2);
    assert_eq!(trigger_count("Titania, Protector of Argoth"), 2);
    assert_eq!(trigger_count("Otharri, Suns' Glory"), 1, "attacks");
    assert_eq!(trigger_count("Moloid Token"), 1, "attacks");
}

#[test]
fn c5b_tokens_have_their_printed_characteristics() {
    for (name, color, subtypes, types, power, toughness) in [
        (
            "Green Wurm Token",
            Some(ManaColor::G),
            &[Subtype::Wurm][..],
            &[CardType::Creature][..],
            5,
            5,
        ),
        (
            "Myr Token",
            None,
            &[Subtype::Myr][..],
            &[CardType::Artifact, CardType::Creature][..],
            1,
            1,
        ),
        (
            "Moloid Token",
            Some(ManaColor::G),
            &[Subtype::Minion][..],
            &[CardType::Creature][..],
            1,
            1,
        ),
        (
            "Green Elemental Token",
            Some(ManaColor::G),
            &[Subtype::Elemental][..],
            &[CardType::Creature][..],
            5,
            3,
        ),
    ] {
        let token = def(name);
        assert!(token.is_token, "{name}");
        assert_eq!(token.types, types, "{name}");
        assert_eq!(token.subtypes, subtypes, "{name}");
        assert_eq!(token.colors.first().copied(), color, "{name}");
        assert_eq!(token.colors.len(), usize::from(color.is_some()), "{name}");
        assert_eq!(
            (token.power, token.toughness),
            (Some(power), Some(toughness)),
            "{name}"
        );
    }
    assert!(def("Green Wurm Token").keywords.has(Keywords::TRAMPLE));
    assert_eq!(def("Moloid Token").object_name, "Moloid");
}

// ---- Worldspine Wurm -----------------------------------------------------------------

#[test]
fn worldspine_wurm_dying_makes_three_trampling_wurms_and_shuffles_into_the_library() {
    let mut state = plains();
    let wurm = put(&mut state, P0, "Worldspine Wurm", Zone::Battlefield);
    to_graveyard(&mut state, wurm);
    settled(&mut state);
    let wurms = named(&state, P0, "Green Wurm Token");
    assert_eq!(wurms.len(), 3);
    for &token in &wurms {
        assert_eq!(power_toughness(&state, token), (5, 5));
        assert!(keyword(&state, token, Keywords::TRAMPLE));
        assert!(engine::has_effective_subtype(&state, token, Subtype::Wurm));
    }
    assert_eq!(
        zone(&state, wurm),
        Zone::Library,
        "shuffled into its owner's library"
    );
    assert_eq!(library_len(&state, P0), 21);
    assert!(state.players[0].graveyard.is_empty());
}

#[test]
fn worldspine_wurm_milled_or_discarded_is_only_shuffled_back() {
    for from in [Zone::Library, Zone::Hand] {
        let mut state = plains();
        let wurm = put(&mut state, P0, "Worldspine Wurm", from);
        to_graveyard(&mut state, wurm);
        settled(&mut state);
        assert_eq!(zone(&state, wurm), Zone::Library, "{from:?}");
        assert!(named(&state, P0, "Green Wurm Token").is_empty(), "{from:?}");
        assert_eq!(library_len(&state, P0), 21, "{from:?}");
        assert!(state.players[0].graveyard.is_empty(), "{from:?}");
    }
}

#[test]
fn worldspine_wurm_exiled_makes_nothing_and_stays_exiled() {
    let mut state = plains();
    let wurm = put(&mut state, P0, "Worldspine Wurm", Zone::Battlefield);
    to_exile(&mut state, wurm);
    settled(&mut state);
    assert_eq!(zone(&state, wurm), Zone::Exile);
    assert!(named(&state, P0, "Green Wurm Token").is_empty());
}

#[test]
fn worldspine_wurm_costs_eleven_and_has_trample_and_fifteen_power() {
    let mut state = plains();
    let wurm = put(&mut state, P0, "Worldspine Wurm", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::G, ManaColor::G],
        7,
    );
    assert!(!castable(&mut state, wurm), "it costs eleven");
    pool(&mut state, P0, &[], 1);
    cast(&mut state, wurm, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, wurm), Zone::Battlefield);
    assert_eq!(power_toughness(&state, wurm), (15, 15));
    assert!(keyword(&state, wurm, Keywords::TRAMPLE));
}

// ---- Myr Battlesphere ----------------------------------------------------------------

/// P0's Battlesphere and `myr` untapped Myr tokens.
fn battlesphere_game(myr: usize) -> (GameState, ObjectId, Vec<ObjectId>) {
    let mut state = plains();
    let sphere = put(&mut state, P0, "Myr Battlesphere", Zone::Battlefield);
    let tokens = (0..myr)
        .map(|_| token(&mut state, P0, "Myr Token"))
        .collect();
    (state, sphere, tokens)
}

#[test]
fn myr_battlesphere_makes_four_untapped_myr_when_it_enters() {
    let mut state = plains();
    let sphere = put(&mut state, P0, "Myr Battlesphere", Zone::Hand);
    pool(&mut state, P0, &[], 7);
    cast(&mut state, sphere, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, sphere), Zone::Battlefield);
    assert_eq!(power_toughness(&state, sphere), (4, 7));
    let myr = named(&state, P0, "Myr Token");
    assert_eq!(myr.len(), 4);
    for &token in &myr {
        assert_eq!(power_toughness(&state, token), (1, 1));
        assert!(!is_tapped(&state, token));
        assert!(engine::object_has_type(&state, token, CardType::Artifact));
        assert!(engine::has_effective_subtype(&state, token, Subtype::Myr));
    }
}

#[test]
fn myr_battlesphere_taps_chosen_myr_for_power_and_damage() {
    let (mut state, sphere, myr) = battlesphere_game(3);
    attack_with(&mut state, vec![sphere], choose_first(2));
    assert_eq!(
        myr.iter()
            .filter(|&&token| is_tapped(&state, token))
            .count(),
        2
    );
    assert_eq!(power_toughness(&state, sphere), (6, 7));
    // 2 damage from the trigger and 6 combat damage.
    assert_eq!(life(&state, P1), 12);
}

#[test]
fn myr_battlesphere_may_tap_every_untapped_myr() {
    let (mut state, sphere, myr) = battlesphere_game(3);
    attack_with(&mut state, vec![sphere], choose_first(3));
    assert!(myr.iter().all(|&token| is_tapped(&state, token)));
    assert_eq!(power_toughness(&state, sphere), (7, 7));
    assert_eq!(life(&state, P1), 20 - 3 - 7);
}

#[test]
fn myr_battlesphere_may_tap_no_myr() {
    let (mut state, sphere, myr) = battlesphere_game(3);
    attack_with(&mut state, vec![sphere], choose_first(0));
    assert!(myr.iter().all(|&token| !is_tapped(&state, token)));
    assert_eq!(power_toughness(&state, sphere), (4, 7));
    assert_eq!(life(&state, P1), 16);
}

#[test]
fn myr_battlesphere_offers_only_untapped_myr() {
    let (mut state, sphere, myr) = battlesphere_game(3);
    tap(&mut state, myr[0]);
    let mut offered: Option<Vec<Target>> = None;
    attack_with(&mut state, vec![sphere], |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            offered.get_or_insert_with(|| legal_targets.clone());
            Some(Action::FinishEffectSelection)
        }
        _ => None,
    });
    let offered = offered.expect("a choice of Myr");
    // The attacking Battlesphere is tapped by attacking and can't pay.
    assert_eq!(offered.len(), 2, "{offered:?}");
    assert!(!offered.contains(&Target::Object(myr[0])));
    assert!(offered.contains(&Target::Object(myr[1])));
    assert!(offered.contains(&Target::Object(myr[2])));
    assert!(!offered.contains(&Target::Object(sphere)));
    assert_eq!(life(&state, P1), 16);
}

#[test]
fn myr_battlesphere_asks_nothing_without_untapped_myr() {
    let (mut state, sphere, _) = battlesphere_game(0);
    let mut asked = false;
    attack_with(&mut state, vec![sphere], |_, decision| {
        if matches!(decision, Decision::ChooseEffectTargets { .. }) {
            asked = true;
        }
        None
    });
    assert!(!asked);
    assert_eq!(life(&state, P1), 16);
}

#[test]
fn myr_battlesphere_counts_only_your_own_myr() {
    let (mut state, sphere, myr) = battlesphere_game(1);
    let theirs = token(&mut state, P1, "Myr Token");
    attack_with(&mut state, vec![sphere], choose_first(5));
    assert!(is_tapped(&state, myr[0]));
    assert!(!is_tapped(&state, theirs));
    assert_eq!(life(&state, P1), 20 - 1 - 5);
}

// ---- Mole Man, Moloid Master ---------------------------------------------------------

#[test]
fn mole_man_makes_a_moloid_for_each_land_that_enters() {
    let mut state = plains();
    put(&mut state, P0, "Mole Man, Moloid Master", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Hand);
    play_land(&mut state, forest);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    let moloids = named(&state, P0, "Moloid Token");
    assert_eq!(moloids.len(), 1);
    let moloid = moloids[0];
    assert_eq!(power_toughness(&state, moloid), (1, 1));
    assert!(engine::has_effective_subtype(
        &state,
        moloid,
        Subtype::Minion
    ));
    assert_eq!(state.objects.get(moloid).name, "Moloid");
}

#[test]
fn mole_man_ignores_lands_your_opponent_controls() {
    let mut state = plains();
    put(&mut state, P0, "Mole Man, Moloid Master", Zone::Battlefield);
    let forest = put(&mut state, P1, "Forest", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(forest, Zone::Battlefield),
    );
    settled(&mut state);
    assert!(named(&state, P0, "Moloid Token").is_empty());
    assert!(named(&state, P1, "Moloid Token").is_empty());
}

#[test]
fn mole_man_plays_lands_from_the_graveyard_as_the_land_drop() {
    let mut state = plains();
    put(&mut state, P0, "Mole Man, Moloid Master", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    let mountain = put(&mut state, P0, "Mountain", Zone::Graveyard);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    match next(&mut state) {
        Decision::CastSpellOrPass { land_drops, .. } => {
            assert!(land_drops.contains(&forest));
            assert!(land_drops.contains(&mountain));
            assert!(!land_drops.contains(&elves), "only land cards");
        }
        other => panic!("expected priority, got {other:?}"),
    }
    play_land(&mut state, forest);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert_eq!(state.players[0].lands_played_this_turn, 1);
    assert_eq!(named(&state, P0, "Moloid Token").len(), 1, "landfall");
    // It is the land drop: Mole Man grants no extra one.
    match next(&mut state) {
        Decision::CastSpellOrPass { land_drops, .. } => assert!(land_drops.is_empty()),
        other => panic!("expected priority, got {other:?}"),
    }
}

#[test]
fn lands_in_the_graveyard_are_not_playable_without_mole_man() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    match next(&mut state) {
        Decision::CastSpellOrPass { land_drops, .. } => assert!(!land_drops.contains(&forest)),
        other => panic!("expected priority, got {other:?}"),
    }
}

#[test]
fn a_moloid_may_mill_a_card_when_it_attacks() {
    let mut state = plains();
    let moloid = token(&mut state, P0, "Moloid Token");
    attack_with(&mut state, vec![moloid], |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        _ => None,
    });
    assert_eq!(library_len(&state, P0), 19);
    assert_eq!(state.players[0].graveyard.len(), 1);
    assert_eq!(life(&state, P1), 19);
}

#[test]
fn a_moloid_may_decline_to_mill() {
    let mut state = plains();
    let moloid = token(&mut state, P0, "Moloid Token");
    attack_with(&mut state, vec![moloid], |_, _| None);
    assert_eq!(library_len(&state, P0), 20);
    assert!(state.players[0].graveyard.is_empty());
    assert_eq!(life(&state, P1), 19);
}

// ---- Torsten, Founder of Benalia -----------------------------------------------------

/// A library whose top seven are a creature, a land, an instant, a land,
/// an instant, a creature and an instant, then thirteen Plains.
fn torsten_library() -> Vec<&'static str> {
    let mut library = vec![
        "Llanowar Elves",
        "Plains",
        "Lightning Bolt",
        "Forest",
        "Counterspell",
        "Thraben Inspector",
        "Giant Growth",
    ];
    library.extend(["Plains"; 13]);
    library
}

fn cast_torsten(state: &mut GameState) -> ObjectId {
    let torsten = put(state, P0, "Torsten, Founder of Benalia", Zone::Hand);
    pool(state, P0, &[ManaColor::G, ManaColor::W], 5);
    cast(state, torsten, &[]);
    torsten
}

#[test]
fn torsten_puts_chosen_creatures_and_lands_in_hand_and_the_rest_on_the_bottom() {
    let mut state = game(&torsten_library());
    let torsten = cast_torsten(&mut state);
    settle_picking(&mut state, &["Llanowar Elves", "Forest"]);
    assert_eq!(zone(&state, torsten), Zone::Battlefield);
    assert_eq!(hand_names(&state, P0), vec!["Forest", "Llanowar Elves"]);
    assert_eq!(library_len(&state, P0), 18);
    let library = library_names(&state, P0);
    assert!(library[..13].iter().all(|&name| name == "Plains"));
    assert_eq!(
        sorted(library[13..].to_vec()),
        sorted(vec![
            "Plains",
            "Lightning Bolt",
            "Counterspell",
            "Thraben Inspector",
            "Giant Growth"
        ])
    );
}

#[test]
fn torsten_offers_only_creature_and_land_cards() {
    let mut state = game(&torsten_library());
    cast_torsten(&mut state);
    let mut offered: Vec<Target> = Vec::new();
    let mut bounds = (0, 0);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            min_targets,
            max_targets,
            ..
        } => {
            bounds = (min_targets, max_targets);
            offered = legal_targets;
            Action::FinishEffectSelection
        }
        other => first_answer(other),
    });
    assert_eq!(bounds, (0, 4));
    assert_eq!(offered.len(), 4);
}

#[test]
fn torsten_may_take_nothing_and_every_revealed_card_goes_to_the_bottom() {
    let mut state = game(&torsten_library());
    cast_torsten(&mut state);
    settle_picking(&mut state, &[]);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(library_len(&state, P0), 20);
    let library = library_names(&state, P0);
    assert!(library[..13].iter().all(|&name| name == "Plains"));
    assert_eq!(
        sorted(library[13..].to_vec()),
        sorted(torsten_library()[..7].to_vec())
    );
}

#[test]
fn torsten_may_take_every_creature_and_land() {
    let mut state = game(&torsten_library());
    cast_torsten(&mut state);
    settle_picking(
        &mut state,
        &["Llanowar Elves", "Plains", "Forest", "Thraben Inspector"],
    );
    assert_eq!(
        hand_names(&state, P0),
        vec!["Forest", "Llanowar Elves", "Plains", "Thraben Inspector"]
    );
    assert_eq!(library_len(&state, P0), 16);
    let library = library_names(&state, P0);
    assert!(library[..13].iter().all(|&name| name == "Plains"));
    assert_eq!(
        sorted(library[13..].to_vec()),
        sorted(vec!["Lightning Bolt", "Counterspell", "Giant Growth"])
    );
}

#[test]
fn torsten_reveals_a_short_library_entirely() {
    let mut state = game(&["Llanowar Elves", "Plains", "Lightning Bolt"]);
    cast_torsten(&mut state);
    settle_picking(&mut state, &["Llanowar Elves"]);
    assert_eq!(hand_names(&state, P0), vec!["Llanowar Elves"]);
    assert_eq!(
        sorted(library_names(&state, P0)),
        vec!["Lightning Bolt", "Plains"]
    );
}

#[test]
fn torsten_with_no_creature_or_land_among_the_seven_asks_nothing() {
    let mut library = vec![
        "Lightning Bolt",
        "Counterspell",
        "Giant Growth",
        "Lightning Bolt",
        "Counterspell",
        "Giant Growth",
        "Lightning Bolt",
    ];
    library.extend(["Plains"; 13]);
    let mut state = game(&library);
    cast_torsten(&mut state);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(library_len(&state, P0), 20);
    let names = library_names(&state, P0);
    assert!(names[..13].iter().all(|&name| name == "Plains"));
}

#[test]
fn torsten_dying_makes_seven_soldiers() {
    let mut state = plains();
    let torsten = put(
        &mut state,
        P0,
        "Torsten, Founder of Benalia",
        Zone::Battlefield,
    );
    assert_eq!(power_toughness(&state, torsten), (7, 7));
    to_graveyard(&mut state, torsten);
    settled(&mut state);
    assert_eq!(zone(&state, torsten), Zone::Graveyard);
    let soldiers = named(&state, P0, "Soldier Token");
    assert_eq!(soldiers.len(), 7);
    for &soldier in &soldiers {
        assert_eq!(power_toughness(&state, soldier), (1, 1));
        assert!(engine::has_effective_subtype(
            &state,
            soldier,
            Subtype::Soldier
        ));
    }
}

// ---- Titania, Protector of Argoth ----------------------------------------------------

fn cast_titania(state: &mut GameState) -> ObjectId {
    let titania = put(state, P0, "Titania, Protector of Argoth", Zone::Hand);
    pool(state, P0, &[ManaColor::G, ManaColor::G], 3);
    cast(state, titania, &[]);
    titania
}

#[test]
fn titania_returns_a_land_from_your_graveyard_untapped() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    let titania = cast_titania(&mut state);
    settle_first(&mut state);
    assert_eq!(zone(&state, titania), Zone::Battlefield);
    assert_eq!(power_toughness(&state, titania), (5, 3));
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert!(!is_tapped(&state, forest));
    assert!(state.players[0].battlefield.contains(&forest));
    assert!(state.players[0].graveyard.is_empty());
}

#[test]
fn titania_returns_the_land_you_choose() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    let mountain = put(&mut state, P0, "Mountain", Zone::Graveyard);
    cast_titania(&mut state);
    settle_choosing(&mut state, mountain);
    assert_eq!(zone(&state, mountain), Zone::Battlefield);
    assert_eq!(zone(&state, forest), Zone::Graveyard);
}

#[test]
fn titania_targets_only_land_cards_in_your_own_graveyard() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    put(&mut state, P1, "Mountain", Zone::Graveyard);
    cast_titania(&mut state);
    let mut offered: Vec<Target> = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            offered = legal_targets.clone();
            Action::ChooseTarget(legal_targets[0])
        }
        other => first_answer(other),
    });
    assert_eq!(offered, vec![Target::Object(forest)]);
}

#[test]
fn titania_does_nothing_without_a_land_card_in_your_graveyard() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Mountain", Zone::Graveyard);
    let titania = cast_titania(&mut state);
    settled(&mut state);
    assert_eq!(zone(&state, titania), Zone::Battlefield);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
}

#[test]
fn titania_does_nothing_when_the_land_has_left_the_graveyard() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    let titania = cast_titania(&mut state);
    until_trigger_on_stack(&mut state);
    to_exile(&mut state, forest);
    settled(&mut state);
    assert_eq!(zone(&state, titania), Zone::Battlefield);
    assert_eq!(zone(&state, forest), Zone::Exile);
    assert!(named(&state, P0, "Forest").is_empty());
}

#[test]
fn titania_makes_a_five_three_elemental_when_a_land_you_control_is_destroyed() {
    let mut state = plains();
    put(
        &mut state,
        P0,
        "Titania, Protector of Argoth",
        Zone::Battlefield,
    );
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    to_graveyard(&mut state, forest);
    settled(&mut state);
    let elementals = named(&state, P0, "Green Elemental Token");
    assert_eq!(elementals.len(), 1);
    assert_eq!(power_toughness(&state, elementals[0]), (5, 3));
    assert!(engine::has_effective_subtype(
        &state,
        elementals[0],
        Subtype::Elemental
    ));
}

#[test]
fn titania_makes_one_elemental_for_each_land_that_dies_at_once() {
    let mut state = plains();
    put(
        &mut state,
        P0,
        "Titania, Protector of Argoth",
        Zone::Battlefield,
    );
    let first = put(&mut state, P0, "Forest", Zone::Battlefield);
    let second = put(&mut state, P0, "Mountain", Zone::Battlefield);
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::zone_change(first, Zone::Graveyard),
            ProposedEvent::zone_change(second, Zone::Graveyard),
        ],
    );
    settled(&mut state);
    assert_eq!(named(&state, P0, "Green Elemental Token").len(), 2);
}

#[test]
fn titania_sees_a_land_that_dies_together_with_it() {
    let mut state = plains();
    let titania = put(
        &mut state,
        P0,
        "Titania, Protector of Argoth",
        Zone::Battlefield,
    );
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::zone_change(titania, Zone::Graveyard),
            ProposedEvent::zone_change(forest, Zone::Graveyard),
        ],
    );
    settled(&mut state);
    assert_eq!(named(&state, P0, "Green Elemental Token").len(), 1);
}

#[test]
fn titania_ignores_lands_that_leave_any_other_way_and_other_permanents() {
    let mut state = plains();
    put(
        &mut state,
        P0,
        "Titania, Protector of Argoth",
        Zone::Battlefield,
    );
    let exiled = put(&mut state, P0, "Forest", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Mountain", Zone::Battlefield);
    to_exile(&mut state, exiled);
    to_graveyard(&mut state, elves);
    to_graveyard(&mut state, theirs);
    settled(&mut state);
    assert!(named(&state, P0, "Green Elemental Token").is_empty());
    assert!(named(&state, P1, "Green Elemental Token").is_empty());
}

#[test]
fn titania_returning_a_land_that_later_dies_makes_an_elemental() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    cast_titania(&mut state);
    settle_first(&mut state);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    to_graveyard(&mut state, forest);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Green Elemental Token").len(), 1);
}

#[test]
fn a_land_dying_without_titania_makes_nothing() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    to_graveyard(&mut state, forest);
    settled(&mut state);
    assert!(named(&state, P0, "Green Elemental Token").is_empty());
}

// ---- Otharri, Suns' Glory ------------------------------------------------------------

fn experience(state: &GameState, player: PlayerId) -> u32 {
    mtg_kernel::eternal_cube_c5b_v1::experience_counters(state, player)
}

#[test]
fn otharri_gets_an_experience_counter_and_a_tapped_attacking_rebel() {
    let mut state = plains();
    let otharri = put(&mut state, P0, "Otharri, Suns' Glory", Zone::Battlefield);
    assert_eq!(experience(&state, P0), 0);
    attack_with(&mut state, vec![otharri], |_, _| None);
    assert_eq!(experience(&state, P0), 1);
    assert_eq!(experience(&state, P1), 0);
    let rebels = named(&state, P0, "Rebel Token");
    assert_eq!(rebels.len(), 1);
    assert_eq!(power_toughness(&state, rebels[0]), (2, 2));
    assert!(is_tapped(&state, rebels[0]));
    // 3 flying lifelink damage and 2 from the Rebel that attacks.
    assert_eq!(life(&state, P1), 15);
    assert_eq!(life(&state, P0), 23);
}

#[test]
fn otharri_makes_a_rebel_for_each_experience_counter_you_have() {
    let mut state = plains();
    let otharri = put(&mut state, P0, "Otharri, Suns' Glory", Zone::Battlefield);
    state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .c5b
        .experience = [2, 0];
    attack_with(&mut state, vec![otharri], |_, _| None);
    assert_eq!(experience(&state, P0), 3);
    assert_eq!(named(&state, P0, "Rebel Token").len(), 3);
    assert_eq!(life(&state, P1), 20 - 3 - 6);
}

#[test]
fn otharri_counts_only_its_controllers_experience() {
    let mut state = plains();
    let otharri = put(&mut state, P0, "Otharri, Suns' Glory", Zone::Battlefield);
    state
        .eternal_v1
        .get_or_insert_with(Default::default)
        .c5b
        .experience = [0, 4];
    attack_with(&mut state, vec![otharri], |_, _| None);
    assert_eq!(named(&state, P0, "Rebel Token").len(), 1);
    assert_eq!(experience(&state, P0), 1);
    assert_eq!(experience(&state, P1), 4);
}

#[test]
fn otharri_returns_from_the_graveyard_tapped_by_tapping_an_untapped_rebel() {
    let mut state = plains();
    let otharri = put(&mut state, P0, "Otharri, Suns' Glory", Zone::Graveyard);
    let rebel = token(&mut state, P0, "Rebel Token");
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 2);
    assert!(activatable(&mut state, otharri, 0));
    activate(&mut state, otharri, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, otharri), Zone::Battlefield);
    assert!(is_tapped(&state, otharri), "it returns tapped");
    assert!(is_tapped(&state, rebel), "the Rebel paid the cost");
    assert_eq!(mana(&state, P0, ManaColor::R), 0);
    assert_eq!(mana(&state, P0, ManaColor::W), 0);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn otharri_cant_return_without_an_untapped_rebel_or_its_mana() {
    // A tapped Rebel can't pay.
    let mut state = plains();
    let otharri = put(&mut state, P0, "Otharri, Suns' Glory", Zone::Graveyard);
    let rebel = token(&mut state, P0, "Rebel Token");
    tap(&mut state, rebel);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 2);
    assert!(!activatable(&mut state, otharri, 0));

    // Neither can another player's Rebel.
    let mut state = plains();
    let otharri = put(&mut state, P0, "Otharri, Suns' Glory", Zone::Graveyard);
    token(&mut state, P1, "Rebel Token");
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 2);
    assert!(!activatable(&mut state, otharri, 0));

    // Three mana is not enough.
    let mut state = plains();
    let otharri = put(&mut state, P0, "Otharri, Suns' Glory", Zone::Graveyard);
    token(&mut state, P0, "Rebel Token");
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 1);
    assert!(!activatable(&mut state, otharri, 0));

    // It works only from the graveyard.
    let mut state = plains();
    let otharri = put(&mut state, P0, "Otharri, Suns' Glory", Zone::Battlefield);
    token(&mut state, P0, "Rebel Token");
    pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 2);
    assert!(!activatable(&mut state, otharri, 0));
}

// ---- Public sessions -----------------------------------------------------------------

#[test]
fn c5b_decks_finish_and_replay_through_public_sessions_both_seats() {
    let ramp_heavy = deck(&[
        ("Forest", 8),
        ("Plains", 5),
        ("Mountain", 5),
        ("Sol Ring", 4),
        ("Mind Stone", 4),
        ("Llanowar Elves", 4),
        ("Worldspine Wurm", 4),
        ("Myr Battlesphere", 4),
        ("Mole Man, Moloid Master", 3),
        ("Torsten, Founder of Benalia", 4),
        ("Titania, Protector of Argoth", 4),
        ("Otharri, Suns' Glory", 4),
        ("Thraben Inspector", 4),
        ("Lightning Bolt", 3),
    ]);
    let tempo = deck(&[
        ("Forest", 7),
        ("Plains", 6),
        ("Mountain", 4),
        ("Sol Ring", 4),
        ("Mind Stone", 3),
        ("Llanowar Elves", 4),
        ("Worldspine Wurm", 3),
        ("Myr Battlesphere", 4),
        ("Mole Man, Moloid Master", 4),
        ("Torsten, Founder of Benalia", 3),
        ("Titania, Protector of Argoth", 3),
        ("Otharri, Suns' Glory", 4),
        ("Thraben Inspector", 4),
        ("Giant Growth", 3),
        ("Lightning Bolt", 4),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    for (pair, first_seed) in [
        ([ramp_heavy.clone(), tempo.clone()], 0x4335_4200_u64),
        ([tempo, ramp_heavy], 0x4335_4210),
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
    for name in C5B_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
