//! Vintage Cube wave C4b: native rules and public-session tests for the
//! magecraft, per-turn resolution counters and cast triggers.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, WardCostDef,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{propose_and_commit, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C4B_CARDS: [&str; 9] = [
    "Sedgemoor Witch",
    "Witherbloom Apprentice",
    "Omnath, Locus of Creation",
    "Scythecat Cub",
    "Pompous Battlemage",
    "Sage of the Skies",
    "Okoye, Mighty and Adored",
    "Ouroboroid",
    "Screaming Nemesis",
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
        0x4355_4245_0000 + 0x3462,
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
fn c4b_cards_are_full() {
    for name in C4B_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

// ---- Helpers of this wave -------------------------------------------------------------

fn name_of(state: &GameState, object: ObjectId) -> &'static str {
    CARD_DEFS[state.objects.get(object).card_def as usize].name
}

/// Whether a triggered ability is the top item of the stack.
fn trigger_on_top(state: &GameState) -> bool {
    state
        .stack
        .last()
        .is_some_and(|item| item.kind == StackItemKind::TriggeredAbility)
}

/// Plays on (passing priority, ordering triggers as given, answering a target
/// prompt with `chosen` when it is legal) until a triggered ability is the top
/// of the stack and its controller holds priority.
fn until_trigger_on_stack(state: &mut GameState, chosen: Option<Target>) {
    for _ in 0..100 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. }) && trigger_on_top(state) {
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(
                chosen
                    .filter(|target| legal_targets.contains(target))
                    .unwrap_or(legal_targets[0]),
            ),
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("no triggered ability reached the stack");
}

/// P0's second main phase with an empty stack.
fn second_main_settled(state: &GameState) -> bool {
    state.step == Step::Main2
        && state.stack.is_empty()
        && state.engine.pending_triggers.is_empty()
        && state.engine.pending_effect.is_none()
}

/// Moves `object` to `to` through the engine's zone-change event.
fn move_to(state: &mut GameState, object: ObjectId, to: Zone) {
    propose_and_commit(state, ProposedEvent::zone_change(object, to));
}

/// Puts a card from outside the game onto the battlefield as an entering
/// permanent (a land the controller did not play, a planeswalker): the
/// engine's zone change, so its entry triggers and loyalty see it.
fn enter(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Library);
    move_to(state, object, Zone::Battlefield);
    object
}

fn counters(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.plus1_plus1
}

fn hand_size(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

/// The copies of the prepare spell `name` in exile.
fn exile_copies(state: &GameState, name: &str) -> Vec<ObjectId> {
    state
        .exile
        .iter()
        .copied()
        .filter(|&object| name_of(state, object) == name)
        .collect()
}

/// P1 casts a Lightning Bolt at `target`; it stays on the stack with
/// whatever it triggered (a ward trigger resolves before it).
fn opponent_casts_bolt(state: &mut GameState, target: Target) -> ObjectId {
    state.priority_player = P1;
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    cast(state, bolt, &[target]);
    bolt
}

/// Answers a ward payment (a Boolean) with `pay`; whether it was asked.
fn settle_paying(state: &mut GameState, pay: bool) -> bool {
    let mut asked = false;
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectBoolean { .. } => {
            asked = true;
            Action::ChooseEffectBoolean(pay)
        }
        other => first_answer(other),
    });
    asked
}

/// Plays on to P0's declare attackers step, answering a target prompt of a
/// beginning-of-combat trigger with `target`. The targets that were offered.
fn to_declare_attackers(state: &mut GameState, target: ObjectId) -> Vec<Target> {
    let mut offered = Vec::new();
    run_until(
        state,
        |state| state.step == Step::DeclareAttackers,
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. } => {
                offered = legal_targets.clone();
                Some(Action::ChooseTarget(Target::Object(target)))
            }
            _ => None,
        },
    );
    offered
}

// ---- Characteristics -----------------------------------------------------------------

