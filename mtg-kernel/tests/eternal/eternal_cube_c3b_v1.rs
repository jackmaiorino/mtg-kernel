//! Vintage Cube wave C3b: native rules and public-session tests for the
//! creatures with small new pieces, and exalted.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{propose_and_commit, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C3B_CARDS: [&str; 11] = [
    "Oliphaunt",
    "Stingcaster Mage",
    "Third Path Iconoclast",
    "Tireless Tracker",
    "Samut, Hazoret's Champion",
    "Marwyn, the Preserver",
    "Spellseeker",
    "Walking Ballista",
    "Noble Hierarch",
    "Spider-Woman, Stunning Savior",
    "Ignoble Hierarch",
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
        0x4355_4245_0000 + 0x3362,
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
fn c3b_cards_are_full() {
    for name in C3B_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

// ---- Helpers of this wave -------------------------------------------------------------

/// A first main phase of P1's turn with P1 holding priority.
fn opponents_turn(state: &mut GameState) {
    state.active_player = P1;
    state.priority_player = P1;
}

/// The triggered abilities on the stack.
fn triggers_on_stack(state: &GameState) -> usize {
    state
        .stack
        .iter()
        .filter(|item| item.kind == StackItemKind::TriggeredAbility)
        .count()
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

/// P0 declares `attackers`, and the game plays on (answering a target prompt
/// with `target`) until a triggered ability is on top of the stack.
fn attack_until_trigger_on_stack(
    state: &mut GameState,
    attackers: Vec<ObjectId>,
    target: Option<ObjectId>,
) {
    pass_until(state, |state| state.step == Step::DeclareAttackers);
    act(state, Action::DeclareAttackers(attackers));
    run_until(state, trigger_on_top, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            let choice = target
                .map(Target::Object)
                .filter(|target| legal_targets.contains(target))
                .unwrap_or(legal_targets[0]);
            Some(Action::ChooseTarget(choice))
        }
        _ => None,
    });
}

/// Moves `object` to `to` through the engine's zone-change event.
fn move_to(state: &mut GameState, object: ObjectId, to: Zone) {
    propose_and_commit(state, ProposedEvent::zone_change(object, to));
}

fn tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
}

fn counters(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.plus1_plus1
}

