//! Pauper field wave Pf1c: native rules and public-session tests for the
//! four cards that each need a new engine piece.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CostKind, Decision, OptionalCostChoice};
use mtg_kernel::eternal_pauper_pf1c_v1::hallow_shield_count;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const PF1C_CARDS: [&str; 4] = ["Boarding Action", "Kaervek's Torch", "Hallow", "Fling"];

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
        0x5046_0000_0000 + 0x3163,
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
fn pf1c_cards_are_full() {
    for name in PF1C_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn pf1c_printed_characteristics() {
    let boarding = def("Boarding Action");
    assert_eq!(boarding.types, &[CardType::Sorcery]);
    assert_eq!(boarding.mana_value, 2);
    assert_eq!(boarding.cost.generic, 1);
    assert_eq!(boarding.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert!(boarding.target_spec == TargetSpec::None);
    assert!(boarding.flashback.is_some(), "Beam me up");
    assert!(boarding.additional_cost.is_none());

    let token = def("Klingon Warrior Token");
    assert!(token.is_token);
    assert_eq!(token.capability, CardCapability::Full);
    assert_eq!(token.types, &[CardType::Creature]);
    assert_eq!(token.subtypes, &[Subtype::Klingon, Subtype::Warrior]);
    assert_eq!((token.power, token.toughness), (Some(2), Some(1)));
    assert_eq!(token.colors, &[ManaColor::R]);
    assert!(token.keywords.has(Keywords::HASTE));
    assert_eq!(Subtype::Klingon as u16, 740);
    assert!(Subtype::Klingon.is_creature_type());

    let torch = def("Kaervek's Torch");
    assert_eq!(torch.types, &[CardType::Sorcery]);
    assert_eq!(torch.mana_value, 1);
    assert_eq!(torch.cost.generic, 0);
    assert_eq!(torch.cost.x_count, 1);
    assert_eq!(torch.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert!(torch.target_spec == TargetSpec::AnyTarget);

    let hallow = def("Hallow");
    assert_eq!(hallow.types, &[CardType::Instant]);
    assert_eq!(hallow.mana_value, 1);
    assert_eq!(hallow.cost.generic, 0);
    assert_eq!(hallow.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert!(hallow.target_spec == TargetSpec::AnySpellOnStack);

    let fling = def("Fling");
    assert_eq!(fling.types, &[CardType::Instant]);
    assert_eq!(fling.mana_value, 2);
    assert_eq!(fling.cost.generic, 1);
    assert_eq!(fling.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert!(fling.target_spec == TargetSpec::AnyTarget);
    assert!(fling.additional_cost.is_some(), "sacrifice a creature");
    assert!(fling.flashback.is_none());
}

/// Casts `spell` announcing `x` and answering each target prompt from
/// `targets` in order.
fn cast_x(state: &mut GameState, spell: ObjectId, x: u16, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    let mut targets = targets.iter();
    for _ in 0..8 {
        match next(state) {
            Decision::ChooseEffectOption { option_count, .. } => {
                assert!(x < option_count, "X {x} of {option_count}");
                act(state, Action::ChooseEffectOption(x));
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = *targets.next().expect("a target answer");
                assert!(
                    legal_targets.contains(&target),
                    "{target:?} not in {legal_targets:?}"
                );
                act(state, Action::ChooseTarget(target));
            }
            _ => return,
        }
    }
    panic!("the cast never finished announcing");
}

/// `player`, who must hold priority, passes it.
fn pass_priority(state: &mut GameState, player: PlayerId) {
    match next(state) {
        Decision::CastSpellOrPass { player: holder, .. } if holder == player => {
            act(state, Action::Pass)
        }
        other => panic!("expected {player:?} to hold priority, got {other:?}"),
    }
}

fn sorted(mut objects: Vec<ObjectId>) -> Vec<ObjectId> {
    objects.sort();
    objects
}

fn payment_pick(state: &mut GameState, kind: CostKind, choice: ObjectId) {
    match next(state) {
        Decision::ChooseCostTargets {
            cost_kind,
            candidates,
            ..
        } => {
            assert_eq!(cost_kind, kind);
            assert!(
                candidates.contains(&choice),
                "{choice:?} absent from {candidates:?}"
            );
            act(state, Action::ChooseCostTarget(choice));
        }
        other => panic!("expected {kind:?}, got {other:?}"),
    }
}

#[test]
fn compound_escape_fling_sacrifices_then_exiles_the_same_card() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let fling = put(&mut state, P0, "Fling", Zone::Graveyard);
    let island = put(&mut state, P0, "Island", Zone::Graveyard);
    let swamp = put(&mut state, P0, "Swamp", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 1);
    token(&mut state, P0, "Thraben Inspector");
    assert!(
        !castable(&mut state, fling),
        "a sacrificed token cannot pay escape"
    );
    let creature = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    cast(&mut state, fling, &[Target::Player(P1)]);
    payment_pick(&mut state, CostKind::SacrificeCreatures, creature);
    payment_pick(&mut state, CostKind::ExileFromGraveyard, creature);
    let mut resumed: GameState =
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    let before = resumed.clone();
    assert!(engine::step(&mut resumed, Action::ChooseCostTarget(creature)).is_err());
    assert_eq!(resumed, before);
    for game in [&mut state, &mut resumed] {
        payment_pick(game, CostKind::ExileFromGraveyard, swamp);
        payment_pick(game, CostKind::ExileFromGraveyard, island);
        assert!(matches!(next(game), Decision::CastSpellOrPass { .. }));
        let receipts = &game.stack.last().unwrap().v4.paid_cost_refs;
        let paid: Vec<_> = receipts
            .iter()
            .map(|r| (r.object, r.zone, r.zone_change_count))
            .collect();
        assert_eq!(
            paid,
            [
                (creature, Zone::Graveyard, 1),
                (island, Zone::Exile, 1),
                (swamp, Zone::Exile, 1),
                (creature, Zone::Exile, 2)
            ]
        );
        settled(game);
        assert_eq!(life(game, P1), 18);
        assert_eq!(zone(game, fling), Zone::Graveyard);
    }
    assert_eq!(resumed, state);
}

fn retrace_emblem(state: &mut GameState) {
    let wrenn = put(state, P0, "Wrenn and Six", Zone::Hand);
    pool(state, P0, &[ManaColor::R, ManaColor::G], 0);
    cast(state, wrenn, &[]);
    settled(state);
    mtg_kernel::planeswalker_v1::change_loyalty(state, wrenn, 4);
    activate(state, wrenn, 2, &[]);
    settled(state);
}

#[test]
fn compound_escape_deprive_returns_a_land_and_exiles_graveyard_cards() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let grave =
        ["Island", "Swamp", "Forest"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let deprive = put(&mut state, P0, "Deprive", Zone::Graveyard);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, deprive, &[Target::Object(bolt)]);
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 2,
            ..
        }
    ));
    act(&mut state, Action::ChooseEffectOption(1));
    for id in grave {
        payment_pick(&mut state, CostKind::ExileFromGraveyard, id);
    }
    payment_pick(&mut state, CostKind::ReturnPermanentsToHand, island);
    settled(&mut state);
    assert_eq!(zone(&state, island), Zone::Hand);
    assert_eq!(zone(&state, deprive), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
}

fn bitter_life_payments(state: &GameState) -> usize {
    state
        .engine
        .event_history
        .iter()
        .filter(|event| {
            matches!(
                event,
                mtg_kernel::event::CommittedEvent::LifeLoss {
                    player: P0,
                    amount: 3
                }
            )
        })
        .count()
}

#[test]
fn bitter_triumph_escape_can_pay_life_without_a_discard_and_checks_available_life() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let source = put(&mut state, P0, "Bitter Triumph", Zone::Graveyard);
    let target = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let cards =
        ["Plains", "Island", "Forest"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
    pool(&mut state, P0, &[ManaColor::B], 1);
    state.players[0].life = 2;
    assert!(!castable(&mut state, source));
    state.players[0].life = 3;
    assert!(
        castable(&mut state, source),
        "paying all remaining life is legal"
    );
    state.players[0].life = 4;
    cast(&mut state, source, &[Target::Object(target)]);
    // With one candidate left the ordinary escape cost pays it automatically.
    for id in cards.into_iter().take(2) {
        payment_pick(&mut state, CostKind::ExileFromGraveyard, id);
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    let item = state.stack.last().unwrap();
    assert_eq!(
        item.v4.cast_method,
        Some(mtg_kernel::state::CastMethodV4::Escape)
    );
    assert!(item.discarded.is_empty());
    assert_eq!(
        item.v4
            .paid_cost_refs
            .iter()
            .map(|r| (r.object, r.zone))
            .collect::<Vec<_>>(),
        cards.map(|id| (id, Zone::Exile))
    );
    assert_eq!(life(&state, P0), 1);
    assert_eq!(bitter_life_payments(&state), 1);
    settled(&mut state);
    assert_eq!(zone(&state, target), Zone::Graveyard);
    assert_eq!(zone(&state, source), Zone::Graveyard);
}

#[test]
fn bitter_triumph_escape_discard_supplies_the_third_card_after_restore() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let source = put(&mut state, P0, "Bitter Triumph", Zone::Graveyard);
    let target = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let cards = ["Plains", "Island"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
    let discard = put(&mut state, P0, "Forest", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, source, &[Target::Object(target)]);
    payment_pick(&mut state, CostKind::DiscardCards, discard);
    let mut restored: GameState =
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    for game in [&mut state, &mut restored] {
        for id in [discard, cards[0], cards[1]] {
            payment_pick(game, CostKind::ExileFromGraveyard, id);
        }
        assert!(matches!(next(game), Decision::CastSpellOrPass { .. }));
        let item = game.stack.last().unwrap();
        assert_eq!(item.discarded, [discard]);
        assert_eq!(
            item.v4.cast_method,
            Some(mtg_kernel::state::CastMethodV4::Escape)
        );
        assert_eq!(
            item.v4
                .paid_cost_refs
                .iter()
                .filter(|r| r.object == discard)
                .map(|r| (r.zone, r.zone_change_count))
                .collect::<Vec<_>>(),
            [(Zone::Graveyard, 1), (Zone::Exile, 2)]
        );
        assert_eq!(bitter_life_payments(game), 0);
        settled(game);
        assert_eq!(zone(game, source), Zone::Graveyard);
    }
    assert_eq!(state, restored);
}

#[test]
fn bitter_triumph_escape_or_snapcaster_can_choose_the_life_payment_while_discard_is_payable() {
    use engine::CastMode;
    for flashback in [false, true] {
        let mut state = plains();
        let source = put(&mut state, P0, "Bitter Triumph", Zone::Graveyard);
        let spare = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        let target = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
        if flashback {
            let mage = put(&mut state, P0, "Snapcaster Mage", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::U], 1);
            cast(&mut state, mage, &[]);
            settle_choosing(&mut state, source);
        } else {
            put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        }
        let cards =
            ["Plains", "Island", "Forest"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
        pool(&mut state, P0, &[ManaColor::B], 1);
        cast(&mut state, source, &[Target::Object(target)]);
        assert!(
            matches!(next(&mut state), Decision::ChooseCastMode { options, .. }
            if options == [CastMode::Normal, CastMode::Alternative])
        );
        act(&mut state, Action::ChooseCastMode(CastMode::Alternative));
        if !flashback {
            // With one candidate left the ordinary escape cost pays it automatically.
            for id in cards.into_iter().take(2) {
                payment_pick(&mut state, CostKind::ExileFromGraveyard, id);
            }
        }
        settled(&mut state);
        assert_eq!(
            zone(&state, source),
            if flashback {
                Zone::Exile
            } else {
                Zone::Graveyard
            }
        );
        assert_eq!(zone(&state, spare), Zone::Hand);
        assert_eq!(bitter_life_payments(&state), 1);
    }
}

#[test]
fn bitter_triumph_retrace_keeps_its_land_cost_with_either_additional_payment() {
    use engine::CastMode;
    let mut base = plains();
    retrace_emblem(&mut base);
    let source = put(&mut base, P0, "Bitter Triumph", Zone::Graveyard);
    let target = put(&mut base, P1, "Thraben Inspector", Zone::Battlefield);
    let land = put(&mut base, P0, "Forest", Zone::Hand);
    pool(&mut base, P0, &[ManaColor::B], 1);
    base.players[0].life = 2;
    assert!(
        !castable(&mut base, source),
        "one card cannot pay two discards"
    );
    base.players[0].life = 4;
    assert!(
        castable(&mut base, source),
        "the single land can pay retrace while life pays the printed cost"
    );
    let spare = put(&mut base, P0, "Lightning Bolt", Zone::Hand);
    for choice in [CastMode::Alternative, CastMode::Normal] {
        let mut state = base.clone();
        cast(&mut state, source, &[Target::Object(target)]);
        assert!(
            matches!(next(&mut state), Decision::ChooseCastMode { options, .. }
            if options == [CastMode::Normal, CastMode::Alternative])
        );
        let mut state: GameState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        act(&mut state, Action::ChooseCastMode(choice));
        if choice == CastMode::Normal {
            assert!(matches!(
                next(&mut state),
                Decision::ChooseEffectOption {
                    option_count: 2,
                    ..
                }
            ));
            act(&mut state, Action::ChooseEffectOption(0));
            payment_pick(&mut state, CostKind::DiscardCards, spare);
            payment_pick(&mut state, CostKind::DiscardCards, land);
        }
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let item = state.stack.last().unwrap();
        assert_eq!(
            item.v4.cast_method,
            Some(mtg_kernel::state::CastMethodV4::Normal)
        );
        let paid_life = choice == CastMode::Alternative;
        assert_eq!(bitter_life_payments(&state), usize::from(paid_life));
        let expected = if paid_life {
            vec![land]
        } else {
            vec![spare, land]
        };
        assert_eq!(
            item.v4
                .paid_cost_refs
                .iter()
                .map(|r| r.object)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(item.discarded, if paid_life { vec![] } else { vec![spare] });
        assert_eq!(
            zone(&state, spare),
            if paid_life {
                Zone::Hand
            } else {
                Zone::Graveyard
            }
        );
        settled(&mut state);
        assert_eq!(zone(&state, source), Zone::Graveyard);
        assert_eq!(zone(&state, target), Zone::Graveyard);
    }
}

#[test]
fn graveyard_discard_escape_uses_the_discarded_card_and_records_both_incarnations() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let source = put(&mut state, P0, "Grab the Prize", Zone::Graveyard);
    let grave = ["Swamp", "Island"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
    pool(&mut state, P0, &[ManaColor::R], 1);
    put(&mut state, P0, "Fiery Temper", Zone::Hand);
    assert!(
        !castable(&mut state, source),
        "Madness sends the discard to exile"
    );
    let discard = put(&mut state, P0, "Mountain", Zone::Hand);
    cast(&mut state, source, &[]);
    payment_pick(&mut state, CostKind::DiscardCards, discard);
    for card in [grave[0], discard, grave[1]] {
        payment_pick(&mut state, CostKind::ExileFromGraveyard, card);
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    let item = state.stack.last().unwrap();
    assert_eq!(item.discarded, [discard]);
    assert_eq!(
        item.v4
            .paid_cost_refs
            .iter()
            .map(|r| (r.object, r.zone, r.zone_change_count))
            .collect::<Vec<_>>(),
        [
            (discard, Zone::Graveyard, 1),
            (grave[0], Zone::Exile, 1),
            (grave[1], Zone::Exile, 1),
            (discard, Zone::Exile, 2)
        ]
    );
    settled(&mut state);
    assert_eq!(zone(&state, source), Zone::Graveyard);
}

#[test]
fn graveyard_discard_retrace_requires_two_different_hand_cards_in_either_order() {
    for route_first in [false, true] {
        let mut state = plains();
        retrace_emblem(&mut state);
        let source = put(&mut state, P0, "Grab the Prize", Zone::Graveyard);
        let land = put(&mut state, P0, "Forest", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], 1);
        assert!(!castable(&mut state, source));
        let other = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        cast(&mut state, source, &[]);
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption {
                option_count: 2,
                ..
            }
        ));
        act(
            &mut state,
            Action::ChooseEffectOption(u16::from(route_first)),
        );
        let [first, second] = if route_first {
            [land, other]
        } else {
            [other, land]
        };
        payment_pick(&mut state, CostKind::DiscardCards, first);
        let snapshot = state.clone();
        assert!(engine::step(&mut state, Action::ChooseCostTarget(first)).is_err());
        assert_eq!(state, snapshot);
        payment_pick(&mut state, CostKind::DiscardCards, second);
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let item = state.stack.last().unwrap();
        assert_eq!(item.discarded, [other]);
        assert_eq!(
            item.v4
                .paid_cost_refs
                .iter()
                .map(|r| (r.object, r.zone))
                .collect::<Vec<_>>(),
            [(first, Zone::Graveyard), (second, Zone::Graveyard)]
        );
        settled(&mut state);
        assert_eq!(zone(&state, source), Zone::Graveyard);
        assert_eq!(life(&state, P1), 18);
    }
}

#[test]
fn graveyard_discard_escape_offers_each_payable_demand_answers_and_bone_shards_mode() {
    for (name, payment_name, color, target) in [
        ("Demand Answers", "Ichor Wellspring", ManaColor::R, false),
        ("Bone Shards", "Thraben Inspector", ManaColor::B, true),
    ] {
        for mode in [0, 1] {
            let mut state = plains();
            put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
            let source = put(&mut state, P0, name, Zone::Graveyard);
            let victim = target.then(|| put(&mut state, P1, "Kor Skyfisher", Zone::Battlefield));
            let payment = put(
                &mut state,
                P0,
                payment_name,
                if mode == 0 {
                    Zone::Battlefield
                } else {
                    Zone::Hand
                },
            );
            let grave = ["Swamp", "Island"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
            pool(&mut state, P0, &[color], u8::from(!target));
            cast_mode(
                &mut state,
                source,
                mode,
                &victim.into_iter().map(Target::Object).collect::<Vec<_>>(),
            );
            let kind = if mode == 1 {
                CostKind::DiscardCards
            } else if target {
                CostKind::SacrificeCreatures
            } else {
                CostKind::SacrificeArtifacts
            };
            payment_pick(&mut state, kind, payment);
            for id in [payment, grave[0], grave[1]] {
                payment_pick(&mut state, CostKind::ExileFromGraveyard, id);
            }
            next(&mut state);
            let item = state
                .stack
                .iter()
                .find(|item| item.source == source)
                .unwrap();
            assert_eq!(item.mode_chosen, mode);
            assert_eq!(
                item.discarded,
                if mode == 1 { vec![payment] } else { vec![] }
            );
            settled(&mut state);
            if let Some(victim) = victim {
                assert_eq!(zone(&state, victim), Zone::Graveyard);
            }
        }
    }
}

#[test]
fn graveyard_dynamic_escape_discard_x_can_supply_its_own_fuel() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let spell = put(&mut state, P0, "Sickening Dreams", Zone::Graveyard);
    let grave = put(&mut state, P0, "Swamp", Zone::Graveyard);
    let first = put(&mut state, P0, "Forest", Zone::Hand);
    let madness = put(&mut state, P0, "Fiery Temper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    assert!(!castable(&mut state, spell));
    let second = put(&mut state, P0, "Island", Zone::Hand);
    cast(&mut state, spell, &[]);
    // X = 2 or 3. Costs cannot exile the Madness card that is discarded to exile.
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 2,
            ..
        }
    ));
    act(&mut state, Action::ChooseEffectOption(0));
    payment_pick(&mut state, CostKind::DiscardCards, first);
    assert!(
        matches!(next(&mut state), Decision::ChooseCostTargets { candidates, .. } if !candidates.contains(&madness))
    );
    payment_pick(&mut state, CostKind::DiscardCards, second);
    for id in [first, grave, second] {
        payment_pick(&mut state, CostKind::ExileFromGraveyard, id);
    }
    next(&mut state);
    let item = state.stack.last().unwrap();
    assert_eq!(item.discarded, [first, second]);
    assert_eq!(item.v4.x_value, 2);
    settled(&mut state);
    assert_eq!((life(&state, P0), life(&state, P1)), (18, 18));
    assert_eq!(zone(&state, madness), Zone::Hand);
}