#[test]
fn c4b_cards_have_their_printed_characteristics() {
    let witch = def("Sedgemoor Witch");
    assert_eq!(witch.mana_value, 3);
    assert_eq!(witch.cost.generic, 2);
    assert_eq!(witch.cost.pips, &[Pip::Colored(ManaColor::B)]);
    assert_eq!(witch.types, &[CardType::Creature]);
    assert_eq!(witch.subtypes, &[Subtype::Human, Subtype::Warlock]);
    assert_eq!((witch.power, witch.toughness), (Some(3), Some(2)));
    assert!(witch.keywords.has(Keywords::MENACE));
    assert!(matches!(witch.ward_cost, Some(WardCostDef::PayLife(3))));

    let apprentice = def("Witherbloom Apprentice");
    assert_eq!(apprentice.mana_value, 2);
    assert_eq!(
        apprentice.cost.pips,
        &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::G)]
    );
    assert_eq!(apprentice.types, &[CardType::Creature]);
    assert_eq!(apprentice.subtypes, &[Subtype::Human, Subtype::Druid]);
    assert_eq!((apprentice.power, apprentice.toughness), (Some(2), Some(2)));

    let omnath = def("Omnath, Locus of Creation");
    assert_eq!(omnath.mana_value, 4);
    assert_eq!(
        omnath.cost.pips,
        &[
            Pip::Colored(ManaColor::R),
            Pip::Colored(ManaColor::G),
            Pip::Colored(ManaColor::W),
            Pip::Colored(ManaColor::U)
        ]
    );
    assert_eq!(omnath.supertypes, &[Supertype::Legendary]);
    assert_eq!(omnath.subtypes, &[Subtype::Elemental]);
    assert_eq!((omnath.power, omnath.toughness), (Some(4), Some(4)));
    assert_eq!(omnath.colors.len(), 4);

    let cub = def("Scythecat Cub");
    assert_eq!(cub.mana_value, 2);
    assert_eq!(cub.cost.generic, 1);
    assert_eq!(cub.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(cub.subtypes, &[Subtype::Cat]);
    assert_eq!((cub.power, cub.toughness), (Some(2), Some(2)));
    assert!(cub.keywords.has(Keywords::TRAMPLE));

    let battlemage = def("Pompous Battlemage");
    assert_eq!(battlemage.mana_value, 1);
    assert_eq!(battlemage.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(battlemage.subtypes, &[Subtype::Goblin, Subtype::Sorcerer]);
    assert_eq!((battlemage.power, battlemage.toughness), (Some(1), Some(1)));
    let act = def("Improvised Act");
    assert_eq!(act.types, &[CardType::Sorcery]);
    assert_eq!(act.mana_value, 1);
    assert_eq!(act.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(act.capability, CardCapability::Full);
    assert!(!act.is_token);

    let sage = def("Sage of the Skies");
    assert_eq!(sage.mana_value, 3);
    assert_eq!(sage.cost.generic, 2);
    assert_eq!(sage.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert_eq!(sage.subtypes, &[Subtype::Human, Subtype::Monk]);
    assert_eq!((sage.power, sage.toughness), (Some(2), Some(3)));
    assert!(sage.keywords.has(Keywords::FLYING) && sage.keywords.has(Keywords::LIFELINK));

    let okoye = def("Okoye, Mighty and Adored");
    assert_eq!(okoye.mana_value, 4);
    assert_eq!(okoye.cost.generic, 2);
    assert_eq!(
        okoye.cost.pips,
        &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::W)]
    );
    assert_eq!(okoye.supertypes, &[Supertype::Legendary]);
    assert_eq!(
        okoye.subtypes,
        &[Subtype::Human, Subtype::Warrior, Subtype::Hero]
    );
    assert_eq!((okoye.power, okoye.toughness), (Some(3), Some(3)));

    let ouroboroid = def("Ouroboroid");
    assert_eq!(ouroboroid.mana_value, 4);
    assert_eq!(ouroboroid.cost.generic, 2);
    assert_eq!(
        ouroboroid.cost.pips,
        &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)]
    );
    assert_eq!(ouroboroid.subtypes, &[Subtype::Plant, Subtype::Wurm]);
    assert_eq!((ouroboroid.power, ouroboroid.toughness), (Some(1), Some(3)));

    let nemesis = def("Screaming Nemesis");
    assert_eq!(nemesis.mana_value, 3);
    assert_eq!(nemesis.cost.generic, 2);
    assert_eq!(nemesis.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(nemesis.subtypes, &[Subtype::Spirit]);
    assert_eq!((nemesis.power, nemesis.toughness), (Some(3), Some(3)));
    assert!(nemesis.keywords.has(Keywords::HASTE));
}

// ---- Sedgemoor Witch ---------------------------------------------------------------

#[test]
fn sedgemoor_witch_makes_a_pest_for_each_instant_or_sorcery_you_cast() {
    let mut state = plains();
    let witch = put(&mut state, P0, "Sedgemoor Witch", Zone::Battlefield);
    assert!(keyword(&state, witch, Keywords::MENACE));
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    let _ = next(&mut state);
    assert_eq!(state.stack.len(), 2, "the spell and the trigger");
    assert!(trigger_on_top(&state), "the trigger resolves first");
    assert!(named(&state, P0, "Pest Token").is_empty());
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
    let pests = named(&state, P0, "Pest Token");
    assert_eq!(pests.len(), 1);
    assert_eq!(power_toughness(&state, pests[0]), (1, 1));
    assert!(engine::has_effective_subtype(
        &state,
        pests[0],
        Subtype::Pest
    ));
    assert!(state.objects.get(pests[0]).v4.is_token);

    // "When this token dies, you gain 1 life."
    let life_before = life(&state, P0);
    opponent_bolts(&mut state, Target::Object(pests[0]));
    settled(&mut state);
    assert_eq!(zone(&state, pests[0]), Zone::Graveyard);
    assert_eq!(life(&state, P0), life_before + 1);
    assert_eq!(named(&state, P0, "Sedgemoor Witch"), vec![witch]);
}

#[test]
fn sedgemoor_witch_ignores_creature_spells_lands_and_the_opponents_spells() {
    let mut state = plains();
    put(&mut state, P0, "Sedgemoor Witch", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Hand);
    play_land(&mut state, forest);
    opponent_bolts(&mut state, Target::Player(P0));
    settled(&mut state);
    assert!(named(&state, P0, "Pest Token").is_empty());
    assert!(named(&state, P1, "Pest Token").is_empty());
}

#[test]
fn sedgemoor_witch_triggers_for_the_spell_and_each_copy_of_a_storm_spell() {
    let mut state = plains();
    put(&mut state, P0, "Sedgemoor Witch", Zone::Battlefield);
    for _ in 0..2 {
        let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::B], 0);
        cast(&mut state, ritual, &[]);
        settled(&mut state);
    }
    assert_eq!(named(&state, P0, "Pest Token").len(), 2, "one per cast");
    let tendrils = put(&mut state, P0, "Tendrils of Agony", Zone::Hand);
    cast(&mut state, tendrils, &[Target::Player(P1)]);
    settle_first(&mut state);
    assert_eq!(life(&state, P1), 14, "the spell and two copies");
    assert_eq!(
        named(&state, P0, "Pest Token").len(),
        5,
        "two rituals, the cast of Tendrils and its two copies"
    );
    // The opponent's Witch is not triggered by my copies.
    assert!(named(&state, P1, "Pest Token").is_empty());
}

