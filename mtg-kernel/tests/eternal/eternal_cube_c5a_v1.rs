//! Vintage Cube wave C5a: native rules and public-session tests for the
//! statics, mana and counter-state creatures.
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
use mtg_kernel::state::{
    AbilitySourceContractV4, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target,
    Zone,
};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C5A_CARDS: [&str; 7] = [
    "Delighted Halfling",
    "Fanatic of Rhonas",
    "Carnage Interpreter",
    "Keen-Eyed Curator",
    "Baloth Prime",
    "Alpha Deathclaw",
    "Ginger, Queen of Sweets",
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
        0x4355_4245_0000 + 0x3561,
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

// ---- Helpers of this wave -------------------------------------------------------------

fn tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
}

fn stun(state: &GameState, object: ObjectId) -> i16 {
    state.objects.get(object).counters.stun
}

fn plus_counters(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.plus1_plus1
}

fn hand_size(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

/// Moves `object` to `to` through the engine's zone-change event.
fn move_to(state: &mut GameState, object: ObjectId, to: Zone) {
    propose_and_commit(state, ProposedEvent::zone_change(object, to));
}

/// Advances until the ability just activated is on the stack with its costs
/// paid (the engine finishes a pending activation as it next advances).
fn on_the_stack(state: &mut GameState) {
    for _ in 0..20 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => return,
            other => {
                let action = first_answer(other);
                act(state, action);
            }
        }
    }
    panic!("the activation did not finish");
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

