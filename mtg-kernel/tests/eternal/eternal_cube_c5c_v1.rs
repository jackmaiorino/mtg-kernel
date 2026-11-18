//! Vintage Cube wave C5c: native rules and public-session tests for the
//! reanimation spells and enchantments, a sweeper and a land.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::collections::BTreeMap;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C5C_CARDS: [&str; 6] = [
    "Corpse Dance",
    "Necromancy",
    "Recurring Nightmare",
    "Aang's Iceberg",
    "Shifting Woodland",
    "Sunfall",
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
        0x4355_4245_0000 + 0x3563,
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
fn c5c_cards_are_full() {
    for name in C5C_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c5c_cards_have_their_printed_characteristics() {
    let colored = Pip::Colored;
    let (w, b) = (ManaColor::W, ManaColor::B);
    for (name, card_type, generic, pips, color) in [
        ("Corpse Dance", CardType::Instant, 2, vec![colored(b)], b),
        ("Necromancy", CardType::Enchantment, 2, vec![colored(b)], b),
        (
            "Recurring Nightmare",
            CardType::Enchantment,
            2,
            vec![colored(b)],
            b,
        ),
        (
            "Aang's Iceberg",
            CardType::Enchantment,
            2,
            vec![colored(w)],
            w,
        ),
        (
            "Sunfall",
            CardType::Sorcery,
            3,
            vec![colored(w), colored(w)],
            w,
        ),
    ] {
        let card = def(name);
        assert_eq!(card.types, &[card_type], "{name}");
        assert_eq!(card.cost.generic, generic, "{name}");
        assert_eq!(card.cost.x_count, 0, "{name}");
        assert_eq!(card.cost.pips.len(), pips.len(), "{name}");
        for pip in &pips {
            assert!(card.cost.pips.contains(pip), "{name} lacks {pip:?}");
        }
        assert_eq!(
            usize::from(card.mana_value),
            usize::from(generic) + pips.len(),
            "{name}"
        );
        assert_eq!(card.colors.len(), 1, "{name}");
        assert!(card.colors.contains(&color), "{name} is {color:?}");
        assert!(card.subtypes.is_empty(), "{name}: Necromancy is no Aura");
        assert!(card.supertypes.is_empty(), "{name}");
        assert!(card.power.is_none() && card.toughness.is_none(), "{name}");
        assert!(!card.is_land && !card.is_token, "{name}");
    }

    // Shifting Woodland: a land with no cost, color or subtype, and its one
    // activated ability (the delirium copy).
    let woodland = def("Shifting Woodland");
    assert_eq!(woodland.types, &[CardType::Land]);
    assert!(woodland.is_land && !woodland.is_token);
    assert!(woodland.colors.is_empty());
    assert!(woodland.subtypes.is_empty(), "it is not a Forest itself");
    assert_eq!(woodland.mana_value, 0);
    assert_eq!(woodland.activated_abilities.len(), 1);

    // Flash: Necromancy is cast as though it had flash; the Iceberg has it.
    for (name, flash) in [
        ("Aang's Iceberg", true),
        ("Necromancy", true),
        ("Corpse Dance", false),
        ("Recurring Nightmare", false),
        ("Sunfall", false),
        ("Shifting Woodland", false),
    ] {
        assert_eq!(def(name).keywords.has(Keywords::FLASH), flash, "{name}");
        assert!(!def(name).keywords.has(Keywords::HASTE), "{name}");
    }

    // Buyback {2} is the one optional additional cost.
    let dance = def("Corpse Dance");
    let kicker = dance.kicker_cost.as_ref().expect("buyback {2}");
    assert_eq!((kicker.generic, kicker.pips.len()), (2, 0));
    for name in [
        "Necromancy",
        "Recurring Nightmare",
        "Aang's Iceberg",
        "Sunfall",
    ] {
        assert!(def(name).kicker_cost.is_none(), "{name}");
    }

    // Recurring Nightmare's one ability, and the Iceberg's four waterbend
    // payments (none to three permanents tapped to help).
    assert_eq!(def("Recurring Nightmare").activated_abilities.len(), 1);
    assert!(def("Recurring Nightmare").activated_abilities[0].sorcery_speed_only);
    assert_eq!(def("Aang's Iceberg").activated_abilities.len(), 4);
}

// ---- Helpers of this wave --------------------------------------------------------

/// Settles, answering a buyback (kicker) offer with `kicked`.
fn settle_kicked(state: &mut GameState, kicked: bool) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseKicker { .. } => Action::ChooseKicker(kicked),
        other => first_answer(other),
    });
}