#[test]
fn sedgemoor_witch_ward_costs_the_opponent_three_life_or_counters_the_spell() {
    for pay in [false, true] {
        let mut state = plains();
        let witch = put(&mut state, P0, "Sedgemoor Witch", Zone::Battlefield);
        let bolt = opponent_casts_bolt(&mut state, Target::Object(witch));
        assert!(
            settle_paying(&mut state, pay),
            "the ward payment is offered"
        );
        assert_eq!(zone(&state, bolt), Zone::Graveyard, "pay {pay}");
        if pay {
            assert_eq!(life(&state, P1), 17, "three life paid");
            assert_eq!(zone(&state, witch), Zone::Graveyard, "the Bolt resolved");
        } else {
            assert_eq!(life(&state, P1), 20);
            assert_eq!(zone(&state, witch), Zone::Battlefield, "countered");
        }
    }
}

#[test]
fn sedgemoor_witch_ward_counters_the_spell_when_the_opponent_cannot_pay() {
    let mut state = plains();
    state.players[P1.index()].life = 2;
    let witch = put(&mut state, P0, "Sedgemoor Witch", Zone::Battlefield);
    let bolt = opponent_casts_bolt(&mut state, Target::Object(witch));
    assert!(!settle_paying(&mut state, true), "nothing to pay with");
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, witch), Zone::Battlefield);
    assert_eq!(life(&state, P1), 2);
}

// ---- Witherbloom Apprentice -----------------------------------------------------------

#[test]
fn witherbloom_apprentice_drains_one_for_each_instant_or_sorcery_cast() {
    let mut state = plains();
    put(&mut state, P0, "Witherbloom Apprentice", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 16, "Bolt and one life lost");
    assert_eq!(life(&state, P0), 21);

    // Not for a creature spell, a land, or the opponent's spell.
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    opponent_bolts(&mut state, Target::Player(P0));
    settled(&mut state);
    assert_eq!(life(&state, P1), 16);
    assert_eq!(life(&state, P0), 18, "only the opponent's Bolt");
}

#[test]
fn witherbloom_apprentice_triggers_for_the_spell_and_each_copy_of_a_storm_spell() {
    let mut state = plains();
    put(&mut state, P0, "Witherbloom Apprentice", Zone::Battlefield);
    for _ in 0..2 {
        let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::B], 0);
        cast(&mut state, ritual, &[]);
        settled(&mut state);
    }
    assert_eq!(life(&state, P1), 18);
    assert_eq!(life(&state, P0), 22);
    let tendrils = put(&mut state, P0, "Tendrils of Agony", Zone::Hand);
    cast(&mut state, tendrils, &[Target::Player(P1)]);
    settle_first(&mut state);
    // Tendrils drains two three times; the Apprentice drains one for the
    // two Rituals, the cast of Tendrils and each of its two copies.
    assert_eq!(life(&state, P1), 9);
    assert_eq!(life(&state, P0), 31);
}

// ---- Omnath, Locus of Creation --------------------------------------------------------

#[test]
fn omnath_draws_a_card_when_it_enters() {
    let mut state = plains();
    let omnath = put(&mut state, P0, "Omnath, Locus of Creation", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::G, ManaColor::W, ManaColor::U],
        0,
    );
    let hand = hand_size(&state, P0);
    let library = state.players[0].library.len();
    cast(&mut state, omnath, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, omnath), Zone::Battlefield);
    assert_eq!(power_toughness(&state, omnath), (4, 4));
    assert_eq!(hand_size(&state, P0), hand, "cast Omnath, drew a card");
    assert_eq!(state.players[0].library.len(), library - 1);
}

#[test]
fn omnath_landfall_gains_life_then_adds_mana_then_deals_damage_then_does_nothing() {
    let mut state = plains();
    let omnath = put(
        &mut state,
        P0,
        "Omnath, Locus of Creation",
        Zone::Battlefield,
    );
    let mine = enter(&mut state, P0, "Wrenn and Six");
    let theirs = enter(&mut state, P1, "Wrenn and Six");
    settled(&mut state);
    let colors = [ManaColor::R, ManaColor::G, ManaColor::W, ManaColor::U];

    // The first resolution: four life.
    enter(&mut state, P0, "Plains");
    settled(&mut state);
    assert_eq!(life(&state, P0), 24);
    assert_eq!(life(&state, P1), 20);
    for color in colors {
        assert_eq!(mana(&state, P0, color), 0);
    }

    // The second: {R}{G}{W}{U}.
    enter(&mut state, P0, "Plains");
    settled(&mut state);
    assert_eq!(life(&state, P0), 24);
    for color in colors {
        assert_eq!(mana(&state, P0, color), 1, "{color:?}");
    }

    // The third: four damage to each opponent and each planeswalker you
    // don't control.
    enter(&mut state, P0, "Plains");
    settled(&mut state);
    assert_eq!(life(&state, P1), 16);
    assert_eq!(life(&state, P0), 24);
    assert_eq!(zone(&state, theirs), Zone::Graveyard, "loyalty 3, 4 damage");
    assert_eq!(zone(&state, mine), Zone::Battlefield, "yours is not hit");
    assert_eq!(zone(&state, omnath), Zone::Battlefield);

    // The fourth and later: nothing.
    enter(&mut state, P0, "Plains");
    settled(&mut state);
    assert_eq!(life(&state, P1), 16);
    assert_eq!(life(&state, P0), 24);
    for color in colors {
        assert_eq!(mana(&state, P0, color), 1, "{color:?}");
    }
}