fn hand_size(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn c3b_cards_have_their_printed_characteristics() {
    let oliphaunt = def("Oliphaunt");
    assert_eq!(oliphaunt.mana_value, 6);
    assert_eq!(oliphaunt.cost.generic, 5);
    assert_eq!(oliphaunt.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(oliphaunt.types, &[CardType::Creature]);
    assert_eq!(oliphaunt.subtypes, &[Subtype::Elephant]);
    assert_eq!((oliphaunt.power, oliphaunt.toughness), (Some(6), Some(4)));
    assert!(oliphaunt.keywords.has(Keywords::TRAMPLE));
    assert_eq!(oliphaunt.activated_abilities.len(), 1, "Mountaincycling");

    let mage = def("Stingcaster Mage");
    assert_eq!(mage.mana_value, 2);
    assert_eq!(mage.cost.generic, 1);
    assert_eq!(mage.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(mage.subtypes, &[Subtype::Human, Subtype::Wizard]);
    assert_eq!((mage.power, mage.toughness), (Some(2), Some(1)));
    assert!(mage.keywords.has(Keywords::HASTE));

    let iconoclast = def("Third Path Iconoclast");
    assert_eq!(iconoclast.mana_value, 2);
    assert_eq!(
        iconoclast.cost.pips,
        &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::R)]
    );
    assert_eq!(iconoclast.subtypes, &[Subtype::Human, Subtype::Monk]);
    assert_eq!((iconoclast.power, iconoclast.toughness), (Some(2), Some(1)));

    let tracker = def("Tireless Tracker");
    assert_eq!(tracker.mana_value, 3);
    assert_eq!(tracker.cost.generic, 2);
    assert_eq!(tracker.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(tracker.subtypes, &[Subtype::Human, Subtype::Scout]);
    assert_eq!((tracker.power, tracker.toughness), (Some(3), Some(2)));

    let samut = def("Samut, Hazoret's Champion");
    assert_eq!(samut.mana_value, 2);
    assert_eq!(samut.supertypes, &[Supertype::Legendary]);
    assert_eq!(
        samut.subtypes,
        &[Subtype::Human, Subtype::Warrior, Subtype::Cleric]
    );
    assert_eq!((samut.power, samut.toughness), (Some(2), Some(2)));

    let marwyn = def("Marwyn, the Preserver");
    assert_eq!(marwyn.mana_value, 2);
    assert_eq!(marwyn.supertypes, &[Supertype::Legendary]);
    assert_eq!(marwyn.subtypes, &[Subtype::Elf, Subtype::Druid]);
    assert_eq!((marwyn.power, marwyn.toughness), (Some(3), Some(2)));
    assert_eq!(marwyn.activated_abilities.len(), 1);

    let seeker = def("Spellseeker");
    assert_eq!(seeker.mana_value, 3);
    assert_eq!(seeker.cost.generic, 2);
    assert_eq!(seeker.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert_eq!(seeker.subtypes, &[Subtype::Human, Subtype::Wizard]);
    assert_eq!((seeker.power, seeker.toughness), (Some(1), Some(1)));

    let ballista = def("Walking Ballista");
    assert_eq!(ballista.mana_value, 0);
    assert_eq!(ballista.cost.x_count, 2, "{{X}}{{X}}");
    assert_eq!((ballista.cost.generic, ballista.cost.pips.len()), (0, 0));
    assert!(ballista.has_type(CardType::Artifact) && ballista.has_type(CardType::Creature));
    assert_eq!(ballista.subtypes, &[Subtype::Construct]);
    assert_eq!((ballista.power, ballista.toughness), (Some(0), Some(0)));
    assert!(ballista.colors.is_empty());
    assert_eq!(ballista.activated_abilities.len(), 2);

    for (name, subtypes, colors) in [
        (
            "Noble Hierarch",
            &[Subtype::Human, Subtype::Druid][..],
            [ManaColor::W, ManaColor::U, ManaColor::G],
        ),
        (
            "Ignoble Hierarch",
            &[Subtype::Goblin, Subtype::Shaman][..],
            [ManaColor::B, ManaColor::R, ManaColor::G],
        ),
    ] {
        let hierarch = def(name);
        assert_eq!(hierarch.mana_value, 1, "{name}");
        assert_eq!(hierarch.cost.pips, &[Pip::Colored(ManaColor::G)], "{name}");
        assert_eq!(hierarch.subtypes, subtypes, "{name}");
        assert_eq!((hierarch.power, hierarch.toughness), (Some(0), Some(1)));
        assert_eq!(hierarch.mana_ability_choices.len(), 3, "{name}");
        for color in colors {
            assert!(hierarch.mana_ability_choices.contains(&color), "{name}");
        }
    }

    let spider = def("Spider-Woman, Stunning Savior");
    assert_eq!(spider.mana_value, 2);
    assert_eq!(spider.cost.generic, 1);
    assert_eq!(spider.cost.pips, &[Pip::Hybrid(ManaColor::W, ManaColor::U)]);
    assert_eq!(spider.supertypes, &[Supertype::Legendary]);
    assert_eq!(
        spider.subtypes,
        &[Subtype::Spider, Subtype::Human, Subtype::Hero]
    );
    assert_eq!((spider.power, spider.toughness), (Some(2), Some(2)));
    assert!(spider.keywords.has(Keywords::FLYING));
    assert!(spider.colors.contains(&ManaColor::W) && spider.colors.contains(&ManaColor::U));

    let soldier = def("Soldier Artifact Token");
    assert!(soldier.is_token);
    assert!(soldier.has_type(CardType::Artifact) && soldier.has_type(CardType::Creature));
    assert_eq!(soldier.subtypes, &[Subtype::Soldier]);
    assert_eq!((soldier.power, soldier.toughness), (Some(1), Some(1)));
    assert!(soldier.colors.is_empty(), "colorless");
    assert_eq!(soldier.capability, CardCapability::Full);
}

// ---- Oliphaunt -----------------------------------------------------------------------

#[test]
fn oliphaunt_is_cast_for_six_and_tramples() {
    let mut state = plains();
    let oliphaunt = put(&mut state, P0, "Oliphaunt", Zone::Hand);
    assert!(!castable(&mut state, oliphaunt), "no mana");
    pool(&mut state, P0, &[ManaColor::R], 4);
    assert!(!castable(&mut state, oliphaunt), "five of six mana");
    pool(&mut state, P0, &[], 1);
    cast(&mut state, oliphaunt, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, oliphaunt), Zone::Battlefield);
    assert_eq!(power_toughness(&state, oliphaunt), (6, 4));
    assert!(keyword(&state, oliphaunt, Keywords::TRAMPLE));
}

#[test]
fn oliphaunt_attack_gives_another_creature_plus_two_power_and_trample() {
    let mut state = plains();
    let oliphaunt = put(&mut state, P0, "Oliphaunt", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(!keyword(&state, elves, Keywords::TRAMPLE));
    let life_before = life(&state, P1);
    let mut offered = Vec::new();
    attack_with(&mut state, vec![oliphaunt], |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            offered = legal_targets.clone();
            Some(Action::ChooseTarget(Target::Object(elves)))
        }
        _ => None,
    });
    assert_eq!(
        offered,
        vec![Target::Object(elves)],
        "another creature you control: not Oliphaunt, not the opponent's"
    );
    assert_eq!(power_toughness(&state, elves), (3, 1));
    assert!(keyword(&state, elves, Keywords::TRAMPLE));
    assert_eq!(power_toughness(&state, oliphaunt), (6, 4));
    assert_eq!(power_toughness(&state, theirs), (1, 1));
    assert!(!keyword(&state, theirs, Keywords::TRAMPLE));
    assert_eq!(life(&state, P1), life_before - 6, "Oliphaunt was unblocked");
}

#[test]
fn oliphaunt_attacking_alone_has_no_target_and_the_trigger_is_removed() {
    let mut state = plains();
    let oliphaunt = put(&mut state, P0, "Oliphaunt", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let life_before = life(&state, P1);
    // A target prompt would panic in `first_answer` (no legal target).
    attack_with(&mut state, vec![oliphaunt], |_, _| None);
    assert_eq!(power_toughness(&state, oliphaunt), (6, 4));
    assert_eq!(power_toughness(&state, theirs), (1, 1));
    assert!(!keyword(&state, theirs, Keywords::TRAMPLE));
    assert_eq!(life(&state, P1), life_before - 6);
}

#[test]
fn oliphaunt_trigger_does_nothing_if_its_target_left_the_battlefield() {
    let mut state = plains();
    let oliphaunt = put(&mut state, P0, "Oliphaunt", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    attack_until_trigger_on_stack(&mut state, vec![oliphaunt], Some(elves));
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].targets, vec![Target::Object(elves)]);
    move_to(&mut state, elves, Zone::Graveyard);
    run_until(&mut state, second_main_settled, |_, _| None);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(power_toughness(&state, oliphaunt), (6, 4));
}

#[test]
fn oliphaunt_mountaincycling_finds_any_card_with_the_mountain_subtype() {
    let mut state = game(&["Island"; 20]);
    let oliphaunt = put(&mut state, P0, "Oliphaunt", Zone::Hand);
    let mountain = put(&mut state, P0, "Mountain", Zone::Library);
    let taiga = put(&mut state, P0, "Taiga", Zone::Library);
    let forest = put(&mut state, P0, "Forest", Zone::Library);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    assert!(!activatable(&mut state, oliphaunt, 0), "{{1}} is needed");
    pool(&mut state, P0, &[], 1);
    activate(&mut state, oliphaunt, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(mountain)));
            assert!(legal_targets.contains(&Target::Object(taiga)), "typed dual");
            assert!(!legal_targets.contains(&Target::Object(forest)));
            assert!(!legal_targets.contains(&Target::Object(bolt)));
            Action::ChooseEffectTarget(Target::Object(taiga))
        }
        other => first_answer(other),
    });
    assert_eq!(
        zone(&state, oliphaunt),
        Zone::Graveyard,
        "discarded as a cost"
    );
    assert!(hand_contains(&state, P0, taiga));
    assert_eq!(zone(&state, mountain), Zone::Library);
}

