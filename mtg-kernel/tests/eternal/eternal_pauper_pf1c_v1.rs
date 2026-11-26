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
    game_for(library, P0)
}

fn game_for(library: &[&str], player: PlayerId) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5046_0000_0000 + 0x3163,
        player,
    );
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
    assert!(
        boarding.flashback.is_none(),
        "Beam me up is its own casting ability"
    );
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

// Follow the public cast menu. Forced costs may be paid without a prompt.
fn finish_optional_escape(
    state: &mut GameState,
    spell: ObjectId,
    target: Option<ObjectId>,
    sacrifice: Option<ObjectId>,
    fuel: &[ObjectId],
    route_first: bool,
) {
    assert!(castable(state, spell));
    act(state, Action::CastSpell(spell));
    for _ in 0..12 {
        let decision = next(state);
        let Some(pending) = state.engine.pending_cast.as_ref() else {
            return;
        };
        let action = match decision {
            Decision::ChooseEffectOption { .. }
                if pending.optional_additional_cost_paid.is_none() =>
            {
                Action::ChooseEffectOption(u16::from(sacrifice.is_some()))
            }
            Decision::ChooseEffectOption { .. } => {
                Action::ChooseEffectOption(u16::from(route_first))
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                let asked = Target::Object(target.expect("a spell to counter"));
                assert!(legal_targets.contains(&asked));
                Action::ChooseTarget(asked)
            }
            Decision::ChooseCostTargets {
                cost_kind: CostKind::ExileFromGraveyard,
                candidates,
                ..
            } => {
                let next_card = fuel.iter().find(|id| candidates.contains(id)).unwrap();
                Action::ChooseCostTarget(*next_card)
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                let permanent = sacrifice.expect("the optional sacrifice");
                assert!(candidates.contains(&permanent));
                Action::ChooseCostTarget(permanent)
            }
            other => panic!("unexpected optional escape decision {other:?}"),
        };
        act(state, action);
    }
    panic!("escape did not finish paying");
}

#[test]
fn optional_escape_can_exile_its_own_bargain_or_casualty_sacrifice() {
    use mtg_kernel::card_def::OptionalAdditionalCostDef::{Bargain, Casualty};

    for name in ["Beseech the Mirror", "Make Disappear"] {
        let mut state = plains();
        let breach = put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        let spell = put(&mut state, P0, name, Zone::Graveyard);
        let island = put(&mut state, P0, "Island", Zone::Graveyard);
        let swamp = put(&mut state, P0, "Swamp", Zone::Graveyard);
        let (sacrifice, kind, target) = if name == "Beseech the Mirror" {
            pool(&mut state, P0, &[ManaColor::B; 3], 1);
            (breach, Bargain, None)
        } else {
            let target = opponent_bolts(&mut state, Target::Player(P0));
            pool(&mut state, P0, &[ManaColor::U], 1);
            assert!(!castable(&mut state, spell));
            let creature = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
            (creature, Casualty(1), Some(target))
        };
        let fuel = [island, swamp, sacrifice];
        finish_optional_escape(&mut state, spell, target, Some(sacrifice), &fuel, false);
        let item = state
            .stack
            .iter()
            .find(|item| item.source == spell && !item.is_copy)
            .unwrap();
        assert_eq!(item.v4.optional_additional_cost_paid, Some(kind));
        assert_eq!(zone(&state, spell), Zone::Stack);
        for card in fuel {
            assert_eq!(zone(&state, card), Zone::Exile);
        }
        let refs: Vec<_> = item
            .v4
            .paid_cost_refs
            .iter()
            .filter(|paid| paid.object == sacrifice)
            .map(|paid| (paid.zone, paid.zone_change_count))
            .collect();
        assert!(refs.contains(&(Zone::Graveyard, 1)));
        assert!(refs.contains(&(Zone::Exile, 2)));
        let mut restored: GameState =
            serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
        assert_eq!(restored.state_hash(), state.state_hash());
        assert_eq!(next(&mut restored), next(&mut state));
    }
}

#[test]
fn optional_escape_cannot_use_a_sacrifice_diverted_to_exile_and_can_decline() {
    let mut state = plains();
    let breach = put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    put(&mut state, P1, "Leyline of the Void", Zone::Battlefield);
    let spell = put(&mut state, P0, "Beseech the Mirror", Zone::Graveyard);
    let island = put(&mut state, P0, "Island", Zone::Graveyard);
    let swamp = put(&mut state, P0, "Swamp", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::B; 3], 1);
    assert!(!castable(&mut state, spell));
    let forest = put(&mut state, P0, "Forest", Zone::Graveyard);
    finish_optional_escape(
        &mut state,
        spell,
        None,
        None,
        &[island, swamp, forest],
        false,
    );
    let item = state
        .stack
        .iter()
        .find(|item| item.source == spell)
        .unwrap();
    assert_eq!(item.v4.optional_additional_cost_paid, None);
    assert_eq!(zone(&state, breach), Zone::Battlefield);
    for card in [island, swamp, forest] {
        assert_eq!(zone(&state, card), Zone::Exile);
    }
}

#[test]
fn optional_escape_rejects_a_stale_restored_bargain_sacrifice() {
    let mut state = plains();
    let breach = put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    put(&mut state, P0, "Ichor Wellspring", Zone::Battlefield);
    let spell = put(&mut state, P0, "Beseech the Mirror", Zone::Graveyard);
    let island = put(&mut state, P0, "Island", Zone::Graveyard);
    put(&mut state, P0, "Swamp", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::B; 3], 1);
    assert!(castable(&mut state, spell));
    act(&mut state, Action::CastSpell(spell));
    let Decision::ChooseCostTargets { candidates, .. } = next(&mut state) else {
        panic!("expected the bargain sacrifice");
    };
    assert!(candidates.contains(&breach));
    act(&mut state, Action::ChooseCostTarget(breach));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseCostTargets {
            cost_kind: CostKind::ExileFromGraveyard,
            ..
        }
    ));
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    restored.objects.get_mut(breach).zone_change_count += 2;
    let before = restored.clone();
    assert!(engine::step(&mut restored, Action::ChooseCostTarget(island)).is_err());
    assert_eq!(restored, before);
}