/// Settles, declining every "up to one" target.
fn settle_declining_targets(state: &mut GameState) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
}

/// Finishes the pending activation: each cost choice takes the first of
/// `prefer` that is offered (else the first candidate), and the ability is on
/// the stack with its costs paid once priority comes back.
fn finish_activation(state: &mut GameState, prefer: &[ObjectId]) {
    for _ in 0..20 {
        match next(state) {
            Decision::ChooseCostTargets { candidates, .. } => {
                let pick = prefer
                    .iter()
                    .copied()
                    .find(|object| candidates.contains(object))
                    .unwrap_or(candidates[0]);
                act(state, Action::ChooseCostTarget(pick));
            }
            Decision::CastSpellOrPass { .. } => return,
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    panic!("the activation never finished");
}

fn graveyard_of(state: &GameState, player: PlayerId) -> &[ObjectId] {
    &state.players[player.index()].graveyard
}

fn gone_from_the_battlefield(state: &GameState, object: ObjectId) -> bool {
    state
        .objects
        .try_get(object)
        .is_none_or(|live| live.zone != Zone::Battlefield)
}

/// The targets on offer for `source`'s ability 0.
fn ability_targets(state: &GameState, source: ObjectId) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(activatable(&mut probe, source, 0));
    act(&mut probe, Action::ActivateAbility(source, 0));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

// ---- Sunfall -------------------------------------------------------------------------

#[test]
fn sunfall_exiles_every_creature_and_incubates_for_the_number_exiled() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    // An indestructible creature is exiled all the same.
    let angel = put(&mut state, P1, "Darksteel Angel", Zone::Battlefield);
    let servo = token(&mut state, P1, "Servo Token");
    let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
    let sunfall = put(&mut state, P0, "Sunfall", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 3);
    cast(&mut state, sunfall, &[]);
    settled(&mut state);

    for creature in [elves, pup, angel] {
        assert_eq!(zone(&state, creature), Zone::Exile);
    }
    assert!(
        gone_from_the_battlefield(&state, servo),
        "a token counts too"
    );
    assert_eq!(zone(&state, stone), Zone::Battlefield, "not a creature");
    assert_eq!(graveyard_of(&state, P0), &[sunfall], "nothing died");
    assert!(graveyard_of(&state, P1).is_empty(), "nothing died");
    // Four creatures were exiled: an Incubator with four counters, under the
    // controller of Sunfall.
    let incubators = named(&state, P0, "Incubator Token");
    assert_eq!(incubators.len(), 1);
    assert_eq!(state.objects.get(incubators[0]).counters.plus1_plus1, 4);
    assert!(named(&state, P1, "Incubator Token").is_empty());
}

#[test]
fn sunfall_with_no_creatures_still_incubates_an_empty_incubator() {
    let mut state = plains();
    let sunfall = put(&mut state, P0, "Sunfall", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 3);
    cast(&mut state, sunfall, &[]);
    settled(&mut state);
    let incubators = named(&state, P0, "Incubator Token");
    assert_eq!(incubators.len(), 1, "X is 0, and the Incubator still comes");
    let incubator = incubators[0];
    assert_eq!(state.objects.get(incubator).counters.plus1_plus1, 0);
    assert!(!engine::object_has_type(
        &state,
        incubator,
        CardType::Creature
    ));

    // It can transform into a 0/0 artifact creature, which dies.
    pool(&mut state, P0, &[], 2);
    activate(&mut state, incubator, 0, &[]);
    settled(&mut state);
    assert!(gone_from_the_battlefield(&state, incubator));
}

#[test]
fn sunfall_leaves_noncreature_permanents_and_lands_alone() {
    let mut state = plains();
    let land = put(&mut state, P0, "Plains", Zone::Battlefield);
    let stone = put(&mut state, P1, "Mind Stone", Zone::Battlefield);
    let nightmare = put(&mut state, P1, "Recurring Nightmare", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let sunfall = put(&mut state, P0, "Sunfall", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 3);
    cast(&mut state, sunfall, &[]);
    settled(&mut state);
    for kept in [land, stone, nightmare] {
        assert_eq!(zone(&state, kept), Zone::Battlefield);
    }
    assert_eq!(zone(&state, elves), Zone::Exile);
    let incubators = named(&state, P0, "Incubator Token");
    assert_eq!(state.objects.get(incubators[0]).counters.plus1_plus1, 1);
}

// ---- Corpse Dance --------------------------------------------------------------------

#[test]
fn corpse_dance_returns_the_top_creature_card_with_haste_and_exiles_it_at_the_end_step() {
    let mut state = plains();
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    put(&mut state, P0, "Island", Zone::Graveyard);
    let dance = put(&mut state, P0, "Corpse Dance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, dance, &[]);
    settled(&mut state);
    assert_eq!(
        zone(&state, elves),
        Zone::Battlefield,
        "the top creature card"
    );
    assert_eq!(state.objects.get(elves).controller, P0);
    assert_eq!(zone(&state, angler), Zone::Graveyard, "only the top one");
    assert!(keyword(&state, elves, Keywords::HASTE));
    assert_eq!(zone(&state, dance), Zone::Graveyard, "no buyback was paid");

    // Exiled at the beginning of the next end step, not before.
    pass_until(&mut state, |state| state.step == Step::End);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(zone(&state, elves), Zone::Exile);
    assert_eq!(zone(&state, angler), Zone::Graveyard);
}

#[test]
fn corpse_dance_haste_lets_the_creature_attack_the_turn_it_returns() {
    let mut state = plains();
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Graveyard);
    let dance = put(&mut state, P0, "Corpse Dance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, dance, &[]);
    settled(&mut state);
    attack_with(&mut state, vec![pup], |_, _| None);
    assert_eq!(life(&state, P1), 18);
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(zone(&state, pup), Zone::Exile);
}

#[test]
fn corpse_dance_with_no_creature_card_does_nothing() {
    let mut state = plains();
    let island = put(&mut state, P0, "Island", Zone::Graveyard);
    let dance = put(&mut state, P0, "Corpse Dance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, dance, &[]);
    // Three mana cannot also pay buyback, so it is not offered.
    settled(&mut state);
    assert_eq!(zone(&state, dance), Zone::Graveyard);
    assert_eq!(zone(&state, island), Zone::Graveyard);
    assert!(state.players[0].battlefield.is_empty());
}

#[test]
fn corpse_dance_buyback_returns_the_spell_to_hand_even_with_no_creature_card() {
    let mut state = plains();
    let island = put(&mut state, P0, "Island", Zone::Graveyard);
    let dance = put(&mut state, P0, "Corpse Dance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 4);
    cast(&mut state, dance, &[]);
    settle_kicked(&mut state, true);
    assert_eq!(zone(&state, dance), Zone::Hand, "bought back");
    assert!(hand_contains(&state, P0, dance));
    assert_eq!(zone(&state, island), Zone::Graveyard);
    assert!(state.players[0].battlefield.is_empty());
    assert_eq!(mana(&state, P0, ManaColor::B), 0, "all five mana paid");
}

#[test]
fn corpse_dance_buyback_can_be_declined_and_then_the_spell_goes_to_the_graveyard() {
    let mut state = plains();
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Graveyard);
    let dance = put(&mut state, P0, "Corpse Dance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 4);
    cast(&mut state, dance, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseKicker { .. }) => act(&mut state, Action::ChooseKicker(false)),
        other => panic!("expected the buyback offer, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, dance), Zone::Graveyard);
    assert_eq!(zone(&state, pup), Zone::Battlefield);
}

#[test]
fn corpse_dance_buyback_with_a_creature_returns_both_and_the_spell_can_be_cast_again() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let dance = put(&mut state, P0, "Corpse Dance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 4);
    cast(&mut state, dance, &[]);
    settle_kicked(&mut state, true);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, dance), Zone::Hand);

    // Cast again without buyback: no creature card is left to return, and the
    // spell now goes to the graveyard.
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, dance, &[]);
    settle_kicked(&mut state, false);
    assert_eq!(zone(&state, dance), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
}

#[test]
fn corpse_dance_countered_with_buyback_paid_goes_to_the_graveyard() {
    let mut state = plains();
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Graveyard);
    let dance = put(&mut state, P0, "Corpse Dance", Zone::Hand);
    let counterspell = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 4);
    assert!(castable(&mut state, dance));
    act(&mut state, Action::CastSpell(dance));
    match next(&mut state) {
        Decision::ChooseKicker { .. } => act(&mut state, Action::ChooseKicker(true)),
        other => panic!("expected the buyback offer, got {other:?}"),
    }
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, counterspell, &[Target::Object(dance)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, dance),
        Zone::Graveyard,
        "countered, not bought back"
    );
    assert_eq!(zone(&state, pup), Zone::Graveyard, "nothing returned");
}

#[test]
fn corpse_dance_cast_in_an_end_step_exiles_at_the_following_end_step() {
    let mut state = plains();
    state.step = Step::End;
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Graveyard);
    let dance = put(&mut state, P0, "Corpse Dance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, dance, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, pup), Zone::Battlefield);
    // This turn's end step has begun already: the next one is the opponent's.
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert_eq!(zone(&state, pup), Zone::Battlefield);
    pass_until(&mut state, |state| state.active_player == P0);
    assert_eq!(zone(&state, pup), Zone::Exile);
}

// ---- Necromancy ----------------------------------------------------------------------

#[test]
fn necromancy_returns_a_creature_from_any_graveyard_attached_and_it_stays() {
    let mut state = plains();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Graveyard);
    let necromancy = put(&mut state, P0, "Necromancy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, necromancy, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(angler)), "theirs");
            assert!(legal_targets.contains(&Target::Object(pup)), "mine");
            Action::ChooseTarget(Target::Object(angler))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, necromancy), Zone::Battlefield);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).controller, P0, "under my control");
    assert_eq!(zone(&state, pup), Zone::Graveyard);
    let attached = state.objects.get(necromancy).v4.attached_to;
    assert_eq!(attached.map(|link| link.object), Some(angler), "attached");
    assert!(
        engine::has_effective_subtype(&state, necromancy, mtg_kernel::card_def::Subtype::Aura),
        "it became an Aura"
    );

    // Cast in a main phase with an empty stack: it is not sacrificed.
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(zone(&state, necromancy), Zone::Battlefield);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
}