#[test]
fn graveyard_dynamic_retrace_reserves_its_land_for_discard_x() {
    for x in [0, 2] {
        let mut state = plains();
        retrace_emblem(&mut state);
        let spell = put(&mut state, P0, "Sickening Dreams", Zone::Graveyard);
        let land = put(&mut state, P0, "Swamp", Zone::Hand);
        let spares = ["Lightning Bolt", "Dark Ritual"].map(|n| put(&mut state, P0, n, Zone::Hand));
        pool(&mut state, P0, &[ManaColor::B], 1);
        cast(&mut state, spell, &[]);
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption {
                option_count: 3,
                ..
            }
        ));
        act(&mut state, Action::ChooseEffectOption(x));
        if x != 0 {
            assert!(matches!(
                next(&mut state),
                Decision::ChooseEffectOption {
                    option_count: 2,
                    ..
                }
            ));
            act(&mut state, Action::ChooseEffectOption(0));
            for id in spares {
                assert!(
                    matches!(next(&mut state), Decision::ChooseCostTargets { candidates, .. } if !candidates.contains(&land))
                );
                payment_pick(&mut state, CostKind::DiscardCards, id);
            }
        }
        payment_pick(&mut state, CostKind::DiscardCards, land);
        next(&mut state);
        assert_eq!(
            state.stack.last().unwrap().discarded,
            spares[..usize::from(x)]
        );
        settled(&mut state);
        assert_eq!(life(&state, P1), 20 - i32::from(x));
    }
}