#[test]
fn oliphaunt_mountaincycling_can_find_nothing_and_works_only_from_hand() {
    let mut state = game(&["Island"; 20]);
    let oliphaunt = put(&mut state, P0, "Oliphaunt", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, oliphaunt, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, oliphaunt), Zone::Graveyard);
    assert_eq!(hand_size(&state, P0), 0, "no Mountain in the library");

    let mut state = game(&["Island"; 20]);
    let oliphaunt = put(&mut state, P0, "Oliphaunt", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, oliphaunt, 0));
}

// ---- Stingcaster Mage ----------------------------------------------------------------

#[test]
fn stingcaster_mage_has_haste() {
    let mut state = plains();
    let mage = put(&mut state, P0, "Stingcaster Mage", Zone::Battlefield);
    state.objects.get_mut(mage).summoning_sick = true;
    assert!(keyword(&state, mage, Keywords::HASTE));
    assert_eq!(power_toughness(&state, mage), (2, 1));
}

#[test]
fn stingcaster_mage_gives_an_instant_flashback_for_its_mana_cost() {
    let mut state = plains();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let mage = put(&mut state, P0, "Stingcaster Mage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(!castable(&mut state, bolt), "no flashback yet");
    cast(&mut state, mage, &[]);
    settle_choosing(&mut state, bolt);
    assert_eq!(zone(&state, mage), Zone::Battlefield);
    assert!(!castable(&mut state, bolt), "{{R}} is not available yet");
    pool(&mut state, P0, &[ManaColor::R], 0);
    let life_before = life(&state, P1);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), life_before - 3);
    assert_eq!(zone(&state, bolt), Zone::Exile, "flashback exiles it");
}

#[test]
fn stingcaster_mage_gives_a_sorcery_flashback_and_offers_only_own_instants_and_sorceries() {
    let mut state = plains();
    let rain = put(&mut state, P0, "Stone Rain", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let land = put(&mut state, P1, "Mountain", Zone::Battlefield);
    let mage = put(&mut state, P0, "Stingcaster Mage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, mage, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseTargets { legal_targets, .. }) => {
            assert!(legal_targets.contains(&Target::Object(rain)), "a sorcery");
            assert!(legal_targets.contains(&Target::Object(bolt)), "an instant");
            for illegal in [elves, forest, theirs] {
                assert!(!legal_targets.contains(&Target::Object(illegal)));
            }
            act(&mut state, Action::ChooseTarget(Target::Object(rain)));
        }
        other => panic!("expected the entry trigger's target, got {other:?}"),
    }
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::R], 2);
    assert!(!castable(&mut state, bolt), "only the chosen card");
    assert!(
        castable(&mut state, rain),
        "{{2}}{{R}} is its flashback cost"
    );
    cast(&mut state, rain, &[Target::Object(land)]);
    settled(&mut state);
    assert_eq!(zone(&state, land), Zone::Graveyard, "Stone Rain resolved");
    assert_eq!(zone(&state, rain), Zone::Exile);
}

#[test]
fn stingcaster_mage_with_no_instant_or_sorcery_in_the_graveyard_has_no_target() {
    let mut state = plains();
    put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let mage = put(&mut state, P0, "Stingcaster Mage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, mage, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mage), Zone::Battlefield);
}

#[test]
fn stingcaster_mage_grants_nothing_if_the_card_left_the_graveyard() {
    let mut state = plains();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let mage = put(&mut state, P0, "Stingcaster Mage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, mage, &[]);
    until_trigger_on_stack(&mut state, Some(Target::Object(bolt)));
    assert_eq!(
        state.stack.last().unwrap().targets,
        vec![Target::Object(bolt)]
    );
    move_to(&mut state, bolt, Zone::Exile);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, bolt));
}

// ---- Third Path Iconoclast -----------------------------------------------------------

