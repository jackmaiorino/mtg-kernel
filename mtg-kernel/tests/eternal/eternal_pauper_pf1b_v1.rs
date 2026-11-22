//! Pauper field wave Pf1b: native rules and public-session tests for the
//! spells and library effects.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CostKind, Decision, OptionalCostChoice};
use mtg_kernel::event::{propose_and_commit, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const PF1B_CARDS: [&str; 9] = [
    "Bitter Reunion",
    "Kruphix's Insight",
    "Scattershot",
    "Deprive",
    "Exclude",
    "Fiery Cannonade",
    "Militia Bugler",
    "Mob Lookout",
    "Elite Interceptor",
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
        0x5046_0000_0000 + 0x3162,
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
fn pf1b_cards_are_full() {
    for name in PF1B_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

// ---- Helpers of this wave -------------------------------------------------------------

fn name_of(state: &GameState, object: ObjectId) -> &'static str {
    CARD_DEFS[state.objects.get(object).card_def as usize].name
}

fn hand_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

fn library_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].library.len()
}

fn graveyard_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].graveyard.len()
}

fn counters(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.plus1_plus1
}

fn move_to(state: &mut GameState, object: ObjectId, to: Zone) {
    propose_and_commit(state, ProposedEvent::zone_change(object, to));
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

/// P1 casts `name` (made active so a creature spell is legal) and the spell
/// stays on the stack.
fn opponent_casts(
    state: &mut GameState,
    name: &str,
    colored: &[ManaColor],
    colorless: u8,
    targets: &[Target],
) -> ObjectId {
    state.active_player = P1;
    state.priority_player = P1;
    let spell = put(state, P1, name, Zone::Hand);
    pool(state, P1, colored, colorless);
    cast(state, spell, targets);
    spell
}

/// Passes priority from P1 so P0 may respond.
fn pass_to_p0(state: &mut GameState) {
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(state, Action::Pass);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
}

/// P0 passes priority with its spell on the stack and P1 answers with a
/// Lightning Bolt at `target`.
fn opponent_responds_with_bolt(state: &mut GameState, target: Target) -> ObjectId {
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(state, Action::Pass);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    cast(state, bolt, &[target]);
    bolt
}

/// Settles, choosing the first of `wanted` that a target, selection or cost
/// prompt offers and otherwise answering with the first legal answer.
fn settle_choosing_any(state: &mut GameState, wanted: &[ObjectId]) {
    settle_with(state, |_, decision| {
        match &decision {
            Decision::ChooseTargets { legal_targets, .. } => {
                if let Some(object) = wanted
                    .iter()
                    .copied()
                    .find(|object| legal_targets.contains(&Target::Object(*object)))
                {
                    return Action::ChooseTarget(Target::Object(object));
                }
            }
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                if let Some(object) = wanted
                    .iter()
                    .copied()
                    .find(|object| legal_targets.contains(&Target::Object(*object)))
                {
                    return Action::ChooseEffectTarget(Target::Object(object));
                }
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                if let Some(object) = wanted
                    .iter()
                    .copied()
                    .find(|object| candidates.contains(object))
                {
                    return Action::ChooseCostTarget(object);
                }
            }
            _ => {}
        }
        first_answer(decision)
    });
}

/// Settles, picking each of `wanted` once from an "up to" selection and then
/// finishing it. A selection that can't finish yet (a graveyard order) takes
/// its first legal answer.
fn settle_picking(state: &mut GameState, wanted: &[ObjectId]) {
    let mut picked: Vec<ObjectId> = Vec::new();
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            let next_pick = wanted.iter().copied().find(|object| {
                !picked.contains(object) && legal_targets.contains(&Target::Object(*object))
            });
            match next_pick {
                Some(object) => {
                    picked.push(object);
                    Action::ChooseEffectTarget(Target::Object(object))
                }
                None if can_finish => Action::FinishEffectSelection,
                None => Action::ChooseEffectTarget(legal_targets[0]),
            }
        }
        other => first_answer(other),
    });
}