#[test]
fn optional_escape_route_first_casualty_resolves_its_copy() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let creature = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let spell = put(&mut state, P0, "Make Disappear", Zone::Graveyard);
    let fuel = ["Island", "Swamp", "Forest"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
    let bolt = opponent_bolts(&mut state, Target::Player(P0));
    pool(&mut state, P0, &[ManaColor::U], 1);
    finish_optional_escape(&mut state, spell, Some(bolt), Some(creature), &fuel, true);
    assert_eq!(zone(&state, creature), Zone::Graveyard);
    for card in fuel {
        assert_eq!(zone(&state, card), Zone::Exile);
    }
    let Some(Decision::ChooseEffectTargets { legal_targets, .. }) = settle(&mut state) else {
        panic!("expected to choose a target for the casualty copy");
    };
    assert!(legal_targets.contains(&Target::Object(bolt)));
    act(&mut state, Action::ChooseEffectTarget(Target::Object(bolt)));
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    let copy = state.stack.last().expect("the new copy");
    assert!(copy.is_copy);
    assert_eq!(copy.targets, [Target::Object(bolt)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
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
    let player = state.active_player;
    let wrenn = put(state, player, "Wrenn and Six", Zone::Hand);
    pool(state, player, &[ManaColor::R, ManaColor::G], 0);
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
fn bitter_triumph_chooses_a_graveyard_permission_before_its_additional_cost() {
    use engine::{sphinx_lore_cast_v1::RouteV1, CastMode};
    let mut base = plains();
    let source = put(&mut base, P0, "Bitter Triumph", Zone::Graveyard);
    install_snapcaster(&mut base, source);
    put(&mut base, P0, "Underworld Breach", Zone::Battlefield);
    let target = put(&mut base, P1, "Thraben Inspector", Zone::Battlefield);
    let spare = put(&mut base, P0, "Lightning Bolt", Zone::Hand);
    let fuel = ["Plains", "Island", "Forest"].map(|name| put(&mut base, P0, name, Zone::Graveyard));
    pool(&mut base, P0, &[ManaColor::B], 1);
    for route in [RouteV1::Snapcaster, RouteV1::Breach] {
        let mut state = base.clone();
        choose_graveyard_route(&mut state, source, route);
        choose_target(&mut state, Target::Object(target));
        assert!(
            matches!(next(&mut state), Decision::ChooseCastMode { options, .. }
            if options == [CastMode::Normal, CastMode::Alternative])
        );
        act(&mut state, Action::ChooseCastMode(CastMode::Alternative));
        settle_first(&mut state);
        let escaped = route == RouteV1::Breach;
        assert_eq!(bitter_life_payments(&state), 1);
        assert_eq!(zone(&state, spare), Zone::Hand);
        assert_eq!(
            zone(&state, source),
            if escaped {
                Zone::Graveyard
            } else {
                Zone::Exile
            }
        );
        assert_eq!(zone(&state, target), Zone::Graveyard);
        assert!(fuel.into_iter().all(|id| zone(&state, id)
            == if escaped {
                Zone::Exile
            } else {
                Zone::Graveyard
            }));
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
fn graveyard_dynamic_retrace_keeps_the_only_land_in_a_large_hand() {
    let mut base = plains();
    retrace_emblem(&mut base);
    let spell = put(&mut base, P0, "Sickening Dreams", Zone::Graveyard);
    let land = put(&mut base, P0, "Swamp", Zone::Hand);
    let names = [
        "Lightning Bolt",
        "Dark Ritual",
        "Duress",
        "Brainstorm",
        "Ponder",
        "Preordain",
    ];
    let nonlands: Vec<_> = names
        .into_iter()
        .cycle()
        .take(21)
        .map(|name| put(&mut base, P0, name, Zone::Hand))
        .collect();
    pool(&mut base, P0, &[ManaColor::B], 1);
    cast(&mut base, spell, &[]);
    assert!(matches!(
        next(&mut base),
        Decision::ChooseEffectOption {
            option_count: 22,
            ..
        }
    ));
    act(&mut base, Action::ChooseEffectOption(11));
    assert!(matches!(
        next(&mut base),
        Decision::ChooseEffectOption {
            option_count: 2,
            ..
        }
    ));
    for route_first in [false, true] {
        let mut state = base.clone();
        act(
            &mut state,
            Action::ChooseEffectOption(u16::from(route_first)),
        );
        if route_first {
            payment_pick(&mut state, CostKind::DiscardCards, land);
        }
        let Decision::ChooseCostTargets { candidates, .. } = next(&mut state) else {
            panic!("missing printed discard choices");
        };
        assert_eq!(sorted(candidates), sorted(nonlands.clone()));
        for &card in &nonlands[..11] {
            payment_pick(&mut state, CostKind::DiscardCards, card);
        }
        if !route_first {
            payment_pick(&mut state, CostKind::DiscardCards, land);
        }
        next(&mut state);
        assert_eq!(state.stack.last().unwrap().discarded, nonlands[..11]);
        assert_eq!(zone(&state, land), Zone::Graveyard);
        assert_eq!(state.players[0].hand.len(), 10);
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

fn retrace_game_on_turn(player: PlayerId, active: PlayerId) -> GameState {
    let mut state = game_for(&["Plains"; 20], player);
    retrace_emblem(&mut state);
    if active != player {
        run_until(
            &mut state,
            |s| s.active_player == active && s.step == Step::Main1,
            |_, _| None,
        );
    }
    state
}

fn retrace_alternative_fixture(
    player: PlayerId,
    active: PlayerId,
    name: &str,
) -> (GameState, ObjectId, Option<ObjectId>) {
    let mut state = retrace_game_on_turn(player, active);
    let spell = put(&mut state, player, name, Zone::Graveyard);
    let bolt = (name != "Gush").then(|| {
        let opponent = player.opponent();
        state.priority_player = opponent;
        let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);
        pool(&mut state, opponent, &[ManaColor::R], 0);
        cast(&mut state, bolt, &[Target::Player(player)]);
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { player: holder, .. } if holder == opponent));
        act(&mut state, Action::Pass);
        bolt
    });
    (state, spell, bolt)
}

fn retrace_pitch_group(state: &mut GameState, kind: CostKind, picks: &[ObjectId], wrong: ObjectId) {
    for (index, &pick) in picks.iter().enumerate() {
        assert!(
            matches!(next(state), Decision::ChooseCostTargets { remaining, .. }
                if usize::from(remaining) == picks.len() - index)
        );
        let before = state.clone();
        assert!(engine::step(state, Action::ChooseCostTarget(wrong)).is_err());
        assert_eq!(*state, before);
        payment_pick(state, kind, pick);
    }
}

fn retrace_pitch_alternative(name: &str, pitch_count: usize, life_cost: i32) {
    for player in [P0, P1] {
        let (mut base, spell, bolt) = retrace_alternative_fixture(player, player, name);
        let bolt = bolt.unwrap();
        let red = put(&mut base, player, "Lightning Bolt", Zone::Hand);
        let mut no_land = base.clone();
        let land = put(&mut base, player, "Forest", Zone::Hand);
        let other_land = put(&mut base, player, "Island", Zone::Hand);
        let blue_names = ["Brainstorm", "Ponder", "Preordain"];
        let mut blue = Vec::new();
        for &card in &blue_names[..pitch_count] {
            assert!(!castable(&mut base, spell), "{name}: too few blue cards");
            blue.push(put(&mut base, player, card, Zone::Hand));
            put(&mut no_land, player, card, Zone::Hand);
        }
        assert!(!castable(&mut no_land, spell), "{name}: no land to discard");
        let spare = put(&mut base, player, blue_names[pitch_count], Zone::Hand);

        let land_group = (CostKind::DiscardCards, std::slice::from_ref(&land));
        let pitch_group = (CostKind::ExileFromHandV1, blue.as_slice());
        for discard_first in [false, true] {
            let mut state = base.clone();
            cast(&mut state, spell, &[Target::Object(bolt)]);
            assert!(matches!(
                next(&mut state),
                Decision::ChooseEffectOption { source, option_count: 2, .. } if source == spell
            ));
            act(
                &mut state,
                Action::ChooseEffectOption(u16::from(discard_first)),
            );
            let [first, second] = if discard_first {
                [land_group, pitch_group]
            } else {
                [pitch_group, land_group]
            };
            retrace_pitch_group(&mut state, first.0, first.1, red);
            assert_eq!(life(&state, player), 20);
            for &pick in first.1 {
                assert_eq!(zone(&state, pick), Zone::Hand);
            }
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for game in [&mut state, &mut restored] {
                retrace_pitch_group(game, second.0, second.1, first.1[0]);
                assert!(matches!(next(game), Decision::CastSpellOrPass { .. }));
                assert_eq!(
                    game.stack.last().unwrap().v4.cast_method,
                    Some(mtg_kernel::state::CastMethodV4::Alternative)
                );
                assert_eq!(zone(game, land), Zone::Graveyard);
                for &card in &blue {
                    assert_eq!(zone(game, card), Zone::Exile);
                }
                for card in [other_land, spare, red] {
                    assert_eq!(zone(game, card), Zone::Hand);
                }
                assert_eq!(life(game, player), 20 - life_cost);
                settle_with(game, |_, decision| match decision {
                    Decision::ChooseEffectTargets { legal_targets, .. } => {
                        let new_target = Target::Player(player.opponent());
                        assert!(legal_targets.contains(&new_target));
                        Action::ChooseEffectTarget(new_target)
                    }
                    other => first_answer(other),
                });
                assert_eq!(zone(game, spell), Zone::Graveyard);
                assert_eq!(zone(game, bolt), Zone::Graveyard);
                assert_eq!(life(game, player), 20 - life_cost);
                let opponent_life = if name == "Commandeer" { 17 } else { 20 };
                assert_eq!(life(game, player.opponent()), opponent_life);
            }
            assert_eq!(state, restored);
        }
    }
}

#[test]
fn retrace_foil_reserves_a_third_card_for_the_land_discard() {
    for player in [P0, P1] {
        let (mut base, foil, bolt) = retrace_alternative_fixture(player, player, "Foil");
        let bolt = bolt.unwrap();
        let forest = put(&mut base, player, "Forest", Zone::Hand);
        let mut only_two = base.clone();
        put(&mut only_two, player, "Island", Zone::Hand);
        assert!(!castable(&mut only_two, foil));
        let other = put(&mut base, player, "Opt", Zone::Hand);
        let mut no_island = base.clone();
        put(&mut no_island, player, "Mountain", Zone::Hand);
        assert!(!castable(&mut no_island, foil));
        let island = put(&mut base, player, "Island", Zone::Hand);
        for route_first in [false, true] {
            let mut state = base.clone();
            cast(&mut state, foil, &[Target::Object(bolt)]);
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
            let picks = if route_first {
                [forest, island, other]
            } else {
                [island, other, forest]
            };
            for pick in picks {
                assert!(
                    matches!(next(&mut state), Decision::ChooseCostTargets { cost_kind: CostKind::DiscardCards, candidates, .. } if candidates == [pick])
                );
                payment_pick(&mut state, CostKind::DiscardCards, pick);
            }
            settled(&mut state);
            for card in [foil, bolt, island, other, forest] {
                assert_eq!(zone(&state, card), Zone::Graveyard);
            }
            assert_eq!(life(&state, player), 20);
        }
    }
}

#[test]
fn retrace_mine_collapse_sacrifices_a_mountain_only_on_its_controllers_turn() {
    for player in [P0, P1] {
        for active in [player, player.opponent()] {
            let mut state = retrace_game_on_turn(player, active);
            if active != player {
                act(&mut state, Action::Pass);
            }
            let spell = put(&mut state, player, "Mine Collapse", Zone::Graveyard);
            let wall = put(
                &mut state,
                player.opponent(),
                "Wall of Roots",
                Zone::Battlefield,
            );
            let furnace = put(&mut state, player, "Great Furnace", Zone::Battlefield);
            state.objects.get_mut(furnace).tapped = true;
            let land = put(&mut state, player, "Island", Zone::Hand);
            assert!(!castable(&mut state, spell));
            let mountain = put(&mut state, player, "Mountain", Zone::Battlefield);
            state.objects.get_mut(mountain).tapped = true;
            assert_eq!(castable(&mut state, spell), active == player);
            if active != player {
                continue;
            }
            cast(&mut state, spell, &[Target::Object(wall)]);
            assert!(matches!(
                next(&mut state),
                Decision::ChooseEffectOption {
                    option_count: 2,
                    ..
                }
            ));
            act(&mut state, Action::ChooseEffectOption(1));
            payment_pick(&mut state, CostKind::DiscardCards, land);
            assert!(
                matches!(next(&mut state),Decision::ChooseCostTargets{candidates,..} if candidates==[mountain])
            );
            payment_pick(&mut state, CostKind::SacrificeLands, mountain);
            settled(&mut state);
            for card in [spell, land, mountain, wall] {
                assert_eq!(zone(&state, card), Zone::Graveyard);
            }
            assert_eq!(zone(&state, furnace), Zone::Battlefield);
        }
    }
}

#[test]
fn retrace_ramosian_rally_needs_a_plains_and_taps_an_untapped_creature() {
    for player in [P0, P1] {
        let mut state = retrace_game_on_turn(player, player);
        let spell = put(&mut state, player, "Ramosian Rally", Zone::Graveyard);
        let land = put(&mut state, player, "Plains", Zone::Hand);
        let inspector = put(&mut state, player, "Thraben Inspector", Zone::Battlefield);
        let opposing = put(
            &mut state,
            player.opponent(),
            "Thraben Inspector",
            Zone::Battlefield,
        );
        assert!(!castable(&mut state, spell));
        let plains = put(&mut state, player, "Plains", Zone::Battlefield);
        state.objects.get_mut(plains).tapped = true;
        state.objects.get_mut(inspector).tapped = true;
        assert!(!castable(&mut state, spell));
        state.objects.get_mut(inspector).tapped = false;
        cast(&mut state, spell, &[]);
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption {
                option_count: 2,
                ..
            }
        ));
        act(&mut state, Action::ChooseEffectOption(0));
        assert!(
            matches!(next(&mut state),Decision::ChooseCostTargets{candidates,..} if candidates==[inspector])
        );
        payment_pick(&mut state, CostKind::TapPermanents, inspector);
        payment_pick(&mut state, CostKind::DiscardCards, land);
        settled(&mut state);
        assert!(state.objects.get(inspector).tapped);
        assert_eq!(power_toughness(&state, inspector), (2, 3));
        assert!(!state.objects.get(opposing).tapped);
        assert_eq!(power_toughness(&state, opposing), (1, 2));
        assert_eq!(zone(&state, land), Zone::Graveyard);
        assert_eq!(zone(&state, spell), Zone::Graveyard);
    }
}

#[test]
fn retrace_force_of_will_exiles_blue_discards_land_and_pays_one_life() {
    retrace_pitch_alternative("Force of Will", 1, 1);
}

#[test]
fn retrace_commandeer_exiles_two_distinct_blue_cards_and_discards_land() {
    retrace_pitch_alternative("Commandeer", 2, 0);
}

#[test]
fn retrace_force_of_negation_rechecks_its_turn_condition_after_restore() {
    use engine::CastMode;
    for player in [P0, P1] {
        for active in [player, player.opponent()] {
            let (mut state, force, bolt) =
                retrace_alternative_fixture(player, active, "Force of Negation");
            let bolt = bolt.unwrap();
            let pitch = put(&mut state, player, "Brainstorm", Zone::Hand);
            let land = put(&mut state, player, "Forest", Zone::Hand);
            let other = put(&mut state, player, "Swamp", Zone::Hand);
            assert_eq!(castable(&mut state, force), active != player);
            pool(&mut state, player, &[ManaColor::U, ManaColor::U], 1);
            cast(&mut state, force, &[Target::Object(bolt)]);
            if active != player {
                assert!(
                    matches!(next(&mut state), Decision::ChooseCastMode { options, .. }
                    if options.contains(&CastMode::Normal) && options.contains(&CastMode::Alternative))
                );
                act(&mut state, Action::ChooseCastMode(CastMode::Alternative));
                assert!(matches!(
                    next(&mut state),
                    Decision::ChooseEffectOption {
                        option_count: 2,
                        ..
                    }
                ));
                act(&mut state, Action::ChooseEffectOption(0));
                payment_pick(&mut state, CostKind::ExileFromHandV1, pitch);
            } else {
                assert!(matches!(
                    next(&mut state),
                    Decision::ChooseCostTargets {
                        cost_kind: CostKind::DiscardCards,
                        ..
                    }
                ));
                assert_eq!(
                    state.engine.pending_cast.as_ref().unwrap().cast_mode,
                    Some(CastMode::Normal)
                );
                // Forge the alternative on a genuine own-turn payment checkpoint,
                // both before choosing the order and after selecting the blue card.
                for (ordered, action) in [
                    (false, Action::ChooseEffectOption(0)),
                    (true, Action::ChooseCostTarget(land)),
                ] {
                    let mut forged: GameState =
                        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                    let pending = forged.engine.pending_cast.as_mut().unwrap();
                    pending.cast_mode = Some(CastMode::Alternative);
                    if ordered {
                        pending.graveyard_cost_first_v1 = Some(false);
                        pending.sacrifice_chosen.push(pitch);
                    }
                    let before = forged.clone();
                    assert_eq!(
                        engine::step(&mut forged, action).unwrap_err(),
                        "compound cast no longer meets its alternative cost condition"
                    );
                    assert_eq!(forged, before);
                }
            }
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for game in [&mut state, &mut restored] {
                payment_pick(game, CostKind::DiscardCards, land);
                settle_first(game);
                assert_eq!(zone(game, force), Zone::Graveyard);
                assert_eq!(zone(game, bolt), Zone::Exile);
                assert_eq!(zone(game, other), Zone::Hand);
                assert_eq!(zone(game, land), Zone::Graveyard);
                assert_eq!(
                    zone(game, pitch),
                    if active == player {
                        Zone::Hand
                    } else {
                        Zone::Exile
                    }
                );
                assert_eq!(
                    game.players[player.index()].mana_pool.iter().sum::<u8>(),
                    if active == player { 0 } else { 3 }
                );
            }
            assert_eq!(state, restored);
        }
    }
}

#[test]
fn retrace_daze_returns_and_discards_the_same_island_after_restore() {
    for player in [P0, P1] {
        let (mut state, daze, bolt) = retrace_alternative_fixture(player, player, "Daze");
        let bolt = bolt.unwrap();
        let forest = put(&mut state, player, "Forest", Zone::Battlefield);
        state.objects.get_mut(forest).tapped = true;
        assert!(!castable(&mut state, daze));
        let island = put(&mut state, player, "Island", Zone::Battlefield);
        state.objects.get_mut(island).tapped = true;
        cast(&mut state, daze, &[Target::Object(bolt)]);
        payment_pick(&mut state, CostKind::ReturnPermanentsToHand, island);
        assert!(
            matches!(next(&mut state), Decision::ChooseCostTargets { cost_kind: CostKind::DiscardCards, candidates, .. } if candidates == [island])
        );
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        let before = restored.clone();
        assert!(engine::step(&mut restored, Action::ChooseCostTarget(forest)).is_err());
        assert_eq!(restored, before);
        for game in [&mut state, &mut restored] {
            payment_pick(game, CostKind::DiscardCards, island);
            assert!(matches!(next(game), Decision::CastSpellOrPass { .. }));
            let item = game.stack.last().unwrap();
            assert_eq!(
                item.v4.cast_method,
                Some(mtg_kernel::state::CastMethodV4::Alternative)
            );
            let receipts: Vec<_> = item
                .v4
                .paid_cost_refs
                .iter()
                .map(|r| (r.object, r.zone, r.zone_change_count))
                .collect();
            assert_eq!(
                receipts,
                [(island, Zone::Hand, 1), (island, Zone::Graveyard, 2)]
            );
            settle_first(game);
            assert_eq!(life(game, player), 20);
            assert_eq!(zone(game, daze), Zone::Graveyard);
            assert_eq!(zone(game, bolt), Zone::Graveyard);
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn retrace_return_alternatives_allow_either_payment_order_and_require_distinct_islands() {
    for player in [P0, P1] {
        for (name, count) in [("Daze", 1), ("Gush", 2), ("Thwart", 3)] {
            let (mut base, spell, bolt) = retrace_alternative_fixture(player, player, name);
            let land = put(&mut base, player, "Forest", Zone::Hand);
            let mut islands = Vec::new();
            for _ in 0..count {
                assert!(!castable(&mut base, spell));
                let island = put(&mut base, player, "Island", Zone::Battlefield);
                base.objects.get_mut(island).tapped = true;
                islands.push(island);
            }
            let targets: Vec<_> = bolt.into_iter().map(Target::Object).collect();
            if name == "Daze" {
                pool(&mut base, player, &[ManaColor::U], 1);
                let mut normal = base.clone();
                let spare = put(&mut normal, player, "Plains", Zone::Hand);
                cast(&mut normal, spell, &targets);
                assert!(
                    matches!(next(&mut normal), Decision::ChooseCastMode { options, .. }
                    if options.contains(&engine::CastMode::Normal) && options.contains(&engine::CastMode::Alternative))
                );
                act(
                    &mut normal,
                    Action::ChooseCastMode(engine::CastMode::Normal),
                );
                payment_pick(&mut normal, CostKind::DiscardCards, land);
                settle_first(&mut normal);
                assert_eq!(normal.players[player.index()].mana_pool, [0; 6]);
                assert_eq!(zone(&normal, islands[0]), Zone::Battlefield);
                assert_eq!(zone(&normal, land), Zone::Graveyard);
                assert_eq!(zone(&normal, spare), Zone::Hand);
                assert_eq!(life(&normal, player), 20);
            }
            for discard_first in [false, true] {
                let mut state = base.clone();
                cast(&mut state, spell, &targets);
                if name == "Daze" {
                    assert!(matches!(next(&mut state), Decision::ChooseCastMode { .. }));
                    act(
                        &mut state,
                        Action::ChooseCastMode(engine::CastMode::Alternative),
                    );
                }
                assert!(matches!(
                    next(&mut state),
                    Decision::ChooseEffectOption {
                        option_count: 2,
                        ..
                    }
                ));
                act(
                    &mut state,
                    Action::ChooseEffectOption(u16::from(discard_first)),
                );
                if discard_first {
                    payment_pick(&mut state, CostKind::DiscardCards, land);
                }
                for (at, &island) in islands.iter().enumerate() {
                    assert!(
                        matches!(next(&mut state),Decision::ChooseCostTargets{remaining,candidates,..}
                        if usize::from(remaining)==islands.len()-at && sorted(candidates.clone())==sorted(islands[at..].to_vec()))
                    );
                    payment_pick(&mut state, CostKind::ReturnPermanentsToHand, island);
                }
                if !discard_first {
                    payment_pick(&mut state, CostKind::DiscardCards, land);
                }
                assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
                assert_eq!(zone(&state, land), Zone::Graveyard);
                for &island in &islands {
                    assert_eq!(zone(&state, island), Zone::Hand);
                }
                assert_eq!(
                    state.players[player.index()].mana_pool,
                    base.players[player.index()].mana_pool
                );
                let hand = state.players[player.index()].hand.len();
                settle_first(&mut state);
                assert_eq!(zone(&state, spell), Zone::Graveyard);
                if let Some(bolt) = bolt {
                    assert_eq!(zone(&state, bolt), Zone::Graveyard);
                    assert_eq!(life(&state, player), 20);
                } else {
                    assert_eq!(state.players[player.index()].hand.len(), hand + 2);
                }
            }
        }
    }
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

fn install_snapcaster(state: &mut GameState, spell: ObjectId) -> ObjectId {
    let mage = put(state, P0, "Snapcaster Mage", Zone::Hand);
    pool(state, P0, &[ManaColor::U], 1);
    cast(state, mage, &[]);
    settle_choosing(state, spell);
    mage
}

fn choose_graveyard_route(
    state: &mut GameState,
    spell: ObjectId,
    route: engine::sphinx_lore_cast_v1::RouteV1,
) {
    cast(state, spell, &[]);
    let Decision::ChooseEffectOption {
        source,
        option_count,
        ..
    } = next(state)
    else {
        panic!("missing cast route choice")
    };
    assert_eq!(source, spell);
    let options = state
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|event| match event {
            mtg_kernel::event::CommittedEvent::SphinxLoreCastOptionsV1 { options }
                if options.spell == spell =>
            {
                Some(options)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(usize::from(option_count), options.routes.len());
    let index = options
        .routes
        .iter()
        .position(|&r| r == route)
        .expect("requested route was offered");
    act(state, Action::ChooseEffectOption(index as u16));
}

#[test]
fn snapcaster_and_printed_flashback_charge_the_selected_cost() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    let mut base = plains();
    let card = put(&mut base, P0, "Moment's Peace", Zone::Graveyard);
    install_snapcaster(&mut base, card);
    for (route, remaining) in [(RouteV1::Snapcaster, 1), (RouteV1::PrintedFlashback, 0)] {
        let mut state = base.clone();
        pool(&mut state, P0, &[ManaColor::G], 2);
        choose_graveyard_route(&mut state, card, route);
        settled(&mut state);
        assert_eq!(
            state.players[0].mana_pool[ManaColor::C.pool_index()],
            remaining
        );
        assert_eq!(zone(&state, card), Zone::Exile);
    }
    pool(&mut base, P0, &[ManaColor::G], 1);
    cast(&mut base, card, &[]);
    settled(&mut base);
    assert_eq!(
        zone(&base, card),
        Zone::Exile,
        "the cheaper grant works as the sole payable method"
    );
}

#[test]
fn breach_and_printed_flashback_accept_each_payable_method_without_snapcaster() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    let mut base = plains();
    put(&mut base, P0, "Underworld Breach", Zone::Battlefield);
    let card = put(&mut base, P0, "Moment's Peace", Zone::Graveyard);
    let fodder =
        ["Island", "Forest", "Mountain"].map(|name| put(&mut base, P0, name, Zone::Graveyard));
    for (route, generic) in [
        (RouteV1::PrintedFlashback, 2),
        (RouteV1::Breach, 2),
        (RouteV1::Breach, 1),
    ] {
        let mut state = base.clone();
        pool(&mut state, P0, &[ManaColor::G], generic);
        if generic == 2 {
            choose_graveyard_route(&mut state, card, route);
        } else {
            cast(&mut state, card, &[]);
        }
        settle_first(&mut state);
        let escaped = route == RouteV1::Breach;
        assert_eq!(
            state.players[0].mana_pool[ManaColor::C.pool_index()],
            if escaped { generic - 1 } else { 0 }
        );
        assert_eq!(
            zone(&state, card),
            if escaped {
                Zone::Graveyard
            } else {
                Zone::Exile
            }
        );
        assert!(fodder.iter().all(|&f| zone(&state, f)
            == if escaped {
                Zone::Exile
            } else {
                Zone::Graveyard
            }));
    }
}

#[test]
fn breach_and_retrace_grants_offer_both_costs_and_restore_the_selected_route() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    use mtg_kernel::state::SpellCastRouteV4;
    for player in [P0, P1] {
        for name in ["Monstrous Emergence", "Lotus Petal"] {
            let mut base = game_for(&["Plains"; 20], player);
            let target = if name == "Monstrous Emergence" {
                retrace_emblem(&mut base);
                pool(&mut base, player, &[ManaColor::G], 1);
                Some(put(
                    &mut base,
                    player.opponent(),
                    "Mulldrifter",
                    Zone::Battlefield,
                ))
            } else {
                put(&mut base, player, "Six", Zone::Battlefield);
                None
            };
            put(&mut base, player, "Underworld Breach", Zone::Battlefield);
            let land = put(&mut base, player, "Dryad Arbor", Zone::Hand);
            for _ in 0..3 {
                put(&mut base, player, "Island", Zone::Graveyard);
            }
            let spell = put(&mut base, player, name, Zone::Graveyard);
            cast(&mut base, spell, &[]);
            assert!(
                matches!(next(&mut base), Decision::ChooseEffectOption { player: holder, source, option_count: 2, .. } if holder == player && source == spell)
            );
            let choice = base
                .engine
                .pending_cast
                .as_ref()
                .unwrap()
                .sphinx_lore_v1
                .unwrap();
            assert!(
                matches!(&base.engine.event_history[choice.options_history_index as usize],
                mtg_kernel::event::CommittedEvent::SphinxLoreCastOptionsV1 { options }
                    if options.routes == [RouteV1::Breach, RouteV1::Retrace])
            );
            for (index, route) in [
                (0, SpellCastRouteV4::GraveyardEscape),
                (1, SpellCastRouteV4::RetraceV1),
            ] {
                let mut live = base.clone();
                let mut restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&base).unwrap()).unwrap();
                for game in [&mut live, &mut restored] {
                    act(game, Action::ChooseEffectOption(index));
                    assert_eq!(
                        game.objects.get(spell).v4.spell_cast_origin.unwrap().route,
                        route
                    );
                    if let Some(target) = target {
                        choose_target(game, Target::Object(target));
                    }
                    settle_first(game);
                    let escaped = route == SpellCastRouteV4::GraveyardEscape;
                    assert_eq!(game.exile.len(), if escaped { 3 } else { 0 });
                    assert_eq!(
                        zone(game, land),
                        if escaped { Zone::Hand } else { Zone::Graveyard }
                    );
                    if let Some(target) = target {
                        assert_eq!(game.objects.get(target).damage, 1);
                        assert_eq!(zone(game, spell), Zone::Graveyard);
                    } else {
                        assert_eq!(zone(game, spell), Zone::Battlefield);
                    }
                }
                assert_eq!(live, restored);
            }
        }
    }
}

#[test]
fn breach_and_retrace_keep_overload_available_only_with_the_land_discard() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    for player in [P0, P1] {
        let mut base = retrace_game_on_turn(player, player);
        put(&mut base, player, "Underworld Breach", Zone::Battlefield);
        for _ in 0..3 {
            put(&mut base, player, "Island", Zone::Graveyard);
        }
        let land = put(&mut base, player, "Forest", Zone::Hand);
        let spell = put(&mut base, player, "Electrickery", Zone::Graveyard);
        let creatures = [0, 1].map(|_| {
            put(
                &mut base,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            )
        });
        pool(&mut base, player, &[ManaColor::R], 1);
        for route in [RouteV1::Breach, RouteV1::Retrace] {
            let mut state = base.clone();
            choose_graveyard_route(&mut state, spell, route);
            if route == RouteV1::Retrace {
                assert!(
                    matches!(next(&mut state), Decision::ChooseSpellMode { legal_modes, .. } if legal_modes == [0, 1])
                );
                act(&mut state, Action::ChooseSpellMode(1));
            } else {
                choose_target(&mut state, Target::Object(creatures[0]));
            }
            settle_first(&mut state);
            let overloaded = route == RouteV1::Retrace;
            assert_eq!(zone(&state, creatures[0]), Zone::Graveyard);
            assert_eq!(
                zone(&state, creatures[1]),
                if overloaded {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            assert_eq!(
                zone(&state, land),
                if overloaded {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                }
            );
            assert_eq!(state.exile.len(), if overloaded { 0 } else { 3 });
            assert_eq!(
                state.players[player.index()].mana_pool[ManaColor::C.pool_index()],
                u8::from(!overloaded)
            );
        }
    }
}

#[test]
fn breach_and_retrace_do_not_borrow_wills_permission_to_overload_without_a_land() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    // Will and Breach remain independent with an absent or unpayable retrace grant.
    for (player, emblem) in [(P0, true), (P1, true), (P0, false), (P1, false)] {
        let mut base = game_for(&["Plains"; 20], player);
        if emblem {
            retrace_emblem(&mut base);
        }
        let will = put(&mut base, player, "Yawgmoth's Will", Zone::Hand);
        pool(&mut base, player, &[ManaColor::B], 2);
        cast(&mut base, will, &[]);
        settled(&mut base);
        put(&mut base, player, "Underworld Breach", Zone::Battlefield);
        for _ in 0..3 {
            put(&mut base, player, "Island", Zone::Graveyard);
        }
        let creatures = [(); 2].map(|_| {
            put(
                &mut base,
                player.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            )
        });
        let spell = put(&mut base, player, "Electrickery", Zone::Graveyard);
        pool(&mut base, player, &[ManaColor::R], 1);
        assert!(base.players[player.index()].hand.is_empty());
        cast(&mut base, spell, &[]);
        assert!(
            matches!(next(&mut base), Decision::ChooseEffectOption { source, option_count: 2, .. } if source == spell)
        );
        let choice = base
            .engine
            .pending_cast
            .as_ref()
            .unwrap()
            .sphinx_lore_v1
            .unwrap();
        assert!(
            matches!(&base.engine.event_history[choice.options_history_index as usize],
            mtg_kernel::event::CommittedEvent::SphinxLoreCastOptionsV1 { options }
                if options.routes == [RouteV1::Breach, RouteV1::Normal])
        );
        for index in [0, 1] {
            let mut state = base.clone();
            act(&mut state, Action::ChooseEffectOption(index));
            if index == 1 {
                assert!(
                    matches!(next(&mut state), Decision::ChooseSpellMode { legal_modes, .. } if legal_modes == [0, 1])
                );
                act(&mut state, Action::ChooseSpellMode(1));
            } else {
                choose_target(&mut state, Target::Object(creatures[0]));
            }
            settle_first(&mut state);
            assert_eq!(zone(&state, creatures[0]), Zone::Graveyard);
            assert_eq!(
                zone(&state, creatures[1]),
                if index == 1 {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
        }
    }
}

#[test]
fn breach_and_retrace_rides_end_quotes_the_land_and_the_selected_targets_cost() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    use mtg_kernel::state::SpellCastRouteV4;
    for player in [P0, P1] {
        let mut base = retrace_game_on_turn(player, player);
        let will = put(&mut base, player, "Yawgmoth's Will", Zone::Hand);
        pool(&mut base, player, &[ManaColor::B], 2);
        cast(&mut base, will, &[]);
        settled(&mut base);
        for _ in 0..3 {
            put(&mut base, player, "Island", Zone::Graveyard);
        }
        let spell = put(&mut base, player, "Ride's End", Zone::Graveyard);
        let tapped = put(
            &mut base,
            player.opponent(),
            "Thraben Inspector",
            Zone::Battlefield,
        );
        base.objects.get_mut(tapped).tapped = true;
        let untapped = put(
            &mut base,
            player.opponent(),
            "Thraben Inspector",
            Zone::Battlefield,
        );
        for (generic, route, breach) in [
            (1, None, true),
            (4, None, true),
            (1, Some(RouteV1::Normal), false),
            (1, Some(RouteV1::Retrace), false),
            (1, Some(RouteV1::Retrace), true),
        ] {
            let mut state = base.clone();
            if breach {
                put(&mut state, player, "Underworld Breach", Zone::Battlefield);
            }
            let land = route.map(|_| put(&mut state, player, "Forest", Zone::Hand));
            pool(&mut state, player, &[ManaColor::W], generic);
            if let Some(route) = route {
                choose_graveyard_route(&mut state, spell, route);
            } else {
                cast(&mut state, spell, &[]);
                assert!(state
                    .engine
                    .pending_cast
                    .as_ref()
                    .unwrap()
                    .sphinx_lore_v1
                    .is_none());
            }
            let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
                panic!("missing Ride's End target")
            };
            assert!(legal_targets.contains(&Target::Object(tapped)));
            assert_eq!(
                legal_targets.contains(&Target::Object(untapped)),
                generic == 4
            );
            let target = if generic == 4 { untapped } else { tapped };
            act(&mut state, Action::ChooseTarget(Target::Object(target)));
            let retraced = route == Some(RouteV1::Retrace);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for game in [&mut state, &mut restored] {
                assert!(matches!(next(game), Decision::CastSpellOrPass { .. }));
                assert_eq!(
                    game.objects.get(spell).v4.spell_cast_origin.unwrap().route
                        == SpellCastRouteV4::RetraceV1,
                    retraced
                );
                if let Some(land) = land {
                    assert_eq!(
                        zone(game, land),
                        if retraced { Zone::Exile } else { Zone::Hand }
                    );
                }
                assert_eq!(game.players[player.index()].mana_pool, [0; 6]);
                settled(game);
                assert_eq!(zone(game, target), Zone::Exile);
            }
            assert_eq!(state, restored);
        }
    }
}

#[test]
fn breach_and_retrace_rides_end_granted_flashback_can_pay_for_its_tapped_target() {
    use mtg_kernel::state::SpellCastRouteV4;
    for player in [P0, P1] {
        let mut state = retrace_game_on_turn(player, player);
        let spell = put(&mut state, player, "Ride's End", Zone::Graveyard);
        let mage = put(&mut state, player, "Snapcaster Mage", Zone::Hand);
        pool(&mut state, player, &[ManaColor::U], 1);
        cast(&mut state, mage, &[]);
        settle_choosing(&mut state, spell);
        let victim = put(
            &mut state,
            player.opponent(),
            "Thraben Inspector",
            Zone::Battlefield,
        );
        state.objects.get_mut(victim).tapped = true;
        pool(&mut state, player, &[ManaColor::W], 1);
        cast(&mut state, spell, &[Target::Object(victim)]);
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(
            state.objects.get(spell).v4.spell_cast_origin.unwrap().route,
            SpellCastRouteV4::SnapcasterFlashbackV1
        );
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        settled(&mut state);
        assert_eq!(zone(&state, victim), Zone::Exile);
        assert_eq!(zone(&state, spell), Zone::Exile);
    }
}

#[test]
fn breach_and_retrace_keep_the_legacy_cast_when_only_one_route_is_payable() {
    use mtg_kernel::state::SpellCastRouteV4;
    for player in [P0, P1] {
        for escape in [false, true] {
            for both_grants in [false, true] {
                let mut state = game_for(&["Plains"; 20], player);
                if !escape || both_grants {
                    retrace_emblem(&mut state);
                }
                if escape || both_grants {
                    put(&mut state, player, "Underworld Breach", Zone::Battlefield);
                }
                if escape || !both_grants {
                    for _ in 0..3 {
                        put(&mut state, player, "Island", Zone::Graveyard);
                    }
                }
                let land = (!escape || !both_grants)
                    .then(|| put(&mut state, player, "Forest", Zone::Hand));
                let spell = put(&mut state, player, "Dark Ritual", Zone::Graveyard);
                pool(&mut state, player, &[ManaColor::B], 0);
                cast(&mut state, spell, &[]);
                let route = state.objects.get(spell).v4.spell_cast_origin.unwrap().route;
                assert!(if escape {
                    route == SpellCastRouteV4::GraveyardEscape
                } else {
                    matches!(route, SpellCastRouteV4::GraveyardPermissionV1 { holder, .. } if holder == player)
                });
                assert!(!state.engine.event_history.iter().any(|e| matches!(e, mtg_kernel::event::CommittedEvent::SphinxLoreCastOptionsV1 {options} if options.spell == spell)));
                let mut restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                for game in [&mut state, &mut restored] {
                    settle_first(game);
                    assert_eq!(game.exile.len(), if escape { 3 } else { 0 });
                    if let Some(land) = land {
                        assert_eq!(
                            zone(game, land),
                            if escape { Zone::Hand } else { Zone::Graveyard }
                        );
                    }
                    assert_eq!(
                        game.players[player.index()].mana_pool[ManaColor::B.pool_index()],
                        3
                    );
                }
                assert_eq!(state, restored);
            }
        }
    }
}

#[test]
fn printed_and_breach_escape_charge_the_selected_sleep_cost_after_restore() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    for (generic, route) in [
        (0, RouteV1::Breach),
        (2, RouteV1::Breach),
        (2, RouteV1::Escape),
    ] {
        let mut state = plains();
        let card = put(&mut state, P0, "Sleep of the Dead", Zone::Graveyard);
        put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        let target = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
        put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
        let fuel =
            ["Swamp", "Island", "Forest"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
        pool(&mut state, P0, &[ManaColor::U], generic);
        if generic == 0 {
            cast(&mut state, card, &[]);
        } else {
            choose_graveyard_route(&mut state, card, route);
        }
        choose_target(&mut state, Target::Object(target));
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for game in [&mut state, &mut restored] {
            settle_first(game);
            assert_eq!(zone(game, card), Zone::Graveyard);
            assert!(game.objects.get(target).tapped);
            assert!(fuel.iter().all(|&id| zone(game, id) == Zone::Exile));
            assert_eq!(
                game.players[0].mana_pool.iter().sum::<u8>(),
                if route == RouteV1::Breach { generic } else { 0 }
            );
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn printed_and_breach_escape_require_the_grant_fuel_target_and_colored_mana() {
    for (owner, fuel, target, blue) in [
        (P1, 3, true, true),
        (P0, 2, true, true),
        (P0, 3, false, true),
        (P0, 3, true, false),
    ] {
        let mut state = plains();
        let card = put(&mut state, P0, "Sleep of the Dead", Zone::Graveyard);
        put(&mut state, owner, "Underworld Breach", Zone::Battlefield);
        if target {
            put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
        }
        for _ in 0..fuel {
            put(&mut state, P0, "Forest", Zone::Graveyard);
        }
        pool(
            &mut state,
            P0,
            if blue {
                &[ManaColor::U]
            } else {
                &[ManaColor::C]
            },
            0,
        );
        assert!(!castable(&mut state, card));
    }
}

#[test]
fn printed_and_breach_escape_use_nethergoyfs_selected_exile_cost() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    for route in [RouteV1::Breach, RouteV1::Escape] {
        let mut state = plains();
        put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        let goyf = put(&mut state, P0, "Nethergoyf", Zone::Graveyard);
        let fuel = [
            "Island",
            "Lightning Bolt",
            "Ponder",
            "Mishra's Bauble",
            "Forest",
        ]
        .map(|name| put(&mut state, P0, name, Zone::Graveyard));
        pool(&mut state, P0, &[ManaColor::B], 2);
        choose_graveyard_route(&mut state, goyf, route);
        let count = if route == RouteV1::Breach { 3 } else { 4 };
        for &card in &fuel[..count] {
            let Decision::ChooseCostTargets { candidates, .. } = next(&mut state) else {
                panic!("missing exile choice for {route:?}");
            };
            assert!(candidates.contains(&card));
            act(&mut state, Action::ChooseCostTarget(card));
            if route == RouteV1::Breach && card == fuel[0] {
                let surface = mtg_kernel::policy_surface_v5::PolicySurfaceV5::new();
                let observed = mtg_kernel::rl::observe_policy_v6(&state, &surface, P0, 0, 0, 0, 1)
                    .unwrap()
                    .extensions
                    .pending_cast_object_cost
                    .unwrap();
                let history =
                    mtg_kernel::heartfire_history_v1::observe(&state, &surface, P0, 0, 0, 0, 1)
                        .unwrap()
                        .extensions
                        .pending_cast_object_cost
                        .unwrap();
                assert_eq!(observed, history);
                assert_eq!(observed.required_count, 3);
                assert_eq!(observed.remaining_count, 2);
                assert_eq!(observed.selected[0].arena_id, card.0);
            }
        }
        if route == RouteV1::Escape {
            assert!(matches!(
                next(&mut state),
                Decision::ChooseEffectTargets {
                    can_finish: true,
                    ..
                }
            ));
            act(&mut state, Action::FinishEffectSelection);
        }
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        settle_first(&mut state);
        assert_eq!(zone(&state, goyf), Zone::Battlefield);
        assert_eq!(
            state.players[0].mana_pool.iter().sum::<u8>(),
            if route == RouteV1::Breach { 2 } else { 0 }
        );
        assert!(fuel[..count]
            .iter()
            .all(|&id| zone(&state, id) == Zone::Exile));
        assert!(fuel[count..]
            .iter()
            .all(|&id| zone(&state, id) == Zone::Graveyard));
    }
}

#[test]
fn printed_and_breach_escape_preserve_escaped_uro_when_only_breach_is_payable() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let uro = put(
        &mut state,
        P0,
        "Uro, Titan of Nature's Wrath",
        Zone::Graveyard,
    );
    for _ in 0..3 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, uro, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, uro), Zone::Battlefield);
    assert_eq!(state.players[0].life, 23);
}

#[test]
fn twinned_vision_discards_only_for_its_printed_flashback() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    for route in [
        RouteV1::PrintedFlashback,
        RouteV1::Breach,
        RouteV1::Snapcaster,
    ] {
        let mut state = plains();
        let card = put(&mut state, P0, "Twinned Vision", Zone::Graveyard);
        let discard = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        let fodder =
            ["Island", "Forest", "Mountain"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
        if route == RouteV1::Snapcaster {
            install_snapcaster(&mut state, card);
        } else {
            put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        }
        pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
        choose_graveyard_route(&mut state, card, route);
        settle_first(&mut state);
        let printed = route == RouteV1::PrintedFlashback;
        let escaped = route == RouteV1::Breach;
        assert_eq!(
            zone(&state, card),
            if escaped {
                Zone::Graveyard
            } else {
                Zone::Exile
            }
        );
        assert_eq!(
            zone(&state, discard),
            if printed { Zone::Graveyard } else { Zone::Hand }
        );
        assert!(fodder.iter().all(|&f| zone(&state, f)
            == if escaped {
                Zone::Exile
            } else {
                Zone::Graveyard
            }));
        assert_eq!(state.players[0].hand.len(), if printed { 2 } else { 3 });
        assert_eq!(
            state.players[0].mana_pool.iter().sum::<u8>(),
            if printed { 0 } else { 1 }
        );
    }
}

#[test]
fn printed_flashback_keeps_the_ordinary_and_retrace_permissions_independent() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    for route in [RouteV1::Normal, RouteV1::Retrace] {
        let mut state = plains();
        let card = put(&mut state, P0, "Moment's Peace", Zone::Graveyard);
        let land = put(&mut state, P0, "Forest", Zone::Hand);
        if route == RouteV1::Normal {
            let will = put(&mut state, P0, "Yawgmoth's Will", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::B], 2);
            cast(&mut state, will, &[]);
            settled(&mut state);
        } else {
            let wrenn = put(&mut state, P0, "Wrenn and Six", Zone::Hand);
            pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
            cast(&mut state, wrenn, &[]);
            settled(&mut state);
            mtg_kernel::planeswalker_v1::change_loyalty(&mut state, wrenn, 4);
            activate(&mut state, wrenn, 2, &[]);
            settled(&mut state);
        }
        pool(&mut state, P0, &[ManaColor::G], 2);
        choose_graveyard_route(&mut state, card, route);
        settle_first(&mut state);
        let retraced = route == RouteV1::Retrace;
        assert_eq!(
            zone(&state, land),
            if retraced {
                Zone::Graveyard
            } else {
                Zone::Hand
            }
        );
        assert_eq!(
            zone(&state, card),
            if retraced {
                Zone::Graveyard
            } else {
                Zone::Exile
            }
        );
        assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 1);
    }
}