#[test]
fn third_path_iconoclast_makes_a_soldier_for_each_noncreature_spell_before_it_resolves() {
    let mut state = plains();
    let iconoclast = put(&mut state, P0, "Third Path Iconoclast", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let life_before = life(&state, P1);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    let _ = next(&mut state);
    assert_eq!(state.stack.len(), 2, "the spell and the trigger");
    assert!(matches!(state.stack[0].kind, StackItemKind::Spell));
    assert!(trigger_on_top(&state), "the trigger resolves first");
    assert_eq!(life(&state, P1), life_before);
    settled(&mut state);
    assert_eq!(life(&state, P1), life_before - 3);
    let soldiers = named(&state, P0, "Soldier Artifact Token");
    assert_eq!(soldiers.len(), 1);
    assert_eq!(power_toughness(&state, soldiers[0]), (1, 1));
    assert!(engine::object_has_type(
        &state,
        soldiers[0],
        CardType::Artifact
    ));
    assert!(engine::object_has_type(
        &state,
        soldiers[0],
        CardType::Creature
    ));
    assert!(engine::has_effective_subtype(
        &state,
        soldiers[0],
        Subtype::Soldier
    ));

    // A second noncreature spell, an artifact: one more token.
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Soldier Artifact Token").len(), 2);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
    assert_eq!(named(&state, P0, "Third Path Iconoclast"), vec![iconoclast]);
}

#[test]
fn third_path_iconoclast_ignores_creature_spells_lands_and_the_opponents_spells() {
    let mut state = plains();
    put(&mut state, P0, "Third Path Iconoclast", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Hand);
    play_land(&mut state, forest);
    assert!(named(&state, P0, "Soldier Artifact Token").is_empty());

    opponent_bolts(&mut state, Target::Player(P0));
    settled(&mut state);
    assert!(named(&state, P0, "Soldier Artifact Token").is_empty());
    assert!(named(&state, P1, "Soldier Artifact Token").is_empty());
}

// ---- Tireless Tracker ----------------------------------------------------------------

#[test]
fn tireless_tracker_investigates_for_each_land_you_control_that_enters() {
    let mut state = plains();
    let tracker = put(&mut state, P0, "Tireless Tracker", Zone::Battlefield);
    assert_eq!(power_toughness(&state, tracker), (3, 2));
    let forest = put(&mut state, P0, "Forest", Zone::Hand);
    play_land(&mut state, forest);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    let clues = named(&state, P0, "Clue Token");
    assert_eq!(clues.len(), 1);
    assert!(engine::has_effective_subtype(
        &state,
        clues[0],
        Subtype::Clue
    ));

    // A land that enters without being played (a search put it onto the
    // battlefield) triggers landfall too, and an opponent's land does not.
    let island = put(&mut state, P0, "Island", Zone::Library);
    move_to(&mut state, island, Zone::Battlefield);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Clue Token").len(), 2);
    let theirs = put(&mut state, P1, "Mountain", Zone::Library);
    move_to(&mut state, theirs, Zone::Battlefield);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Clue Token").len(), 2);
    assert!(named(&state, P1, "Clue Token").is_empty());
    assert_eq!(
        power_toughness(&state, tracker),
        (3, 2),
        "no Clue was sacrificed"
    );
}

#[test]
fn tireless_tracker_enters_without_investigating_and_a_land_without_it_makes_no_clue() {
    let mut state = plains();
    let forest = put(&mut state, P0, "Forest", Zone::Hand);
    play_land(&mut state, forest);
    assert!(named(&state, P0, "Clue Token").is_empty(), "no Tracker yet");
    let tracker = put(&mut state, P0, "Tireless Tracker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, tracker, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, tracker), Zone::Battlefield);
    assert!(
        named(&state, P0, "Clue Token").is_empty(),
        "it is not a land"
    );
}

#[test]
fn tireless_tracker_gets_a_counter_when_a_clue_is_sacrificed_above_the_clues_draw() {
    let mut state = plains();
    let tracker = put(&mut state, P0, "Tireless Tracker", Zone::Battlefield);
    let clue = token(&mut state, P0, "Clue Token");
    pool(&mut state, P0, &[], 2);
    let hand_before = hand_size(&state, P0);
    activate(&mut state, clue, 0, &[]);
    let _ = next(&mut state);
    assert_eq!(state.stack.len(), 2);
    assert!(matches!(
        state.stack[0].kind,
        StackItemKind::ActivatedAbility
    ));
    assert!(trigger_on_top(&state), "the sacrifice was a cost");
    assert_eq!(counters(&state, tracker), 0, "the draw has not resolved");
    settled(&mut state);
    assert_eq!(counters(&state, tracker), 1);
    assert_eq!(power_toughness(&state, tracker), (4, 3));
    assert_eq!(
        hand_size(&state, P0),
        hand_before + 1,
        "the Clue drew a card"
    );
    assert!(named(&state, P0, "Clue Token").is_empty());

    // Each Clue sacrificed adds one more.
    for sacrificed in 2..=3 {
        let clue = token(&mut state, P0, "Clue Token");
        pool(&mut state, P0, &[], 2);
        activate(&mut state, clue, 0, &[]);
        settled(&mut state);
        assert_eq!(counters(&state, tracker), sacrificed);
    }
}

#[test]
fn tireless_tracker_ignores_other_sacrifices_and_the_opponents_clues() {
    let mut state = plains();
    let tracker = put(&mut state, P0, "Tireless Tracker", Zone::Battlefield);
    let treasure = token(&mut state, P0, "Treasure Token");
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(treasure));
    settled(&mut state);
    assert!(named(&state, P0, "Treasure Token").is_empty());
    assert_eq!(counters(&state, tracker), 0, "a Treasure is not a Clue");

    let theirs = token(&mut state, P1, "Clue Token");
    state.priority_player = P1;
    pool(&mut state, P1, &[], 2);
    activate(&mut state, theirs, 0, &[]);
    settled(&mut state);
    assert_eq!(counters(&state, tracker), 0, "you sacrifice a Clue");
}

#[test]
fn tireless_tracker_that_left_the_battlefield_gets_no_counter() {
    let mut state = plains();
    let tracker = put(&mut state, P0, "Tireless Tracker", Zone::Battlefield);
    let clue = token(&mut state, P0, "Clue Token");
    pool(&mut state, P0, &[], 2);
    activate(&mut state, clue, 0, &[]);
    let _ = next(&mut state);
    assert!(trigger_on_top(&state));
    move_to(&mut state, tracker, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(zone(&state, tracker), Zone::Graveyard);
    assert_eq!(counters(&state, tracker), 0);
}

// ---- Samut, Hazoret's Champion -------------------------------------------------------

#[test]
fn samut_gives_haste_to_your_creatures_including_itself_and_later_arrivals() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(elves).summoning_sick = true;
    assert!(!keyword(&state, elves, Keywords::HASTE), "no Samut yet");
    let samut = put(
        &mut state,
        P0,
        "Samut, Hazoret's Champion",
        Zone::Battlefield,
    );
    state.objects.get_mut(samut).summoning_sick = true;
    assert!(keyword(&state, samut, Keywords::HASTE), "itself");
    assert!(
        keyword(&state, elves, Keywords::HASTE),
        "creatures already there"
    );
    let later = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(later).summoning_sick = true;
    assert!(
        keyword(&state, later, Keywords::HASTE),
        "creatures that arrive later"
    );

    // Only creatures, only yours.
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(!keyword(&state, forest, Keywords::HASTE));
    assert!(!keyword(&state, ring, Keywords::HASTE));
    assert!(!keyword(&state, theirs, Keywords::HASTE));

    // The grant ends when Samut leaves.
    move_to(&mut state, samut, Zone::Graveyard);
    settled(&mut state);
    assert!(!keyword(&state, elves, Keywords::HASTE));
    assert!(!keyword(&state, later, Keywords::HASTE));
}

#[test]
fn samut_lets_a_creature_that_was_just_cast_attack() {
    let mut state = plains();
    let samut = put(&mut state, P0, "Samut, Hazoret's Champion", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, samut, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, samut), Zone::Battlefield);
    assert!(state.objects.get(samut).summoning_sick, "it just arrived");
    let life_before = life(&state, P1);
    // The declaration is refused for a creature without haste that arrived
    // this turn, so this attack proves the grant.
    attack_with(&mut state, vec![samut], |_, _| None);
    assert_eq!(life(&state, P1), life_before - 2);
}