#[test]
fn omnath_counts_its_resolutions_per_turn_and_per_omnath() {
    let mut state = plains();
    put(
        &mut state,
        P0,
        "Omnath, Locus of Creation",
        Zone::Battlefield,
    );
    put(
        &mut state,
        P1,
        "Omnath, Locus of Creation",
        Zone::Battlefield,
    );
    enter(&mut state, P0, "Plains");
    settled(&mut state);
    assert_eq!(life(&state, P0), 24);
    // The opponent's land is the first resolution of the opponent's Omnath.
    enter(&mut state, P1, "Plains");
    settled(&mut state);
    assert_eq!(life(&state, P1), 24, "its own first time");
    assert_eq!(life(&state, P0), 24, "not triggered by their land");
    for color in [ManaColor::R, ManaColor::G, ManaColor::W, ManaColor::U] {
        assert_eq!(mana(&state, P0, color), 0);
        assert_eq!(mana(&state, P1, color), 0);
    }
    // The second land of each.
    enter(&mut state, P0, "Plains");
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    assert_eq!(mana(&state, P1, ManaColor::R), 0);
    enter(&mut state, P1, "Plains");
    settled(&mut state);
    assert_eq!(mana(&state, P1, ManaColor::R), 1);

    // A new turn starts the count again.
    state.turn += 1;
    state.players[P0.index()].mana_pool = [0; 6];
    enter(&mut state, P0, "Plains");
    settled(&mut state);
    assert_eq!(life(&state, P0), 28, "the first time this turn");
    assert_eq!(mana(&state, P0, ManaColor::R), 0);

    // So does the other player's turn of the same round.
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    let life_before = life(&state, P0);
    enter(&mut state, P0, "Plains");
    settled(&mut state);
    assert_eq!(
        life(&state, P0),
        life_before + 4,
        "the first time in the opponent's turn"
    );
    assert_eq!(mana(&state, P0, ManaColor::R), 0);
}

// ---- Scythecat Cub -----------------------------------------------------------------

#[test]
fn scythecat_cub_adds_a_counter_then_doubles_on_the_second_resolution_only() {
    let mut state = plains();
    let cub = put(&mut state, P0, "Scythecat Cub", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let mut offered = Vec::new();
    enter(&mut state, P0, "Plains");
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            offered = legal_targets.clone();
            Action::ChooseTarget(Target::Object(elves))
        }
        other => first_answer(other),
    });
    assert!(offered.contains(&Target::Object(cub)));
    assert!(offered.contains(&Target::Object(elves)));
    assert!(
        !offered.contains(&Target::Object(theirs)),
        "a creature you control"
    );
    assert_eq!(counters(&state, elves), 1, "the first time: one counter");
    assert_eq!(power_toughness(&state, elves), (2, 2));

    enter(&mut state, P0, "Plains");
    settle_choosing(&mut state, elves);
    assert_eq!(counters(&state, elves), 2, "the second time: doubled");

    enter(&mut state, P0, "Plains");
    settle_choosing(&mut state, elves);
    assert_eq!(counters(&state, elves), 3, "the third time: one counter");

    enter(&mut state, P0, "Plains");
    settle_choosing(&mut state, elves);
    assert_eq!(counters(&state, elves), 4);
    assert_eq!(counters(&state, theirs), 0);
}