#[test]
fn snapcaster_can_use_mana_cost_instead_of_printed_escape() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    let mut base = plains();
    let card = put(&mut base, P0, "Sleep of the Dead", Zone::Graveyard);
    let creature = install_snapcaster(&mut base, card);
    put(&mut base, P1, "Thraben Inspector", Zone::Battlefield);
    let fodder: Vec<_> = (0..3)
        .map(|_| put(&mut base, P0, "Plains", Zone::Graveyard))
        .collect();
    for route in [RouteV1::Snapcaster, RouteV1::Escape] {
        let mut state = base.clone();
        pool(&mut state, P0, &[ManaColor::U], 2);
        choose_graveyard_route(&mut state, card, route);
        choose_target(&mut state, Target::Object(creature));
        settle_first(&mut state);
        let escaped = route == RouteV1::Escape;
        assert_eq!(
            zone(&state, card),
            if escaped {
                Zone::Graveyard
            } else {
                Zone::Exile
            }
        );
        assert!(fodder.iter().all(|&f| zone(&state, f)
            == if escaped {
                Zone::Exile
            } else {
                Zone::Graveyard
            }));
        assert_eq!(
            state.players[0].mana_pool[ManaColor::C.pool_index()],
            if escaped { 0 } else { 2 }
        );
        assert!(state.objects.get(creature).tapped);
    }
}