/// A library of `top` (first card on top) followed by `filler` Plains.
fn library_of(top: &[&'static str], filler: usize) -> Vec<&'static str> {
    let mut library = top.to_vec();
    library.extend(vec!["Plains"; filler]);
    library
}

// ---- Characteristics (continued) ---------------------------------------------------

#[test]
fn pf1b_printed_characteristics() {
    let reunion = def("Bitter Reunion");
    assert_eq!(reunion.types, &[CardType::Enchantment]);
    assert_eq!(reunion.mana_value, 2);
    assert_eq!(reunion.cost.generic, 1);
    assert_eq!(reunion.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(reunion.activated_abilities.len(), 1);

    let insight = def("Kruphix's Insight");
    assert_eq!(insight.types, &[CardType::Sorcery]);
    assert_eq!(insight.mana_value, 3);
    assert_eq!(insight.cost.generic, 2);
    assert_eq!(insight.cost.pips, &[Pip::Colored(ManaColor::G)]);

    let shot = def("Scattershot");
    assert_eq!(shot.types, &[CardType::Instant]);
    assert_eq!(shot.mana_value, 3);
    assert_eq!(shot.cost.generic, 2);
    assert_eq!(shot.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert!(shot.target_spec == TargetSpec::Creature);

    let deprive = def("Deprive");
    assert_eq!(deprive.types, &[CardType::Instant]);
    assert_eq!(deprive.mana_value, 2);
    assert_eq!(deprive.cost.generic, 0);
    assert_eq!(
        deprive.cost.pips,
        &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)]
    );
    assert!(deprive.additional_cost.is_some());

    let exclude = def("Exclude");
    assert_eq!(exclude.types, &[CardType::Instant]);
    assert_eq!(exclude.mana_value, 3);
    assert_eq!(exclude.cost.generic, 2);
    assert_eq!(exclude.cost.pips, &[Pip::Colored(ManaColor::U)]);
    assert!(exclude.target_spec == TargetSpec::CreatureSpellOnStack);

    let cannonade = def("Fiery Cannonade");
    assert_eq!(cannonade.types, &[CardType::Instant]);
    assert_eq!(cannonade.mana_value, 3);
    assert_eq!(cannonade.cost.generic, 2);
    assert_eq!(cannonade.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert!(cannonade.target_spec == TargetSpec::None);

    let bugler = def("Militia Bugler");
    assert_eq!(bugler.types, &[CardType::Creature]);
    assert_eq!(bugler.subtypes, &[Subtype::Human, Subtype::Soldier]);
    assert_eq!((bugler.power, bugler.toughness), (Some(2), Some(3)));
    assert_eq!(bugler.mana_value, 3);
    assert_eq!(bugler.cost.generic, 2);
    assert_eq!(bugler.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert!(bugler.keywords.has(Keywords::VIGILANCE));

    let lookout = def("Mob Lookout");
    assert_eq!(lookout.types, &[CardType::Creature]);
    assert_eq!(
        lookout.subtypes,
        &[Subtype::Human, Subtype::Rogue, Subtype::Villain]
    );
    assert_eq!((lookout.power, lookout.toughness), (Some(0), Some(3)));
    assert_eq!(lookout.mana_value, 2);
    assert_eq!(lookout.cost.generic, 1);
    assert!(lookout.colors.contains(&ManaColor::U) && lookout.colors.contains(&ManaColor::B));

    let interceptor = def("Elite Interceptor");
    assert_eq!(interceptor.types, &[CardType::Creature]);
    assert_eq!(interceptor.subtypes, &[Subtype::Human, Subtype::Wizard]);
    assert_eq!(
        (interceptor.power, interceptor.toughness),
        (Some(1), Some(2))
    );
    assert_eq!(interceptor.mana_value, 1);
    assert_eq!(interceptor.cost.pips, &[Pip::Colored(ManaColor::W)]);

    let rejoinder = def("Rejoinder");
    assert_eq!(rejoinder.types, &[CardType::Sorcery]);
    assert_eq!(rejoinder.mana_value, 2);
    assert_eq!(rejoinder.cost.generic, 1);
    assert_eq!(rejoinder.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert_eq!(rejoinder.capability, CardCapability::Full);
    assert!(rejoinder.target_spec == TargetSpec::Creature);
}

// ---- Exclude ---------------------------------------------------------------------

#[test]
fn exclude_counters_a_creature_spell_then_draws_a_card() {
    let mut state = plains();
    let elves = opponent_casts(&mut state, "Llanowar Elves", &[ManaColor::G], 0, &[]);
    pass_to_p0(&mut state);
    let exclude = put(&mut state, P0, "Exclude", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    assert_eq!(cast_targets(&state, exclude), vec![Target::Object(elves)]);
    cast(&mut state, exclude, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "countered");
    assert_eq!(zone(&state, exclude), Zone::Graveyard);
    assert_eq!(hand_len(&state, P0), 1, "drew a card");
    assert_eq!(library_len(&state, P0), 19);
}

#[test]
fn exclude_targets_only_creature_spells() {
    let mut state = plains();
    opponent_bolts(&mut state, Target::Player(P0));
    let exclude = put(&mut state, P0, "Exclude", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    assert!(!castable(&mut state, exclude), "a Bolt is not a creature");
}

#[test]
fn exclude_still_draws_when_the_creature_spell_cant_be_countered() {
    let mut state = plains();
    let squelcher = opponent_casts(&mut state, "Hexing Squelcher", &[ManaColor::R], 1, &[]);
    pass_to_p0(&mut state);
    let exclude = put(&mut state, P0, "Exclude", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, exclude, &[Target::Object(squelcher)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, squelcher),
        Zone::Battlefield,
        "it can't be countered"
    );
    assert_eq!(hand_len(&state, P0), 1, "the draw still happens");
}

#[test]
fn exclude_does_nothing_when_its_target_already_left_the_stack() {
    let mut state = plains();
    let elves = opponent_casts(&mut state, "Llanowar Elves", &[ManaColor::G], 0, &[]);
    pass_to_p0(&mut state);
    let first = put(&mut state, P0, "Exclude", Zone::Hand);
    let second = put(&mut state, P0, "Exclude", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 4);
    cast(&mut state, first, &[Target::Object(elves)]);
    cast(&mut state, second, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
    assert_eq!(
        hand_len(&state, P0),
        1,
        "only the resolving Exclude drew; the other had no target"
    );
}

// ---- Fiery Cannonade -------------------------------------------------------------

#[test]
fn fiery_cannonade_hits_every_non_pirate_creature_on_both_sides() {
    let mut state = plains();
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let commodore = put(&mut state, P1, "Crimson Fleet Commodore", Zone::Battlefield);
    let vandal = put(&mut state, P1, "Masked Vandal", Zone::Battlefield);
    let cannonade = put(&mut state, P0, "Fiery Cannonade", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, cannonade, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard, "1/2 dies");
    assert_eq!(zone(&state, elves), Zone::Graveyard, "1/1 dies");
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).damage, 2, "2 damage exactly");
    // A Pirate and a changeling (which is every creature type) take none.
    assert_eq!(zone(&state, commodore), Zone::Battlefield);
    assert_eq!(state.objects.get(commodore).damage, 0, "a Pirate");
    assert_eq!(zone(&state, vandal), Zone::Battlefield);
    assert_eq!(
        state.objects.get(vandal).damage,
        0,
        "a changeling is a Pirate"
    );
    assert_eq!(life(&state, P0), 20);
    assert_eq!(life(&state, P1), 20);
}

#[test]
fn fiery_cannonade_resolves_without_targets_and_without_creatures() {
    let mut state = plains();
    let cannonade = put(&mut state, P0, "Fiery Cannonade", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, cannonade, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, cannonade), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
    assert_eq!(life(&state, P1), 20);
}

// ---- Scattershot -----------------------------------------------------------------

#[test]
fn scattershot_deals_one_damage_to_a_target_creature_only() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let shot = put(&mut state, P0, "Scattershot", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    let targets = cast_targets(&state, shot);
    assert!(targets.contains(&Target::Object(mine)) && targets.contains(&Target::Object(angler)));
    assert!(!targets.contains(&Target::Player(P0)) && !targets.contains(&Target::Player(P1)));
    cast(&mut state, shot, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(state.objects.get(angler).damage, 1, "no storm copies");
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(zone(&state, shot), Zone::Graveyard);
}

#[test]
fn scattershot_storm_copies_each_deal_one_damage() {
    let mut state = plains();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    for _ in 0..2 {
        let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], 0);
        cast(&mut state, bolt, &[Target::Player(P1)]);
        settled(&mut state);
    }
    assert_eq!(life(&state, P1), 14);
    let shot = put(&mut state, P0, "Scattershot", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, shot, &[Target::Object(angler)]);
    // Each copy keeps its target.
    settle_first(&mut state);
    assert_eq!(
        state.objects.get(angler).damage,
        3,
        "the spell and two storm copies"
    );
}

#[test]
fn scattershot_storm_copy_may_choose_a_new_target() {
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    let shot = put(&mut state, P0, "Scattershot", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, shot, &[Target::Object(angler)]);
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            offered.push((legal_targets.clone(), can_finish));
            Action::ChooseEffectTarget(Target::Object(elves))
        }
        other => first_answer(other),
    });
    assert_eq!(offered.len(), 1, "one copy, one choice");
    assert!(offered[0].1, "a copy may keep its target");
    assert!(offered[0].0.contains(&Target::Object(elves)));
    assert!(
        !offered[0].0.contains(&Target::Player(P1)),
        "creatures only"
    );
    assert_eq!(zone(&state, elves), Zone::Graveyard, "the copy hit Elves");
    assert_eq!(
        state.objects.get(angler).damage,
        1,
        "the original hit the Angler"
    );
}

#[test]
fn scattershot_original_does_nothing_when_its_copy_removed_the_target() {
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    let shot = put(&mut state, P0, "Scattershot", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, shot, &[Target::Object(elves)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, shot), Zone::Graveyard);
    assert!(state.stack.is_empty());
}

#[test]
fn scattershot_does_nothing_when_its_target_left_before_it_resolved() {
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let shot = put(&mut state, P0, "Scattershot", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, shot, &[Target::Object(elves)]);
    opponent_responds_with_bolt(&mut state, Target::Object(elves));
    settle_first(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(state.objects.get(angler).damage, 0);
    assert_eq!(zone(&state, shot), Zone::Graveyard);
}

// ---- Bitter Reunion ----------------------------------------------------------------

#[test]
fn bitter_reunion_discards_then_draws_two_on_entry() {
    let mut state = plains();
    let fodder = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let reunion = put(&mut state, P0, "Bitter Reunion", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, reunion, &[]);
    settle_choosing_any(&mut state, &[fodder]);
    assert_eq!(zone(&state, reunion), Zone::Battlefield);
    assert_eq!(zone(&state, fodder), Zone::Graveyard, "discarded");
    assert_eq!(hand_len(&state, P0), 2, "drew two cards");
    assert_eq!(library_len(&state, P0), 18);
}

#[test]
fn bitter_reunion_discard_may_be_declined_or_have_nothing_to_discard() {
    // Declined: nothing is discarded or drawn.
    let mut state = plains();
    let fodder = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let reunion = put(&mut state, P0, "Bitter Reunion", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, reunion, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, reunion), Zone::Battlefield);
    assert_eq!(zone(&state, fodder), Zone::Hand);
    assert_eq!(hand_len(&state, P0), 1);
    assert_eq!(library_len(&state, P0), 20);

    // With an empty hand there is nothing to discard, so nothing is drawn.
    let mut state = plains();
    let reunion = put(&mut state, P0, "Bitter Reunion", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, reunion, &[]);
    settle_first(&mut state);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(library_len(&state, P0), 20);
}

#[test]
fn bitter_reunion_sacrifices_to_give_creatures_you_control_haste() {
    let mut state = plains();
    let reunion = put(&mut state, P0, "Bitter Reunion", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(elves).summoning_sick = true;
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(!activatable(&mut state, reunion, 0), "{{1}} is unpaid");
    pool(&mut state, P0, &[], 1);
    activate(&mut state, reunion, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, reunion), Zone::Graveyard, "sacrificed");
    assert!(keyword(&state, elves, Keywords::HASTE));
    assert!(
        !keyword(&state, theirs, Keywords::HASTE),
        "only your creatures"
    );
    let late = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(
        !keyword(&state, late, Keywords::HASTE),
        "only the creatures on the battlefield as it resolved"
    );
}

#[test]
fn bitter_reunion_haste_lets_a_new_creature_attack() {
    let mut state = plains();
    let reunion = put(&mut state, P0, "Bitter Reunion", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(elves).summoning_sick = true;
    pool(&mut state, P0, &[], 1);
    activate(&mut state, reunion, 0, &[]);
    settled(&mut state);
    attack_with(&mut state, vec![elves], |_, _| None);
    assert_eq!(life(&state, P1), 19);
}

// ---- Kruphix's Insight ---------------------------------------------------------------

#[test]
fn kruphixs_insight_takes_up_to_three_enchantments_and_mills_the_rest() {
    let mut state = game(&library_of(
        &[
            "Rancor",
            "Lightning Bolt",
            "Abundant Growth",
            "Plains",
            "Spirit Link",
            "Plains",
        ],
        10,
    ));
    let library = state.players[0].library.clone();
    let (rancor, bolt, growth, link) = (library[0], library[1], library[2], library[4]);
    let insight = put(&mut state, P0, "Kruphix's Insight", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, insight, &[]);
    let mut offered: Vec<(Vec<Target>, u16, u16)> = Vec::new();
    let mut picked: Vec<ObjectId> = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            min_targets,
            max_targets,
            can_finish,
            ..
        } => {
            if picked.is_empty() {
                offered.push((legal_targets.clone(), min_targets, max_targets));
            }
            let wanted = [rancor, link];
            match wanted.into_iter().find(|object| {
                !picked.contains(object) && legal_targets.contains(&Target::Object(*object))
            }) {
                Some(object) => {
                    picked.push(object);
                    Action::ChooseEffectTarget(Target::Object(object))
                }
                None if can_finish => Action::FinishEffectSelection,
                // The rest's graveyard order (the default profile asks).
                None => Action::ChooseEffectTarget(legal_targets[0]),
            }
        }
        other => first_answer(other),
    });
    assert_eq!(offered.len(), 1);
    let (legal, min, max) = &offered[0];
    assert_eq!((*min, *max), (0, 3), "up to three");
    for enchantment in [rancor, growth, link] {
        assert!(legal.contains(&Target::Object(enchantment)));
    }
    assert!(!legal.contains(&Target::Object(bolt)), "not an enchantment");
    assert!(
        !legal.contains(&Target::Object(library[3])),
        "not an enchantment"
    );
    assert!(hand_contains(&state, P0, rancor) && hand_contains(&state, P0, link));
    assert_eq!(hand_len(&state, P0), 2);
    // The chosen enchantments were revealed, so the opponent still knows
    // them in hand.
    for enchantment in [rancor, link] {
        assert!(
            state
                .known_hand_cards(P1, P0)
                .iter()
                .any(|entry| entry.object == enchantment),
            "a revealed enchantment taken into hand stays known"
        );
    }
    // The rest of the six revealed cards are in the graveyard: the Bolt,
    // the unchosen enchantment and the two Plains, then the spell itself.
    for card in [bolt, growth, library[3], library[5]] {
        assert_eq!(zone(&state, card), Zone::Graveyard);
    }
    assert_eq!(graveyard_len(&state, P0), 5);
    assert_eq!(library_len(&state, P0), 10, "six revealed of sixteen");
}