#[test]
fn scythecat_cub_doubling_a_creature_without_counters_adds_none() {
    let mut state = plains();
    let cub = put(&mut state, P0, "Scythecat Cub", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    enter(&mut state, P0, "Plains");
    settle_choosing(&mut state, cub);
    assert_eq!(counters(&state, cub), 1);
    // The second resolution doubles the Elves' nothing.
    enter(&mut state, P0, "Plains");
    settle_choosing(&mut state, elves);
    assert_eq!(counters(&state, elves), 0);
    // The third is an ordinary counter.
    enter(&mut state, P0, "Plains");
    settle_choosing(&mut state, elves);
    assert_eq!(counters(&state, elves), 1);
}

#[test]
fn scythecat_cub_trigger_with_a_vanished_target_does_not_count_as_resolved() {
    let mut state = plains();
    let cub = put(&mut state, P0, "Scythecat Cub", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    enter(&mut state, P0, "Plains");
    settle_choosing(&mut state, cub);
    assert_eq!(counters(&state, cub), 1);

    // The second trigger loses its only target before it resolves.
    enter(&mut state, P0, "Plains");
    until_trigger_on_stack(&mut state, Some(Target::Object(elves)));
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].targets, vec![Target::Object(elves)]);
    move_to(&mut state, elves, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(counters(&state, cub), 1);

    // So the next one is the second resolution: it doubles.
    enter(&mut state, P0, "Plains");
    settle_choosing(&mut state, cub);
    assert_eq!(counters(&state, cub), 2);
}

// ---- Pompous Battlemage ----------------------------------------------------------------

#[test]
fn pompous_battlemage_enters_prepared_and_its_improvised_act_copy_may_loot() {
    let mut state = plains();
    let battlemage = put(&mut state, P0, "Pompous Battlemage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, battlemage, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, battlemage), Zone::Battlefield);
    assert_eq!(power_toughness(&state, battlemage), (1, 1));
    let copies = exile_copies(&state, "Improvised Act");
    assert_eq!(copies.len(), 1, "it enters prepared");
    let act = copies[0];
    assert!(!castable(&mut state, act), "Improvised Act costs {{R}}");

    let fodder = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let library = state.players[0].library.len();
    cast(&mut state, act, &[]);
    settle_choosing(&mut state, fodder);
    assert_eq!(zone(&state, fodder), Zone::Graveyard, "discarded");
    assert_eq!(state.players[0].library.len(), library - 1, "and drew");
    assert_eq!(hand_size(&state, P0), 1);
    assert!(
        exile_copies(&state, "Improvised Act").is_empty(),
        "casting the copy unprepares it"
    );
    assert_eq!(
        power_toughness(&state, battlemage),
        (2, 2),
        "prowess: the copy was a noncreature spell cast"
    );
}

#[test]
fn pompous_battlemage_improvised_act_draw_may_be_dredged() {
    // "If you do, draw a card": the draw after the discard may be replaced by
    // dredge (702.52), here Golgari Thug's dredge 4.
    let mut state = plains();
    let battlemage = put(&mut state, P0, "Pompous Battlemage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, battlemage, &[]);
    settled(&mut state);
    let act = exile_copies(&state, "Improvised Act")[0];
    let thug = put(&mut state, P0, "Golgari Thug", Zone::Graveyard);
    let fodder = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let library = state.players[0].library.len();
    cast(&mut state, act, &[]);
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(fodder)) =>
        {
            Action::ChooseEffectTarget(Target::Object(fodder))
        }
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } => {
            offered.push((player, legal_targets));
            Action::ChooseEffectTarget(Target::Object(thug))
        }
        other => first_answer(other),
    });
    assert_eq!(offered, vec![(P0, vec![Target::Object(thug)])], "dredge");
    assert_eq!(zone(&state, fodder), Zone::Graveyard, "discarded");
    assert_eq!(zone(&state, thug), Zone::Hand, "dredged instead of drawing");
    assert_eq!(state.players[0].library.len(), library - 4);
}