#[test]
fn an_opposing_samut_gives_its_controller_haste_not_yours() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(mine).summoning_sick = true;
    let samut = put(
        &mut state,
        P1,
        "Samut, Hazoret's Champion",
        Zone::Battlefield,
    );
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(keyword(&state, samut, Keywords::HASTE));
    assert!(keyword(&state, theirs, Keywords::HASTE));
    assert!(!keyword(&state, mine, Keywords::HASTE));
}

// ---- Marwyn, the Preserver -----------------------------------------------------------

#[test]
fn marwyn_gives_your_lands_hexproof_so_opponents_cannot_target_them() {
    let mut state = plains();
    let marwyn = put(&mut state, P0, "Marwyn, the Preserver", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Mountain", Zone::Battlefield);
    assert!(keyword(&state, forest, Keywords::HEXPROOF));
    assert!(!keyword(&state, theirs, Keywords::HEXPROOF));
    assert!(!keyword(&state, marwyn, Keywords::HEXPROOF), "not a land");

    // P1's Stone Rain may name their own land but not yours.
    opponents_turn(&mut state);
    let rain = put(&mut state, P1, "Stone Rain", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 2);
    let targets = cast_targets(&state, rain);
    assert!(targets.contains(&Target::Object(theirs)));
    assert!(!targets.contains(&Target::Object(forest)), "hexproof");
    // Marwyn is not a land: an opponent's Lightning Bolt may name it.
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(cast_targets(&state, bolt).contains(&Target::Object(marwyn)));

    // You may still target your own lands.
    let mut mine = plains();
    put(&mut mine, P0, "Marwyn, the Preserver", Zone::Battlefield);
    let own = put(&mut mine, P0, "Forest", Zone::Battlefield);
    let rain = put(&mut mine, P0, "Stone Rain", Zone::Hand);
    pool(&mut mine, P0, &[ManaColor::R], 2);
    assert!(cast_targets(&mine, rain).contains(&Target::Object(own)));

    // Without Marwyn the land is a legal target again.
    move_to(&mut state, marwyn, Zone::Graveyard);
    settled(&mut state);
    assert!(!keyword(&state, forest, Keywords::HEXPROOF));
    assert!(cast_targets(&state, rain).contains(&Target::Object(forest)));
}

#[test]
fn marwyn_returns_a_land_card_from_your_graveyard_to_your_hand() {
    let mut state = plains();
    let marwyn = put(&mut state, P0, "Marwyn, the Preserver", Zone::Battlefield);
    assert!(!activatable(&mut state, marwyn, 0), "no land card, no mana");
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Mountain", Zone::Graveyard);
    assert!(!activatable(&mut state, marwyn, 0), "{{2}} is needed");
    pool(&mut state, P0, &[], 2);
    assert!(activatable(&mut state, marwyn, 0));
    act(&mut state, Action::ActivateAbility(marwyn, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(
                legal_targets,
                vec![Target::Object(forest)],
                "only land cards in your own graveyard"
            );
            assert!(!legal_targets.contains(&Target::Object(bolt)));
            assert!(!legal_targets.contains(&Target::Object(theirs)));
        }
        other => panic!("expected a target, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(forest)));
    settled(&mut state);
    assert!(hand_contains(&state, P0, forest));
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(zone(&state, marwyn), Zone::Battlefield);
}

#[test]
fn marwyns_ability_does_nothing_if_its_land_left_the_graveyard() {
    let mut state = plains();
    let marwyn = put(&mut state, P0, "Marwyn, the Preserver", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, marwyn, 0, &[Target::Object(forest)]);
    move_to(&mut state, forest, Zone::Exile);
    settled(&mut state);
    assert_eq!(zone(&state, forest), Zone::Exile);
    assert!(!hand_contains(&state, P0, forest));
}

// ---- Spellseeker ---------------------------------------------------------------------

#[test]
fn spellseeker_finds_an_instant_or_sorcery_with_mana_value_two_or_less() {
    let mut state = game(&["Island"; 20]);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Library);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Library);
    let gush = put(&mut state, P0, "Gush", Zone::Library);
    let frantic = put(&mut state, P0, "Frantic Search", Zone::Library);
    let time_walk = put(&mut state, P0, "Time Walk", Zone::Library);
    let drain = put(&mut state, P0, "Mana Drain", Zone::Library);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    assert_eq!(def("Frantic Search").mana_value, 3);
    assert_eq!(def("Gush").mana_value, 5);
    assert_eq!(def("Mana Drain").mana_value, 2);
    let seeker = put(&mut state, P0, "Spellseeker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    let library_before = state.players[0].library.len();
    cast(&mut state, seeker, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            for legal in [bolt, drain, time_walk] {
                assert!(legal_targets.contains(&Target::Object(legal)));
            }
            for illegal in [elves, ring, gush, frantic] {
                assert!(!legal_targets.contains(&Target::Object(illegal)));
            }
            Action::ChooseEffectTarget(Target::Object(drain))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, seeker), Zone::Battlefield);
    assert!(hand_contains(&state, P0, drain));
    assert_eq!(state.players[0].library.len(), library_before - 1);
    assert_eq!(zone(&state, bolt), Zone::Library);
}

#[test]
fn spellseeker_search_may_be_declined_or_find_nothing() {
    let mut state = game(&["Island"; 20]);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    let seeker = put(&mut state, P0, "Spellseeker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, seeker, &[]);
    // `first_answer` finishes the selection without choosing.
    settle_first(&mut state);
    assert_eq!(zone(&state, seeker), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Library);
    assert_eq!(hand_size(&state, P0), 0);

    // Nothing in the library qualifies: the creature still enters.
    let mut state = game(&["Island"; 20]);
    put(&mut state, P0, "Llanowar Elves", Zone::Library);
    let seeker = put(&mut state, P0, "Spellseeker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, seeker, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, seeker), Zone::Battlefield);
    assert_eq!(hand_size(&state, P0), 0);
}

// ---- Walking Ballista ----------------------------------------------------------------

/// Casts `ballista` choosing X = `x`, then lets it resolve.
fn cast_ballista(state: &mut GameState, ballista: ObjectId, x: u16) {
    cast(state, ballista, &[]);
    settle_option(state, x);
}

#[test]
fn walking_ballista_enters_with_x_counters_and_costs_x_twice() {
    let mut state = plains();
    let ballista = put(&mut state, P0, "Walking Ballista", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast_ballista(&mut state, ballista, 1);
    assert_eq!(zone(&state, ballista), Zone::Battlefield);
    assert_eq!(counters(&state, ballista), 1);
    assert_eq!(power_toughness(&state, ballista), (1, 1));
    assert_eq!(
        mana(&state, P0, ManaColor::C),
        2,
        "X = 1 paid {{X}}{{X}} = 2 of the 4"
    );

    let mut state = plains();
    let ballista = put(&mut state, P0, "Walking Ballista", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast_ballista(&mut state, ballista, 2);
    assert_eq!(counters(&state, ballista), 2);
    assert_eq!(power_toughness(&state, ballista), (2, 2));
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn walking_ballista_cast_with_x_zero_dies_as_a_zero_zero() {
    let mut state = plains();
    let ballista = put(&mut state, P0, "Walking Ballista", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast_ballista(&mut state, ballista, 0);
    assert_eq!(zone(&state, ballista), Zone::Graveyard);
    assert_eq!(mana(&state, P0, ManaColor::C), 2, "X = 0 paid nothing");
}

#[test]
fn walking_ballista_x_is_limited_by_the_mana_available() {
    let mut state = plains();
    let ballista = put(&mut state, P0, "Walking Ballista", Zone::Hand);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, ballista, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 3, "X = 0, 1 or 2 with five mana");
            Action::ChooseEffectOption(2)
        }
        other => first_answer(other),
    });
    assert_eq!(counters(&state, ballista), 2);
}

#[test]
fn walking_ballista_pays_four_for_a_counter() {
    let mut state = plains();
    let ballista = put(&mut state, P0, "Walking Ballista", Zone::Battlefield);
    state.objects.get_mut(ballista).counters.plus1_plus1 = 2;
    assert!(!activatable(&mut state, ballista, 0), "{{4}} is needed");
    pool(&mut state, P0, &[], 3);
    assert!(!activatable(&mut state, ballista, 0), "three of four");
    pool(&mut state, P0, &[], 1);
    activate(&mut state, ballista, 0, &[]);
    settled(&mut state);
    assert_eq!(counters(&state, ballista), 3);
    assert_eq!(power_toughness(&state, ballista), (3, 3));
}

#[test]
fn walking_ballista_removes_a_counter_to_deal_one_damage_to_any_target() {
    let mut state = plains();
    let ballista = put(&mut state, P0, "Walking Ballista", Zone::Battlefield);
    state.objects.get_mut(ballista).counters.plus1_plus1 = 3;
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let life_before = life(&state, P1);

    activate(&mut state, ballista, 1, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), life_before - 1);
    assert_eq!(counters(&state, ballista), 2);

    activate(&mut state, ballista, 1, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "a 1/1 creature");
    assert_eq!(counters(&state, ballista), 1);

    // Damage to yourself is legal too.
    activate(&mut state, ballista, 1, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 19);
}

#[test]
fn walking_ballista_removing_its_last_counter_still_deals_the_damage() {
    let mut state = plains();
    let ballista = put(&mut state, P0, "Walking Ballista", Zone::Battlefield);
    state.objects.get_mut(ballista).counters.plus1_plus1 = 1;
    let life_before = life(&state, P1);
    activate(&mut state, ballista, 1, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, ballista), Zone::Graveyard, "a 0/0");
    assert_eq!(life(&state, P1), life_before - 1);
}

// ---- Noble Hierarch and Ignoble Hierarch ---------------------------------------------

/// Whether tapping `source` for `color` on a copy of the game adds exactly
/// one mana of that color.
fn taps_for(state: &GameState, source: ObjectId, color: ManaColor) -> bool {
    let mut probe = state.clone();
    next(&mut probe);
    engine::step(&mut probe, Action::ActivateManaAbilityChoice(source, color)).is_ok()
        && mana(&probe, P0, color) == 1
        && tapped(&probe, source)
}

#[test]
fn noble_hierarch_taps_for_green_white_or_blue() {
    let mut state = plains();
    let hierarch = put(&mut state, P0, "Noble Hierarch", Zone::Battlefield);
    for color in [ManaColor::G, ManaColor::W, ManaColor::U] {
        assert!(taps_for(&state, hierarch, color), "{color:?}");
    }
    for color in [ManaColor::R, ManaColor::B] {
        assert!(!taps_for(&state, hierarch, color), "{color:?}");
    }
    assert_eq!(power_toughness(&state, hierarch), (0, 1));
}

#[test]
fn ignoble_hierarch_taps_for_black_red_or_green() {
    let mut state = plains();
    let hierarch = put(&mut state, P0, "Ignoble Hierarch", Zone::Battlefield);
    for color in [ManaColor::B, ManaColor::R, ManaColor::G] {
        assert!(taps_for(&state, hierarch, color), "{color:?}");
    }
    for color in [ManaColor::W, ManaColor::U] {
        assert!(!taps_for(&state, hierarch, color), "{color:?}");
    }
    assert_eq!(power_toughness(&state, hierarch), (0, 1));
}

#[test]
fn a_hierarch_cast_this_turn_cannot_tap_until_samut_gives_it_haste() {
    let mut state = plains();
    let hierarch = put(&mut state, P0, "Noble Hierarch", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, hierarch, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, hierarch), Zone::Battlefield);
    assert!(!taps_for(&state, hierarch, ManaColor::G), "summoning sick");
    put(
        &mut state,
        P0,
        "Samut, Hazoret's Champion",
        Zone::Battlefield,
    );
    assert!(taps_for(&state, hierarch, ManaColor::G), "haste");
}