#[test]
fn kruphixs_insight_takes_three_of_four_enchantments() {
    let mut state = game(&library_of(
        &[
            "Rancor",
            "Abundant Growth",
            "Spirit Link",
            "Utopia Sprawl",
            "Plains",
            "Plains",
        ],
        10,
    ));
    let library = state.players[0].library.clone();
    let insight = put(&mut state, P0, "Kruphix's Insight", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, insight, &[]);
    settle_picking(
        &mut state,
        &[library[0], library[1], library[2], library[3]],
    );
    for card in &library[0..3] {
        assert!(hand_contains(&state, P0, *card));
    }
    assert_eq!(hand_len(&state, P0), 3, "at most three");
    assert_eq!(
        zone(&state, library[3]),
        Zone::Graveyard,
        "the fourth is milled"
    );
    assert_eq!(library_len(&state, P0), 10);
}

#[test]
fn kruphixs_insight_may_take_none_and_mills_every_revealed_card() {
    let mut state = game(&library_of(
        &[
            "Rancor",
            "Plains",
            "Abundant Growth",
            "Plains",
            "Plains",
            "Plains",
        ],
        10,
    ));
    let library = state.players[0].library.clone();
    let insight = put(&mut state, P0, "Kruphix's Insight", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, insight, &[]);
    settle_first(&mut state);
    assert_eq!(hand_len(&state, P0), 0);
    for card in &library[0..6] {
        assert_eq!(zone(&state, *card), Zone::Graveyard);
    }
    assert_eq!(graveyard_len(&state, P0), 7);
    assert_eq!(library_len(&state, P0), 10);
}