/// After P0 cast `spell` (which waits on the stack), P1 counters it with a
/// Counterspell. Returns the Counterspell, resolved or not.
fn opponent_counters(state: &mut GameState, spell: ObjectId) -> ObjectId {
    priority(state);
    act(state, Action::Pass);
    let counterspell = put(state, P1, "Counterspell", Zone::Hand);
    pool(state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(state, counterspell, &[Target::Object(spell)]);
    settled(state);
    counterspell
}

/// Passes until P1's draw step: P1's upkeep, with what it set off, is over.
fn to_opponents_draw_step(state: &mut GameState) {
    pass_until(state, |state| {
        state.active_player == P1 && state.step == Step::Draw
    });
}

/// Passes until P0's next draw step: P0's upkeep, with what it set off, is
/// over.
fn to_own_draw_step(state: &mut GameState) {
    pass_until(state, |state| {
        state.active_player == P0 && state.step == Step::Draw
    });
}

/// Makes `player` the monarch, as an object of theirs having made them so.
fn make_monarch(state: &mut GameState, player: PlayerId, source: ObjectId) {
    state.monarch = Some(player);
    state.engine.monarch_source = Some(AbilitySourceContractV4::capture(state, source));
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn c5a_cards_are_full() {
    for name in C5A_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c5a_cards_have_their_printed_characteristics() {
    let halfling = def("Delighted Halfling");
    assert_eq!(halfling.mana_value, 1);
    assert_eq!(halfling.cost.generic, 0);
    assert_eq!(halfling.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(halfling.types, &[CardType::Creature]);
    assert_eq!(halfling.subtypes, &[Subtype::Halfling, Subtype::Citizen]);
    assert_eq!((halfling.power, halfling.toughness), (Some(1), Some(2)));
    assert_eq!(halfling.restricted_mana_abilities.len(), 1);
    assert!(halfling.additional_mana_abilities.is_empty());

    let fanatic = def("Fanatic of Rhonas");
    assert_eq!(fanatic.mana_value, 2);
    assert_eq!(fanatic.cost.generic, 1);
    assert_eq!(fanatic.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(fanatic.types, &[CardType::Creature]);
    assert_eq!(fanatic.subtypes, &[Subtype::Snake, Subtype::Druid]);
    assert_eq!((fanatic.power, fanatic.toughness), (Some(1), Some(4)));
    assert_eq!(fanatic.additional_mana_abilities.len(), 1, "ferocious");
    assert_eq!(fanatic.activated_abilities.len(), 1, "eternalize");
    assert_eq!(
        fanatic.activated_abilities[0].activation_zone,
        Zone::Graveyard
    );
    assert!(fanatic.activated_abilities[0].sorcery_speed_only);

    let carnage = def("Carnage Interpreter");
    assert_eq!(carnage.mana_value, 3);
    assert_eq!(carnage.cost.generic, 1);
    assert_eq!(
        carnage.cost.pips,
        &[
            Pip::Hybrid(ManaColor::B, ManaColor::R),
            Pip::Hybrid(ManaColor::B, ManaColor::R)
        ]
    );
    assert_eq!(carnage.types, &[CardType::Creature]);
    assert_eq!(carnage.subtypes, &[Subtype::Devil, Subtype::Detective]);
    assert_eq!((carnage.power, carnage.toughness), (Some(3), Some(3)));
    assert!(
        !carnage.keywords.has(Keywords::MENACE),
        "only conditionally"
    );

    let curator = def("Keen-Eyed Curator");
    assert_eq!(curator.mana_value, 2);
    assert_eq!(curator.cost.generic, 0);
    assert_eq!(
        curator.cost.pips,
        &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)]
    );
    assert_eq!(curator.types, &[CardType::Creature]);
    assert_eq!(curator.subtypes, &[Subtype::Raccoon, Subtype::Scout]);
    assert_eq!((curator.power, curator.toughness), (Some(3), Some(3)));
    assert_eq!(curator.activated_abilities.len(), 1);
    assert!(
        !curator.keywords.has(Keywords::TRAMPLE),
        "only conditionally"
    );

    let baloth = def("Baloth Prime");
    assert_eq!(baloth.mana_value, 4);
    assert_eq!(baloth.cost.generic, 3);
    assert_eq!(baloth.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(baloth.types, &[CardType::Creature]);
    assert_eq!(baloth.subtypes, &[Subtype::Beast]);
    assert_eq!((baloth.power, baloth.toughness), (Some(10), Some(10)));
    assert!(baloth.enters_battlefield_tapped);
    assert_eq!(baloth.activated_abilities.len(), 1);

    let claw = def("Alpha Deathclaw");
    assert_eq!(claw.mana_value, 6);
    assert_eq!(claw.cost.generic, 4);
    assert_eq!(
        claw.cost.pips,
        &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::G)]
    );
    assert_eq!(claw.types, &[CardType::Creature]);
    assert_eq!(claw.subtypes, &[Subtype::Lizard, Subtype::Mutant]);
    assert_eq!((claw.power, claw.toughness), (Some(6), Some(6)));
    assert!(claw.keywords.has(Keywords::MENACE));
    assert!(claw.keywords.has(Keywords::TRAMPLE));
    assert_eq!(claw.activated_abilities.len(), 1, "monstrosity");

    let ginger = def("Ginger, Queen of Sweets");
    assert_eq!(ginger.mana_value, 6);
    assert_eq!(ginger.cost.generic, 6);
    assert!(ginger.cost.pips.is_empty());
    assert!(ginger.colors.is_empty());
    assert_eq!(ginger.supertypes, &[Supertype::Legendary]);
    assert_eq!(ginger.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(ginger.subtypes, &[Subtype::Food, Subtype::Noble]);
    assert_eq!((ginger.power, ginger.toughness), (Some(6), Some(4)));
    assert_eq!(ginger.activated_abilities.len(), 1);

    let beast = def("Baloth Beast Token");
    assert!(beast.is_token);
    assert_eq!(beast.types, &[CardType::Creature]);
    assert_eq!(beast.subtypes, &[Subtype::Beast]);
    assert_eq!(beast.colors, &[ManaColor::G]);
    assert_eq!((beast.power, beast.toughness), (Some(4), Some(4)));

    let brute = def("Gingerbrute Token");
    assert!(brute.is_token);
    assert_eq!(brute.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(brute.subtypes, &[Subtype::Food, Subtype::Golem]);
    assert!(brute.colors.is_empty());
    assert_eq!((brute.power, brute.toughness), (Some(1), Some(1)));
    assert!(brute.keywords.has(Keywords::HASTE));
    assert_eq!(brute.activated_abilities.len(), 2);

    let eternalized = def("Fanatic of Rhonas Eternalized Token");
    assert!(eternalized.is_token);
    assert_eq!(eternalized.types, &[CardType::Creature]);
    assert_eq!(
        eternalized.subtypes,
        &[Subtype::Zombie, Subtype::Snake, Subtype::Druid]
    );
    assert_eq!(eternalized.colors, &[ManaColor::B]);
    assert_eq!(
        (eternalized.power, eternalized.toughness),
        (Some(4), Some(4))
    );
    assert_eq!(eternalized.additional_mana_abilities.len(), 1, "ferocious");
}

// ---- Carnage Interpreter -------------------------------------------------------------

#[test]
fn carnage_interpreter_discards_the_hand_then_investigates_four_times() {
    let mut state = plains();
    let carnage = put(&mut state, P0, "Carnage Interpreter", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let forest = put(&mut state, P0, "Forest", Zone::Hand);
    let theirs = put(&mut state, P1, "Forest", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::R], 1);
    cast(&mut state, carnage, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, carnage), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, forest), Zone::Graveyard);
    assert_eq!(zone(&state, theirs), Zone::Hand, "only your own hand");
    assert_eq!(hand_size(&state, P0), 0);
    assert_eq!(named(&state, P0, "Clue Token").len(), 4);
    // The discard came first, so the hand is empty: 5/5 with menace.
    assert_eq!(power_toughness(&state, carnage), (5, 5));
    assert!(keyword(&state, carnage, Keywords::MENACE));

    // An empty hand still investigates four times.
    let mut state = plains();
    let carnage = put(&mut state, P0, "Carnage Interpreter", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 1);
    cast(&mut state, carnage, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, carnage), Zone::Battlefield);
    assert_eq!(named(&state, P0, "Clue Token").len(), 4);
}

#[test]
fn carnage_interpreter_is_a_five_five_with_menace_only_with_one_or_fewer_cards_in_hand() {
    for (cards, big) in [(0_usize, true), (1, true), (2, false), (3, false)] {
        let mut state = plains();
        let carnage = put(&mut state, P0, "Carnage Interpreter", Zone::Battlefield);
        for _ in 0..cards {
            put(&mut state, P0, "Forest", Zone::Hand);
        }
        // Only the controller's own hand counts.
        for _ in 0..5 {
            put(&mut state, P1, "Forest", Zone::Hand);
        }
        let expected = if big { (5, 5) } else { (3, 3) };
        assert_eq!(power_toughness(&state, carnage), expected, "{cards} cards");
        assert_eq!(
            keyword(&state, carnage, Keywords::MENACE),
            big,
            "{cards} cards"
        );
    }
}

#[test]
fn carnage_interpreter_follows_the_hand_as_cards_are_cast_and_drawn() {
    let mut state = plains();
    let carnage = put(&mut state, P0, "Carnage Interpreter", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    put(&mut state, P0, "Forest", Zone::Hand);
    let clue = token(&mut state, P0, "Clue Token");
    assert_eq!(power_toughness(&state, carnage), (3, 3));
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(hand_size(&state, P0), 1);
    assert_eq!(power_toughness(&state, carnage), (5, 5));
    assert!(keyword(&state, carnage, Keywords::MENACE));
    // Cracking the Clue draws a card: two cards in hand, so back to 3/3.
    pool(&mut state, P0, &[], 2);
    activate(&mut state, clue, 0, &[]);
    settled(&mut state);
    assert_eq!(hand_size(&state, P0), 2);
    assert_eq!(power_toughness(&state, carnage), (3, 3));
    assert!(!keyword(&state, carnage, Keywords::MENACE));
}

// ---- Delighted Halfling --------------------------------------------------------------

#[test]
fn delighted_halfling_taps_for_colorless_mana() {
    let mut state = plains();
    let halfling = put(&mut state, P0, "Delighted Halfling", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(halfling, ManaColor::C),
    );
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
    assert!(tapped(&state, halfling));
    assert!(state.players[0].restricted_mana_pool.0.is_empty());
    assert!(
        engine::step(
            &mut state,
            Action::ActivateManaAbilityChoice(halfling, ManaColor::C)
        )
        .is_err(),
        "it is tapped"
    );
}

#[test]
fn delighted_halfling_taps_for_restricted_mana_of_any_color() {
    for color in [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ] {
        let mut state = plains();
        let halfling = put(&mut state, P0, "Delighted Halfling", Zone::Battlefield);
        next(&mut state);
        act(
            &mut state,
            Action::ActivateManaAbilityChoice(halfling, color),
        );
        assert_eq!(mana(&state, P0, color), 0, "{color:?} is not free mana");
        let units = &state.players[0].restricted_mana_pool.0;
        assert_eq!(units.len(), 1, "{color:?}");
        assert_eq!(units[0].color, color);
        assert_eq!(
            units[0].restriction,
            mtg_kernel::card_def::ManaSpendRestrictionDef::LegendarySpell
        );
        assert!(tapped(&state, halfling), "{color:?}");
    }
}

#[test]
fn delighted_halfling_needs_to_be_untapped_and_free_of_summoning_sickness() {
    let mut state = plains();
    let halfling = put(&mut state, P0, "Delighted Halfling", Zone::Battlefield);
    state.objects.get_mut(halfling).summoning_sick = true;
    next(&mut state);
    for color in [ManaColor::C, ManaColor::U] {
        assert!(
            engine::step(
                &mut state,
                Action::ActivateManaAbilityChoice(halfling, color)
            )
            .is_err(),
            "{color:?}: summoning sick"
        );
    }
}

#[test]
fn delighted_halfling_restricted_mana_pays_only_for_legendary_spells() {
    let mut state = plains();
    let halfling = put(&mut state, P0, "Delighted Halfling", Zone::Battlefield);
    let marwyn = put(&mut state, P0, "Marwyn, the Preserver", Zone::Hand);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(halfling, ManaColor::G),
    );
    assert!(
        !castable(&mut state, elves),
        "{{G}} of restricted mana does not pay a nonlegendary spell"
    );
    assert!(
        !castable(&mut state, marwyn),
        "the legendary {{1}}{{G}} still needs its generic mana"
    );
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, marwyn));
    assert!(!castable(&mut state, elves));
    cast(&mut state, marwyn, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, marwyn), Zone::Battlefield);
    assert!(state.players[0].restricted_mana_pool.0.is_empty());
}