#[test]
fn exalted_gives_a_lone_attacker_plus_one_plus_one_for_each_instance() {
    let mut state = plains();
    let noble = put(&mut state, P0, "Noble Hierarch", Zone::Battlefield);
    let ignoble = put(&mut state, P0, "Ignoble Hierarch", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let life_before = life(&state, P1);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![elves]));
    run_until(
        &mut state,
        |state| triggers_on_stack(state) == 2,
        |_, _| None,
    );
    assert_eq!(state.stack.len(), 2, "one trigger for each instance");
    let sources: Vec<ObjectId> = state.stack.iter().map(|item| item.source).collect();
    assert!(sources.contains(&noble) && sources.contains(&ignoble));
    assert!(state.stack.iter().all(|item| item.targets.is_empty()));
    run_until(&mut state, second_main_settled, |_, _| None);
    assert_eq!(power_toughness(&state, elves), (3, 3));
    assert_eq!(life(&state, P1), life_before - 3);
    assert_eq!(power_toughness(&state, noble), (0, 1), "it did not attack");
    assert_eq!(power_toughness(&state, ignoble), (0, 1));
}

#[test]
fn a_hierarch_attacking_alone_gets_its_own_exalted() {
    for name in ["Noble Hierarch", "Ignoble Hierarch"] {
        let mut state = plains();
        let hierarch = put(&mut state, P0, name, Zone::Battlefield);
        let life_before = life(&state, P1);
        attack_with(&mut state, vec![hierarch], |_, _| None);
        assert_eq!(power_toughness(&state, hierarch), (1, 2), "{name}");
        assert_eq!(life(&state, P1), life_before - 1, "{name}");
    }
}