#[test]
fn graveyard_dynamic_escape_escalate_sets_the_discard_count_from_the_modes() {
    // Drain, reveal + drain, then all three. Each extra mode supplies one
    // otherwise missing escape card. Mode 6 is the only payable mode in the last case.
    for (mode, discards) in [(3, 0), (4, 1), (6, 2)] {
        let mut state = plains();
        put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        let spell = put(&mut state, P0, "Collective Brutality", Zone::Graveyard);
        let creature = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
        let grave: Vec<_> = (discards..3)
            .map(|_| put(&mut state, P0, "Swamp", Zone::Graveyard))
            .collect();
        let hand: Vec<_> = (0..discards)
            .map(|_| put(&mut state, P0, "Forest", Zone::Hand))
            .collect();
        pool(&mut state, P0, &[ManaColor::B], 1);
        let mut targets = vec![Target::Player(P1)];
        if mode == 6 {
            targets.push(Target::Object(creature));
        }
        if discards > 0 {
            targets.push(Target::Player(P1));
        }
        cast_mode(&mut state, spell, mode, &targets);
        for &id in &hand {
            payment_pick(&mut state, CostKind::DiscardCards, id);
        }
        for &id in grave.iter().chain(&hand) {
            payment_pick(&mut state, CostKind::ExileFromGraveyard, id);
        }
        next(&mut state);
        assert_eq!(state.stack.last().unwrap().discarded, hand);
        settled(&mut state);
        assert_eq!((life(&state, P0), life(&state, P1)), (22, 18));
        if mode == 6 {
            assert_eq!(zone(&state, creature), Zone::Graveyard);
        }
    }
}

#[test]
fn graveyard_dynamic_retrace_escalate_offers_only_modes_payable_after_the_land() {
    let mut state = plains();
    retrace_emblem(&mut state);
    let spell = put(&mut state, P0, "Collective Brutality", Zone::Graveyard);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let land = put(&mut state, P0, "Swamp", Zone::Hand);
    let other = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, spell, &[]);
    assert!(
        matches!(next(&mut state), Decision::ChooseSpellMode { legal_modes, .. } if legal_modes == [0,1,2,3,4,5])
    );
    act(&mut state, Action::ChooseSpellMode(4));
    for _ in 0..2 {
        choose_target(&mut state, Target::Player(P1));
    }
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 2,
            ..
        }
    ));
    act(&mut state, Action::ChooseEffectOption(1));
    payment_pick(&mut state, CostKind::DiscardCards, land);
    payment_pick(&mut state, CostKind::DiscardCards, other);
    next(&mut state);
    assert_eq!(state.stack.last().unwrap().discarded, [other]);
    settled(&mut state);
    assert_eq!((life(&state, P0), life(&state, P1)), (22, 18));
}