#[test]
fn kruphixs_insight_without_enchantments_just_mills_six() {
    let mut state = plains();
    let insight = put(&mut state, P0, "Kruphix's Insight", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, insight, &[]);
    // The six revealed cards' graveyard order is the owner's choice.
    settle_first(&mut state);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(graveyard_len(&state, P0), 7, "six revealed and the spell");
    assert_eq!(library_len(&state, P0), 14);
}

#[test]
fn kruphixs_insight_reveals_only_what_a_short_library_holds() {
    let mut state = game(&["Rancor", "Plains", "Plains"]);
    let rancor = state.players[0].library[0];
    let insight = put(&mut state, P0, "Kruphix's Insight", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, insight, &[]);
    settle_picking(&mut state, &[rancor]);
    assert!(hand_contains(&state, P0, rancor));
    assert_eq!(library_len(&state, P0), 0);
    assert_eq!(graveyard_len(&state, P0), 3, "two Plains and the spell");
}

/// CR 404.3: the rest go to the graveyard in an order their owner chooses
/// when the runtime profile exposes graveyard order (the default), and in
/// the automatic order when the profile orders unobservable batches itself.
#[test]
fn kruphixs_insight_asks_for_the_rests_graveyard_order_only_when_the_profile_does() {
    for automatic in [false, true] {
        let mut state = game(&library_of(
            &[
                "Rancor",
                "Lightning Bolt",
                "Plains",
                "Plains",
                "Plains",
                "Plains",
            ],
            4,
        ));
        state.engine.runtime_rules.auto_unobservable_graveyard_order = automatic;
        let library = state.players[0].library.clone();
        let rancor = library[0];
        let rest = library[1..6].to_vec();
        let insight = put(&mut state, P0, "Kruphix's Insight", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::G], 2);
        cast(&mut state, insight, &[]);
        let mut order_prompts = 0;
        let mut ordered: Vec<ObjectId> = Vec::new();
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets {
                legal_targets,
                min_targets,
                max_targets,
                can_finish,
                ..
            } if min_targets == max_targets && min_targets >= 2 => {
                if ordered.is_empty() {
                    order_prompts += 1;
                    assert_eq!(usize::from(min_targets), rest.len());
                }
                if can_finish {
                    return Action::FinishEffectSelection;
                }
                // Choose them in reverse of the offered order.
                let target = *legal_targets.last().unwrap();
                if let Target::Object(object) = target {
                    ordered.push(object);
                }
                Action::ChooseEffectTarget(target)
            }
            Decision::ChooseEffectTargets {
                legal_targets,
                can_finish,
                ..
            } => {
                if legal_targets.contains(&Target::Object(rancor)) {
                    Action::ChooseEffectTarget(Target::Object(rancor))
                } else if can_finish {
                    Action::FinishEffectSelection
                } else {
                    Action::ChooseEffectTarget(legal_targets[0])
                }
            }
            other => first_answer(other),
        });
        assert!(hand_contains(&state, P0, rancor), "automatic {automatic}");
        for card in &rest {
            assert_eq!(
                zone(&state, *card),
                Zone::Graveyard,
                "automatic {automatic}"
            );
        }
        assert_eq!(graveyard_len(&state, P0), 6, "the rest and the spell");
        if automatic {
            assert_eq!(order_prompts, 0, "the profile orders them");
            assert!(ordered.is_empty());
        } else {
            assert_eq!(order_prompts, 1, "the owner orders them");
            // The owner picks all but the last card; the forced final card
            // auto-completes the exact-count ordered selection.
            let mut chosen = ordered.clone();
            chosen.sort();
            chosen.dedup();
            assert_eq!(chosen.len(), rest.len() - 1, "each explicit pick once");
            assert!(
                chosen.iter().all(|card| rest.contains(card)),
                "only the rest is ordered"
            );
        }
    }
}