#[test]
fn exalted_does_not_trigger_when_two_creatures_attack() {
    let mut state = plains();
    let hierarch = put(&mut state, P0, "Noble Hierarch", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let life_before = life(&state, P1);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![hierarch, elves]));
    run_until(&mut state, second_main_settled, |state, _| {
        assert_eq!(triggers_on_stack(state), 0, "attacking alone is required");
        None
    });
    assert_eq!(power_toughness(&state, hierarch), (0, 1));
    assert_eq!(power_toughness(&state, elves), (1, 1));
    assert_eq!(life(&state, P1), life_before - 1);
}

#[test]
fn exalted_of_the_opponent_does_not_trigger_for_your_lone_attacker() {
    let mut state = plains();
    put(&mut state, P1, "Noble Hierarch", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let life_before = life(&state, P1);
    attack_with(&mut state, vec![elves], |_, _| None);
    assert_eq!(power_toughness(&state, elves), (1, 1));
    assert_eq!(life(&state, P1), life_before - 1);
}

#[test]
fn exalted_without_a_hierarch_gives_nothing() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let life_before = life(&state, P1);
    attack_with(&mut state, vec![elves], |_, _| None);
    assert_eq!(power_toughness(&state, elves), (1, 1));
    assert_eq!(life(&state, P1), life_before - 1);
}