#[test]
fn a_legendary_spell_paid_with_delighted_halflings_mana_cant_be_countered() {
    // The restricted mana was floated first.
    let mut state = plains();
    let halfling = put(&mut state, P0, "Delighted Halfling", Zone::Battlefield);
    let marwyn = put(&mut state, P0, "Marwyn, the Preserver", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(halfling, ManaColor::G),
    );
    cast(&mut state, marwyn, &[]);
    let counterspell = opponent_counters(&mut state, marwyn);
    assert_eq!(zone(&state, counterspell), Zone::Graveyard, "it resolved");
    assert_eq!(
        zone(&state, marwyn),
        Zone::Battlefield,
        "it can't be countered"
    );

    // The Halfling is tapped for the {G} of the cost as the spell is paid.
    let mut state = plains();
    let halfling = put(&mut state, P0, "Delighted Halfling", Zone::Battlefield);
    let marwyn = put(&mut state, P0, "Marwyn, the Preserver", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, marwyn, &[]);
    let counterspell = opponent_counters(&mut state, marwyn);
    assert_eq!(zone(&state, counterspell), Zone::Graveyard, "it resolved");
    assert_eq!(
        zone(&state, marwyn),
        Zone::Battlefield,
        "it can't be countered"
    );
    assert!(tapped(&state, halfling), "the Halfling paid for the {{G}}");
}