#[test]
fn compound_retrace_fling_discards_before_sacrificing() {
    let mut state = plains();
    retrace_emblem(&mut state);
    let land = put(&mut state, P0, "Island", Zone::Hand);
    let fling = put(&mut state, P0, "Fling", Zone::Graveyard);
    let goyf = put(&mut state, P0, "Nethergoyf", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, fling, &[Target::Player(P1)]);
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 2,
            ..
        }
    ));
    act(&mut state, Action::ChooseEffectOption(1));
    payment_pick(&mut state, CostKind::DiscardCards, land);
    payment_pick(&mut state, CostKind::SacrificeCreatures, goyf);
    settled(&mut state);
    // Planeswalker and land are in the graveyard when Nethergoyf is sacrificed.
    assert_eq!(life(&state, P1), 18);
}

#[test]
fn compound_escape_crop_rotation_taps_then_sacrifices_its_land() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let source = put(&mut state, P0, "Crop Rotation", Zone::Graveyard);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let tower = put(&mut state, P0, "Urza's Tower", Zone::Library);
    let cards = ["Swamp", "Island"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
    cast(&mut state, source, &[]);
    payment_pick(&mut state, CostKind::SacrificeLands, forest);
    for card in [cards[0], cards[1], forest] {
        payment_pick(&mut state, CostKind::ExileFromGraveyard, card);
    }
    settle_choosing(&mut state, tower);
    assert_eq!(zone(&state, source), Zone::Graveyard);
    assert_eq!(zone(&state, forest), Zone::Exile);
    assert_eq!(zone(&state, tower), Zone::Battlefield);
}

#[test]
fn compound_retrace_deprive_returns_then_discards_the_same_land() {
    let mut state = plains();
    retrace_emblem(&mut state);
    let land = put(&mut state, P0, "Island", Zone::Battlefield);
    let deprive = put(&mut state, P0, "Deprive", Zone::Graveyard);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, deprive, &[Target::Object(bolt)]);
    payment_pick(&mut state, CostKind::ReturnPermanentsToHand, land);
    payment_pick(&mut state, CostKind::DiscardCards, land);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    let receipts = &state.stack.last().unwrap().v4.paid_cost_refs;
    assert_eq!(
        receipts
            .iter()
            .map(|r| (r.object, r.zone, r.zone_change_count))
            .collect::<Vec<_>>(),
        [(land, Zone::Hand, 1), (land, Zone::Graveyard, 2)]
    );
    settled(&mut state);
    assert_eq!(life(&state, P0), 20);
    assert_eq!(zone(&state, deprive), Zone::Graveyard);
}

#[test]
fn compound_escape_cost_order_changes_flings_last_known_power() {
    for (option, expected_damage) in [(0, 3), (1, 0)] {
        let mut state = plains();
        put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        let fling = put(&mut state, P0, "Fling", Zone::Graveyard);
        let cards = ["Forest", "Thraben Inspector", "Lightning Bolt"]
            .map(|name| put(&mut state, P0, name, Zone::Graveyard));
        let goyf = put(&mut state, P0, "Nethergoyf", Zone::Battlefield);
        pool(&mut state, P0, &[ManaColor::R], 1);
        cast(&mut state, fling, &[Target::Player(P1)]);
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption {
                option_count: 2,
                ..
            }
        ));
        act(&mut state, Action::ChooseEffectOption(option));
        if option == 0 {
            payment_pick(&mut state, CostKind::SacrificeCreatures, goyf);
        }
        for card in cards {
            payment_pick(&mut state, CostKind::ExileFromGraveyard, card);
        }
        if option == 1 {
            payment_pick(&mut state, CostKind::SacrificeCreatures, goyf);
        }
        settled(&mut state);
        assert_eq!(life(&state, P1), 20 - expected_damage);
    }
}

// ---- Fling ---------------------------------------------------------------------------