#[test]
fn exalted_does_nothing_if_the_lone_attacker_left_the_battlefield() {
    let mut state = plains();
    put(&mut state, P0, "Noble Hierarch", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let life_before = life(&state, P1);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![elves]));
    run_until(&mut state, trigger_on_top, |_, _| None);
    move_to(&mut state, elves, Zone::Graveyard);
    run_until(&mut state, second_main_settled, |_, _| None);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(life(&state, P1), life_before);
}

// ---- Spider-Woman, Stunning Savior ---------------------------------------------------

#[test]
fn spider_woman_has_flying() {
    let mut state = plains();
    let spider = put(
        &mut state,
        P0,
        "Spider-Woman, Stunning Savior",
        Zone::Battlefield,
    );
    assert!(keyword(&state, spider, Keywords::FLYING));
    assert_eq!(power_toughness(&state, spider), (2, 2));
}

#[test]
fn spider_woman_makes_opposing_creatures_and_artifacts_enter_tapped() {
    let mut state = plains();
    opponents_turn(&mut state);
    // Before Spider-Woman: an opposing creature enters untapped.
    let early = put(&mut state, P1, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::G], 0);
    cast(&mut state, early, &[]);
    settled(&mut state);
    assert!(!tapped(&state, early));

    let spider = put(
        &mut state,
        P0,
        "Spider-Woman, Stunning Savior",
        Zone::Battlefield,
    );
    assert!(!tapped(&state, spider));
    assert!(
        !tapped(&state, early),
        "permanents already there are unaffected"
    );

    // A creature spell, an artifact spell and a land played.
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert!(tapped(&state, elves), "creature");
    let ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    pool(&mut state, P1, &[], 1);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
    assert!(tapped(&state, ring), "artifact");
    let forest = put(&mut state, P1, "Forest", Zone::Hand);
    play_land(&mut state, forest);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert!(!tapped(&state, forest), "lands are neither");

    // A creature and the artifact token it creates.
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    assert!(tapped(&state, inspector));
    let clues = named(&state, P1, "Clue Token");
    assert_eq!(clues.len(), 1);
    assert!(tapped(&state, clues[0]), "tokens too");

    // A permanent that returns without being cast.
    let reanimated = put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    move_to(&mut state, reanimated, Zone::Battlefield);
    settled(&mut state);
    assert_eq!(zone(&state, reanimated), Zone::Battlefield);
    assert!(tapped(&state, reanimated), "any route onto the battlefield");

    // Your own creatures enter untapped.
    state.active_player = P0;
    state.priority_player = P0;
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, mine, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert!(!tapped(&state, mine));

    // Without Spider-Woman an opposing creature enters untapped again.
    move_to(&mut state, spider, Zone::Graveyard);
    settled(&mut state);
    let late = put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    move_to(&mut state, late, Zone::Battlefield);
    settled(&mut state);
    assert!(!tapped(&state, late));
}

#[test]
fn an_opposing_spider_woman_taps_your_creatures_as_they_enter() {
    let mut state = plains();
    put(
        &mut state,
        P1,
        "Spider-Woman, Stunning Savior",
        Zone::Battlefield,
    );
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert!(tapped(&state, elves));
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    move_to(&mut state, theirs, Zone::Battlefield);
    settled(&mut state);
    assert!(
        !tapped(&state, theirs),
        "its controller's creatures are fine"
    );
}

// ---- Public sessions -----------------------------------------------------------------

#[test]
fn c3b_decks_finish_and_replay_through_public_sessions_both_seats() {
    let green_red = deck(&[
        ("Forest", 8),
        ("Mountain", 7),
        ("Noble Hierarch", 3),
        ("Ignoble Hierarch", 3),
        ("Tireless Tracker", 4),
        ("Marwyn, the Preserver", 3),
        ("Samut, Hazoret's Champion", 3),
        ("Oliphaunt", 3),
        ("Walking Ballista", 3),
        ("Llanowar Elves", 3),
    ]);
    let blue_red = deck(&[
        ("Island", 8),
        ("Mountain", 6),
        ("Spellseeker", 4),
        ("Third Path Iconoclast", 4),
        ("Spider-Woman, Stunning Savior", 4),
        ("Stingcaster Mage", 4),
        ("Lightning Bolt", 4),
        ("Brainstorm", 3),
        ("Time Walk", 2),
        ("Walking Ballista", 1),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    for (pair, first_seed) in [
        ([green_red.clone(), blue_red.clone()], 0x4333_4200_u64),
        ([blue_red, green_red], 0x4333_4210),
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
    for name in C3B_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