#[test]
fn a_legendary_spell_is_countered_when_delighted_halflings_mana_was_not_spent_on_it() {
    // Ordinary mana only.
    let mut state = plains();
    let marwyn = put(&mut state, P0, "Marwyn, the Preserver", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, marwyn, &[]);
    let counterspell = opponent_counters(&mut state, marwyn);
    assert_eq!(zone(&state, counterspell), Zone::Graveyard);
    assert_eq!(zone(&state, marwyn), Zone::Graveyard, "countered");

    // The Halfling's {C} pays only the generic part: unrestricted mana.
    let mut state = plains();
    let halfling = put(&mut state, P0, "Delighted Halfling", Zone::Battlefield);
    let marwyn = put(&mut state, P0, "Marwyn, the Preserver", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(halfling, ManaColor::C),
    );
    cast(&mut state, marwyn, &[]);
    let counterspell = opponent_counters(&mut state, marwyn);
    assert_eq!(zone(&state, counterspell), Zone::Graveyard);
    assert_eq!(zone(&state, marwyn), Zone::Graveyard, "countered");
}

// ---- Fanatic of Rhonas ---------------------------------------------------------------

#[test]
fn fanatic_of_rhonas_taps_for_green() {
    let mut state = plains();
    let fanatic = put(&mut state, P0, "Fanatic of Rhonas", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(fanatic, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert!(tapped(&state, fanatic));
    assert!(
        engine::step(
            &mut state,
            Action::ActivateManaAbilityChoice(fanatic, ManaColor::G)
        )
        .is_err(),
        "it is tapped"
    );
}

#[test]
fn fanatic_of_rhonas_ferocious_adds_four_green_only_with_a_creature_of_power_four() {
    // No creature of power four or greater: your own 3/3 and their 4/4 do not
    // count.
    let mut state = plains();
    let fanatic = put(&mut state, P0, "Fanatic of Rhonas", Zone::Battlefield);
    let curator = put(&mut state, P0, "Keen-Eyed Curator", Zone::Battlefield);
    token(&mut state, P1, "Baloth Beast Token");
    next(&mut state);
    let before = state.clone();
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(fanatic, ManaColor::C)
    )
    .is_err());
    assert_eq!(state, before, "nothing changed");

    // A +1/+1 counter makes the Curator a 4/3: ferocious is on.
    state.objects.get_mut(curator).counters.plus1_plus1 = 1;
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(fanatic, ManaColor::C),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 4);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert!(tapped(&state, fanatic));
}

#[test]
fn fanatic_of_rhonas_ferocious_counts_a_four_four_beside_it() {
    let mut state = plains();
    let fanatic = put(&mut state, P0, "Fanatic of Rhonas", Zone::Battlefield);
    token(&mut state, P0, "Baloth Beast Token");
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(fanatic, ManaColor::C),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 4);
    // Both abilities tap it.
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(fanatic, ManaColor::G)
    )
    .is_err());
}

#[test]
fn fanatic_of_rhonas_eternalizes_into_a_four_four_black_zombie_snake_druid() {
    let mut state = plains();
    let fanatic = put(&mut state, P0, "Fanatic of Rhonas", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    assert!(activatable(&mut state, fanatic, 0));
    activate(&mut state, fanatic, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, fanatic), Zone::Exile, "exiled as a cost");
    let tokens = named(&state, P0, "Fanatic of Rhonas Eternalized Token");
    assert_eq!(tokens.len(), 1);
    let token = tokens[0];
    assert_eq!(power_toughness(&state, token), (4, 4));
    assert!(engine::object_has_type(&state, token, CardType::Creature));
    for subtype in [Subtype::Zombie, Subtype::Snake, Subtype::Druid] {
        assert!(engine::has_effective_subtype(&state, token, subtype));
    }
    assert_eq!(
        engine::object_color_mask(&state, token),
        mtg_kernel::card_def::mana_color_mask(ManaColor::B)
    );
    assert_eq!(
        CARD_DEFS[state.objects.get(token).card_def as usize].object_name,
        "Fanatic of Rhonas"
    );

    // The copy keeps both mana abilities: it is itself a creature with power
    // four, so ferocious is on.
    state.objects.get_mut(token).summoning_sick = false;
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(token, ManaColor::C),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 4);

    let mut state = plains();
    let fanatic = put(&mut state, P0, "Fanatic of Rhonas", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    activate(&mut state, fanatic, 0, &[]);
    settled(&mut state);
    let token = named(&state, P0, "Fanatic of Rhonas Eternalized Token")[0];
    state.objects.get_mut(token).summoning_sick = false;
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(token, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1, "{{T}}: Add {{G}}");
}