#[test]
fn pompous_battlemage_improvised_act_may_be_declined_or_have_nothing_to_discard() {
    // Declined: nothing is discarded or drawn.
    let mut state = plains();
    let battlemage = put(&mut state, P0, "Pompous Battlemage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, battlemage, &[]);
    settled(&mut state);
    let act = exile_copies(&state, "Improvised Act")[0];
    let fodder = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let library = state.players[0].library.len();
    cast(&mut state, act, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, fodder), Zone::Hand);
    assert_eq!(state.players[0].library.len(), library);
    assert!(exile_copies(&state, "Improvised Act").is_empty());

    // With an empty hand there is nothing to discard, so nothing is drawn.
    let mut state = plains();
    let battlemage = put(&mut state, P0, "Pompous Battlemage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, battlemage, &[]);
    settled(&mut state);
    let act = exile_copies(&state, "Improvised Act")[0];
    pool(&mut state, P0, &[ManaColor::R], 0);
    let library = state.players[0].library.len();
    cast(&mut state, act, &[]);
    settle_first(&mut state);
    assert_eq!(state.players[0].library.len(), library);
    assert_eq!(hand_size(&state, P0), 0);
}

#[test]
fn pompous_battlemage_has_prowess_and_its_copy_leaves_with_it() {
    let mut state = plains();
    let battlemage = put(&mut state, P0, "Pompous Battlemage", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(
        power_toughness(&state, battlemage),
        (2, 2),
        "noncreature spell"
    );
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(
        power_toughness(&state, battlemage),
        (2, 2),
        "a creature spell is not enough"
    );

    // Entered by being put onto the battlefield (not cast): still prepared,
    // and the copy ceases to exist when the creature leaves.
    let mut state = plains();
    let battlemage = enter(&mut state, P0, "Pompous Battlemage");
    settled(&mut state);
    assert_eq!(exile_copies(&state, "Improvised Act").len(), 1);
    move_to(&mut state, battlemage, Zone::Graveyard);
    settled(&mut state);
    assert!(exile_copies(&state, "Improvised Act").is_empty());
}

#[test]
fn pompous_battlemage_does_not_enter_prepared_without_its_abilities_or_face_down() {
    // Under Dress Down ("Creatures lose all abilities") it enters unprepared
    // and no Improvised Act copy is created.
    let mut state = plains();
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    let blanked = put(&mut state, P0, "Pompous Battlemage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, dress), Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, blanked, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, blanked), Zone::Battlefield);
    assert!(
        exile_copies(&state, "Improvised Act").is_empty(),
        "no prepare spell copy under Dress Down"
    );

    // A face-down permanent (Abhorrent Oculus manifests dread at each
    // opponent's upkeep) has no abilities, so it is not prepared either.
    let mut state = game(&["Pompous Battlemage"; 20]);
    put(&mut state, P0, "Abhorrent Oculus", Zone::Battlefield);
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    let face_down: Vec<ObjectId> = state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| state.objects.get(object).v4.face_down_v1.is_some())
        .collect();
    assert_eq!(face_down.len(), 1, "manifested one");
    assert!(
        exile_copies(&state, "Improvised Act").is_empty(),
        "a manifested Battlemage is not prepared and reveals nothing"
    );
}

// ---- Sage of the Skies --------------------------------------------------------------------

/// Casts `spell` with `colors` plus `generic` colorless mana and plays on until it resolves.
fn cast_and_resolve(
    state: &mut GameState,
    spell: ObjectId,
    colors: &[ManaColor],
    generic: u8,
    targets: &[Target],
) {
    pool(state, P0, colors, generic);
    cast(state, spell, targets);
    settled(state);
}

#[test]
fn sage_of_the_skies_copies_itself_if_you_cast_another_spell_this_turn() {
    let mut state = plains();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    cast_and_resolve(&mut state, bolt, &[ManaColor::R], 0, &[Target::Player(P1)]);
    let sage = put(&mut state, P0, "Sage of the Skies", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, sage, &[]);
    let _ = next(&mut state);
    assert_eq!(state.stack.len(), 2, "the Sage and its cast trigger");
    assert!(trigger_on_top(&state));
    settled(&mut state);
    let sages = named(&state, P0, "Sage of the Skies");
    assert_eq!(sages.len(), 2, "the Sage and its copy");
    assert!(sages.contains(&sage));
    assert!(!state.objects.get(sage).v4.is_token);
    let tokens = sages
        .iter()
        .filter(|&&object| state.objects.get(object).v4.is_token)
        .count();
    assert_eq!(tokens, 1, "the copy became a token");
    for &object in &sages {
        assert_eq!(power_toughness(&state, object), (2, 3));
        assert!(keyword(&state, object, Keywords::FLYING));
        assert!(keyword(&state, object, Keywords::LIFELINK));
    }
    assert_eq!(
        state.players[0].spells_cast_this_turn, 2,
        "the copy was not cast"
    );
}

#[test]
fn sage_of_the_skies_does_not_copy_itself_as_your_first_spell_of_the_turn() {
    // Nothing cast before, and an opponent's spell does not count.
    let mut state = plains();
    opponent_bolts(&mut state, Target::Player(P0));
    settled(&mut state);
    let sage = put(&mut state, P0, "Sage of the Skies", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, sage, &[]);
    let _ = next(&mut state);
    assert_eq!(state.stack.len(), 1, "no cast trigger");
    settled(&mut state);
    assert_eq!(named(&state, P0, "Sage of the Skies"), vec![sage]);
    assert!(!state.objects.get(sage).v4.is_token);
}

#[test]
fn magecraft_does_not_see_the_copy_of_a_creature_spell() {
    let mut state = plains();
    put(&mut state, P0, "Sedgemoor Witch", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    cast_and_resolve(&mut state, bolt, &[ManaColor::R], 0, &[Target::Player(P1)]);
    assert_eq!(named(&state, P0, "Pest Token").len(), 1);
    let sage = put(&mut state, P0, "Sage of the Skies", Zone::Hand);
    cast_and_resolve(&mut state, sage, &[ManaColor::W], 2, &[]);
    assert_eq!(named(&state, P0, "Sage of the Skies").len(), 2);
    assert_eq!(
        named(&state, P0, "Pest Token").len(),
        1,
        "neither the Sage nor its copy is an instant or sorcery"
    );
}

// ---- Okoye, Mighty and Adored -------------------------------------------------------------

#[test]
fn okoye_makes_you_the_monarch_when_it_enters() {
    let mut state = plains();
    state.monarch = Some(P1);
    let okoye = put(&mut state, P0, "Okoye, Mighty and Adored", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::W], 2);
    cast(&mut state, okoye, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, okoye), Zone::Battlefield);
    assert_eq!(power_toughness(&state, okoye), (3, 3));
    assert_eq!(state.monarch, Some(P0));
}

/// A game with Okoye and an Elves of P0 in which the beginning-of-combat
/// trigger chose the Elves and they attacked P1 alone; `monarch` holds the
/// monarchy first.
fn okoye_attack(monarch: Option<PlayerId>) -> (GameState, ObjectId, Vec<Target>) {
    let mut state = plains();
    put(
        &mut state,
        P0,
        "Okoye, Mighty and Adored",
        Zone::Battlefield,
    );
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    state.monarch = monarch;
    let offered = to_declare_attackers(&mut state, elves);
    assert!(offered.contains(&Target::Object(theirs)), "any creature");
    attack_with(&mut state, vec![elves], |_, _| None);
    (state, elves, offered)
}

#[test]
fn okoye_counter_and_a_creature_that_attacks_the_monarch_gains_double_strike_and_trample() {
    let (state, elves, offered) = okoye_attack(Some(P1));
    assert_eq!(offered.len(), 3, "Okoye, her Elves and theirs");
    assert_eq!(counters(&state, elves), 1);
    assert_eq!(power_toughness(&state, elves), (2, 2));
    assert!(keyword(&state, elves, Keywords::DOUBLE_STRIKE));
    assert!(keyword(&state, elves, Keywords::TRAMPLE));
    assert_eq!(life(&state, P1), 16, "two damage in each damage step");
}

#[test]
fn okoye_grants_nothing_when_the_creature_does_not_attack_the_monarch() {
    // Nobody is the monarch.
    let (state, elves, _) = okoye_attack(None);
    assert_eq!(counters(&state, elves), 1, "the counter is unconditional");
    assert!(!keyword(&state, elves, Keywords::DOUBLE_STRIKE));
    assert!(!keyword(&state, elves, Keywords::TRAMPLE));
    assert_eq!(life(&state, P1), 18);

    // The attacker's own controller is the monarch.
    let (state, elves, _) = okoye_attack(Some(P0));
    assert_eq!(counters(&state, elves), 1);
    assert!(!keyword(&state, elves, Keywords::DOUBLE_STRIKE));
    assert!(!keyword(&state, elves, Keywords::TRAMPLE));
    assert_eq!(life(&state, P1), 18);
}

#[test]
fn okoye_mark_ends_with_the_turn_even_within_the_same_round() {
    // Okoye's controller is the monarch and marks the opponent's creature.
    // "This turn" is over when that creature attacks the monarch in its own
    // controller's turn of the same round.
    let mut state = plains();
    // Okoye is cast, so the monarchy has the source the end step draw needs.
    let okoye = put(&mut state, P0, "Okoye, Mighty and Adored", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::W], 2);
    cast(&mut state, okoye, &[]);
    settled(&mut state);
    assert_eq!(state.monarch, Some(P0));
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    to_declare_attackers(&mut state, theirs);
    act(&mut state, Action::DeclareAttackers(Vec::new()));
    assert_eq!(counters(&state, theirs), 1, "the counter is put on it");
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::DeclareAttackers
    });
    assert_eq!(state.monarch, Some(P0));
    act(&mut state, Action::DeclareAttackers(vec![theirs]));
    run_until(
        &mut state,
        |state| {
            state.step == Step::Main2
                && state.stack.is_empty()
                && state.engine.pending_triggers.is_empty()
                && state.engine.pending_effect.is_none()
        },
        |_, _| None,
    );
    assert!(!keyword(&state, theirs, Keywords::DOUBLE_STRIKE));
    assert!(!keyword(&state, theirs, Keywords::TRAMPLE));
    assert_eq!(life(&state, P0), 18, "one damage step of a 2/2");
}