// ---- Militia Bugler ----------------------------------------------------------------

fn bugler_game() -> (GameState, Vec<ObjectId>) {
    let state = game(&library_of(
        &[
            "Webweaver Changeling",
            "Wild Mongrel",
            "Thraben Inspector",
            "Lightning Bolt",
        ],
        12,
    ));
    let library = state.players[0].library.clone();
    (state, library)
}

fn cast_bugler(state: &mut GameState) -> ObjectId {
    let bugler = put(state, P0, "Militia Bugler", Zone::Hand);
    pool(state, P0, &[ManaColor::W], 2);
    cast(state, bugler, &[]);
    bugler
}

fn assert_on_the_bottom(state: &GameState, cards: &[ObjectId]) {
    let library = &state.players[0].library;
    let mut bottom = library[library.len() - cards.len()..].to_vec();
    bottom.sort();
    let mut expected = cards.to_vec();
    expected.sort();
    assert_eq!(bottom, expected, "on the bottom, in some order");
}

#[test]
fn militia_bugler_takes_a_small_creature_and_bottoms_the_rest() {
    let (mut state, library) = bugler_game();
    let (changeling, mongrel, inspector, bolt) = (library[0], library[1], library[2], library[3]);
    let bugler = cast_bugler(&mut state);
    let mut offered: Vec<Vec<Target>> = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            offered.push(legal_targets.clone());
            Action::ChooseEffectTarget(Target::Object(inspector))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bugler), Zone::Battlefield);
    assert_eq!(offered.len(), 1);
    assert!(offered[0].contains(&Target::Object(mongrel)), "power 2");
    assert!(offered[0].contains(&Target::Object(inspector)), "power 1");
    assert!(
        !offered[0].contains(&Target::Object(changeling)),
        "power 3 is too big"
    );
    assert!(
        !offered[0].contains(&Target::Object(bolt)),
        "not a creature"
    );
    assert!(hand_contains(&state, P0, inspector));
    assert_eq!(hand_len(&state, P0), 1);
    assert_eq!(library_len(&state, P0), 15);
    assert_on_the_bottom(&state, &[changeling, mongrel, bolt]);
    assert_eq!(
        state.players[0].library[0], library[4],
        "the fifth card is on top"
    );
}