#[test]
fn fanatic_of_rhonas_eternalize_needs_its_mana_the_graveyard_and_sorcery_timing() {
    // Short of mana.
    let mut state = plains();
    let fanatic = put(&mut state, P0, "Fanatic of Rhonas", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G], 2);
    assert!(!activatable(&mut state, fanatic, 0), "{{2}}{{G}}{{G}}");

    // Not from the battlefield.
    let mut state = plains();
    let fanatic = put(&mut state, P0, "Fanatic of Rhonas", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    assert!(
        !activatable(&mut state, fanatic, 0),
        "only from a graveyard"
    );

    // Only as a sorcery: not with a spell on the stack.
    let mut state = plains();
    let fanatic = put(&mut state, P0, "Fanatic of Rhonas", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    opponent_bolts(&mut state, Target::Player(P0));
    assert!(!activatable(&mut state, fanatic, 0), "sorcery timing");
}

// ---- Keen-Eyed Curator ---------------------------------------------------------------

/// Pays {1} and activates `curator`'s exile of `card`.
fn curator_exiles(state: &mut GameState, curator: ObjectId, card: ObjectId) {
    pool(state, P0, &[], 1);
    activate(state, curator, 0, &[Target::Object(card)]);
    settled(state);
    assert_eq!(zone(state, card), Zone::Exile);
}

#[test]
fn keen_eyed_curator_exiles_a_card_from_any_graveyard_for_one_mana() {
    let mut state = plains();
    let curator = put(&mut state, P0, "Keen-Eyed Curator", Zone::Battlefield);
    let mine = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Forest", Zone::Graveyard);
    assert!(!activatable(&mut state, curator, 0), "{{1}} is needed");
    curator_exiles(&mut state, curator, mine);
    curator_exiles(&mut state, curator, theirs);
    assert_eq!(power_toughness(&state, curator), (3, 3), "two types so far");
    assert!(!keyword(&state, curator, Keywords::TRAMPLE));
}

#[test]
fn keen_eyed_curator_gets_four_four_and_trample_with_four_card_types_exiled() {
    let mut state = plains();
    let curator = put(&mut state, P0, "Keen-Eyed Curator", Zone::Battlefield);
    let ballista = put(&mut state, P1, "Walking Ballista", Zone::Graveyard);
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Graveyard);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    // An artifact creature counts as two types.
    curator_exiles(&mut state, curator, ballista);
    assert_eq!(power_toughness(&state, curator), (3, 3));
    curator_exiles(&mut state, curator, forest);
    assert_eq!(power_toughness(&state, curator), (3, 3), "three types");
    assert!(!keyword(&state, curator, Keywords::TRAMPLE));
    // A card of a type already among them adds nothing.
    curator_exiles(&mut state, curator, ring);
    curator_exiles(&mut state, curator, elves);
    assert_eq!(
        power_toughness(&state, curator),
        (3, 3),
        "still three types"
    );
    // Instant: the fourth type.
    curator_exiles(&mut state, curator, bolt);
    assert_eq!(power_toughness(&state, curator), (7, 7));
    assert!(keyword(&state, curator, Keywords::TRAMPLE));

    // A card that leaves exile no longer counts.
    move_to(&mut state, bolt, Zone::Graveyard);
    assert_eq!(power_toughness(&state, curator), (3, 3));
    assert!(!keyword(&state, curator, Keywords::TRAMPLE));
}

#[test]
fn keen_eyed_curator_counts_only_the_cards_it_exiled_itself() {
    let mut state = plains();
    let curator = put(&mut state, P0, "Keen-Eyed Curator", Zone::Battlefield);
    let other = put(&mut state, P0, "Keen-Eyed Curator", Zone::Battlefield);
    let cards = [
        put(&mut state, P1, "Walking Ballista", Zone::Graveyard),
        put(&mut state, P0, "Forest", Zone::Graveyard),
        put(&mut state, P1, "Lightning Bolt", Zone::Graveyard),
    ];
    for card in cards {
        curator_exiles(&mut state, curator, card);
    }
    assert_eq!(power_toughness(&state, curator), (7, 7));
    assert_eq!(
        power_toughness(&state, other),
        (3, 3),
        "its own list is empty"
    );
    assert!(!keyword(&state, other, Keywords::TRAMPLE));

    // Leaving the battlefield loses the link: the returned Curator is a new
    // object.
    move_to(&mut state, curator, Zone::Hand);
    move_to(&mut state, curator, Zone::Battlefield);
    assert_eq!(zone(&state, curator), Zone::Battlefield);
    assert_eq!(power_toughness(&state, curator), (3, 3));
    assert!(!keyword(&state, curator, Keywords::TRAMPLE));
}

#[test]
fn keen_eyed_curator_exiles_nothing_when_its_target_left_the_graveyard() {
    let mut state = plains();
    let curator = put(&mut state, P0, "Keen-Eyed Curator", Zone::Battlefield);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, curator, 0, &[Target::Object(bolt)]);
    on_the_stack(&mut state);
    move_to(&mut state, bolt, Zone::Hand);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Hand, "not exiled");
}

// ---- Baloth Prime --------------------------------------------------------------------