#[test]
fn okoye_grants_nothing_to_a_creature_that_attacks_a_planeswalker_of_the_monarch() {
    let mut state = plains();
    put(
        &mut state,
        P0,
        "Okoye, Mighty and Adored",
        Zone::Battlefield,
    );
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let walker = enter(&mut state, P1, "Wrenn and Six");
    settled(&mut state);
    state.monarch = Some(P1);
    to_declare_attackers(&mut state, elves);
    attack_with(&mut state, vec![elves], |_, decision| match decision {
        Decision::ChooseAttackTarget { candidates, .. } => candidates
            .iter()
            .copied()
            .find(|target| matches!(target, Target::Object(_)))
            .map(Action::ChooseAttackTarget),
        _ => None,
    });
    assert_eq!(zone(&state, walker), Zone::Battlefield);
    assert_eq!(life(&state, P1), 20, "the planeswalker was attacked");
    assert!(!keyword(&state, elves, Keywords::DOUBLE_STRIKE));
    assert!(!keyword(&state, elves, Keywords::TRAMPLE));
}

// ---- Ouroboroid ---------------------------------------------------------------------------

#[test]
fn ouroboroid_puts_x_counters_on_each_creature_you_control_with_x_fixed_first() {
    let mut state = plains();
    let ouroboroid = put(&mut state, P0, "Ouroboroid", Zone::Battlefield);
    state.objects.get_mut(ouroboroid).counters.plus1_plus1 = 2;
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert_eq!(power_toughness(&state, ouroboroid), (3, 5));
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    assert_eq!(counters(&state, elves), 3, "X is its power, 3");
    assert_eq!(
        counters(&state, ouroboroid),
        5,
        "its own counters do not change X"
    );
    assert_eq!(counters(&state, theirs), 0, "only your creatures");
}

#[test]
fn ouroboroid_alone_grows_by_its_power_each_combat() {
    let mut state = plains();
    let ouroboroid = put(&mut state, P0, "Ouroboroid", Zone::Battlefield);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    assert_eq!(counters(&state, ouroboroid), 1);
    assert_eq!(power_toughness(&state, ouroboroid), (2, 4));
}

#[test]
fn ouroboroid_that_left_the_battlefield_uses_its_last_known_power() {
    let mut state = plains();
    let ouroboroid = put(&mut state, P0, "Ouroboroid", Zone::Battlefield);
    state.objects.get_mut(ouroboroid).counters.plus1_plus1 = 2;
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    until_trigger_on_stack(&mut state, None);
    assert_eq!(state.stack.len(), 1);
    move_to(&mut state, ouroboroid, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(zone(&state, ouroboroid), Zone::Graveyard);
    assert_eq!(counters(&state, elves), 3, "its power was 3 as it left");
}

// ---- Screaming Nemesis --------------------------------------------------------------------

#[test]
fn screaming_nemesis_deals_the_damage_it_was_dealt_to_any_other_target() {
    let mut state = plains();
    let nemesis = put(&mut state, P0, "Screaming Nemesis", Zone::Battlefield);
    assert!(keyword(&state, nemesis, Keywords::HASTE));
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    opponent_bolts(&mut state, Target::Object(nemesis));
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            offered = legal_targets.clone();
            Action::ChooseTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    assert!(
        !offered.contains(&Target::Object(nemesis)),
        "any other target"
    );
    assert!(offered.contains(&Target::Player(P0)));
    assert!(offered.contains(&Target::Player(P1)));
    assert!(offered.contains(&Target::Object(elves)));
    assert_eq!(zone(&state, nemesis), Zone::Graveyard, "the Bolt killed it");
    assert_eq!(
        life(&state, P1),
        17,
        "that much damage, from a dead Nemesis"
    );
}

#[test]
fn screaming_nemesis_that_survives_the_damage_hits_a_creature_and_locks_no_one() {
    let mut state = plains();
    let nemesis = put(&mut state, P0, "Screaming Nemesis", Zone::Battlefield);
    state.objects.get_mut(nemesis).counters.plus1_plus1 = 2;
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    opponent_bolts(&mut state, Target::Object(nemesis));
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            offered = legal_targets.clone();
            Action::ChooseTarget(Target::Object(elves))
        }
        other => first_answer(other),
    });
    assert!(
        !offered.contains(&Target::Object(nemesis)),
        "any other target: not the Nemesis itself, still on the battlefield"
    );
    assert!(offered.contains(&Target::Object(elves)));
    assert_eq!(zone(&state, nemesis), Zone::Battlefield, "5/5 survives");
    assert_eq!(state.objects.get(nemesis).damage, 3);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    // A creature was dealt the damage, so both players can still gain life.
    let before = (life(&state, P0), life(&state, P1));
    enter(&mut state, P0, "Thornwood Falls");
    settled(&mut state);
    enter(&mut state, P1, "Thornwood Falls");
    settled(&mut state);
    assert_eq!(life(&state, P0), before.0 + 1);
    assert_eq!(life(&state, P1), before.1 + 1);
}