#[test]
fn fling_deals_the_sacrificed_creatures_power_as_it_last_existed() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    state.objects.get_mut(inspector).counters.plus1_plus1 = 2;
    let skyfisher = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let fling = put(&mut state, P0, "Fling", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert_eq!(
        power_toughness(&state, inspector),
        (3, 4),
        "pumped by counters"
    );
    cast(&mut state, fling, &[Target::Player(P1)]);
    match next(&mut state) {
        Decision::ChooseCostTargets {
            cost_kind,
            candidates,
            ..
        } => {
            assert_eq!(cost_kind, CostKind::SacrificeCreatures);
            assert_eq!(sorted(candidates), sorted(vec![inspector, skyfisher]));
            act(&mut state, Action::ChooseCostTarget(inspector));
        }
        other => panic!("expected the sacrifice, got {other:?}"),
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(
        zone(&state, inspector),
        Zone::Graveyard,
        "sacrificed as a cost"
    );
    settled(&mut state);
    assert_eq!(life(&state, P1), 17, "the counters' power counts");
    assert_eq!(life(&state, P0), 20);
    assert_eq!(zone(&state, skyfisher), Zone::Battlefield);
    assert_eq!(zone(&state, fling), Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn fling_kills_a_creature_with_the_sacrificed_power() {
    let mut state = plains();
    let skyfisher = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let target = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let fling = put(&mut state, P0, "Fling", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    // The only creature is sacrificed without a choice.
    cast(&mut state, fling, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(zone(&state, skyfisher), Zone::Graveyard);
    assert_eq!(zone(&state, target), Zone::Graveyard, "two damage to a 1/2");
}

#[test]
fn fling_with_a_zero_power_creature_deals_no_damage() {
    let mut state = plains();
    let lookout = put(&mut state, P0, "Mob Lookout", Zone::Battlefield);
    let fling = put(&mut state, P0, "Fling", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert_eq!(power_toughness(&state, lookout).0, 0);
    cast(&mut state, fling, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, lookout), Zone::Graveyard);
    assert_eq!(zone(&state, fling), Zone::Graveyard);
    assert_eq!(life(&state, P1), 20);
}

#[test]
fn fling_needs_a_creature_you_control_to_sacrifice() {
    let mut state = plains();
    put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let fling = put(&mut state, P0, "Fling", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(!castable(&mut state, fling), "no creature of yours");
}

#[test]
fn fling_at_the_creature_it_sacrifices_does_nothing() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    state.objects.get_mut(inspector).counters.plus1_plus1 = 2;
    let bystander = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let fling = put(&mut state, P0, "Fling", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    // Targets are chosen before costs are paid, so the creature about to be
    // sacrificed is a legal target; it is gone when Fling resolves.
    cast(&mut state, fling, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(zone(&state, fling), Zone::Graveyard);
    assert_eq!(zone(&state, bystander), Zone::Battlefield);
    assert_eq!(state.objects.get(bystander).damage, 0);
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));
}

#[test]
fn fling_whose_target_died_in_response_still_cost_its_creature() {
    let mut state = plains();
    let skyfisher = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let target = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let fling = put(&mut state, P0, "Fling", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, fling, &[Target::Object(target)]);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(target)]);
    settled(&mut state);
    assert_eq!(zone(&state, target), Zone::Graveyard, "the Bolt killed it");
    assert_eq!(
        zone(&state, skyfisher),
        Zone::Graveyard,
        "the cost was paid"
    );
    assert_eq!(zone(&state, fling), Zone::Graveyard);
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));
}

// ---- Boarding Action ---------------------------------------------------------------------

#[test]
fn boarding_action_creates_a_hasty_two_one_red_klingon_warrior() {
    let mut state = plains();
    let boarding = put(&mut state, P0, "Boarding Action", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, boarding, &[]);
    settled(&mut state);
    let tokens = named(&state, P0, "Klingon Warrior Token");
    assert_eq!(tokens.len(), 1);
    let token = tokens[0];
    assert!(state.objects.get(token).v4.is_token);
    assert_eq!(power_toughness(&state, token), (2, 1));
    assert!(keyword(&state, token, Keywords::HASTE));
    assert!(engine::has_effective_subtype(
        &state,
        token,
        Subtype::Klingon
    ));
    assert!(engine::has_effective_subtype(
        &state,
        token,
        Subtype::Warrior
    ));
    assert_eq!(zone(&state, boarding), Zone::Graveyard);
    // Haste: it attacks the turn it was made.
    attack_with(&mut state, vec![token], |_, _| None);
    assert_eq!(life(&state, P1), 18);
}

#[test]
fn boarding_action_beams_up_returning_a_creature_you_control_then_is_exiled() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let skyfisher = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let boarding = put(&mut state, P0, "Boarding Action", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 2);
    assert!(castable(&mut state, boarding));
    act(&mut state, Action::CastSpell(boarding));
    match next(&mut state) {
        Decision::ChooseCostTargets {
            cost_kind,
            candidates,
            ..
        } => {
            assert_eq!(cost_kind, CostKind::ReturnPermanentsToHand);
            assert_eq!(
                sorted(candidates),
                sorted(vec![inspector, skyfisher]),
                "only creatures you control"
            );
            act(&mut state, Action::ChooseCostTarget(skyfisher));
        }
        other => panic!("expected the creature to return, got {other:?}"),
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(zone(&state, skyfisher), Zone::Hand, "returned as a cost");
    settled(&mut state);
    assert_eq!(named(&state, P0, "Klingon Warrior Token").len(), 1);
    assert_eq!(zone(&state, boarding), Zone::Exile, "then exile this spell");
    assert_eq!(zone(&state, inspector), Zone::Battlefield);
    assert_eq!(zone(&state, theirs), Zone::Battlefield);
    assert_eq!(state.players[0].mana_pool, [0; 6], "{{2}}{{R}} paid");
}

#[test]
fn boarding_action_beam_me_up_needs_a_creature_you_control_and_two_r() {
    // No creature of yours to return.
    let mut state = plains();
    put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let boarding = put(&mut state, P0, "Boarding Action", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 2);
    assert!(!castable(&mut state, boarding), "no creature you control");

    // A creature, but only {1}{R}: its mana cost doesn't pay Beam me up.
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let boarding = put(&mut state, P0, "Boarding Action", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(
        !castable(&mut state, boarding),
        "Beam me up costs {{2}}{{R}}"
    );

    // With {2}{R} and one creature, that creature returns without a choice.
    pool(&mut state, P0, &[], 1);
    cast(&mut state, boarding, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Hand);
    assert_eq!(zone(&state, boarding), Zone::Exile);
    assert_eq!(named(&state, P0, "Klingon Warrior Token").len(), 1);
}

#[test]
fn boarding_action_beamed_up_and_countered_is_still_exiled() {
    let mut state = plains();
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let boarding = put(&mut state, P0, "Boarding Action", Zone::Graveyard);
    let counterspell = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, boarding, &[]);
    pass_priority(&mut state, P0);
    cast(&mut state, counterspell, &[Target::Object(boarding)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, boarding),
        Zone::Exile,
        "exiled though countered"
    );
    assert_eq!(zone(&state, counterspell), Zone::Graveyard);
    assert_eq!(zone(&state, inspector), Zone::Hand, "the cost stays paid");
    assert!(named(&state, P0, "Klingon Warrior Token").is_empty());
}

// ---- Kaervek's Torch -----------------------------------------------------------------------

#[test]
fn kaerveks_torch_deals_x_damage_to_any_target() {
    let mut state = plains();
    let torch = put(&mut state, P0, "Kaervek's Torch", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast_x(&mut state, torch, 3, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(zone(&state, torch), Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool, [0; 6]);

    // X = 2 at a creature.
    let mut state = plains();
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let torch = put(&mut state, P0, "Kaervek's Torch", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast_x(&mut state, torch, 2, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(life(&state, P1), 20);
}

#[test]
fn kaerveks_torch_with_x_zero_deals_no_damage() {
    let mut state = plains();
    let torch = put(&mut state, P0, "Kaervek's Torch", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast_x(&mut state, torch, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(zone(&state, torch), Zone::Graveyard);
}

#[test]
fn kaerveks_torch_does_nothing_when_its_target_is_gone() {
    let mut state = plains();
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let torch = put(&mut state, P0, "Kaervek's Torch", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast_x(&mut state, torch, 2, &[Target::Object(inspector)]);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(zone(&state, torch), Zone::Graveyard);
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));
}

#[test]
fn spells_that_target_kaerveks_torch_cost_two_more() {
    let mut state = plains();
    let torch = put(&mut state, P0, "Kaervek's Torch", Zone::Hand);
    let counterspell = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_x(&mut state, torch, 1, &[Target::Player(P1)]);
    pass_priority(&mut state, P0);
    // {U}{U} alone can't pay for the only target, the Torch.
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    assert!(
        !castable(&mut state, counterspell),
        "the Torch's tax is unpaid"
    );
    pool(&mut state, P1, &[], 2);
    cast(&mut state, counterspell, &[Target::Object(torch)]);
    settled(&mut state);
    assert_eq!(zone(&state, torch), Zone::Graveyard, "countered");
    assert_eq!(zone(&state, counterspell), Zone::Graveyard);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(
        state.players[1].mana_pool, [0; 6],
        "{{U}}{{U}} and {{2}} paid"
    );
}

#[test]
fn spells_that_dont_target_kaerveks_torch_pay_no_tax() {
    let mut state = plains();
    let torch = put(&mut state, P0, "Kaervek's Torch", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_x(&mut state, torch, 1, &[Target::Player(P1)]);
    pass_priority(&mut state, P0);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 17);
    assert_eq!(life(&state, P1), 19);
    assert_eq!(state.players[1].mana_pool, [0; 6]);
}

#[test]
fn a_torch_is_offered_as_a_target_only_when_the_tax_is_payable() {
    let mut state = plains();
    let torch = put(&mut state, P0, "Kaervek's Torch", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let hallow = put(&mut state, P1, "Hallow", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast_x(&mut state, torch, 2, &[Target::Player(P1)]);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    pass_priority(&mut state, P0);

    // With {W} alone Hallow may target the Bolt but not the Torch.
    pool(&mut state, P1, &[ManaColor::W], 0);
    let offered = cast_targets(&state, hallow);
    assert!(offered.contains(&Target::Object(bolt)));
    assert!(!offered.contains(&Target::Object(torch)), "{offered:?}");

    // With {2}{W} it may target the Torch too, and pays the tax.
    pool(&mut state, P1, &[], 2);
    let offered = cast_targets(&state, hallow);
    assert!(offered.contains(&Target::Object(torch)), "{offered:?}");
    cast(&mut state, hallow, &[Target::Object(torch)]);
    settled(&mut state);
    // The Bolt deals 3; the Torch's 2 is prevented and gains P1 2 life.
    assert_eq!(life(&state, P1), 19);
    assert_eq!(state.players[1].mana_pool, [0; 6]);
}

#[test]
fn a_spell_whose_only_target_is_an_unpayable_torch_is_not_offered() {
    let mut state = plains();
    let torch = put(&mut state, P0, "Kaervek's Torch", Zone::Hand);
    let hallow = put(&mut state, P1, "Hallow", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_x(&mut state, torch, 1, &[Target::Player(P1)]);
    pass_priority(&mut state, P0);
    pool(&mut state, P1, &[ManaColor::W], 1);
    assert!(
        !castable(&mut state, hallow),
        "{{1}}{{W}} can't pay {{2}}{{W}}"
    );
    pool(&mut state, P1, &[], 1);
    assert!(castable(&mut state, hallow));
}

// ---- Hallow --------------------------------------------------------------------------------

#[test]
fn hallow_prevents_its_target_spells_damage_and_gains_that_much_life() {
    let mut state = plains();
    let hallow = put(&mut state, P0, "Hallow", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    cast(&mut state, hallow, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 23, "3 prevented, 3 gained");
    assert_eq!(life(&state, P1), 20);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, hallow), Zone::Graveyard);
}

#[test]
fn hallow_prevents_damage_to_every_recipient_and_gains_the_total() {
    let mut state = plains();
    let mine = [
        put(&mut state, P0, "Thraben Inspector", Zone::Battlefield),
        put(&mut state, P0, "Thraben Inspector", Zone::Battlefield),
    ];
    let theirs = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let hallow = put(&mut state, P0, "Hallow", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    state.priority_player = P1;
    let cannonade = put(&mut state, P1, "Fiery Cannonade", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 2);
    cast(&mut state, cannonade, &[]);
    pass_priority(&mut state, P1);
    cast(&mut state, hallow, &[Target::Object(cannonade)]);
    settled(&mut state);
    for creature in [mine[0], mine[1], theirs] {
        assert_eq!(zone(&state, creature), Zone::Battlefield);
        assert_eq!(state.objects.get(creature).damage, 0);
    }
    assert_eq!(
        life(&state, P0),
        26,
        "2 prevented to each of three creatures"
    );
    assert_eq!(life(&state, P1), 20);
}

#[test]
fn hallow_leaves_other_spells_damage_alone() {
    let mut state = plains();
    let hallow = put(&mut state, P0, "Hallow", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    state.priority_player = P1;
    let first = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let second = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R, ManaColor::R], 0);
    cast(&mut state, first, &[Target::Player(P0)]);
    cast(&mut state, second, &[Target::Player(P0)]);
    pass_priority(&mut state, P1);
    cast(&mut state, hallow, &[Target::Object(first)]);
    settled(&mut state);
    // The second Bolt deals 3; the first is prevented and gains 3.
    assert_eq!(life(&state, P0), 20);
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
}

#[test]
fn hallow_does_nothing_when_its_target_spell_is_gone() {
    let mut state = plains();
    let hallow = put(&mut state, P0, "Hallow", Zone::Hand);
    let counterspell = put(&mut state, P0, "Counterspell", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::U, ManaColor::U],
        0,
    );
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    cast(&mut state, hallow, &[Target::Object(bolt)]);
    cast(&mut state, counterspell, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "countered");
    assert_eq!(zone(&state, hallow), Zone::Graveyard);
    assert_eq!(
        hallow_shield_count(&state),
        0,
        "no shield without its target"
    );
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn hallow_follows_a_permanent_spell_onto_the_battlefield() {
    let mut state = plains();
    let hallow = put(&mut state, P0, "Hallow", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    state.active_player = P1;
    state.priority_player = P1;
    let other = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, fanatic, &[]);
    pass_priority(&mut state, P1);
    cast(&mut state, hallow, &[Target::Object(fanatic)]);
    settled(&mut state);
    assert_eq!(zone(&state, fanatic), Zone::Battlefield);
    assert_eq!(hallow_shield_count(&state), 1);
    // CR 400.7c: the prevention applies to the permanent the spell became,
    // even as its sacrificed self deals the damage, and Hallow's controller
    // gains the life.
    activate(&mut state, fanatic, 0, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(zone(&state, fanatic), Zone::Graveyard);
    assert_eq!(life(&state, P0), 21, "1 prevented, 1 gained");
    // Another Mogg Fanatic was never that spell: its damage is dealt.
    activate(&mut state, other, 0, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(zone(&state, other), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn hallow_does_not_follow_the_permanent_through_a_later_zone_change() {
    let mut state = plains();
    let hallow = put(&mut state, P0, "Hallow", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    state.active_player = P1;
    state.priority_player = P1;
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, fanatic, &[]);
    pass_priority(&mut state, P1);
    cast(&mut state, hallow, &[Target::Object(fanatic)]);
    settled(&mut state);
    let first_incarnation = state.objects.get(fanatic).zone_change_count;

    // Blinking the permanent creates a later incarnation. Only the
    // original resolving spell-to-permanent transition carries Hallow.
    let ephemerate = put(&mut state, P1, "Ephemerate", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, ephemerate, &[Target::Object(fanatic)]);
    settled(&mut state);
    assert_eq!(zone(&state, fanatic), Zone::Battlefield);
    assert!(state.objects.get(fanatic).zone_change_count > first_incarnation);
    activate(&mut state, fanatic, 0, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(zone(&state, fanatic), Zone::Graveyard);
    assert_eq!(
        life(&state, P0),
        19,
        "the later incarnation is not shielded"
    );
}

// ---- Granted flashback and a mandatory additional cost (W1 review) ----------------------

/// Deprive (wave Pf1b) given flashback by Snapcaster Mage still pays its
/// mandatory additional cost: the cast stages the land to return although
/// the card has no printed flashback.
#[test]
fn deprive_cast_by_a_granted_flashback_still_returns_a_land() {
    let mut state = plains();
    let mountain = put(&mut state, P0, "Mountain", Zone::Battlefield);
    let deprive = put(&mut state, P0, "Deprive", Zone::Graveyard);
    let snapcaster = put(&mut state, P0, "Snapcaster Mage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, snapcaster, &[]);
    settle_choosing(&mut state, deprive);
    assert_eq!(zone(&state, snapcaster), Zone::Battlefield);
    assert_eq!(zone(&state, mountain), Zone::Battlefield);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, deprive, &[Target::Object(bolt)]);
    // The only land is returned without a choice.
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(zone(&state, mountain), Zone::Hand, "returned as a cost");
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "countered");
    assert_eq!(zone(&state, deprive), Zone::Exile, "flashback exiles it");
    assert_eq!(life(&state, P0), 20);
}

// ---- Second review round ------------------------------------------------------------------

/// P0 plays a Gemstone Mine and taps it twice, leaving it untapped with its
/// last mining counter and P0's mana pool empty again.
fn mine_with_last_counter(state: &mut GameState) -> ObjectId {
    let mine = put(state, P0, "Gemstone Mine", Zone::Hand);
    next(state);
    act(state, Action::PlayLand(mine));
    for _ in 0..2 {
        state.objects.get_mut(mine).tapped = false;
        next(state);
        act(state, Action::ActivateManaAbilityChoice(mine, ManaColor::W));
    }
    state.objects.get_mut(mine).tapped = false;
    state.players[P0.index()].mana_pool = [0; 6];
    assert_eq!(zone(state, mine), Zone::Battlefield);
    mine
}

/// CR 702.73a: a changeling is every creature type, Klingon included, so an
/// Engineered Plague naming Klingon shrinks Masked Vandal.
#[test]
fn engineered_plague_naming_klingon_shrinks_a_changeling() {
    let options = mtg_kernel::eternal_premodern_p3b_v1::statics::plague_type_options();
    let klingon = options
        .iter()
        .position(|subtype| *subtype == Subtype::Klingon)
        .expect("Klingon can be named");
    let mut state = plains();
    let vandal = put(&mut state, P1, "Masked Vandal", Zone::Battlefield);
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    let plague = put(&mut state, P0, "Engineered Plague", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    assert_eq!(power_toughness(&state, vandal), (1, 3));
    assert!(engine::has_effective_subtype(
        &state,
        vandal,
        Subtype::Klingon
    ));
    cast(&mut state, plague, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => {
            Action::ChooseEffectOption(u16::try_from(klingon).unwrap())
        }
        other => first_answer(other),
    });
    assert_eq!(power_toughness(&state, vandal), (0, 2), "a Klingon");
    assert_eq!(power_toughness(&state, inspector), (1, 2), "not a Klingon");
}

/// Deprive with one floating {U} and a Gemstone Mine holding its last
/// counter as the only land: paying {U}{U} would sacrifice the Mine, so it
/// can't also be returned, and Deprive isn't offered.
#[test]
fn deprive_is_not_offered_when_its_payment_would_sacrifice_the_only_land() {
    let mut state = plains();
    let mine = mine_with_last_counter(&mut state);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(
        !castable(&mut state, deprive),
        "returning the Mine leaves {{U}}"
    );
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Stack);
}

/// With an Island as well, Deprive is offered and payable: with the floating
/// {U} the Mine may be returned (the Island pays), and without it only the
/// Island may be returned (the Mine pays and is sacrificed).
#[test]
fn deprive_with_a_second_land_returns_a_land_the_payment_leaves() {
    // Floating {U}: the Mine is a legal return, the Island pays.
    let mut state = plains();
    let mine = mine_with_last_counter(&mut state);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, deprive, &[Target::Object(bolt)]);
    match next(&mut state) {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(sorted(candidates), sorted(vec![mine, island]));
            act(&mut state, Action::ChooseCostTarget(mine));
        }
        other => panic!("expected the land to return, got {other:?}"),
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(zone(&state, mine), Zone::Hand, "returned, not sacrificed");
    assert!(state.objects.get(island).tapped, "the Island paid");
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "countered");
    assert_eq!(life(&state, P0), 20);

    // No floating mana: both lands pay, so only the Island can be returned.
    let mut state = plains();
    let mine = mine_with_last_counter(&mut state);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
    cast(&mut state, deprive, &[Target::Object(bolt)]);
    // The Island is the only return left, chosen without asking.
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(zone(&state, island), Zone::Hand);
    assert_eq!(zone(&state, mine), Zone::Graveyard, "its last counter paid");
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "countered");
    assert_eq!(life(&state, P0), 20);
}

fn depletion_land_with_last_counter(state: &mut GameState, name: &str) -> ObjectId {
    let land = put(state, P0, name, Zone::Hand);
    play_land(state, land);
    // Fixture untaps and the pool reset stand for intervening turns. The
    // real mana ability removes the first of the two depletion counters.
    state.objects.get_mut(land).tapped = false;
    next(state);
    act(state, Action::ActivateManaAbility(land));
    state.objects.get_mut(land).tapped = false;
    state.players[0].mana_pool = [0; 6];
    assert_eq!(
        mtg_kernel::eternal_pauper_e8b_v1::depletion_counters(state, land),
        1
    );
    land
}

fn woodlot_with_last_counter(state: &mut GameState) -> ObjectId {
    depletion_land_with_last_counter(state, "Hickory Woodlot")
}

#[test]
fn compound_escape_deprive_can_return_a_last_counter_needle_while_paying_torch_tax() {
    let mut state = plains();
    let needle = depletion_land_with_last_counter(&mut state, "Sandstone Needle");
    let lands = ["Island", "Island", "Mountain", "Mountain"]
        .map(|name| put(&mut state, P0, name, Zone::Battlefield));
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let grave =
        ["Island", "Swamp", "Forest"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
    let deprive = put(&mut state, P0, "Deprive", Zone::Graveyard);
    state.active_player = P1;
    state.priority_player = P1;
    let torch = put(&mut state, P1, "Kaervek's Torch", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 1);
    cast_x(&mut state, torch, 1, &[Target::Player(P0)]);
    pass_priority(&mut state, P1);
    cast(&mut state, deprive, &[Target::Object(torch)]);
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 2,
            ..
        }
    ));
    act(&mut state, Action::ChooseEffectOption(0));
    payment_pick(&mut state, CostKind::ReturnPermanentsToHand, needle);
    for card in grave {
        payment_pick(&mut state, CostKind::ExileFromGraveyard, card);
    }
    settled(&mut state);
    assert_eq!(zone(&state, needle), Zone::Hand);
    assert!(lands.iter().all(|&id| state.objects.get(id).tapped));
    assert_eq!(zone(&state, torch), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn compound_escape_fling_needs_a_creature_even_with_forty_graveyard_cards() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    for _ in 0..40 {
        put(&mut state, P0, "Forest", Zone::Graveyard);
    }
    let fling = put(&mut state, P0, "Fling", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(!castable(&mut state, fling));
    put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    assert!(castable(&mut state, fling));
}

#[test]
fn deprive_refuses_a_last_counter_woodlot_under_sphere_of_resistance() {
    let mut state = plains();
    let woodlot = woodlot_with_last_counter(&mut state);
    put(&mut state, P0, "Sphere of Resistance", Zone::Battlefield);
    // The opponent also pays Sphere's tax on the Bolt.
    pool(&mut state, P1, &[], 1);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    assert!(
        !castable(&mut state, deprive),
        "the {{1}}{{U}}{{U}} payment loses its return land"
    );
    assert_eq!(zone(&state, woodlot), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Stack);
    assert_eq!(
        mana(&state, P0, ManaColor::U),
        2,
        "the quote did not pay mana"
    );
    assert_eq!(
        mtg_kernel::eternal_pauper_e8b_v1::depletion_counters(&state, woodlot),
        1
    );
    act(&mut state, Action::ActivateManaAbility(woodlot));
    assert_eq!(zone(&state, woodlot), Zone::Graveyard);
    assert!(!castable(&mut state, deprive), "no land remains to return");
}

#[test]
fn deprive_keeps_healthy_return_choices_when_woodlot_pays_spheres_tax() {
    let mut state = plains();
    let woodlot = woodlot_with_last_counter(&mut state);
    let mountain = put(&mut state, P0, "Mountain", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    // These can be returned, but cannot replace the Woodlot's mana.
    state.objects.get_mut(mountain).tapped = true;
    state.objects.get_mut(forest).tapped = true;
    put(&mut state, P0, "Sphere of Resistance", Zone::Battlefield);
    pool(&mut state, P1, &[], 1);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    // Depletion lands use an explicit mana action. Float the Woodlot's two
    // green before announcing Deprive; the other lands remain returnable.
    assert!(!castable(&mut state, deprive));
    act(&mut state, Action::ActivateManaAbility(woodlot));
    assert_eq!(zone(&state, woodlot), Zone::Graveyard);
    assert_eq!(mana(&state, P0, ManaColor::G), 2);
    cast(&mut state, deprive, &[Target::Object(bolt)]);
    match next(&mut state) {
        Decision::ChooseCostTargets {
            cost_kind,
            candidates,
            ..
        } => {
            assert_eq!(cost_kind, CostKind::ReturnPermanentsToHand);
            assert_eq!(sorted(candidates), sorted(vec![mountain, forest]));
            act(&mut state, Action::ChooseCostTarget(mountain));
        }
        other => panic!("expected the surviving return lands, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(
        zone(&state, woodlot),
        Zone::Graveyard,
        "paid the generic tax"
    );
    assert_eq!(zone(&state, mountain), Zone::Hand);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, deprive), Zone::Graveyard);
    assert_eq!(
        mana(&state, P0, ManaColor::G),
        1,
        "the unused Woodlot mana remains"
    );
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn deprive_can_save_a_last_counter_woodlot_using_another_mana_source() {
    let mut state = plains();
    let woodlot = woodlot_with_last_counter(&mut state);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    put(&mut state, P0, "Sphere of Resistance", Zone::Battlefield);
    pool(&mut state, P1, &[], 1);
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    let deprive = put(&mut state, P0, "Deprive", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, deprive, &[Target::Object(bolt)]);
    match next(&mut state) {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(sorted(candidates), sorted(vec![woodlot, island]));
            act(&mut state, Action::ChooseCostTarget(woodlot));
        }
        other => panic!("expected both return lands, got {other:?}"),
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(
        zone(&state, woodlot),
        Zone::Hand,
        "saved by the other mana source"
    );
    assert!(
        state.objects.get(island).tapped,
        "the Island paid Sphere's tax"
    );
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, deprive), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
}

/// Fling reads the creature's power as it last existed, after its mana
/// abilities ran: Gemstone Mine's last counter puts it into a graveyard
/// before Terravore is sacrificed, so Terravore was 2/2.
#[test]
fn fling_reads_power_after_its_mana_abilities_changed_the_board() {
    let mut state = plains();
    let mine = mine_with_last_counter(&mut state);
    put(&mut state, P1, "Plains", Zone::Graveyard);
    let terravore = put(&mut state, P0, "Terravore", Zone::Battlefield);
    let fling = put(&mut state, P0, "Fling", Zone::Hand);
    assert_eq!(power_toughness(&state, terravore), (1, 1));
    pool(&mut state, P0, &[], 1);
    cast(&mut state, fling, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, mine),
        Zone::Graveyard,
        "its last counter paid {{R}}"
    );
    assert_eq!(zone(&state, terravore), Zone::Graveyard);
    assert_eq!(life(&state, P1), 18, "Terravore was 2/2 when sacrificed");
}

/// A modal spell keeps its untargeted mode while a Torch is on the stack:
/// with exactly {3}{U}{U}, Mystic Confluence can't pay to counter the Torch
/// but can draw three cards.
#[test]
fn a_modal_spell_keeps_its_untargeted_mode_while_a_torch_is_on_the_stack() {
    let mut state = plains();
    let torch = put(&mut state, P0, "Kaervek's Torch", Zone::Hand);
    let mystic = put(&mut state, P1, "Mystic Confluence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_x(&mut state, torch, 1, &[Target::Player(P1)]);
    pass_priority(&mut state, P0);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 3);
    assert!(
        castable(&mut state, mystic),
        "drawing three costs {{3}}{{U}}{{U}}"
    );
    act(&mut state, Action::CastSpell(mystic));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert_eq!(legal_modes, vec![9], "no counter mode pays the tax");
            Action::ChooseSpellMode(9)
        }
        other => first_answer(other),
    });
    assert_eq!(state.players[1].hand.len(), 3, "drew three");
    assert_eq!(zone(&state, torch), Zone::Graveyard, "not countered");
    assert_eq!(life(&state, P1), 19);
    assert_eq!(state.players[1].mana_pool, [0; 6]);
}

#[test]
fn a_torch_counter_mode_requires_its_tax_even_when_another_mode_is_payable() {
    let mut state = plains();
    let torch = put(&mut state, P0, "Kaervek's Torch", Zone::Hand);
    let mystic = put(&mut state, P1, "Mystic Confluence", Zone::Hand);
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_x(&mut state, torch, 1, &[Target::Player(P1)]);
    pass_priority(&mut state, P0);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 3);
    let mut paid = state.clone();

    assert!(castable(&mut state, mystic));
    act(&mut state, Action::CastSpell(mystic));
    let Decision::ChooseSpellMode { legal_modes, .. } = next(&mut state) else {
        panic!("expected bounce and draw choices")
    };
    assert_eq!(legal_modes, vec![6, 7, 8, 9]);
    assert!(engine::step(&mut state, Action::ChooseSpellMode(2)).is_err());
    act(&mut state, Action::ChooseSpellMode(9));
    settled(&mut state);
    assert_eq!(state.players[P1.index()].hand.len(), 3);
    assert_eq!(life(&state, P1), 19);

    // Two counter modes targeting the same Torch pay a single {2} increase.
    pool(&mut paid, P1, &[], 2);
    cast_mode(
        &mut paid,
        mystic,
        2,
        &[Target::Object(torch), Target::Object(torch)],
    );
    settled(&mut paid);
    assert_eq!(paid.players[P1.index()].mana_pool, [0; 6]);
    assert_eq!(paid.players[P1.index()].hand.len(), 1);
    assert_eq!(life(&paid, P1), 20);
}

/// P1 casts a Torch at P0 (X = 1) and passes; P0 holds Counterspell with two
/// untapped Mountains. Returns the Torch and the Counterspell.
fn torch_against_counterspell(omniscience: bool) -> (GameState, ObjectId, ObjectId) {
    let mut state = plains();
    if omniscience {
        put(&mut state, P0, "Omniscience", Zone::Battlefield);
    }
    put(&mut state, P0, "Mountain", Zone::Battlefield);
    put(&mut state, P0, "Mountain", Zone::Battlefield);
    let counterspell = put(&mut state, P0, "Counterspell", Zone::Hand);
    state.active_player = P1;
    state.priority_player = P1;
    let torch = put(&mut state, P1, "Kaervek's Torch", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 1);
    cast_x(&mut state, torch, 1, &[Target::Player(P0)]);
    pass_priority(&mut state, P1);
    (state, torch, counterspell)
}

/// Under Omniscience, Counterspell cast without paying its mana cost still
/// pays the Torch's {2}, which two Mountains cover.
#[test]
fn a_free_cast_under_omniscience_pays_only_the_torchs_tax() {
    let (mut state, torch, counterspell) = torch_against_counterspell(true);
    assert!(
        castable(&mut state, counterspell),
        "free, plus the {{2}} tax"
    );
    cast(&mut state, counterspell, &[Target::Object(torch)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, torch), Zone::Graveyard, "countered");
    assert_eq!(zone(&state, counterspell), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
    let mountains = named(&state, P0, "Mountain");
    assert_eq!(mountains.len(), 2);
    assert!(mountains
        .iter()
        .all(|&mountain| state.objects.get(mountain).tapped));
}

/// Without Omniscience, {U}{U} plus the tax is out of reach of two
/// Mountains: Counterspell, whose only target is the Torch, isn't offered.
#[test]
fn without_omniscience_counterspell_cant_target_the_torch_with_two_mountains() {
    let (mut state, _, counterspell) = torch_against_counterspell(false);
    assert!(!castable(&mut state, counterspell));
}

// ---- Public sessions -----------------------------------------------------------------------

#[derive(Default)]
struct Pf1cEvents {
    casts: [usize; PF1C_CARDS.len()],
}

impl Pf1cEvents {
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
        if let Some(index) = PF1C_CARDS.iter().position(|&card| card == name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn pf1c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let boros = deck(&[
        ("Mountain", 10),
        ("Plains", 7),
        ("Fling", 5),
        ("Kaervek's Torch", 5),
        ("Boarding Action", 5),
        ("Hallow", 4),
        ("Mogg Fanatic", 4),
        ("Goblin Bushwhacker", 2),
        ("Lightning Bolt", 4),
    ]);
    let white = deck(&[
        ("Plains", 17),
        ("Hallow", 6),
        ("Thraben Inspector", 5),
        ("Kor Skyfisher", 4),
        ("Swords to Plowshares", 4),
        ("Prismatic Strands", 4),
    ]);
    let mut events = Pf1cEvents::default();
    // Twelve games plus their twelve replays, with both policies and seats.
    for pair in [[boros.clone(), white.clone()], [white, boros]] {
        for seed in 0x9F1C_u64..0x9F1F {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played = play_observed(pair.clone(), None, seed, policy, &mut |d, a| {
                    events.observe(d, a)
                });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, casts) in PF1C_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} never cast through a public session");
    }
}

#[test]
fn monstrous_emergence_escape_pays_three_exiles_and_uses_its_chosen_creature() {
    for reveal in [false, true] {
        let mut state = plains();
        put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        let grave =
            ["Forest", "Swamp", "Island"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
        let target = put(&mut state, P1, "Mulldrifter", Zone::Battlefield);
        let spell = put(&mut state, P0, "Monstrous Emergence", Zone::Graveyard);
        pool(&mut state, P0, &[ManaColor::G], 1);
        assert!(
            !castable(&mut state, spell),
            "a creature must be chosen or revealed"
        );
        let origin = if reveal {
            Zone::Hand
        } else {
            Zone::Battlefield
        };
        let creature = put(&mut state, P0, "Kor Skyfisher", origin);
        cast(&mut state, spell, &[Target::Object(target)]);
        for object in grave {
            if !matches!(
                next(&mut state),
                Decision::ChooseCostTargets {
                    cost_kind: CostKind::ExileFromGraveyard,
                    ..
                }
            ) {
                break; // The remaining exiles may already be forced.
            }
            payment_pick(&mut state, CostKind::ExileFromGraveyard, object);
        }
        assert!(grave.iter().all(|id| zone(&state, *id) == Zone::Exile));
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let refs = &state.stack.last().unwrap().v4.paid_cost_refs;
        assert_eq!(refs.len(), 4);
        assert!(refs[..3]
            .iter()
            .all(|r| r.zone == Zone::Exile && r.power_lki.is_none()));
        let chosen = refs[3];
        assert_eq!(
            (
                chosen.object,
                chosen.zone,
                chosen.power_lki,
                chosen.visible_to_mask
            ),
            (creature, origin, Some(2), 3)
        );
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for game in [&mut state, &mut restored] {
            settled(game);
            assert_eq!(zone(game, target), Zone::Graveyard);
            assert_eq!(zone(game, spell), Zone::Graveyard);
            assert_eq!(zone(game, creature), origin);
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn monstrous_emergence_retrace_reveals_the_creature_land_it_discards() {
    let mut state = plains();
    retrace_emblem(&mut state);
    let spell = put(&mut state, P0, "Monstrous Emergence", Zone::Graveyard);
    let target = put(&mut state, P1, "Mulldrifter", Zone::Battlefield);
    let land = put(&mut state, P0, "Dryad Arbor", Zone::Hand);
    let generation = state.objects.get(land).zone_change_count;
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, spell, &[Target::Object(target)]);
    if matches!(next(&mut state), Decision::ChooseCostTargets { .. }) {
        payment_pick(&mut state, CostKind::DiscardCards, land);
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    let refs = &state.stack.last().unwrap().v4.paid_cost_refs;
    assert_eq!(refs.len(), 2);
    assert_eq!(
        (refs[0].object, refs[0].zone, refs[0].zone_change_count),
        (land, Zone::Graveyard, generation + 1)
    );
    assert_eq!(
        (
            refs[1].object,
            refs[1].zone,
            refs[1].zone_change_count,
            refs[1].power_lki,
            refs[1].visible_to_mask
        ),
        (land, Zone::Hand, generation, Some(1), 3)
    );
    settled(&mut state);
    assert_eq!(state.objects.get(target).damage, 1);
    assert_eq!(zone(&state, land), Zone::Graveyard);
    assert_eq!(zone(&state, spell), Zone::Graveyard);
}