#[test]
fn snapcaster_cost_ignores_lava_darts_printed_sacrifice_and_expires_on_zone_change() {
    use mtg_kernel::event::{self, ProposedEvent};
    let mut state = plains();
    let card = put(&mut state, P0, "Lava Dart", Zone::Graveyard);
    install_snapcaster(&mut state, card);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let mut moved = state.clone();
    event::propose_and_commit(&mut moved, ProposedEvent::zone_change(card, Zone::Hand));
    event::propose_and_commit(
        &mut moved,
        ProposedEvent::zone_change(card, Zone::Graveyard),
    );
    assert!(
        !castable(&mut moved, card),
        "the new incarnation has only its printed Mountain cost"
    );
    cast(&mut state, card, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, card), Zone::Exile);
    assert_eq!(life(&state, P1), 19);
}

#[test]
fn boarding_action_beam_and_snapcaster_distinguish_bounce_payment_and_flashback() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    use mtg_kernel::state::CastMethodV4;
    let mut base = plains();
    let card = put(&mut base, P0, "Boarding Action", Zone::Graveyard);
    let mage = install_snapcaster(&mut base, card);
    for (route, method, mage_zone, remaining) in [
        (RouteV1::BeamMeUp, CastMethodV4::Alternative, Zone::Hand, 0),
        (
            RouteV1::Snapcaster,
            CastMethodV4::Flashback,
            Zone::Battlefield,
            1,
        ),
    ] {
        let mut state = base.clone();
        pool(&mut state, P0, &[ManaColor::R], 2);
        choose_graveyard_route(&mut state, card, route);
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let row = state.stack.last().unwrap();
        assert_eq!(row.v4.cast_method, Some(method));
        assert_eq!(row.is_flashback, route == RouteV1::Snapcaster);
        settled(&mut state);
        assert_eq!(zone(&state, mage), mage_zone);
        assert_eq!(zone(&state, card), Zone::Exile);
        assert_eq!(
            state.players[0].mana_pool[ManaColor::C.pool_index()],
            remaining
        );
    }
}