#[test]
fn necromancy_cast_at_instant_speed_is_sacrificed_at_cleanup_taking_the_creature() {
    let mut state = plains();
    state.step = Step::End;
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    let necromancy = put(&mut state, P0, "Necromancy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, necromancy, &[]);
    settle_choosing(&mut state, angler);
    assert_eq!(zone(&state, necromancy), Zone::Battlefield);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(zone(&state, necromancy), Zone::Graveyard, "sacrificed");
    assert_eq!(zone(&state, angler), Zone::Graveyard, "its creature too");
}

#[test]
fn necromancy_leaving_the_battlefield_makes_that_creatures_controller_sacrifice_it() {
    let mut state = plains();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    let necromancy = put(&mut state, P0, "Necromancy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, necromancy, &[]);
    settle_choosing(&mut state, angler);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(necromancy, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Graveyard);
    assert!(graveyard_of(&state, P1).contains(&angler), "to its owner");
}

#[test]
fn necromancy_goes_to_the_graveyard_when_its_creature_leaves_as_an_unattached_aura() {
    let mut state = plains();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    let necromancy = put(&mut state, P0, "Necromancy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, necromancy, &[]);
    settle_choosing(&mut state, angler);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(angler, Zone::Exile));
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Exile);
    assert_eq!(zone(&state, necromancy), Zone::Graveyard);
}

#[test]
fn necromancy_with_a_vanished_target_stays_as_an_enchantment_and_with_no_target_does_nothing() {
    let mut state = plains();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Graveyard);
    let necromancy = put(&mut state, P0, "Necromancy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, necromancy, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseTargets { .. }) => {
            act(&mut state, Action::ChooseTarget(Target::Object(angler)));
        }
        other => panic!("expected the trigger's target, got {other:?}"),
    }
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    // The card is exiled in response: the trigger finds no legal target.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(angler, Zone::Exile));
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Exile);
    assert_eq!(zone(&state, pup), Zone::Graveyard, "nothing else returned");
    assert_eq!(zone(&state, necromancy), Zone::Battlefield);
    assert!(state.objects.get(necromancy).v4.attached_to.is_none());
    assert!(!engine::has_effective_subtype(
        &state,
        necromancy,
        mtg_kernel::card_def::Subtype::Aura
    ));
    // Not an Aura: it stays through the whole turn.
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(zone(&state, necromancy), Zone::Battlefield);

    // With no creature card in any graveyard there is no target to choose.
    let mut state = plains();
    put(&mut state, P1, "Island", Zone::Graveyard);
    let necromancy = put(&mut state, P0, "Necromancy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, necromancy, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, necromancy), Zone::Battlefield);
    assert!(state.objects.get(necromancy).v4.attached_to.is_none());
}