#[test]
fn militia_bugler_may_decline_and_puts_all_four_on_the_bottom() {
    let (mut state, library) = bugler_game();
    cast_bugler(&mut state);
    settle_first(&mut state);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(library_len(&state, P0), 16);
    assert_on_the_bottom(&state, &library[0..4]);
    assert_eq!(state.players[0].library[0], library[4]);
}

#[test]
fn militia_bugler_with_no_small_creature_just_bottoms_the_four() {
    let mut state = game(&library_of(
        &[
            "Webweaver Changeling",
            "Gurmag Angler",
            "Lightning Bolt",
            "Plains",
        ],
        12,
    ));
    let library = state.players[0].library.clone();
    cast_bugler(&mut state);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(library_len(&state, P0), 16);
    assert_on_the_bottom(&state, &library[0..4]);
    assert_eq!(state.players[0].library[0], library[4]);
}

#[test]
fn militia_bugler_looks_at_fewer_than_four_cards_of_a_short_library() {
    let mut state = game(&["Thraben Inspector", "Plains"]);
    let library = state.players[0].library.clone();
    cast_bugler(&mut state);
    settle_choosing_any(&mut state, &[library[0]]);
    assert!(hand_contains(&state, P0, library[0]));
    assert_eq!(state.players[0].library, vec![library[1]]);
}

#[test]
fn militia_bugler_has_vigilance() {
    let mut state = plains();
    let bugler = put(&mut state, P0, "Militia Bugler", Zone::Battlefield);
    assert!(keyword(&state, bugler, Keywords::VIGILANCE));
    assert_eq!(power_toughness(&state, bugler), (2, 3));
    attack_with(&mut state, vec![bugler], |_, _| None);
    assert!(!state.objects.get(bugler).tapped, "vigilance");
    assert_eq!(life(&state, P1), 18);
}

// ---- Mob Lookout ------------------------------------------------------------------------

#[test]
fn mob_lookout_connives_a_target_creature_and_a_nonland_discard_adds_a_counter() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let lookout = put(&mut state, P0, "Mob Lookout", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, lookout, &[]);
    settle_choosing_any(&mut state, &[inspector, bolt]);
    assert_eq!(zone(&state, lookout), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "discarded");
    assert_eq!(hand_len(&state, P0), 1, "drew a card");
    assert_eq!(library_len(&state, P0), 19);
    assert_eq!(counters(&state, inspector), 1, "the target connived");
    assert_eq!(power_toughness(&state, inspector), (2, 3));
    assert_eq!(counters(&state, lookout), 0);
    assert_eq!(power_toughness(&state, lookout), (0, 3));
}

#[test]
fn mob_lookout_land_discard_adds_no_counter() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let lookout = put(&mut state, P0, "Mob Lookout", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, lookout, &[]);
    // The only card in hand is the drawn Plains.
    settle_choosing_any(&mut state, &[inspector]);
    assert_eq!(counters(&state, inspector), 0);
    assert_eq!(counters(&state, lookout), 0);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(graveyard_len(&state, P0), 1, "the discarded Plains");
}

#[test]
fn mob_lookout_may_target_itself() {
    let mut state = plains();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let lookout = put(&mut state, P0, "Mob Lookout", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, lookout, &[]);
    settle_choosing_any(&mut state, &[lookout, bolt]);
    assert_eq!(counters(&state, lookout), 1);
    assert_eq!(power_toughness(&state, lookout), (1, 4));
}