#[test]
fn boarding_action_can_escape_or_retrace_without_returning_a_creature() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    for route in [RouteV1::Breach, RouteV1::Retrace, RouteV1::Normal] {
        let mut state = plains();
        let creature = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
        let card = put(&mut state, P0, "Boarding Action", Zone::Graveyard);
        let land = put(&mut state, P0, "Forest", Zone::Hand);
        let fodder: Vec<_> = (0..3)
            .map(|_| put(&mut state, P0, "Plains", Zone::Graveyard))
            .collect();
        match route {
            RouteV1::Breach => {
                put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
            }
            RouteV1::Retrace => {
                let wrenn = put(&mut state, P0, "Wrenn and Six", Zone::Hand);
                pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
                cast(&mut state, wrenn, &[]);
                settled(&mut state);
                mtg_kernel::planeswalker_v1::change_loyalty(&mut state, wrenn, 4);
                activate(&mut state, wrenn, 2, &[]);
                settled(&mut state);
            }
            RouteV1::Normal => {
                let will = put(&mut state, P0, "Yawgmoth's Will", Zone::Hand);
                pool(&mut state, P0, &[ManaColor::B], 2);
                cast(&mut state, will, &[]);
                settled(&mut state);
            }
            _ => unreachable!(),
        }
        pool(&mut state, P0, &[ManaColor::R], 2);
        choose_graveyard_route(&mut state, card, route);
        settle_first(&mut state);
        assert_eq!(zone(&state, creature), Zone::Battlefield);
        assert_eq!(
            zone(&state, land),
            if route == RouteV1::Retrace {
                Zone::Graveyard
            } else {
                Zone::Hand
            }
        );
        assert!(fodder.iter().all(|&f| zone(&state, f)
            == if route == RouteV1::Breach {
                Zone::Exile
            } else {
                Zone::Graveyard
            }));
        assert_eq!(
            zone(&state, card),
            if route == RouteV1::Normal {
                Zone::Exile
            } else {
                Zone::Graveyard
            }
        );
        assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 1);
    }
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
        put(&mut state, P0, "Llanowar Elves", origin);
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
        assert!(matches!(
            next(&mut state),
            Decision::ChooseCostTargets {
                cost_kind: CostKind::ChooseCreatureOrRevealCreature,
                ..
            }
        ));
        assert_eq!(
            state
                .engine
                .pending_cast
                .as_ref()
                .unwrap()
                .sacrifice_chosen
                .len(),
            3
        );
        // With the three exiles staged and the creature choice still open,
        // schema 5 must refuse by name rather than drop the creature cost.
        let error = public_observation(&state).unwrap_err();
        assert!(error.contains("chosen-creature additional cost"), "{error}");
        payment_pick(
            &mut state,
            CostKind::ChooseCreatureOrRevealCreature,
            creature,
        );
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert!(grave.iter().all(|id| zone(&state, *id) == Zone::Exile));
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

/// P0's schema 5 policy observation of `state` under the public shroud
/// contract, as the JSONL session serializes it, or the refusal.
fn public_observation(state: &GameState) -> Result<Value, String> {
    let mut state = state.clone();
    mtg_kernel::rl::enable_public_shroud_v1(&mut state);
    let surface = mtg_kernel::policy_surface_v5::PolicySurfaceV5::new();
    mtg_kernel::rl::observe_policy_v5(&state, &surface, P0, 0, 0, 0, 1)
        .map(|observation| serde_json::to_value(observation).unwrap())
        .map_err(|error| error.0)
}

/// The staged Escape record of P0's public observation (`Value::Null` when
/// absent). The same state without the public contract keeps refusing.
fn staged_escape(state: &GameState) -> Value {
    let observation = public_observation(state).unwrap_or_else(|error| panic!("{error}"));
    let staged = observation["projection"]["staged_escape"].clone();
    // Present exactly while the engine stages an Escape cast; otherwise the
    // key is absent from the serialized bytes (not null, not empty).
    let escaping = state.engine.pending_cast.as_ref().is_some_and(|pending| {
        pending.source_contract.cast_method == mtg_kernel::state::CastMethodV4::Escape
    });
    assert_eq!(staged.is_object(), escaping, "{staged}");
    assert_eq!(observation.to_string().contains("staged_escape"), escaping);
    if !staged.is_null() {
        let mut legacy_state = state.clone();
        legacy_state.public_shroud_v1 = false;
        let surface = mtg_kernel::policy_surface_v5::PolicySurfaceV5::new();
        let legacy = mtg_kernel::rl::observe_policy_v5(&legacy_state, &surface, P0, 0, 0, 0, 1);
        assert!(legacy.unwrap_err().0.contains("staged Escape"));
        // The legacy sacrifice binding keeps its frozen meaning.
        assert_eq!(
            observation["projection"]["engine_context"]["pending_cast"]["sacrifice_chosen"],
            serde_json::json!([])
        );
    }
    staged
}

fn selected_ids(group: &Value) -> Vec<u32> {
    group["selected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|card| card["arena_id"].as_u64().unwrap() as u32)
        .collect()
}

fn ids(objects: &[ObjectId]) -> Vec<u32> {
    objects.iter().map(|object| object.0).collect()
}

#[test]
fn public_escape_stages_nethergoyfs_printed_and_breach_costs() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    for route in [RouteV1::Escape, RouteV1::Breach] {
        let mut state = plains();
        put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        let goyf = put(&mut state, P0, "Nethergoyf", Zone::Graveyard);
        let fuel = [
            "Island",
            "Lightning Bolt",
            "Ponder",
            "Mishra's Bauble",
            "Forest",
        ]
        .map(|name| put(&mut state, P0, name, Zone::Graveyard));
        pool(&mut state, P0, &[ManaColor::B], 2);
        assert!(staged_escape(&state).is_null());
        cast(&mut state, goyf, &[]);
        let Decision::ChooseEffectOption { option_count, .. } = next(&mut state) else {
            panic!("missing cast route choice")
        };
        assert_eq!(option_count, 2);
        let staged = staged_escape(&state);
        assert_eq!(staged["source"]["arena_id"], goyf.0);
        assert_eq!(staged["controller"], "p0");
        assert_eq!(
            staged["stage"],
            serde_json::json!({"kind": "choosing_route",
                "routes": ["printed_escape", "breach_escape"]})
        );
        let breach = route == RouteV1::Breach;
        act(&mut state, Action::ChooseEffectOption(u16::from(breach)));
        let count = if breach { 3 } else { 4 };
        for (picked, &card) in fuel[..count].iter().enumerate() {
            let Decision::ChooseCostTargets {
                cost_kind,
                candidates,
                ..
            } = next(&mut state)
            else {
                panic!("missing exile choice for {route:?}");
            };
            assert_eq!(cost_kind, CostKind::ExileFromGraveyard);
            assert!(candidates.contains(&card));
            assert!(candidates.iter().all(|c| !fuel[..picked].contains(c)));
            let staged = staged_escape(&state);
            let stage = &staged["stage"];
            assert_eq!(stage["kind"], "paying");
            assert_eq!(stage["breach_escape"], breach);
            assert_eq!(stage["payment_order_chosen"], true);
            let groups = stage["cost_groups"].as_array().unwrap();
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0]["kind"], "ExileFromGraveyard");
            assert_eq!(groups[0]["escape"], true);
            if breach {
                assert_eq!(groups[0]["count"], 3);
                assert!(groups[0].get("min_card_types").is_none());
            } else {
                assert!(groups[0].get("count").is_none());
                assert_eq!(groups[0]["min_card_types"], 4);
            }
            assert!(groups[0].get("finished").is_none());
            assert_eq!(selected_ids(&groups[0]), ids(&fuel[..picked]));
            act(&mut state, Action::ChooseCostTarget(card));
        }
        if !breach {
            // Four card types are covered: exile more, or finish.
            assert!(matches!(
                next(&mut state),
                Decision::ChooseEffectTargets {
                    can_finish: true,
                    ..
                }
            ));
            let staged = staged_escape(&state);
            let group = &staged["stage"]["cost_groups"][0];
            assert_eq!(selected_ids(group), ids(&fuel[..4]));
            assert!(group.get("finished").is_none());
            act(&mut state, Action::FinishEffectSelection);
        }
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert!(state.engine.pending_cast.is_none());
        assert!(staged_escape(&state).is_null());
        settle_first(&mut state);
        assert_eq!(zone(&state, goyf), Zone::Battlefield);
        assert!(fuel[..count]
            .iter()
            .all(|&id| zone(&state, id) == Zone::Exile));
    }
}

#[test]
fn public_escape_stages_a_titans_printed_cost_and_a_breach_grant() {
    // Uro's printed escape is its only graveyard route: no route choice.
    let mut state = plains();
    let uro = put(
        &mut state,
        P0,
        "Uro, Titan of Nature's Wrath",
        Zone::Graveyard,
    );
    // One spare card keeps every pick a real choice.
    let fuel = ["Island", "Forest", "Ponder", "Brainstorm", "Opt", "Swamp"]
        .map(|name| put(&mut state, P0, name, Zone::Graveyard));
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::G, ManaColor::U, ManaColor::U],
        0,
    );
    cast(&mut state, uro, &[]);
    for picked in 0..5 {
        let decision = next(&mut state);
        let Decision::ChooseCostTargets { candidates, .. } = decision else {
            panic!("missing Uro exile choice: {decision:?}")
        };
        let staged = staged_escape(&state);
        assert_eq!(staged["source"]["arena_id"], uro.0);
        let stage = &staged["stage"];
        assert_eq!(stage["kind"], "paying");
        assert_eq!(stage["breach_escape"], false);
        assert_eq!(stage["cost_groups"][0]["count"], 5);
        assert_eq!(selected_ids(&stage["cost_groups"][0]), ids(&fuel[..picked]));
        assert!(candidates.contains(&fuel[picked]));
        act(&mut state, Action::ChooseCostTarget(fuel[picked]));
    }
    settle_first(&mut state);
    assert_eq!(zone(&state, uro), Zone::Battlefield);

    // A card without a printed escape escapes only by Breach's grant.
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let petal = put(&mut state, P0, "Lotus Petal", Zone::Graveyard);
    let fuel = ["Island", "Forest", "Ponder", "Swamp"]
        .map(|name| put(&mut state, P0, name, Zone::Graveyard));
    cast(&mut state, petal, &[]);
    for picked in 0..3 {
        let decision = next(&mut state);
        assert!(
            matches!(decision, Decision::ChooseCostTargets { .. }),
            "{decision:?}"
        );
        let staged = staged_escape(&state);
        let stage = &staged["stage"];
        assert_eq!(stage["kind"], "paying");
        assert_eq!(stage["breach_escape"], true);
        assert_eq!(stage["cost_groups"][0]["count"], 3);
        assert_eq!(selected_ids(&stage["cost_groups"][0]), ids(&fuel[..picked]));
        act(&mut state, Action::ChooseCostTarget(fuel[picked]));
    }
    settle_first(&mut state);
    assert_eq!(zone(&state, petal), Zone::Battlefield);
}

#[test]
fn public_escape_stages_both_groups_of_an_escape_with_a_bargain() {
    for route_first in [false, true] {
        let mut state = plains();
        let breach = put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        let spell = put(&mut state, P0, "Beseech the Mirror", Zone::Graveyard);
        let fuel =
            ["Island", "Swamp", "Forest"].map(|name| put(&mut state, P0, name, Zone::Graveyard));
        pool(&mut state, P0, &[ManaColor::B; 3], 1);
        assert!(castable(&mut state, spell));
        act(&mut state, Action::CastSpell(spell));
        let mut order_choices = 0;
        let mut last = Value::Null;
        for _ in 0..12 {
            let decision = next(&mut state);
            let Some(pending) = state.engine.pending_cast.as_ref() else {
                break;
            };
            let staged = staged_escape(&state);
            let action = match decision {
                Decision::ChooseEffectOption { .. }
                    if pending.optional_additional_cost_paid.is_none() =>
                {
                    Action::ChooseEffectOption(1)
                }
                Decision::ChooseEffectOption { .. } => {
                    order_choices += 1;
                    let stage = &staged["stage"];
                    assert_eq!(stage["payment_order_chosen"], false);
                    assert_eq!(
                        stage["cost_groups"],
                        serde_json::json!([
                            {"kind": "SacrificePermanents", "escape": false, "count": 1,
                             "selected": []},
                            {"kind": "ExileFromGraveyard", "escape": true, "count": 3,
                             "selected": []},
                        ])
                    );
                    Action::ChooseEffectOption(u16::from(route_first))
                }
                Decision::ChooseCostTargets {
                    cost_kind: CostKind::ExileFromGraveyard,
                    candidates,
                    ..
                } => {
                    let card = fuel.iter().find(|id| candidates.contains(id)).unwrap();
                    Action::ChooseCostTarget(*card)
                }
                Decision::ChooseCostTargets { candidates, .. } => {
                    assert!(candidates.contains(&breach));
                    Action::ChooseCostTarget(breach)
                }
                other => panic!("unexpected escape decision {other:?}"),
            };
            last = staged;
            act(&mut state, action);
        }
        assert_eq!(order_choices, 1);
        // Before the last pick: three picks staged, split by payment order.
        let stage = &last["stage"];
        assert_eq!(stage["breach_escape"], true);
        assert_eq!(stage["payment_order_chosen"], true);
        let groups = stage["cost_groups"].as_array().unwrap();
        let (exile, sacrifice) = if route_first {
            (&groups[0], &groups[1])
        } else {
            (&groups[1], &groups[0])
        };
        assert_eq!(exile["escape"], true);
        assert_eq!(sacrifice["escape"], false);
        if route_first {
            assert_eq!(selected_ids(exile), ids(&fuel));
            assert!(selected_ids(sacrifice).is_empty());
        } else {
            assert_eq!(selected_ids(sacrifice), ids(&[breach]));
            assert_eq!(selected_ids(exile), ids(&fuel[..2]));
        }
        assert!(state.engine.pending_cast.is_none());
        assert!(fuel.iter().all(|&id| zone(&state, id) == Zone::Exile));
    }
}

/// Staged Escape casts seen through public sessions.
#[derive(Default)]
struct EscapeEvents {
    route_choices: usize,
    breach_picks: usize,
    printed_picks: usize,
    card_type_picks: usize,
    sources: std::collections::BTreeSet<&'static str>,
}