#[test]
fn necromancy_leaving_before_its_trigger_resolves_returns_nothing() {
    let mut state = plains();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Graveyard);
    let necromancy = put(&mut state, P0, "Necromancy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, necromancy, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseTargets { .. }) => {
            act(&mut state, Action::ChooseTarget(Target::Object(angler)));
        }
        other => panic!("expected the trigger's target, got {other:?}"),
    }
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    // "If it's on the battlefield": it is gone, so the ability does nothing.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(necromancy, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, necromancy), Zone::Graveyard);
    assert_eq!(zone(&state, angler), Zone::Graveyard);
    assert_eq!(zone(&state, pup), Zone::Graveyard);
}

// ---- Recurring Nightmare ------------------------------------------------------------

#[test]
fn recurring_nightmare_sacrifices_a_creature_and_returns_itself_as_costs() {
    let mut state = plains();
    let nightmare = put(&mut state, P0, "Recurring Nightmare", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    activate(&mut state, nightmare, 0, &[Target::Object(angler)]);
    finish_activation(&mut state, &[pup]);
    // Both costs are paid with the ability on the stack.
    assert_eq!(zone(&state, nightmare), Zone::Hand);
    assert!(hand_contains(&state, P0, nightmare));
    assert_eq!(zone(&state, pup), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Battlefield, "the other creature");
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).controller, P0);
}