#[test]
fn mob_lookout_targets_only_creatures_you_control() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let lookout = put(&mut state, P0, "Mob Lookout", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, lookout, &[]);
    let mut offered = None;
    for _ in 0..40 {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                offered = Some(legal_targets);
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => act(&mut state, first_answer(other)),
        }
    }
    let offered = offered.expect("the connive trigger asks for its target");
    assert!(offered.contains(&Target::Object(mine)));
    assert!(offered.contains(&Target::Object(lookout)));
    assert!(!offered.contains(&Target::Object(theirs)), "not yours");
}

#[test]
fn mob_lookout_does_nothing_when_its_target_left_before_the_trigger_resolved() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let lookout = put(&mut state, P0, "Mob Lookout", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, lookout, &[]);
    until_trigger_on_stack(&mut state, Some(Target::Object(inspector)));
    assert_eq!(state.stack.len(), 1);
    move_to(&mut state, inspector, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 1, "no draw and no discard");
    assert_eq!(zone(&state, bolt), Zone::Hand);
    assert_eq!(library_len(&state, P0), 20);
    assert_eq!(counters(&state, lookout), 0);
}

#[test]
fn mob_lookout_is_cast_for_either_of_its_hybrid_colors() {
    for color in [ManaColor::U, ManaColor::B] {
        let mut state = plains();
        let lookout = put(&mut state, P0, "Mob Lookout", Zone::Hand);
        pool(&mut state, P0, &[color], 1);
        assert!(castable(&mut state, lookout), "{color:?}");
    }
    let mut state = plains();
    let lookout = put(&mut state, P0, "Mob Lookout", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(!castable(&mut state, lookout), "red mana does not pay it");
}

// ---- Elite Interceptor and Rejoinder -----------------------------------------------------

fn cast_interceptor(state: &mut GameState) -> (ObjectId, ObjectId) {
    let interceptor = put(state, P0, "Elite Interceptor", Zone::Hand);
    pool(state, P0, &[ManaColor::W], 0);
    cast(state, interceptor, &[]);
    settled(state);
    let copies = exile_copies(state, "Rejoinder");
    assert_eq!(copies.len(), 1, "it enters prepared");
    (interceptor, copies[0])
}

#[test]
fn elite_interceptor_enters_prepared_and_its_rejoinder_copy_taps_and_draws() {
    let mut state = plains();
    let (interceptor, rejoinder) = cast_interceptor(&mut state);
    assert_eq!(zone(&state, interceptor), Zone::Battlefield);
    assert_eq!(power_toughness(&state, interceptor), (1, 2));
    assert!(
        !castable(&mut state, rejoinder),
        "Rejoinder costs {{1}}{{W}}"
    );
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, rejoinder, &[Target::Object(angler)]);
    let mut options = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            options = option_count;
            Action::ChooseEffectOption(0)
        }
        other => first_answer(other),
    });
    assert_eq!(options, 3, "tap, untap or neither");
    assert!(state.objects.get(angler).tapped, "tapped");
    assert_eq!(hand_len(&state, P0), 1, "drew a card");
    assert!(
        exile_copies(&state, "Rejoinder").is_empty(),
        "casting the copy unprepares it"
    );
}

#[test]
fn rejoinder_untaps_or_may_do_neither_and_always_draws() {
    for (option, tapped_before, tapped_after) in
        [(1, true, false), (2, true, true), (2, false, false)]
    {
        let mut state = plains();
        let (_, rejoinder) = cast_interceptor(&mut state);
        let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
        state.objects.get_mut(angler).tapped = tapped_before;
        pool(&mut state, P0, &[ManaColor::W], 1);
        cast(&mut state, rejoinder, &[Target::Object(angler)]);
        settle_option(&mut state, option);
        assert_eq!(
            state.objects.get(angler).tapped,
            tapped_after,
            "option {option}"
        );
        assert_eq!(hand_len(&state, P0), 1, "drew a card");
    }
}

#[test]
fn rejoinder_does_nothing_when_its_target_left_before_it_resolved() {
    let mut state = plains();
    let (_, rejoinder) = cast_interceptor(&mut state);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, rejoinder, &[Target::Object(elves)]);
    opponent_responds_with_bolt(&mut state, Target::Object(elves));
    settle_first(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(hand_len(&state, P0), 0, "no draw");
    assert_eq!(library_len(&state, P0), 20);
}

#[test]
fn elite_interceptors_rejoinder_copy_leaves_with_it() {
    let mut state = plains();
    let (interceptor, _) = cast_interceptor(&mut state);
    move_to(&mut state, interceptor, Zone::Graveyard);
    settled(&mut state);
    assert!(exile_copies(&state, "Rejoinder").is_empty());
}

// ---- Deprive ------------------------------------------------------------------------------

/// Casts `deprive` at `spell`, answering the land to return with `land`
/// when the cast asks for it.
fn cast_deprive(state: &mut GameState, deprive: ObjectId, spell: ObjectId, land: Option<ObjectId>) {
    cast(state, deprive, &[Target::Object(spell)]);
    if let Decision::ChooseCostTargets {
        cost_kind,
        candidates,
        ..
    } = next(state)
    {
        assert_eq!(cost_kind, CostKind::ReturnPermanentsToHand);
        let land = land.expect("a land was chosen for the cost");
        assert!(candidates.contains(&land));
        act(state, Action::ChooseCostTarget(land));
    }
    next(state);
}