#[test]
fn baloth_prime_enters_tapped_with_six_stun_counters() {
    let mut state = plains();
    let baloth = put(&mut state, P0, "Baloth Prime", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 3);
    cast(&mut state, baloth, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, baloth), Zone::Battlefield);
    assert!(tapped(&state, baloth));
    assert_eq!(stun(&state, baloth), 6);
    assert_eq!(power_toughness(&state, baloth), (10, 10));
}

#[test]
fn baloth_prime_enters_without_stun_counters_when_it_has_no_abilities() {
    // Dress Down: "Creatures lose all abilities." The entry replacement is
    // one of them.
    let mut state = plains();
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, dress), Zone::Battlefield);
    let baloth = put(&mut state, P0, "Baloth Prime", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 3);
    cast(&mut state, baloth, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, baloth), Zone::Battlefield);
    assert_eq!(stun(&state, baloth), 0);
}

#[test]
fn baloth_prime_loses_one_stun_counter_at_each_untap_step_instead_of_untapping() {
    let mut state = plains();
    let baloth = put(&mut state, P0, "Baloth Prime", Zone::Battlefield);
    state.objects.get_mut(baloth).tapped = true;
    state.objects.get_mut(baloth).counters.stun = 2;
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Upkeep
    });
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Upkeep
    });
    assert_eq!(stun(&state, baloth), 1);
    assert!(tapped(&state, baloth), "the untap was replaced");
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Upkeep
    });
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Upkeep
    });
    assert_eq!(stun(&state, baloth), 0);
    assert!(tapped(&state, baloth), "the second untap was replaced too");
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Upkeep
    });
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Upkeep
    });
    assert!(!tapped(&state, baloth), "no counters left: it untaps");
}

#[test]
fn baloth_prime_makes_a_tapped_beast_and_untaps_whenever_you_sacrifice_a_land() {
    // Sacrificing the land to its own ability triggers it.
    let mut state = plains();
    let baloth = put(&mut state, P0, "Baloth Prime", Zone::Battlefield);
    state.objects.get_mut(baloth).tapped = true;
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    pool(&mut state, P0, &[], 4);
    assert!(activatable(&mut state, baloth, 0));
    activate(&mut state, baloth, 0, &[]);
    settle_choosing(&mut state, forest);
    assert_eq!(zone(&state, forest), Zone::Graveyard);
    assert_eq!(life(&state, P0), 22, "gain 2 life");
    let beasts = named(&state, P0, "Baloth Beast Token");
    assert_eq!(beasts.len(), 1);
    assert!(tapped(&state, beasts[0]), "a tapped Beast");
    assert_eq!(power_toughness(&state, beasts[0]), (4, 4));
    assert!(!tapped(&state, baloth), "untapped");

    // With stun counters left, the untap removes one instead.
    let mut state = plains();
    let baloth = put(&mut state, P0, "Baloth Prime", Zone::Battlefield);
    state.objects.get_mut(baloth).tapped = true;
    state.objects.get_mut(baloth).counters.stun = 6;
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    pool(&mut state, P0, &[], 4);
    activate(&mut state, baloth, 0, &[]);
    settle_choosing(&mut state, forest);
    assert_eq!(named(&state, P0, "Baloth Beast Token").len(), 1);
    assert_eq!(stun(&state, baloth), 5);
    assert!(tapped(&state, baloth));
}

#[test]
fn baloth_prime_triggers_for_each_land_sacrificed_and_only_for_lands() {
    let mut state = plains();
    let baloth = put(&mut state, P0, "Baloth Prime", Zone::Battlefield);
    let first = put(&mut state, P0, "Forest", Zone::Battlefield);
    let second = put(&mut state, P0, "Swamp", Zone::Battlefield);
    for land in [first, second] {
        pool(&mut state, P0, &[], 4);
        activate(&mut state, baloth, 0, &[]);
        settle_choosing(&mut state, land);
        assert_eq!(zone(&state, land), Zone::Graveyard);
    }
    assert_eq!(named(&state, P0, "Baloth Beast Token").len(), 2);
    assert_eq!(life(&state, P0), 24);

    // Without a land the ability can't be paid.
    pool(&mut state, P0, &[], 4);
    assert!(!activatable(&mut state, baloth, 0));

    // A sacrificed creature is not a land.
    let mut state = plains();
    put(&mut state, P0, "Baloth Prime", Zone::Battlefield);
    let ginger = put(&mut state, P0, "Ginger, Queen of Sweets", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, ginger, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ginger), Zone::Graveyard);
    assert!(named(&state, P0, "Baloth Beast Token").is_empty());
}

// ---- Alpha Deathclaw -----------------------------------------------------------------

#[test]
fn alpha_deathclaw_destroys_a_target_permanent_when_it_enters() {
    let mut state = plains();
    let claw = put(&mut state, P0, "Alpha Deathclaw", Zone::Hand);
    let victim = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let land = put(&mut state, P1, "Forest", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 4);
    cast(&mut state, claw, &[]);
    settle_choosing(&mut state, victim);
    assert_eq!(zone(&state, claw), Zone::Battlefield);
    assert_eq!(zone(&state, victim), Zone::Graveyard);
    assert_eq!(zone(&state, land), Zone::Battlefield);
    assert_eq!(power_toughness(&state, claw), (6, 6));

    // Any permanent: a land, too.
    let mut state = plains();
    let claw = put(&mut state, P0, "Alpha Deathclaw", Zone::Hand);
    let land = put(&mut state, P1, "Forest", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 4);
    cast(&mut state, claw, &[]);
    settle_choosing(&mut state, land);
    assert_eq!(zone(&state, land), Zone::Graveyard);
}