#[test]
fn recurring_nightmare_targets_only_creature_cards_in_its_controllers_graveyard() {
    let mut state = plains();
    let nightmare = put(&mut state, P0, "Recurring Nightmare", Zone::Battlefield);
    put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    // The target is chosen before the sacrifice, so with no creature card in
    // the graveyard yet there is nothing to target.
    put(&mut state, P0, "Island", Zone::Graveyard);
    assert!(!activatable(&mut state, nightmare, 0));
    // Only a creature card in the controller's own graveyard is a target.
    let theirs = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    assert!(!activatable(&mut state, nightmare, 0), "their graveyard");
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let targets = ability_targets(&state, nightmare);
    assert!(targets.contains(&Target::Object(angler)));
    assert!(targets.contains(&Target::Object(elves)));
    assert!(!targets.contains(&Target::Object(theirs)));
}

#[test]
fn recurring_nightmare_needs_a_creature_to_sacrifice() {
    let mut state = plains();
    let nightmare = put(&mut state, P0, "Recurring Nightmare", Zone::Battlefield);
    put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    put(&mut state, P0, "Mind Stone", Zone::Battlefield);
    assert!(!activatable(&mut state, nightmare, 0), "no creature");
    // The opponent's creature cannot be sacrificed.
    put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    assert!(!activatable(&mut state, nightmare, 0), "theirs");
    put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    assert!(activatable(&mut state, nightmare, 0));
}

#[test]
fn recurring_nightmare_cannot_be_the_creature_it_sacrifices() {
    // Opalescence: "Each other non-Aura enchantment is a creature." The
    // Nightmare can't pay both of its costs with itself.
    let mut state = plains();
    let nightmare = put(&mut state, P0, "Recurring Nightmare", Zone::Battlefield);
    put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    let opalescence = put(&mut state, P0, "Opalescence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, opalescence, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, opalescence), Zone::Battlefield);
    assert!(engine::object_has_type(
        &state,
        nightmare,
        CardType::Creature
    ));
    assert!(
        !activatable(&mut state, nightmare, 0),
        "itself is the only creature"
    );
    put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    assert!(activatable(&mut state, nightmare, 0));
}

#[test]
fn necromancy_stops_being_an_opalescence_creature_once_it_is_an_aura() {
    // Opalescence animates each other non-Aura enchantment. Necromancy enters
    // as one, then its entry trigger makes it an Aura: no longer a creature.
    let mut state = plains();
    let opalescence = put(&mut state, P0, "Opalescence", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, opalescence, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, opalescence), Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    let necromancy = put(&mut state, P0, "Necromancy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, necromancy, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(angler)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, necromancy), Zone::Battlefield);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(
        state
            .objects
            .get(necromancy)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(angler)
    );
    assert!(engine::has_effective_subtype(
        &state,
        necromancy,
        mtg_kernel::card_def::Subtype::Aura
    ));
    assert!(
        !engine::object_has_type(&state, necromancy, CardType::Creature),
        "an Aura is not animated by Opalescence"
    );
}

#[test]
fn recurring_nightmare_is_back_in_hand_even_if_its_target_vanishes() {
    let mut state = plains();
    let nightmare = put(&mut state, P0, "Recurring Nightmare", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    activate(&mut state, nightmare, 0, &[Target::Object(angler)]);
    finish_activation(&mut state, &[pup]);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(angler, Zone::Exile));
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Exile);
    assert_eq!(zone(&state, nightmare), Zone::Hand, "already returned");
    assert_eq!(zone(&state, pup), Zone::Graveyard, "already sacrificed");
}