#[test]
fn deprive_returns_a_land_to_counter_a_spell_even_a_tapped_one() {
    for tapped in [false, true] {
        let mut state = plains();
        let kept = put(&mut state, P0, "Mountain", Zone::Battlefield);
        let returned = put(&mut state, P0, "Mountain", Zone::Battlefield);
        state.objects.get_mut(returned).tapped = tapped;
        let bolt = opponent_bolts(&mut state, Target::Player(P0));
        let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
        cast_deprive(&mut state, deprive, bolt, Some(returned));
        assert_eq!(zone(&state, returned), Zone::Hand, "returned as a cost");
        assert_eq!(zone(&state, kept), Zone::Battlefield);
        settled(&mut state);
        assert_eq!(zone(&state, bolt), Zone::Graveyard, "countered");
        assert_eq!(zone(&state, deprive), Zone::Graveyard);
        assert_eq!(life(&state, P0), 20);
    }
}

#[test]
fn deprive_returns_the_only_land_without_asking() {
    let mut state = plains();
    let land = put(&mut state, P0, "Mountain", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, deprive, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, land), Zone::Hand);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn deprive_offers_only_lands_you_control_and_is_not_castable_without_one() {
    let mut state = plains();
    let mine_a = put(&mut state, P0, "Mountain", Zone::Battlefield);
    let mine_b = put(&mut state, P0, "Forest", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Mountain", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, deprive, &[Target::Object(bolt)]);
    match next(&mut state) {
        Decision::ChooseCostTargets { candidates, .. } => {
            let mut offered = candidates.clone();
            offered.sort();
            let mut expected = vec![mine_a, mine_b];
            expected.sort();
            assert_eq!(offered, expected, "only your own lands");
            act(&mut state, Action::ChooseCostTarget(mine_b));
        }
        other => panic!("expected the land choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, mine_b), Zone::Hand);
    assert_eq!(zone(&state, theirs), Zone::Battlefield);

    // With no land to return the spell can't be cast.
    let mut state = plains();
    opponent_bolts(&mut state, Target::Player(P0));
    let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    assert!(!castable(&mut state, deprive), "no land to return");
}

#[test]
fn deprive_needs_a_spell_to_target() {
    let mut state = plains();
    put(&mut state, P0, "Mountain", Zone::Battlefield);
    let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    assert!(!castable(&mut state, deprive), "no spell on the stack");
}

#[test]
fn deprive_whose_target_was_already_countered_still_returned_its_land() {
    let mut state = plains();
    let first_land = put(&mut state, P0, "Mountain", Zone::Battlefield);
    let second_land = put(&mut state, P0, "Mountain", Zone::Battlefield);
    let kept = put(&mut state, P0, "Mountain", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let first = put(&mut state, P0, "Deprive", Zone::Hand);
    let second = put(&mut state, P0, "Deprive", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::U, ManaColor::U, ManaColor::U, ManaColor::U],
        0,
    );
    cast_deprive(&mut state, first, bolt, Some(first_land));
    cast_deprive(&mut state, second, bolt, Some(second_land));
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
    assert_eq!(zone(&state, first_land), Zone::Hand, "both costs were paid");
    assert_eq!(zone(&state, second_land), Zone::Hand);
    assert_eq!(zone(&state, kept), Zone::Battlefield);
}

// ---- Public sessions --------------------------------------------------------------------------

#[derive(Default)]
struct Pf1bEvents {
    casts: [usize; PF1B_CARDS.len()],
}

impl Pf1bEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        if semantic["action_kind"].as_str() != Some("cast_spell") {
            return;
        }
        let Some(card_id) = semantic["source"]["card_db_id"]
            .as_u64()
            .and_then(|id| usize::try_from(id).ok())
        else {
            return;
        };
        let name = CARD_DEFS[card_id].name;
        if let Some(index) = PF1B_CARDS.iter().position(|&card| card == name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn pf1b_decks_finish_and_replay_through_public_sessions_both_seats() {
    let blue_red = deck(&[
        ("Island", 13),
        ("Mountain", 9),
        ("Deprive", 6),
        ("Exclude", 5),
        ("Scattershot", 4),
        ("Fiery Cannonade", 4),
        ("Bitter Reunion", 4),
        ("Mob Lookout", 4),
        ("Goblin Bushwhacker", 3),
        ("Lightning Bolt", 4),
        ("Mulldrifter", 4),
    ]);
    let green_white = deck(&[
        ("Forest", 11),
        ("Plains", 12),
        ("Kruphix's Insight", 5),
        ("Militia Bugler", 5),
        ("Elite Interceptor", 5),
        ("Rancor", 4),
        ("Abundant Growth", 4),
        ("Spirit Link", 3),
        ("Thraben Inspector", 4),
        ("Wild Mongrel", 3),
        ("Kor Skyfisher", 4),
    ]);
    let mut events = Pf1bEvents::default();
    // Twelve games plus their twelve replays, with both policies and seats.
    for pair in [
        [blue_red.clone(), green_white.clone()],
        [green_white, blue_red],
    ] {
        for seed in 0x9F1B_u64..0x9F1E {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played = play_observed(pair.clone(), None, seed, policy, &mut |d, a| {
                    events.observe(d, a)
                });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, casts) in PF1B_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} never cast through a public session");
    }
}