#[test]
fn alpha_deathclaw_entry_trigger_does_nothing_when_its_target_is_gone() {
    let mut state = plains();
    let claw = put(&mut state, P0, "Alpha Deathclaw", Zone::Hand);
    let victim = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 4);
    cast(&mut state, claw, &[]);
    until_trigger_on_stack(&mut state, Some(Target::Object(victim)));
    move_to(&mut state, victim, Zone::Hand);
    settled(&mut state);
    assert_eq!(zone(&state, victim), Zone::Hand);
    assert_eq!(zone(&state, claw), Zone::Battlefield);
}

#[test]
fn alpha_deathclaw_monstrosity_adds_four_counters_once_and_destroys_a_permanent() {
    let mut state = plains();
    let claw = put(&mut state, P0, "Alpha Deathclaw", Zone::Battlefield);
    let first = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let second = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    assert!(!activatable(&mut state, claw, 0), "{{5}}{{B}}{{G}}");
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 5);
    activate(&mut state, claw, 0, &[]);
    settle_choosing(&mut state, first);
    assert_eq!(plus_counters(&state, claw), 4);
    assert_eq!(power_toughness(&state, claw), (10, 10));
    assert_eq!(zone(&state, first), Zone::Graveyard, "becomes monstrous");
    assert_eq!(zone(&state, second), Zone::Battlefield);

    // Already monstrous: no more counters and no trigger.
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 5);
    activate(&mut state, claw, 0, &[]);
    settle_choosing(&mut state, second);
    assert_eq!(plus_counters(&state, claw), 4);
    assert_eq!(
        zone(&state, second),
        Zone::Battlefield,
        "it did not trigger"
    );
}

#[test]
fn alpha_deathclaw_monstrous_trigger_belongs_to_its_controller_as_it_resolves() {
    // The opponent takes control of the Deathclaw with the monstrosity
    // ability on the stack: the counters still go on, and the new controller
    // controls the trigger and chooses its target.
    let mut state = plains();
    let claw = put(&mut state, P0, "Alpha Deathclaw", Zone::Battlefield);
    let mine = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 5);
    activate(&mut state, claw, 0, &[]);
    on_the_stack(&mut state);
    state.players[P0.index()]
        .battlefield
        .retain(|&object| object != claw);
    state.players[P1.index()].battlefield.push(claw);
    state.objects.get_mut(claw).controller = P1;
    let mut chooser = None;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            player,
            legal_targets,
            ..
        } => {
            chooser = Some(player);
            assert!(legal_targets.contains(&Target::Object(mine)));
            Action::ChooseTarget(Target::Object(mine))
        }
        other => first_answer(other),
    });
    assert_eq!(chooser, Some(P1), "the new controller chooses");
    assert_eq!(plus_counters(&state, claw), 4);
    assert_eq!(zone(&state, mine), Zone::Graveyard);
    assert_eq!(zone(&state, theirs), Zone::Battlefield);
}

#[test]
fn alpha_deathclaw_without_its_abilities_becomes_monstrous_and_destroys_nothing() {
    // Dress Down resolves with the monstrosity ability on the stack.
    let mut state = plains();
    let claw = put(&mut state, P0, "Alpha Deathclaw", Zone::Battlefield);
    let victim = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 5);
    activate(&mut state, claw, 0, &[]);
    on_the_stack(&mut state);
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, dress), Zone::Battlefield);
    assert_eq!(plus_counters(&state, claw), 4, "the ability still resolved");
    assert_eq!(zone(&state, victim), Zone::Battlefield, "no trigger");
}

// ---- Ginger, Queen of Sweets ---------------------------------------------------------

#[test]
fn ginger_makes_you_the_monarch_when_it_enters() {
    let mut state = plains();
    let ginger = put(&mut state, P0, "Ginger, Queen of Sweets", Zone::Hand);
    pool(&mut state, P0, &[], 6);
    assert_eq!(state.monarch, None);
    cast(&mut state, ginger, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ginger), Zone::Battlefield);
    assert_eq!(state.monarch, Some(P0));
    assert_eq!(power_toughness(&state, ginger), (6, 4));
}

#[test]
fn ginger_sacrifices_itself_for_six_life() {
    let mut state = plains();
    let ginger = put(&mut state, P0, "Ginger, Queen of Sweets", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, ginger, 0), "{{2}}");
    pool(&mut state, P0, &[], 1);
    activate(&mut state, ginger, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ginger), Zone::Graveyard);
    assert_eq!(life(&state, P0), 26);

    // {T} is part of the cost: not while it is summoning sick.
    let mut state = plains();
    let ginger = put(&mut state, P0, "Ginger, Queen of Sweets", Zone::Battlefield);
    state.objects.get_mut(ginger).summoning_sick = true;
    pool(&mut state, P0, &[], 2);
    assert!(!activatable(&mut state, ginger, 0));
}