#[test]
fn recurring_nightmare_activates_only_as_a_sorcery() {
    let mut state = plains();
    let nightmare = put(&mut state, P0, "Recurring Nightmare", Zone::Battlefield);
    put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    assert!(
        activatable(&mut state, nightmare, 0),
        "main phase, empty stack"
    );
    // Not with something on the stack.
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    assert!(
        !activatable(&mut state, nightmare, 0),
        "the stack is not empty"
    );
    settled(&mut state);
    assert!(activatable(&mut state, nightmare, 0));
    // Not in another step.
    state.step = Step::BeginCombat;
    assert!(
        !activatable(&mut state, nightmare, 0),
        "beginning of combat"
    );
    state.step = Step::End;
    assert!(!activatable(&mut state, nightmare, 0), "end step");
}

// ---- Aang's Iceberg ------------------------------------------------------------------

/// P0 casts the Iceberg at instant speed, exiling the opponent's Jackal Pup.
fn iceberg_exiling_a_pup(state: &mut GameState) -> (ObjectId, ObjectId) {
    state.step = Step::End;
    let pup = put(state, P1, "Jackal Pup", Zone::Battlefield);
    let iceberg = put(state, P0, "Aang's Iceberg", Zone::Hand);
    pool(state, P0, &[ManaColor::W], 2);
    cast(state, iceberg, &[]);
    settle_choosing(state, pup);
    (iceberg, pup)
}

#[test]
fn aangs_iceberg_has_flash_and_exiles_up_to_one_other_nonland_permanent_until_it_leaves() {
    let mut state = plains();
    let land = put(&mut state, P1, "Plains", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.step = Step::End;
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let iceberg = put(&mut state, P0, "Aang's Iceberg", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, iceberg, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(can_finish, "up to one target");
            assert!(legal_targets.contains(&Target::Object(pup)));
            assert!(
                legal_targets.contains(&Target::Object(elves)),
                "any controller"
            );
            assert!(!legal_targets.contains(&Target::Object(land)), "a nonland");
            assert!(
                !legal_targets.contains(&Target::Object(iceberg)),
                "an other permanent"
            );
            Action::ChooseTarget(Target::Object(pup))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, iceberg), Zone::Battlefield);
    assert_eq!(zone(&state, pup), Zone::Exile);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    // It returns, under its owner's control, as the Iceberg leaves.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(iceberg, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, pup), Zone::Battlefield);
    assert_eq!(state.objects.get(pup).controller, P1);
}

#[test]
fn aangs_iceberg_may_choose_no_target_and_exiles_a_token_for_good() {
    let mut state = plains();
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let iceberg = put(&mut state, P0, "Aang's Iceberg", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, iceberg, &[]);
    settle_declining_targets(&mut state);
    assert_eq!(zone(&state, iceberg), Zone::Battlefield);
    assert_eq!(zone(&state, pup), Zone::Battlefield, "nothing exiled");

    // An exiled token ceases to exist and does not come back.
    let mut state = plains();
    let servo = token(&mut state, P1, "Servo Token");
    let iceberg = put(&mut state, P0, "Aang's Iceberg", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, iceberg, &[]);
    settle_choosing(&mut state, servo);
    assert!(gone_from_the_battlefield(&state, servo));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(iceberg, Zone::Graveyard),
    );
    settled(&mut state);
    assert!(gone_from_the_battlefield(&state, servo));
    assert!(named(&state, P1, "Servo Token").is_empty());
}

#[test]
fn aangs_iceberg_waterbend_with_mana_sacrifices_it_ends_the_exile_and_scries_two() {
    let mut state = plains();
    let (iceberg, pup) = iceberg_exiling_a_pup(&mut state);
    assert_eq!(zone(&state, pup), Zone::Exile);
    pool(&mut state, P0, &[], 3);
    activate(&mut state, iceberg, 0, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            legal_targets,
            max_targets,
            ..
        }) => {
            assert_eq!(max_targets, 2, "scry 2");
            assert_eq!(legal_targets.len(), 2);
            act(&mut state, Action::FinishEffectSelection);
        }
        other => panic!("expected the scry, got {other:?}"),
    }
    settle_first(&mut state);
    assert_eq!(zone(&state, iceberg), Zone::Graveyard, "sacrificed");
    assert_eq!(zone(&state, pup), Zone::Battlefield, "the exile ended");
}