#[test]
fn screaming_nemesis_other_target_rule_does_not_apply_to_an_ability_granted_to_it() {
    // Viridian Longbow: "Equipped creature has '{T}: This creature deals 1
    // damage to any target.'" The granted ability may target the Nemesis
    // itself; only the Nemesis's own damage trigger says "any other target".
    let mut state = plains();
    let nemesis = put(&mut state, P0, "Screaming Nemesis", Zone::Battlefield);
    let longbow = put(&mut state, P0, "Viridian Longbow", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    activate(&mut state, longbow, 0, &[Target::Object(nemesis)]);
    settled(&mut state);
    assert_eq!(
        state
            .objects
            .get(longbow)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(nemesis),
        "equipped"
    );

    let granted = u8::try_from(def("Screaming Nemesis").activated_abilities.len()).unwrap();
    assert!(activatable(&mut state, nemesis, granted));
    act(&mut state, Action::ActivateAbility(nemesis, granted));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&Target::Object(nemesis)),
                "the granted ability may target the Nemesis: {legal_targets:?}"
            );
        }
        other => panic!("expected targets, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(nemesis)));

    // The ping damages the Nemesis, whose own trigger still never offers it.
    let mut trigger_offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            trigger_offered = legal_targets.clone();
            Action::ChooseTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    assert_eq!(state.objects.get(nemesis).damage, 1, "pinged itself");
    assert!(
        !trigger_offered.is_empty(),
        "its own trigger chose a target"
    );
    assert!(
        !trigger_offered.contains(&Target::Object(nemesis)),
        "its trigger says any other target"
    );
    assert_eq!(
        life(&state, P1),
        19,
        "that much damage, to the other target"
    );
}

#[test]
fn screaming_nemesis_stops_a_player_it_damaged_from_gaining_life() {
    let mut state = plains();
    let nemesis = put(&mut state, P0, "Screaming Nemesis", Zone::Battlefield);
    state.objects.get_mut(nemesis).counters.plus1_plus1 = 2;
    opponent_bolts(&mut state, Target::Object(nemesis));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(life(&state, P1), 17);
    // The damaged player gains nothing; the other player still can.
    enter(&mut state, P1, "Thornwood Falls");
    settled(&mut state);
    assert_eq!(
        life(&state, P1),
        17,
        "can't gain life for the rest of the game"
    );
    enter(&mut state, P0, "Thornwood Falls");
    settled(&mut state);
    assert_eq!(life(&state, P0), 21);
    // It lasts: a later gain is also stopped.
    enter(&mut state, P1, "Thornwood Falls");
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
}

// ---- Public sessions ----------------------------------------------------------------------

#[test]
fn c4b_decks_finish_and_replay_through_public_sessions_both_seats() {
    let four_colors = deck(&[
        ("Taiga", 2),
        ("Savannah", 2),
        ("Plateau", 2),
        ("Tropical Island", 2),
        ("Tundra", 2),
        ("Volcanic Island", 2),
        ("Mana Confluence", 2),
        ("Forest", 1),
        ("Mountain", 1),
        ("Plains", 1),
        ("Omnath, Locus of Creation", 4),
        ("Okoye, Mighty and Adored", 3),
        ("Scythecat Cub", 3),
        ("Ouroboroid", 3),
        ("Sage of the Skies", 3),
        ("Screaming Nemesis", 3),
        ("Lightning Bolt", 4),
    ]);
    let black_green_red = deck(&[
        ("Swamp", 5),
        ("Forest", 5),
        ("Mountain", 4),
        ("Bayou", 2),
        ("Badlands", 2),
        ("Taiga", 2),
        ("Sedgemoor Witch", 4),
        ("Witherbloom Apprentice", 4),
        ("Pompous Battlemage", 4),
        ("Lightning Bolt", 4),
        ("Dark Ritual", 2),
        ("Llanowar Elves", 2),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    for (pair, first_seed) in [
        (
            [four_colors.clone(), black_green_red.clone()],
            0x4334_4200_u64,
        ),
        ([black_green_red, four_colors], 0x4334_4210),
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
    for name in C4B_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