#[test]
fn ginger_creates_a_gingerbrute_at_each_upkeep_while_you_are_the_monarch() {
    let mut state = plains();
    let ginger = put(&mut state, P0, "Ginger, Queen of Sweets", Zone::Hand);
    pool(&mut state, P0, &[], 6);
    cast(&mut state, ginger, &[]);
    settled(&mut state);
    assert_eq!(state.monarch, Some(P0));
    assert!(named(&state, P0, "Gingerbrute Token").is_empty());
    // The opponent's upkeep, too: "each upkeep".
    to_opponents_draw_step(&mut state);
    assert_eq!(named(&state, P0, "Gingerbrute Token").len(), 1);
    assert!(named(&state, P1, "Gingerbrute Token").is_empty());
    to_own_draw_step(&mut state);
    assert_eq!(named(&state, P0, "Gingerbrute Token").len(), 2);
}

#[test]
fn ginger_does_nothing_at_upkeep_unless_its_controller_is_the_monarch() {
    // Nobody is the monarch.
    let mut state = plains();
    put(&mut state, P0, "Ginger, Queen of Sweets", Zone::Battlefield);
    to_opponents_draw_step(&mut state);
    to_own_draw_step(&mut state);
    assert!(named(&state, P0, "Gingerbrute Token").is_empty());

    // The opponent is the monarch.
    let mut state = plains();
    put(&mut state, P0, "Ginger, Queen of Sweets", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    make_monarch(&mut state, P1, elves);
    to_opponents_draw_step(&mut state);
    to_own_draw_step(&mut state);
    assert!(named(&state, P0, "Gingerbrute Token").is_empty());
    assert!(named(&state, P1, "Gingerbrute Token").is_empty());
}

#[test]
fn ginger_trigger_checks_the_monarch_again_as_it_resolves() {
    let mut state = plains();
    let ginger = put(&mut state, P0, "Ginger, Queen of Sweets", Zone::Hand);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 6);
    cast(&mut state, ginger, &[]);
    settled(&mut state);
    assert_eq!(state.monarch, Some(P0));
    // P1's upkeep: the trigger waits on the stack.
    run_until(
        &mut state,
        |state| state.active_player == P1 && state.step == Step::Upkeep && !state.stack.is_empty(),
        |_, _| None,
    );
    assert_eq!(state.stack.len(), 1);
    // The crown changes hands in response.
    make_monarch(&mut state, P1, elves);
    settled(&mut state);
    assert!(named(&state, P0, "Gingerbrute Token").is_empty());
}

#[test]
fn the_gingerbrute_token_is_a_hasty_food_golem_with_evasion_and_a_sacrifice_ability() {
    let mut state = plains();
    let brute = token(&mut state, P0, "Gingerbrute Token");
    let hasty = put(&mut state, P1, "Gingerbrute", Zone::Battlefield);
    let ordinary = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert_eq!(power_toughness(&state, brute), (1, 1));
    assert!(keyword(&state, brute, Keywords::HASTE));
    assert!(engine::object_has_type(&state, brute, CardType::Artifact));
    assert!(engine::object_has_type(&state, brute, CardType::Creature));
    assert_eq!(engine::object_mana_value(&state, brute), 1, "a {{1}} token");
    // "{1}: can't be blocked this turn except by creatures with haste."
    pool(&mut state, P0, &[], 1);
    activate(&mut state, brute, 0, &[]);
    settled(&mut state);
    let blockers = legal_blockers(&mut state, brute);
    assert!(blockers.contains(&hasty));
    assert!(!blockers.contains(&ordinary));

    // "{2}, {T}, Sacrifice this token: You gain 3 life."
    let mut state = plains();
    let brute = token(&mut state, P0, "Gingerbrute Token");
    pool(&mut state, P0, &[], 2);
    activate(&mut state, brute, 1, &[]);
    settled(&mut state);
    assert!(named(&state, P0, "Gingerbrute Token").is_empty());
    assert_eq!(life(&state, P0), 23);
}

// ---- Public sessions -----------------------------------------------------------------

#[test]
fn c5a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let ramp = deck(&[
        ("Forest", 9),
        ("Swamp", 7),
        ("Sol Ring", 3),
        ("Delighted Halfling", 4),
        ("Fanatic of Rhonas", 4),
        ("Keen-Eyed Curator", 3),
        ("Carnage Interpreter", 3),
        ("Baloth Prime", 3),
        ("Alpha Deathclaw", 2),
        ("Ginger, Queen of Sweets", 2),
    ]);
    let midrange = deck(&[
        ("Forest", 7),
        ("Swamp", 7),
        ("Mind Stone", 2),
        ("Delighted Halfling", 3),
        ("Fanatic of Rhonas", 3),
        ("Keen-Eyed Curator", 4),
        ("Carnage Interpreter", 4),
        ("Baloth Prime", 4),
        ("Alpha Deathclaw", 3),
        ("Ginger, Queen of Sweets", 3),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    for (pair, first_seed) in [
        ([ramp.clone(), midrange.clone()], 0x4335_4100_u64),
        ([midrange, ramp], 0x4335_4110),
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
    for name in C5A_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