impl EscapeEvents {
    fn observe(&mut self, decision: &Value, action: &Value) {
        let staged = &decision["observation"]["projection"]["staged_escape"];
        if staged.is_null() {
            return;
        }
        assert_eq!(&staged["controller"], &action["semantic"]["actor"]);
        let source = CARD_DEFS[staged["source"]["card_db_id"].as_u64().unwrap() as usize].name;
        self.sources.insert(source);
        let stage = &staged["stage"];
        let actions = decision["legal_actions"].as_array().unwrap();
        if stage["kind"] == "choosing_route" {
            self.route_choices += 1;
            let routes = stage["routes"].as_array().unwrap().len() as u64;
            assert!(actions.iter().all(|a| {
                a["semantic"]["action_kind"] == "choose_effect_option"
                    && a["semantic"]["option_count"].as_u64() == Some(routes)
            }));
            return;
        }
        assert_eq!(stage["kind"], "paying");
        if action["semantic"]["action_kind"] != "choose_cost_target" {
            return;
        }
        // Each offered pick names its card, none already chosen for the
        // group it pays.
        let candidates: Vec<u64> = actions
            .iter()
            .map(|a| a["semantic"]["candidate"]["arena_id"].as_u64().unwrap())
            .collect();
        let unique: std::collections::BTreeSet<u64> = candidates.iter().copied().collect();
        assert_eq!(unique.len(), candidates.len());
        let group = stage["cost_groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|group| {
                group["count"].as_u64().is_none_or(|count| {
                    group["selected"].as_array().unwrap().len() < count as usize
                })
            })
            .unwrap();
        if group["escape"] == true {
            let chosen = selected_ids(group);
            assert!(candidates
                .iter()
                .all(|candidate| !chosen.contains(&(*candidate as u32))));
            if group["min_card_types"].is_number() {
                self.card_type_picks += 1;
            } else if stage["breach_escape"] == true {
                self.breach_picks += 1;
            } else {
                self.printed_picks += 1;
            }
        }
    }
}

/// The Nethergoyf and Breach, Uro and Bloodbraid Challenger, and Phlage
/// decks.
fn escape_decks() -> [CustomDeckV1; 3] {
    let graveyard = crate::eternal_catalog_v1::stress_deck(
        &[("Swamp", 12), ("Mountain", 12)],
        [
            "Nethergoyf",
            "Underworld Breach",
            "Faithless Looting",
            "Mishra's Bauble",
            "Lotus Petal",
            "Lightning Bolt",
            "Dark Ritual",
            "Chain Lightning",
            "Thoughtseize",
        ],
    );
    let titans = crate::eternal_catalog_v1::stress_deck(
        &[("Forest", 10), ("Island", 7), ("Mountain", 7)],
        [
            "Uro, Titan of Nature's Wrath",
            "Bloodbraid Challenger",
            "Thought Scour",
            "Mishra's Bauble",
            "Lotus Petal",
            "Brainstorm",
            "Lightning Bolt",
            "Faithless Looting",
            "Underworld Breach",
        ],
    );
    let boros = crate::eternal_catalog_v1::stress_deck(
        &[("Plains", 12), ("Mountain", 12)],
        [
            "Phlage, Titan of Fire's Fury",
            "Underworld Breach",
            "Lightning Bolt",
            "Chain Lightning",
            "Lotus Petal",
            "Mishra's Bauble",
            "Faithless Looting",
            "Thraben Inspector",
            "Swords to Plowshares",
        ],
    );
    [graveyard, titans, boros]
}

/// Escape cards and Underworld Breach play each other through public
/// sessions, both seats, with ranked and seeded random actions. Every game
/// follows its Escape casts to a natural end.
#[test]
fn public_escape_decks_finish_and_replay_through_public_sessions() {
    let [graveyard, titans, boros] = escape_decks();
    let mut events = EscapeEvents::default();
    let mut games = 0;
    for (pair, seed) in [
        ([graveyard.clone(), titans.clone()], 0x4553_4300_u64),
        ([titans, boros.clone()], 0x4553_4310),
        ([boros, graveyard], 0x4553_4320),
    ] {
        for flip in [false, true] {
            let decks = if flip {
                [pair[1].clone(), pair[0].clone()]
            } else {
                pair.clone()
            };
            let seed = seed + u64::from(flip);
            for policy in [
                Policy::Ranked,
                Policy::Random(seed),
                Policy::Random(seed + 16),
                Policy::Random(seed + 32),
            ] {
                let played = play_observed(decks.clone(), None, seed, policy, &mut |d, a| {
                    events.observe(d, a)
                });
                let replay = play_observed(decks.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
                games += 1;
            }
        }
    }
    println!(
        "{games} games: route choices {}, Breach picks {}, printed picks {}, \
         card-type picks {}, sources {:?}",
        events.route_choices,
        events.breach_picks,
        events.printed_picks,
        events.card_type_picks,
        events.sources
    );
    // A route choice needs Breach out while a printed escape is payable both
    // ways, which these games may not reach; the hand-built Nethergoyf test
    // covers it, and every route choice met here is checked above.
    assert!(events.breach_picks > 0, "no Breach escape exile was chosen");
    assert!(
        events.printed_picks > 0,
        "no printed escape exile was chosen"
    );
    assert!(
        events.card_type_picks > 0,
        "no Nethergoyf escape exile was chosen"
    );
}

/// Schema 4 has no staged Escape record: the same decks keep their historical
/// refusal there, and no reply carries the schema 5 field.
#[test]
fn public_escape_schema_4_keeps_the_refusal_and_its_bytes() {
    use mtg_kernel::limited_session_v1::LimitedJsonlServerV1;
    let [graveyard, titans, _] = escape_decks();
    let mut refusals = 0;
    for seed in 0..12_u64 {
        let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
        let mut request = serde_json::json!({"schema_version": 4, "request_type": "reset",
            "request_id": "reset", "decks": [graveyard, titans], "episode_id": seed,
            "env_seed": 0x4553_3400 + seed, "max_physical_decisions": 65536,
            "max_policy_steps": 20000})
        .to_string();
        let mut random = 0x4553_3400 + seed;
        for index in 0.. {
            let raw = server.handle_line(&request);
            assert!(!raw.contains("staged_escape"), "seed={seed}: {raw}");
            let reply: Value = serde_json::from_str(&raw).unwrap();
            assert_ne!(reply["response_type"], "error", "seed={seed}: {reply}");
            if reply["response_type"] == "terminal" {
                let reason = reply["terminal"]["terminal_reason"].as_str().unwrap();
                if reason.contains("staged Escape") {
                    assert_eq!(
                        reason,
                        concat!(
                            "fail_closed:observation:policy schema v5 cannot represent a staged ",
                            "Escape graveyard-exile prefix; a versioned object-cost successor is ",
                            "required"
                        )
                    );
                    refusals += 1;
                }
                break;
            }
            let decision = &reply["decision"];
            let actions = decision["legal_actions"].as_array().unwrap();
            let pick = &actions[(crate::eternal_catalog_v1::splitmix64(&mut random)
                % actions.len() as u64) as usize];
            request = serde_json::json!({"schema_version": 4, "request_type": "step",
                "request_id": format!("step-{index}"), "episode_id": decision["episode_id"],
                "expected_step": decision["step"], "selected_index": pick["selected_index"],
                "selected_action_id": pick["stable_id"]})
            .to_string();
        }
    }
    println!("schema 4 Escape refusals: {refusals} of 12 games");
    assert!(refusals > 0, "no schema 4 game staged an Escape cast");
}

/// The nonland cards of the Vintage Cube list the census plays.
fn cube_nonland_cards() -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../data/cube_v1/mtgo_vintage_cube_20261007.json");
    let cube: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    cube["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|card| card["name"].as_str().unwrap().to_owned())
        .filter(|name| !def(name).is_land)
        .collect()
}

/// A main phase where P0 controls Underworld Breach with `card` in its
/// graveyard among varied fuel, plenty of mana, permanents to sacrifice,
/// tap or return, and cards in hand; P1 has creatures, an artifact and an
/// enchantment to target. With `opposing_spell`, P1's Lightning Bolt is on
/// the stack.
fn breach_board(card: &str, opposing_spell: bool) -> (GameState, ObjectId) {
    let mut state = plains();
    mtg_kernel::rl::enable_public_shroud_v1(&mut state);
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    for name in [
        "Llanowar Elves",
        "Thraben Inspector",
        "Mishra's Bauble",
        "Island",
        "Forest",
        "Swamp",
        "Mountain",
        "Plains",
    ] {
        put(&mut state, P0, name, Zone::Battlefield);
    }
    for name in [
        "Island",
        "Lightning Bolt",
        "Counterspell",
        "Forest",
        "Swamp",
    ] {
        put(&mut state, P0, name, Zone::Hand);
    }
    let spell = put(&mut state, P0, card, Zone::Graveyard);
    for name in [
        "Island",
        "Lightning Bolt",
        "Ponder",
        "Mishra's Bauble",
        "Llanowar Elves",
        "Forest",
        "Brainstorm",
        "Swamp",
        "Seal of Fire",
    ] {
        put(&mut state, P0, name, Zone::Graveyard);
    }
    for name in [
        "Thraben Inspector",
        "Llanowar Elves",
        "Mishra's Bauble",
        "Seal of Fire",
    ] {
        put(&mut state, P1, name, Zone::Battlefield);
    }
    for name in ["Island", "Lightning Bolt"] {
        put(&mut state, P1, name, Zone::Hand);
    }
    if opposing_spell {
        opponent_bolts(&mut state, Target::Player(P0));
    }
    let colors = [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ];
    let colored: Vec<ManaColor> = colors.iter().cycle().take(20).copied().collect();
    pool(&mut state, P0, &colored, 8);
    (state, spell)
}

/// What one Breach-route cast through its staged payment showed.
#[derive(Default)]
struct SweepOutcome {
    /// Public observations refused or malformed during the cast.
    failures: Vec<String>,
    /// Casts reversed after at least one staged choice.
    aborts: Vec<String>,
    /// Casts reversed before any staged choice (an offered cast with no
    /// payable form).
    silent_reversals: Vec<String>,
    /// Casts that reached the stack.
    casts: usize,
    /// Staged decisions observed, by record stage.
    stages: std::collections::BTreeMap<String, usize>,
}

/// Whether `card` is expected to be offered from P0's graveyard on
/// `breach_board(card, opposing_spell)`.
///
/// Timing and stack-only target requirements are derived from `CardDef`.
/// Cards that are not offered on this board are listed by name with their
/// reason rather than skipped silently:
/// - `Emry, Lurker of the Loch`: an engine limitation, not a rule. It carries a
///   spell-local `generic_cost_reduction`, and the engine excludes such
///   reducers from `CastMethodV4::Escape`, although the reduction could
///   legally apply to an escape cost. Recorded as a follow-up.
/// - `Stern Scolding`: targets only a creature spell on the stack with power
///   or toughness 2 or less; the sweep's opposing spell is Lightning Bolt (an
///   Instant), so no legal target exists in either board state.
fn expected_breach_offered(card: &str, opposing_spell: bool) -> bool {
    use mtg_kernel::standard_cards_v1::StandardTargetV1;
    let definition = def(card);
    if opposing_spell
        && !definition.types.contains(&CardType::Instant)
        && !definition.keywords.has(Keywords::FLASH)
    {
        return false;
    }
    if card == "Emry, Lurker of the Loch" {
        assert!(definition.generic_cost_reduction.is_some());
        return false;
    }
    if card == "Stern Scolding" {
        assert_eq!(
            definition.target_spec,
            TargetSpec::StandardV1(StandardTargetV1::CreatureSpellPowerOrToughnessAtMostTwoV1)
        );
        return false;
    }
    let needs_stack_spell = definition.mode2.is_none()
        && definition.kicked_target_spec.is_none()
        && matches!(
            definition.target_spec,
            TargetSpec::AnySpellOnStack | TargetSpec::NoncreatureSpellOnStack
        );
    if needs_stack_spell && !opposing_spell {
        return false;
    }
    true
}

/// Casts `card` from P0's graveyard and answers every staged choice with
/// `policy` (`None` takes the first legal action, `Some(seed)` a seeded
/// random one), observing P0's schema 5 view at each choice.
fn sweep_breach_cast(
    card: &str,
    opposing_spell: bool,
    policy: Option<u64>,
    out: &mut SweepOutcome,
) {
    use mtg_kernel::surface::{SurfaceAction, SurfaceDecision};
    let expected_offered = expected_breach_offered(card, opposing_spell);
    let label = format!("{card} (stack={opposing_spell}, policy={policy:?})");
    let (mut state, spell) = breach_board(card, opposing_spell);
    let decision = engine::advance_until_decision(&mut state);
    if let Decision::Halted { .. } = decision {
        out.failures
            .push(format!("{label}: halted before cast {decision:?}"));
        return;
    }
    let castable = matches!(
        decision,
        Decision::CastSpellOrPass {
            ref castable_spells,
            ..
        } if castable_spells.contains(&spell)
    );
    if castable != expected_offered {
        out.failures.push(format!(
            "{label}: offered={castable}, expected={expected_offered}"
        ));
        return;
    }
    if !castable {
        return;
    }
    if let Err(error) = engine::step(&mut state, Action::CastSpell(spell)) {
        out.failures.push(format!("{label}: cast refused: {error}"));
        return;
    }
    let mut random = policy.unwrap_or(0);
    let mut choices = 0;
    for _ in 0..96 {
        let decision = engine::advance_until_decision(&mut state);
        if let Decision::Halted { .. } = decision {
            out.failures.push(format!("{label}: halted {decision:?}"));
            return;
        }
        if state.engine.pending_cast.is_none() {
            break;
        }
        let observation = match public_observation(&state) {
            Ok(observation) => observation,
            Err(error) => {
                out.failures
                    .push(format!("{label}: {error} at {decision:?}"));
                return;
            }
        };
        let staged = &observation["projection"]["staged_escape"];
        let escaping = state.engine.pending_cast.as_ref().is_some_and(|p| {
            p.source_contract.cast_method == mtg_kernel::state::CastMethodV4::Escape
        });
        match staged["stage"]["kind"].as_str() {
            Some(kind) => *out.stages.entry(kind.to_owned()).or_default() += 1,
            // A route menu whose provisional first route is not an escape
            // keeps its unrecorded bytes.
            None if !escaping => {
                *out.stages
                    .entry("unrecorded_route_menu".into())
                    .or_default() += 1
            }
            None => {
                out.failures
                    .push(format!("{label}: no staged_escape record at {decision:?}"));
                return;
            }
        }
        if let Some(groups) = staged["stage"]["cost_groups"].as_array() {
            if groups.iter().any(|group| group["count"] == 0) {
                out.failures
                    .push(format!("{label}: a group owing nothing at {decision:?}"));
                return;
            }
        }
        if let Some(options) = staged["stage"]["x_options"].as_array() {
            let Decision::ChooseEffectOption { option_count, .. } = decision else {
                out.failures.push(format!(
                    "{label}: x_options outside an X choice {decision:?}"
                ));
                return;
            };
            if options.len() != usize::from(option_count) {
                out.failures.push(format!(
                    "{label}: {} x_options for {option_count}",
                    options.len()
                ));
                return;
            }
            *out.stages.entry("x_choice".into()).or_default() += 1;
        }
        let rows = match mtg_kernel::rl::legal_action_candidates_v1(
            &SurfaceDecision::Decision(decision.clone()),
            &state,
        ) {
            Ok(rows) if !rows.is_empty() => rows,
            other => {
                out.failures.push(format!(
                    "{label}: no legal actions {other:?} at {decision:?}"
                ));
                return;
            }
        };
        let index = match policy {
            None => 0,
            Some(_) => {
                (crate::eternal_catalog_v1::splitmix64(&mut random) % rows.len() as u64) as usize
            }
        };
        let SurfaceAction::Action(action) = rows[index].surface_action.clone() else {
            out.failures
                .push(format!("{label}: non-engine action at {decision:?}"));
            return;
        };
        if let Err(error) = engine::step(&mut state, action.clone()) {
            out.failures
                .push(format!("{label}: offered {action:?} refused: {error}"));
            return;
        }
        choices += 1;
    }
    if state.engine.pending_cast.is_some() {
        out.failures.push(format!("{label}: cast still pending"));
    } else if zone(&state, spell) == Zone::Stack {
        out.casts += 1;
        // After payment the record is gone.
        let observation = public_observation(&state).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert!(
            !observation.to_string().contains("staged_escape"),
            "{label}"
        );
    } else if choices > 0 {
        out.aborts.push(label);
    } else {
        out.silent_reversals.push(label);
    }
}

/// Every nonland Vintage Cube card cast through Underworld Breach, with first,
/// random and opposing-stack choices: each staged choice has a schema 5
/// public record, every offered action is accepted, and no cast is reversed
/// after a staged choice.
#[test]
fn public_escape_every_cube_card_through_breach_stages_its_cost() {
    let cards = cube_nonland_cards();
    let mut out = SweepOutcome::default();
    let mut expected_casts = 0;
    for card in &cards {
        for opposing_spell in [false, true] {
            for policy in [None, Some(1), Some(2), Some(3)] {
                if expected_breach_offered(card, opposing_spell) {
                    expected_casts += 1;
                }
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut one = SweepOutcome::default();
                    sweep_breach_cast(card, opposing_spell, policy, &mut one);
                    one
                }));
                match caught {
                    Ok(one) => {
                        out.failures.extend(one.failures);
                        out.aborts.extend(one.aborts);
                        out.silent_reversals.extend(one.silent_reversals);
                        out.casts += one.casts;
                        for (kind, count) in one.stages {
                            *out.stages.entry(kind).or_default() += count;
                        }
                    }
                    Err(_) => out.failures.push(format!(
                        "{card} (stack={opposing_spell}, policy={policy:?}): panicked"
                    )),
                }
            }
        }
    }
    println!(
        "{} cube cards, {}/{} Breach casts reached the stack, stages {:?}",
        cards.len(),
        out.casts,
        expected_casts,
        out.stages
    );
    for line in &out.aborts {
        println!("abort: {line}");
    }
    for line in &out.silent_reversals {
        println!("silent reversal: {line}");
    }
    for line in &out.failures {
        println!("failure: {line}");
    }
    assert!(out.failures.is_empty(), "{} failures", out.failures.len());
    assert!(out.aborts.is_empty(), "{} aborts", out.aborts.len());
    assert!(
        out.silent_reversals.is_empty(),
        "{} offered casts were reversed without a choice",
        out.silent_reversals.len()
    );
    assert_eq!(
        out.casts, expected_casts,
        "expected {expected_casts} casts, got {}",
        out.casts
    );
}