#[test]
fn aangs_iceberg_waterbend_may_tap_artifacts_and_creatures_even_summoning_sick() {
    let mut state = plains();
    let iceberg = put(&mut state, P0, "Aang's Iceberg", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(elves).summoning_sick = true;
    let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
    // Two helpers are not enough to pay {3} without mana.
    assert!(!activatable(&mut state, iceberg, 3));
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    assert!(activatable(&mut state, iceberg, 3));
    activate(&mut state, iceberg, 3, &[]);
    finish_activation(&mut state, &[elves, stone, pup]);
    for permanent in [elves, stone, pup] {
        assert!(state.objects.get(permanent).tapped, "tapped to help");
    }
    settle_first(&mut state);
    assert_eq!(zone(&state, iceberg), Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool, [0; 6], "no mana was needed");
}

#[test]
fn aangs_iceberg_waterbend_pays_the_rest_with_mana() {
    let mut state = plains();
    let iceberg = put(&mut state, P0, "Aang's Iceberg", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let their_stone = put(&mut state, P1, "Mind Stone", Zone::Battlefield);
    // One permanent and {2}.
    pool(&mut state, P0, &[], 2);
    activate(&mut state, iceberg, 1, &[]);
    finish_activation(&mut state, &[pup]);
    assert!(state.objects.get(pup).tapped);
    assert!(!state.objects.get(their_stone).tapped, "the opponent's");
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    settle_first(&mut state);
    assert_eq!(zone(&state, iceberg), Zone::Graveyard);

    // Two permanents and {1}.
    let mut state = plains();
    let iceberg = put(&mut state, P0, "Aang's Iceberg", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, iceberg, 2, &[]);
    finish_activation(&mut state, &[pup, stone]);
    assert!(state.objects.get(pup).tapped && state.objects.get(stone).tapped);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    settle_first(&mut state);
    assert_eq!(zone(&state, iceberg), Zone::Graveyard);
}

// ---- Shifting Woodland ---------------------------------------------------------------

/// Four card types in P0's graveyard: a creature, an instant, an artifact and
/// a land, in that order. Returns the creature card.
fn delirium_graveyard(state: &mut GameState) -> ObjectId {
    let angler = put(state, P0, "Gurmag Angler", Zone::Graveyard);
    put(state, P0, "Lightning Bolt", Zone::Graveyard);
    put(state, P0, "Mind Stone", Zone::Graveyard);
    put(state, P0, "Island", Zone::Graveyard);
    angler
}

#[test]
fn shifting_woodland_enters_tapped_unless_you_control_a_forest() {
    let mut state = plains();
    let land = put(&mut state, P0, "Shifting Woodland", Zone::Hand);
    play_land(&mut state, land);
    assert!(state.objects.get(land).tapped, "no Forest");

    // A Forest an opponent controls does not count.
    let mut state = plains();
    put(&mut state, P1, "Forest", Zone::Battlefield);
    let land = put(&mut state, P0, "Shifting Woodland", Zone::Hand);
    play_land(&mut state, land);
    assert!(state.objects.get(land).tapped, "their Forest");

    let mut state = plains();
    put(&mut state, P0, "Forest", Zone::Battlefield);
    let land = put(&mut state, P0, "Shifting Woodland", Zone::Hand);
    play_land(&mut state, land);
    assert!(!state.objects.get(land).tapped, "a Forest");
    act(&mut state, Action::ActivateManaAbility(land));
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
}

#[test]
fn shifting_woodland_needs_delirium_to_activate() {
    let mut state = plains();
    let woodland = put(&mut state, P0, "Shifting Woodland", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    // Creature, instant and artifact: three card types.
    put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Mind Stone", Zone::Graveyard);
    assert!(!activatable(&mut state, woodland, 0), "three types");
    // Only the controller's graveyard counts.
    put(&mut state, P1, "Island", Zone::Graveyard);
    assert!(!activatable(&mut state, woodland, 0), "theirs");
    put(&mut state, P0, "Island", Zone::Graveyard);
    assert!(activatable(&mut state, woodland, 0), "four types");
}

#[test]
fn shifting_woodland_becomes_a_copy_of_the_graveyard_card_until_end_of_turn() {
    let mut state = plains();
    let woodland = put(&mut state, P0, "Shifting Woodland", Zone::Battlefield);
    let angler = delirium_graveyard(&mut state);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    activate(&mut state, woodland, 0, &[Target::Object(angler)]);
    settled(&mut state);
    // A copy of a nonland card: no longer a land, and without its own ability.
    assert_eq!(
        CARD_DEFS[state.objects.get(woodland).card_def as usize].name,
        "Gurmag Angler"
    );
    assert!(engine::object_has_type(
        &state,
        woodland,
        CardType::Creature
    ));
    assert!(!engine::object_has_type(&state, woodland, CardType::Land));
    assert_eq!(power_toughness(&state, woodland), (5, 5));
    assert_eq!(zone(&state, angler), Zone::Graveyard, "the card stays put");
    assert!(!activatable(&mut state, woodland, 0), "its ability is gone");
    // It reverts as the turn ends.
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(
        CARD_DEFS[state.objects.get(woodland).card_def as usize].name,
        "Shifting Woodland"
    );
    assert!(engine::object_has_type(&state, woodland, CardType::Land));
    assert!(!engine::object_has_type(
        &state,
        woodland,
        CardType::Creature
    ));
}

#[test]
fn shifting_woodland_copying_a_land_card_taps_for_that_lands_mana() {
    let mut state = plains();
    let woodland = put(&mut state, P0, "Shifting Woodland", Zone::Battlefield);
    delirium_graveyard(&mut state);
    let island = state.players[0].graveyard[3];
    assert_eq!(state.objects.get(island).name, "Island");
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    activate(&mut state, woodland, 0, &[Target::Object(island)]);
    settled(&mut state);
    assert_eq!(
        CARD_DEFS[state.objects.get(woodland).card_def as usize].name,
        "Island"
    );
    assert!(engine::object_has_type(&state, woodland, CardType::Land));
    act(&mut state, Action::ActivateManaAbility(woodland));
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
    assert_eq!(
        mana(&state, P0, ManaColor::G),
        0,
        "not its own mana any more"
    );
}

#[test]
fn shifting_woodland_copy_does_nothing_if_its_target_vanishes() {
    let mut state = plains();
    let woodland = put(&mut state, P0, "Shifting Woodland", Zone::Battlefield);
    let angler = delirium_graveyard(&mut state);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    activate(&mut state, woodland, 0, &[Target::Object(angler)]);
    finish_activation(&mut state, &[]);
    // The delirium check was on activation only, and the card leaves.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(angler, Zone::Exile));
    settled(&mut state);
    assert_eq!(
        CARD_DEFS[state.objects.get(woodland).card_def as usize].name,
        "Shifting Woodland"
    );
    assert!(engine::object_has_type(&state, woodland, CardType::Land));
}

// ---- Public sessions -------------------------------------------------------------

#[derive(Default)]
struct C5cEvents {
    plays: BTreeMap<&'static str, usize>,
}

impl C5cEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let Some(card_id) = semantic["source"]["card_db_id"].as_u64() else {
            return;
        };
        let name = CARD_DEFS[card_id as usize].name;
        match semantic["action_kind"].as_str() {
            Some("cast_spell" | "play_land") if C5C_CARDS.contains(&name) => {
                *self.plays.entry(name).or_default() += 1;
            }
            _ => {}
        }
    }

    fn add(&mut self, other: C5cEvents) {
        for (name, count) in other.plays {
            *self.plays.entry(name).or_default() += count;
        }
    }
}

#[test]
fn c5c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let reanimator = deck(&[
        ("Swamp", 12),
        ("Forest", 4),
        ("Shifting Woodland", 4),
        ("Necromancy", 4),
        ("Corpse Dance", 4),
        ("Recurring Nightmare", 4),
        ("Gurmag Angler", 4),
        ("Llanowar Elves", 4),
    ]);
    let white = deck(&[
        ("Plains", 20),
        ("Aang's Iceberg", 4),
        ("Sunfall", 4),
        ("Thraben Inspector", 6),
        ("Mother of Runes", 6),
    ]);

    let mut totals = C5cEvents::default();
    for (pair, first_seed) in [
        ([reanimator.clone(), white.clone()], 0x4335_6300_u64),
        ([white, reanimator], 0x4335_6310),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let mut events = C5cEvents::default();
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action)
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
                totals.add(events);
            }
        }
    }
    for name in C5C_CARDS {
        assert!(
            totals.plays.get(name).copied().unwrap_or(0) > 0,
            "{name} was never cast or played"
        );
    }
}