/// `player`'s schema 5 public policy observation of `state`, or the refusal.
fn public_observation_for(state: &GameState, player: PlayerId) -> Result<Value, String> {
    let mut state = state.clone();
    mtg_kernel::rl::enable_public_shroud_v1(&mut state);
    let surface = mtg_kernel::policy_surface_v5::PolicySurfaceV5::new();
    mtg_kernel::rl::observe_policy_v5(&state, &surface, player, 0, 0, 0, 1)
        .map(|observation| serde_json::to_value(observation).unwrap())
        .map_err(|error| error.0)
}

const STAGED_ESCAPE_REFUSAL: &str = concat!(
    "policy schema v5 cannot represent a staged Escape graveyard-exile prefix; ",
    "a versioned object-cost successor is required"
);

/// Every card reference in `value` with this arena id.
fn references_to(value: &Value, arena_id: u32) -> Vec<Value> {
    let mut out = Vec::new();
    match value {
        Value::Object(map) => {
            if map.get("arena_id").and_then(Value::as_u64) == Some(u64::from(arena_id)) {
                out.push(value.clone());
            }
            for child in map.values() {
                out.extend(references_to(child, arena_id));
            }
        }
        Value::Array(items) => {
            for child in items {
                out.extend(references_to(child, arena_id));
            }
        }
        _ => {}
    }
    out
}

/// Bitter Triumph under Breach: while the caster still chooses between
/// discarding a card and paying 3 life, the record owes no discard; each
/// choice then fixes its own groups. The other seat is refused throughout,
/// including after the private discard pick, and sees the exiled fuel once
/// the cost is paid.
#[test]
fn public_escape_waits_for_bitter_triumphs_cost_choice_and_serves_only_the_caster() {
    use engine::CastMode;
    for discard_chosen in [true, false] {
        let mut state = plains();
        put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        let source = put(&mut state, P0, "Bitter Triumph", Zone::Graveyard);
        let spare = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        let target = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
        let fuel = ["Plains", "Island", "Forest", "Swamp"]
            .map(|name| put(&mut state, P0, name, Zone::Graveyard));
        pool(&mut state, P0, &[ManaColor::B], 1);
        cast(&mut state, source, &[Target::Object(target)]);
        assert!(matches!(
            next(&mut state),
            Decision::ChooseCastMode { options, .. }
                if options == [CastMode::Normal, CastMode::Alternative]
        ));
        let stage = staged_escape(&state)["stage"].clone();
        assert_eq!(stage["kind"], "paying");
        assert_eq!(stage["costs_chosen"], false);
        assert_eq!(
            stage["cost_groups"],
            serde_json::json!([{"kind": "ExileFromGraveyard", "escape": true, "count": 3,
                "selected": []}])
        );
        assert_eq!(
            public_observation_for(&state, P1).unwrap_err(),
            STAGED_ESCAPE_REFUSAL
        );
        act(
            &mut state,
            Action::ChooseCastMode(if discard_chosen {
                CastMode::Normal
            } else {
                CastMode::Alternative
            }),
        );
        let mut exiled = Vec::new();
        let mut discard_seen = false;
        for _ in 0..12 {
            let decision = next(&mut state);
            if state.engine.pending_cast.is_none() {
                break;
            }
            let stage = staged_escape(&state)["stage"].clone();
            assert_eq!(stage["costs_chosen"], true);
            let groups = stage["cost_groups"].as_array().unwrap().clone();
            let discard = groups.iter().find(|group| group["escape"] == false);
            assert_eq!(discard.is_some(), discard_chosen, "{stage}");
            if let Some(discard) = discard {
                assert_eq!(discard["kind"], "DiscardCards");
                assert_eq!(discard["count"], 1);
            }
            assert_eq!(
                public_observation_for(&state, P1).unwrap_err(),
                STAGED_ESCAPE_REFUSAL
            );
            let action = match decision {
                Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
                Decision::ChooseCostTargets {
                    cost_kind: CostKind::DiscardCards,
                    candidates,
                    ..
                } => {
                    assert!(candidates.contains(&spare));
                    discard_seen = true;
                    Action::ChooseCostTarget(spare)
                }
                Decision::ChooseCostTargets {
                    cost_kind: CostKind::ExileFromGraveyard,
                    candidates,
                    ..
                } => {
                    let card = *fuel.iter().find(|id| candidates.contains(id)).unwrap();
                    exiled.push(card);
                    Action::ChooseCostTarget(card)
                }
                other => panic!("unexpected Bitter Triumph decision {other:?}"),
            };
            act(&mut state, action);
            if discard_seen && state.engine.pending_cast.is_some() {
                // The private discard pick is staged: still the caster's only.
                let staged = staged_escape(&state);
                let discard = staged["stage"]["cost_groups"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|group| group["escape"] == false)
                    .unwrap()
                    .clone();
                assert_eq!(selected_ids(&discard), ids(&[spare]));
                assert_eq!(
                    public_observation_for(&state, P1).unwrap_err(),
                    STAGED_ESCAPE_REFUSAL
                );
            }
        }
        assert_eq!(discard_seen, discard_chosen);
        assert!(state.engine.pending_cast.is_none());
        assert_eq!(zone(&state, spare) == Zone::Hand, !discard_chosen);
        // Paid: the other seat's view shows the exiled fuel in exile.
        let seen = public_observation_for(&state, P1).unwrap();
        assert!(!seen.to_string().contains("staged_escape"));
        let exiled: Vec<ObjectId> = fuel
            .iter()
            .copied()
            .filter(|&id| zone(&state, id) == Zone::Exile)
            .collect();
        assert_eq!(exiled.len(), 3);
        for card in exiled {
            assert!(references_to(&seen, card.0)
                .iter()
                .any(|reference| reference["zone"] == "Exile"));
        }
    }
}

/// The other seat is refused during the caster's route menu too.
#[test]
fn public_escape_route_menu_is_refused_to_the_other_seat() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let goyf = put(&mut state, P0, "Nethergoyf", Zone::Graveyard);
    for name in [
        "Island",
        "Lightning Bolt",
        "Ponder",
        "Mishra's Bauble",
        "Forest",
    ] {
        put(&mut state, P0, name, Zone::Graveyard);
    }
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, goyf, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption { .. }
    ));
    assert_eq!(staged_escape(&state)["stage"]["kind"], "choosing_route");
    assert_eq!(
        public_observation_for(&state, P1).unwrap_err(),
        STAGED_ESCAPE_REFUSAL
    );
}

/// Sickening Dreams under Breach announces X (here 2 or 3) before its
/// discard X and escape exiles: the record lists the X each option
/// announces and owes only the escape exile until X is chosen.
#[test]
fn public_escape_lists_the_x_options_before_a_discard_x_cost() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let spell = put(&mut state, P0, "Sickening Dreams", Zone::Graveyard);
    put(&mut state, P0, "Swamp", Zone::Graveyard);
    let first = put(&mut state, P0, "Forest", Zone::Hand);
    put(&mut state, P0, "Fiery Temper", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    let second = put(&mut state, P0, "Island", Zone::Hand);
    cast(&mut state, spell, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 2,
            ..
        }
    ));
    let stage = staged_escape(&state)["stage"].clone();
    assert_eq!(stage["costs_chosen"], false);
    assert_eq!(stage["x_options"], serde_json::json!([2, 3]));
    assert_eq!(stage["cost_groups"].as_array().unwrap().len(), 1);
    act(&mut state, Action::ChooseEffectOption(0));
    payment_pick(&mut state, CostKind::DiscardCards, first);
    let stage = staged_escape(&state)["stage"].clone();
    assert_eq!(stage["costs_chosen"], true);
    assert!(stage.get("x_options").is_none());
    let groups = stage["cost_groups"].as_array().unwrap();
    assert_eq!(groups[0]["kind"], "DiscardCards");
    assert_eq!(groups[0]["count"], 2);
    assert_eq!(selected_ids(&groups[0]), ids(&[first]));
    assert_eq!(groups[1]["escape"], true);
    assert_eq!(groups[1]["count"], 3);
    payment_pick(&mut state, CostKind::DiscardCards, second);
}

/// A Breach-cast X spell chooses X after its exiles; the record lists the X
/// each option announces (Walking Ballista).
#[test]
fn public_escape_lists_the_x_options_of_a_breach_cast_x_spell() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let ballista = put(&mut state, P0, "Walking Ballista", Zone::Graveyard);
    let fuel = ["Island", "Forest", "Ponder", "Swamp"]
        .map(|name| put(&mut state, P0, name, Zone::Graveyard));
    pool(&mut state, P0, &[], 4);
    cast(&mut state, ballista, &[]);
    for &card in &fuel[..3] {
        payment_pick(&mut state, CostKind::ExileFromGraveyard, card);
    }
    let Decision::ChooseEffectOption { option_count, .. } = next(&mut state) else {
        panic!("missing the X choice")
    };
    let stage = staged_escape(&state)["stage"].clone();
    assert_eq!(stage["costs_chosen"], true);
    assert_eq!(selected_ids(&stage["cost_groups"][0]), ids(&fuel[..3]));
    let options: Vec<u64> = stage["x_options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap())
        .collect();
    assert_eq!(options.len(), usize::from(option_count));
    assert_eq!(options, (0..=2).collect::<Vec<u64>>());
    act(&mut state, Action::ChooseEffectOption(option_count - 1));
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    settle_first(&mut state);
    assert_eq!(zone(&state, ballista), Zone::Battlefield);
}

/// A route menu whose provisional first route is not an escape (Faithless
/// Looting under Breach: printed flashback, then Breach) was never refused
/// and keeps its bytes: no record there; the record appears once Breach is
/// chosen.
#[test]
fn public_escape_keeps_a_flashback_first_route_menu_unrecorded() {
    use engine::sphinx_lore_cast_v1::RouteV1;
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let looting = put(&mut state, P0, "Faithless Looting", Zone::Graveyard);
    for name in ["Island", "Forest", "Ponder", "Swamp"] {
        put(&mut state, P0, name, Zone::Graveyard);
    }
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, looting, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption {
            option_count: 2,
            ..
        }
    ));
    let options = state
        .engine
        .event_history
        .iter()
        .rev()
        .find_map(|event| match event {
            mtg_kernel::event::CommittedEvent::SphinxLoreCastOptionsV1 { options } => {
                Some(options.routes.clone())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(options, [RouteV1::PrintedFlashback, RouteV1::Breach]);
    // The legacy observation already accepted this state.
    let surface = mtg_kernel::policy_surface_v5::PolicySurfaceV5::new();
    mtg_kernel::rl::observe_policy_v5(&state, &surface, P0, 0, 0, 0, 1).unwrap();
    assert!(staged_escape(&state).is_null());
    act(&mut state, Action::ChooseEffectOption(1));
    next(&mut state);
    let staged = staged_escape(&state);
    assert_eq!(staged["stage"]["breach_escape"], true);
}

/// A staged Escape snapshot restores to the same record, hash and next
/// transition.
#[test]
fn public_escape_mid_payment_snapshot_restores_exactly() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let goyf = put(&mut state, P0, "Nethergoyf", Zone::Graveyard);
    let fuel = [
        "Island",
        "Lightning Bolt",
        "Ponder",
        "Mishra's Bauble",
        "Forest",
    ]
    .map(|name| put(&mut state, P0, name, Zone::Graveyard));
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, goyf, &[]);
    next(&mut state);
    act(&mut state, Action::ChooseEffectOption(1));
    payment_pick(&mut state, CostKind::ExileFromGraveyard, fuel[0]);
    let mut restored: GameState =
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    assert_eq!(restored.state_hash(), state.state_hash());
    assert_eq!(next(&mut restored), next(&mut state));
    assert_eq!(
        public_observation(&restored).unwrap(),
        public_observation(&state).unwrap()
    );
    for game in [&mut state, &mut restored] {
        payment_pick(game, CostKind::ExileFromGraveyard, fuel[1]);
    }
    assert_eq!(restored.state_hash(), state.state_hash());
    assert_eq!(
        public_observation(&restored).unwrap(),
        public_observation(&state).unwrap()
    );
}

/// Springheart Nantuko cast with Breach's escape is cast as its creature:
/// escape and bestow are both alternative costs.
#[test]
fn public_escape_casts_a_bestow_card_as_its_creature() {
    let mut state = plains();
    mtg_kernel::rl::enable_public_shroud_v1(&mut state);
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let nantuko = put(&mut state, P0, "Springheart Nantuko", Zone::Graveyard);
    let fuel = ["Island", "Forest", "Ponder", "Swamp"]
        .map(|name| put(&mut state, P0, name, Zone::Graveyard));
    pool(&mut state, P0, &[ManaColor::G], 4);
    cast(&mut state, nantuko, &[]);
    for &card in &fuel[..3] {
        let decision = next(&mut state);
        assert!(
            matches!(decision, Decision::ChooseCostTargets { .. }),
            "{decision:?}"
        );
        assert_eq!(staged_escape(&state)["stage"]["kind"], "paying");
        act(&mut state, Action::ChooseCostTarget(card));
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    let item = state.stack.last().unwrap();
    assert_eq!(item.source, nantuko);
    assert_eq!(item.mode_chosen, 0);
    assert!(public_observation(&state).is_ok());
    settle_first(&mut state);
    assert_eq!(zone(&state, nantuko), Zone::Battlefield);
    assert!(state.objects.get(nantuko).attachments.is_empty());
}

/// A forged staged Escape snapshot of Springheart Nantuko with `mode_chosen`
/// set to 1 (the bestow form) is rejected by `validate_pending_cast`,
/// matching the form menu.
#[test]
fn public_escape_rejects_forged_bestow_form_snapshot() {
    let mut state = plains();
    mtg_kernel::rl::enable_public_shroud_v1(&mut state);
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let host = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let nantuko = put(&mut state, P0, "Springheart Nantuko", Zone::Graveyard);
    for name in ["Island", "Forest", "Ponder", "Swamp"] {
        put(&mut state, P0, name, Zone::Graveyard);
    }
    pool(&mut state, P0, &[ManaColor::G], 4);
    cast(&mut state, nantuko, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::ChooseCostTargets {
            cost_kind: CostKind::ExileFromGraveyard,
            ..
        }
    ));
    assert_eq!(
        state.engine.pending_cast.as_ref().unwrap().mode_chosen,
        Some(0)
    );
    let mut forged: GameState =
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    forged.engine.pending_cast.as_mut().unwrap().mode_chosen = Some(1);
    let error = public_observation(&forged).unwrap_err();
    assert!(error.contains("first form"), "{error}");
    assert!(engine::step(&mut forged, Action::ChooseTarget(Target::Object(host))).is_err());
    assert!(matches!(
        engine::advance_until_decision(&mut forged),
        Decision::Halted { .. }
    ));
}

/// Payments staged outside the object prefix have no record: Vitu-Ghazi
/// Inspector's collect evidence (a Pauper card; no cube card reaches one
/// under Breach) refuses rather than drops its picks.
#[test]
fn public_escape_refuses_collect_evidence_it_does_not_record() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Vitu-Ghazi Inspector", Zone::Graveyard);
    for name in [
        "Island",
        "Forest",
        "Ponder",
        "Swamp",
        "Mulldrifter",
        "Exalted Angel",
    ] {
        put(&mut state, P0, name, Zone::Graveyard);
    }
    let colors = [ManaColor::W, ManaColor::G, ManaColor::U];
    pool(&mut state, P0, &colors.repeat(3), 6);
    cast(&mut state, inspector, &[]);
    let mut refused = false;
    for _ in 0..8 {
        let decision = next(&mut state);
        let Some(pending) = state.engine.pending_cast.as_ref() else {
            break;
        };
        if pending.optional_additional_cost_paid == Some(true) {
            let error = public_observation(&state).unwrap_err();
            assert!(error.contains("collect evidence"), "{error}");
            refused = true;
            break;
        }
        staged_escape(&state);
        let action = match decision {
            Decision::ChooseEffectOption { .. }
                if pending.optional_additional_cost_paid.is_none() =>
            {
                Action::ChooseEffectOption(1)
            }
            other => first_answer(other),
        };
        act(&mut state, action);
    }
    assert!(refused, "collect evidence was never chosen");
}

/// Casts `card` from P0's graveyard through Underworld Breach, targeting
/// P1's Thraben Inspector when asked and exiling three of four fuel cards,
/// checking the staged record at every choice. Returns the state with the
/// spell on the stack, the spell and the target.
fn breach_cast_first_form(
    card: &str,
    colored: &[ManaColor],
    generic: u8,
) -> (GameState, ObjectId, ObjectId) {
    let mut state = plains();
    mtg_kernel::rl::enable_public_shroud_v1(&mut state);
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let spell = put(&mut state, P0, card, Zone::Graveyard);
    let fuel = ["Island", "Forest", "Ponder", "Swamp"]
        .map(|name| put(&mut state, P0, name, Zone::Graveyard));
    let target = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, colored, generic);
    assert!(
        castable(&mut state, spell),
        "{card} is not offered through Breach"
    );
    act(&mut state, Action::CastSpell(spell));
    let mut exiled = 0;
    for _ in 0..16 {
        let decision = next(&mut state);
        if state.engine.pending_cast.is_none() {
            break;
        }
        let stage = staged_escape(&state)["stage"].clone();
        assert_eq!(stage["kind"], "paying", "{card}: {stage}");
        assert_eq!(stage["breach_escape"], true);
        let action = match decision {
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(target)) =>
            {
                Action::ChooseTarget(Target::Object(target))
            }
            Decision::ChooseCostTargets {
                cost_kind: CostKind::ExileFromGraveyard,
                candidates,
                ..
            } => {
                assert_eq!(selected_ids(&stage["cost_groups"][0]).len(), exiled);
                exiled += 1;
                Action::ChooseCostTarget(*fuel.iter().find(|id| candidates.contains(id)).unwrap())
            }
            other => first_answer(other),
        };
        act(&mut state, action);
    }
    assert!(state.engine.pending_cast.is_none(), "{card} still staged");
    assert_eq!(exiled, 3, "{card}");
    let item = state.stack.last().unwrap();
    assert_eq!(item.source, spell, "{card} was not cast");
    assert_eq!(item.mode_chosen, 0);
    assert_eq!(
        item.v4.cast_method,
        Some(mtg_kernel::state::CastMethodV4::Escape)
    );
    assert_eq!(
        fuel.iter()
            .filter(|&&id| zone(&state, id) == Zone::Exile)
            .count(),
        3
    );
    (state, spell, target)
}

/// Overload is an alternative cost, which cannot combine with escape: Damn
/// cast through Breach is the single-target spell.
#[test]
fn public_escape_casts_damn_without_overload() {
    let (mut state, spell, target) = breach_cast_first_form("Damn", &[ManaColor::B; 2], 2);
    settle_first(&mut state);
    assert_eq!(zone(&state, target), Zone::Graveyard);
    assert_eq!(zone(&state, spell), Zone::Graveyard);
}

/// An Adventure card cast through Breach is cast as its creature.
#[test]
fn public_escape_casts_bonecrusher_giant_as_its_creature() {
    let (mut state, spell, _) = breach_cast_first_form("Bonecrusher Giant", &[ManaColor::R], 4);
    settle_first(&mut state);
    assert_eq!(zone(&state, spell), Zone::Battlefield);
}

/// Legacy sessions (`!state.public_shroud_v1`, schema 4) keep the parent's
/// transition when casting an Adventure card through Underworld Breach:
/// Bonecrusher Giant is offered, reversed before any staged decision, and
/// P0 regains priority with no staged Escape and no observation refusal.
#[test]
fn public_escape_legacy_schema_4_reverses_bonecrusher_giant_through_breach() {
    let mut state = plains();
    assert!(!state.public_shroud_v1);
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let giant = put(&mut state, P0, "Bonecrusher Giant", Zone::Graveyard);
    let fuel = ["Island", "Forest", "Ponder", "Swamp"]
        .map(|name| put(&mut state, P0, name, Zone::Graveyard));
    put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 4);
    assert!(castable(&mut state, giant));
    act(&mut state, Action::CastSpell(giant));
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(zone(&state, giant), Zone::Graveyard);
    assert!(fuel.iter().all(|&id| zone(&state, id) == Zone::Graveyard));
    let surface = mtg_kernel::policy_surface_v5::PolicySurfaceV5::new();
    let legacy = mtg_kernel::rl::observe_policy_v5(&state, &surface, P0, 0, 0, 0, 1)
        .expect("legacy observation should accept the reversed cast");
    let bytes = serde_json::to_string(&legacy).unwrap();
    assert!(!bytes.contains("staged_escape"), "{bytes}");
}

/// A split card cast through Breach is cast as its first half.
#[test]
fn public_escape_casts_dead_gone_as_its_first_half() {
    let (mut state, spell, target) = breach_cast_first_form("Dead // Gone", &[ManaColor::R; 3], 3);
    settle_first(&mut state);
    assert_eq!(zone(&state, target), Zone::Graveyard);
    assert_eq!(zone(&state, spell), Zone::Graveyard);
}

/// Exact X options of cube X spells cast through Breach, offered after the
/// three exiles: Engineered Explosives and Mind Twist with three spare mana.
#[test]
fn public_escape_lists_exact_x_options_for_cube_x_spells() {
    for (card, colored) in [
        ("Engineered Explosives", &[][..]),
        ("Mind Twist", &[ManaColor::B][..]),
    ] {
        let mut state = plains();
        put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
        let spell = put(&mut state, P0, card, Zone::Graveyard);
        let fuel = ["Island", "Forest", "Ponder", "Swamp"]
            .map(|name| put(&mut state, P0, name, Zone::Graveyard));
        pool(&mut state, P0, colored, 3);
        assert!(castable(&mut state, spell), "{card}");
        act(&mut state, Action::CastSpell(spell));
        let mut seen = None;
        for _ in 0..12 {
            let decision = next(&mut state);
            if state.engine.pending_cast.is_none() {
                break;
            }
            let stage = staged_escape(&state)["stage"].clone();
            let action = match decision {
                Decision::ChooseCostTargets { candidates, .. } => Action::ChooseCostTarget(
                    *fuel.iter().find(|id| candidates.contains(id)).unwrap(),
                ),
                Decision::ChooseEffectOption { option_count, .. } => {
                    assert_eq!(stage["costs_chosen"], true, "{card}");
                    assert_eq!(selected_ids(&stage["cost_groups"][0]), ids(&fuel[..3]));
                    seen = Some(stage["x_options"].clone());
                    Action::ChooseEffectOption(option_count - 1)
                }
                Decision::ChooseTargets { legal_targets, .. } => {
                    assert!(stage.get("x_options").is_none());
                    Action::ChooseTarget(
                        *legal_targets
                            .iter()
                            .find(|t| **t == Target::Player(P1))
                            .unwrap_or(&legal_targets[0]),
                    )
                }
                other => first_answer(other),
            };
            act(&mut state, action);
        }
        assert_eq!(seen, Some(serde_json::json!([0, 1, 2, 3])), "{card}");
        assert_eq!(state.stack.last().unwrap().v4.x_value, 3, "{card}");
    }
}

/// A restored earlier snapshot that elected bargain before the ordered
/// payment existed stages the exiles and the sacrifice in separate fields;
/// the record has no place for that sacrifice and refuses rather than drop
/// it.
#[test]
fn public_escape_refuses_an_elected_bargain_it_does_not_record() {
    let mut state = plains();
    put(&mut state, P0, "Underworld Breach", Zone::Battlefield);
    let spell = put(&mut state, P0, "Beseech the Mirror", Zone::Graveyard);
    let fuel = ["Island", "Swamp", "Forest", "Plains"]
        .map(|name| put(&mut state, P0, name, Zone::Graveyard));
    pool(&mut state, P0, &[ManaColor::B; 3], 1);
    act(&mut state, Action::CastSpell(spell));
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption { .. }
    ));
    act(&mut state, Action::ChooseEffectOption(1));
    next(&mut state);
    let pending = state.engine.pending_cast.as_mut().unwrap();
    assert_eq!(pending.optional_additional_cost_paid, Some(true));
    // The earlier shape: exiles staged with no payment order, the bargain
    // permanent still to choose.
    pending.graveyard_cost_first_v1 = None;
    pending.sacrifice_chosen = fuel[..3].to_vec();
    let error = public_observation(&state).unwrap_err();
    assert!(error.contains("optional additional cost"), "{error}");
}
