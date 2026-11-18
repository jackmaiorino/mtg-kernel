//! Vintage Cube wave C1c: native rules and public-session tests for the
//! red and multicolor instants and sorceries.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{in_parallel, play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, AltCostCondition, CardCapability, CardDef, CardType, Keywords, Subtype,
    CARD_DEFS,
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

/// The cards of this wave that are finished (Full, with tests below).
const C1C_CARDS: [&str; 18] = [
    "Dead // Gone",
    "Delayed Blast Fireball",
    "Escape to the Wilds",
    "Expressive Iteration",
    "Fire Covenant",
    "Flame Slash",
    "Flashback",
    "Impractical Joke",
    "Kolaghan's Command",
    "Lingering Souls",
    "Mine Collapse",
    "No More Lies",
    "Pillage the Bog",
    "Sear",
    "Tarfire",
    "Traumatic Critique",
    "Vicious Rivalry",
    "Wheel of Fortune",
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
        0x4355_4245_0000 + 0x3163,
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

// ---- Helpers of this wave ------------------------------------------------------

/// A turn of `active` in its first main phase, `priority` holding priority.
fn set_active(state: &mut GameState, active: PlayerId, priority: PlayerId) {
    state.active_player = active;
    state.priority_player = priority;
    state.step = Step::Main1;
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

/// P1 (holding priority as the active player) casts `spell` from its hand
/// with `targets`, then passes, leaving it on the stack with P0 to respond.
fn opponent_casts(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    set_active(state, P1, P1);
    cast(state, spell, targets);
    match next(state) {
        Decision::CastSpellOrPass { player: P1, .. } => act(state, Action::Pass),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
}

/// Settles, answering a "pay" question with `pay` and every other choice
/// with the first legal answer.
fn settle_paying(state: &mut GameState, pay: bool) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(pay),
        other => first_answer(other),
    });
}

/// Passes priority once the engine has advanced to the current player's
/// priority window.
fn pass_priority(state: &mut GameState) {
    match next(state) {
        Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
        other => panic!("expected priority, got {other:?}"),
    }
}

fn loyalty(state: &GameState, object: ObjectId) -> Option<u32> {
    mtg_kernel::planeswalker_v1::loyalty(state, object)
}

fn hand_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

fn library_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].library.len()
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn c1c_cards_are_full_with_their_printed_characteristics() {
    let (w, u, r) = (ManaColor::W, ManaColor::U, ManaColor::R);
    let (b, g) = (ManaColor::B, ManaColor::G);
    let colored = |color: ManaColor| Pip::Colored(color);
    for (name, generic, x_count, pips, card_type, subtypes, colors) in [
        (
            "Dead // Gone",
            0,
            0,
            vec![colored(r)],
            CardType::Instant,
            vec![],
            vec![r],
        ),
        (
            "Delayed Blast Fireball",
            1,
            0,
            vec![colored(r), colored(r)],
            CardType::Instant,
            vec![],
            vec![r],
        ),
        (
            "Escape to the Wilds",
            3,
            0,
            vec![colored(r), colored(g)],
            CardType::Sorcery,
            vec![],
            vec![r, g],
        ),
        (
            "Expressive Iteration",
            0,
            0,
            vec![colored(u), colored(r)],
            CardType::Sorcery,
            vec![],
            vec![u, r],
        ),
        (
            "Fire Covenant",
            1,
            0,
            vec![colored(b), colored(r)],
            CardType::Instant,
            vec![],
            vec![b, r],
        ),
        (
            "Flashback",
            0,
            0,
            vec![colored(r)],
            CardType::Instant,
            vec![],
            vec![r],
        ),
        (
            "Impractical Joke",
            0,
            0,
            vec![colored(r)],
            CardType::Sorcery,
            vec![],
            vec![r],
        ),
        (
            "Pillage the Bog",
            0,
            0,
            vec![colored(b), colored(g)],
            CardType::Sorcery,
            vec![],
            vec![b, g],
        ),
        (
            "Vicious Rivalry",
            2,
            0,
            vec![colored(b), colored(g)],
            CardType::Sorcery,
            vec![],
            vec![b, g],
        ),
        (
            "Flame Slash",
            0,
            0,
            vec![colored(r)],
            CardType::Sorcery,
            vec![],
            vec![r],
        ),
        (
            "Kolaghan's Command",
            1,
            0,
            vec![colored(ManaColor::B), colored(r)],
            CardType::Instant,
            vec![],
            vec![ManaColor::B, r],
        ),
        (
            "Lingering Souls",
            2,
            0,
            vec![colored(w)],
            CardType::Sorcery,
            vec![],
            vec![w],
        ),
        (
            "Mine Collapse",
            3,
            0,
            vec![colored(r)],
            CardType::Instant,
            vec![],
            vec![r],
        ),
        (
            "No More Lies",
            0,
            0,
            vec![colored(w), colored(u)],
            CardType::Instant,
            vec![],
            vec![w, u],
        ),
        (
            "Sear",
            1,
            0,
            vec![colored(r)],
            CardType::Instant,
            vec![],
            vec![r],
        ),
        (
            "Tarfire",
            0,
            0,
            vec![colored(r)],
            CardType::Instant,
            vec![Subtype::Goblin],
            vec![r],
        ),
        (
            "Traumatic Critique",
            0,
            1,
            vec![colored(u), colored(r)],
            CardType::Instant,
            vec![],
            vec![u, r],
        ),
        (
            "Wheel of Fortune",
            2,
            0,
            vec![colored(r)],
            CardType::Sorcery,
            vec![],
            vec![r],
        ),
    ] {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
        assert!(!card.is_token, "{name}");
        assert_eq!(card.cost.generic, generic, "{name}");
        assert_eq!(card.cost.x_count, x_count, "{name}");
        assert_eq!(card.cost.pips, pips.as_slice(), "{name}");
        if name == "Tarfire" {
            // A Kindred Instant (#716).
            assert_eq!(card.types, &[CardType::Kindred, card_type], "{name}");
        } else {
            assert_eq!(card.types, &[card_type], "{name}");
        }
        assert_eq!(card.subtypes, subtypes.as_slice(), "{name}");
        assert_eq!(card.colors.len(), colors.len(), "{name}");
        for color in colors {
            assert!(card.colors.contains(&color), "{name} is {color:?}");
        }
    }
    for name in C1C_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert_eq!(
            def(name).alt_cost.is_some(),
            name == "Mine Collapse" || name == "Delayed Blast Fireball",
            "{name}"
        );
        assert_eq!(
            def(name).flashback.is_some(),
            name == "Lingering Souls",
            "{name}"
        );
        assert_eq!(
            def(name).plot_cost.is_some(),
            name == "Pillage the Bog",
            "{name}"
        );
        assert_eq!(
            def(name).adventure.is_some(),
            name == "Dead // Gone",
            "{name}"
        );
    }

    // Gone, the split card's second half: {2}{R}, an instant.
    let gone = def("Dead // Gone").adventure.as_ref().expect("Gone");
    assert_eq!(
        (gone.cost.generic, gone.cost.pips),
        (2, &[Pip::Colored(r)][..])
    );
    assert_eq!(gone.types, &[CardType::Instant]);

    // Delayed Blast Fireball: foretell {4}{R}{R} for the foretold card only,
    // and the foretell special action as its hand ability.
    let fireball = def("Delayed Blast Fireball");
    assert_eq!(
        fireball.alt_cost.expect("foretell cost").condition,
        AltCostCondition::ForetoldV1
    );
    assert_eq!(fireball.activated_abilities.len(), 1);

    // Pillage the Bog: Plot {1}{B}{G}.
    let plot = def("Pillage the Bog").plot_cost.expect("plot");
    assert_eq!(
        (plot.generic, plot.pips),
        (1, &[Pip::Colored(b), Pip::Colored(g)][..])
    );

    let spirit = def("White Flying Spirit Token");
    assert!(spirit.is_token);
    assert_eq!(spirit.types, &[CardType::Creature]);
    assert_eq!(spirit.subtypes, &[Subtype::Spirit]);
    assert_eq!(spirit.colors, &[ManaColor::W]);
    assert_eq!((spirit.power, spirit.toughness), (Some(1), Some(1)));
    assert!(spirit.keywords.has(Keywords::FLYING));
}

// ---- Flame Slash, Sear and Tarfire ----------------------------------------------

#[test]
fn flame_slash_deals_four_damage_to_a_creature_and_only_at_sorcery_speed() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let slash = put(&mut state, P0, "Flame Slash", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);

    // Only creatures are legal targets: no players.
    let legal = cast_targets(&state, slash);
    assert!(legal.contains(&Target::Object(specter)));
    assert!(legal.contains(&Target::Object(wall)));
    assert!(!legal
        .iter()
        .any(|target| matches!(target, Target::Player(_))));

    cast(&mut state, slash, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(zone(&state, slash), Zone::Graveyard);
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));

    // Four damage does not kill a 0/5.
    let slash = put(&mut state, P0, "Flame Slash", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, slash, &[Target::Object(wall)]);
    settled(&mut state);
    assert_eq!(zone(&state, wall), Zone::Battlefield);
    assert_eq!(state.objects.get(wall).damage, 4);

    // A sorcery: not castable outside a main phase.
    let slash = put(&mut state, P0, "Flame Slash", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, slash));
    state.step = Step::Upkeep;
    assert!(!castable(&mut state, slash));
}

#[test]
fn flame_slash_does_nothing_when_its_creature_is_gone() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let slash = put(&mut state, P0, "Flame Slash", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, slash, &[Target::Object(specter)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    pass_priority(&mut state);
    // P1 kills its own Specter with a Bolt in response.
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(zone(&state, slash), Zone::Graveyard);
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));
}

#[test]
fn sear_deals_four_to_a_creature_or_planeswalker_at_instant_speed() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, karn), Some(5));

    let sear = put(&mut state, P0, "Sear", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    // Creatures and planeswalkers, never players.
    let legal = cast_targets(&state, sear);
    assert!(legal.contains(&Target::Object(specter)));
    assert!(legal.contains(&Target::Object(karn)));
    assert!(!legal
        .iter()
        .any(|target| matches!(target, Target::Player(_))));

    // Four damage removes four loyalty from a planeswalker.
    cast(&mut state, sear, &[Target::Object(karn)]);
    settled(&mut state);
    assert_eq!(loyalty(&state, karn), Some(1));
    assert_eq!(zone(&state, sear), Zone::Graveyard);

    // At instant speed, outside a main phase, and it kills a creature.
    let sear = put(&mut state, P0, "Sear", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    state.step = Step::Upkeep;
    cast(&mut state, sear, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));
}

#[test]
fn sear_does_nothing_when_its_target_is_gone() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let sear = put(&mut state, P0, "Sear", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, sear, &[Target::Object(specter)]);
    pass_priority(&mut state);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(zone(&state, sear), Zone::Graveyard);
}

#[test]
fn tarfire_deals_two_damage_to_any_target_at_instant_speed() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let tarfire = put(&mut state, P0, "Tarfire", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    // Any target: players and creatures.
    let legal = cast_targets(&state, tarfire);
    assert!(legal.contains(&Target::Player(P0)));
    assert!(legal.contains(&Target::Player(P1)));
    assert!(legal.contains(&Target::Object(specter)));

    state.step = Step::Upkeep;
    cast(&mut state, tarfire, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 18);
    assert_eq!(zone(&state, tarfire), Zone::Graveyard);

    // Two damage kills a 2/2.
    let tarfire = put(&mut state, P0, "Tarfire", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, tarfire, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);

    // It can hit its own controller too.
    let tarfire = put(&mut state, P0, "Tarfire", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, tarfire, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 18);
}

#[test]
fn tarfire_does_nothing_when_its_creature_is_gone() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let tarfire = put(&mut state, P0, "Tarfire", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, tarfire, &[Target::Object(specter)]);
    pass_priority(&mut state);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(zone(&state, tarfire), Zone::Graveyard);
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));
}

// ---- Mine Collapse ---------------------------------------------------------------

#[test]
fn mine_collapse_deals_five_damage_to_a_creature_or_planeswalker() {
    let mut state = plains();
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, karn), Some(5));

    // Hard cast for {3}{R}: not castable without the mana.
    let collapse = put(&mut state, P0, "Mine Collapse", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    assert!(!castable(&mut state, collapse));
    pool(&mut state, P0, &[], 1);
    let legal = cast_targets(&state, collapse);
    assert!(legal.contains(&Target::Object(wall)));
    assert!(legal.contains(&Target::Object(karn)));
    assert!(!legal
        .iter()
        .any(|target| matches!(target, Target::Player(_))));

    // Five damage kills a 0/5.
    cast(&mut state, collapse, &[Target::Object(wall)]);
    settled(&mut state);
    assert_eq!(zone(&state, wall), Zone::Graveyard);
    assert_eq!(zone(&state, collapse), Zone::Graveyard);

    // And all of a planeswalker's loyalty, at instant speed.
    let collapse = put(&mut state, P0, "Mine Collapse", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    state.step = Step::Upkeep;
    cast(&mut state, collapse, &[Target::Object(karn)]);
    settled(&mut state);
    assert_eq!(zone(&state, karn), Zone::Graveyard);
}

#[test]
fn mine_collapse_may_sacrifice_a_mountain_instead_of_its_cost_on_your_turn() {
    let mut state = plains();
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let collapse = put(&mut state, P0, "Mine Collapse", Zone::Hand);
    // No Mountain: not castable without mana.
    let plains_land = put(&mut state, P0, "Plains", Zone::Battlefield);
    assert!(!castable(&mut state, collapse));

    // A Mountain (a basic or any land with the type) is the only candidate.
    let mountain = put(&mut state, P0, "Mountain", Zone::Battlefield);
    let island = put(&mut state, P0, "Volcanic Island", Zone::Battlefield);
    assert!(castable(&mut state, collapse));
    act(&mut state, Action::CastSpell(collapse));
    let mut offered = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&engine::CastMode::Alternative));
            Action::ChooseCastMode(engine::CastMode::Alternative)
        }
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&mountain));
            assert!(candidates.contains(&island));
            assert!(!candidates.contains(&plains_land));
            offered = true;
            Action::ChooseCostTarget(mountain)
        }
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(wall)));
            Action::ChooseTarget(Target::Object(wall))
        }
        other => first_answer(other),
    });
    assert!(offered, "the Mountains were offered as the cost");
    assert_eq!(zone(&state, mountain), Zone::Graveyard);
    assert_eq!(zone(&state, island), Zone::Battlefield);
    assert_eq!(zone(&state, plains_land), Zone::Battlefield);
    assert_eq!(zone(&state, wall), Zone::Graveyard, "five damage");
    assert_eq!(zone(&state, collapse), Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool, [0; 6], "no mana was spent");

    // A land that merely taps for red is not a Mountain.
    let mut state = plains();
    let collapse = put(&mut state, P0, "Mine Collapse", Zone::Hand);
    put(&mut state, P0, "Great Furnace", Zone::Battlefield);
    assert!(!castable(&mut state, collapse));
}

#[test]
fn mine_collapse_alternative_cost_is_only_offered_on_your_own_turn() {
    let mut state = plains();
    put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let collapse = put(&mut state, P0, "Mine Collapse", Zone::Hand);
    let mountain = put(&mut state, P0, "Mountain", Zone::Battlefield);
    assert!(castable(&mut state, collapse), "your turn");

    // On the opponent's turn the Mountain does not pay for it.
    set_active(&mut state, P1, P0);
    assert!(!castable(&mut state, collapse), "opponent's turn");

    // With the mana, the printed cost is still available then, and the
    // Mountain is neither sacrificed nor offered.
    pool(&mut state, P0, &[ManaColor::R], 3);
    assert!(castable(&mut state, collapse));
    act(&mut state, Action::CastSpell(collapse));
    let mut offered_mode = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => {
            offered_mode = true;
            Action::ChooseCastMode(engine::CastMode::Normal)
        }
        other => first_answer(other),
    });
    assert!(!offered_mode, "no choice of cost on another player's turn");
    assert_eq!(zone(&state, mountain), Zone::Battlefield);
}

// ---- Wheel of Fortune ------------------------------------------------------------

#[test]
fn wheel_of_fortune_makes_each_player_discard_their_hand_then_draw_seven() {
    let mut state = plains();
    let wheel = put(&mut state, P0, "Wheel of Fortune", Zone::Hand);
    let mine: Vec<ObjectId> = (0..2)
        .map(|_| put(&mut state, P0, "Plains", Zone::Hand))
        .collect();
    let theirs: Vec<ObjectId> = (0..4)
        .map(|_| put(&mut state, P1, "Mountain", Zone::Hand))
        .collect();
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, wheel, &[]);
    settled(&mut state);

    for card in mine.into_iter().chain(theirs) {
        assert_eq!(zone(&state, card), Zone::Graveyard);
    }
    assert_eq!(zone(&state, wheel), Zone::Graveyard);
    assert_eq!(hand_len(&state, P0), 7);
    assert_eq!(hand_len(&state, P1), 7);
    assert_eq!(library_len(&state, P0), 13);
    assert_eq!(library_len(&state, P1), 13);
    assert_eq!(
        state.players[0].graveyard.len(),
        3,
        "two discards and Wheel"
    );
    assert_eq!(state.players[1].graveyard.len(), 4);
}

#[test]
fn wheel_of_fortune_draws_seven_for_a_player_with_an_empty_hand() {
    let mut state = plains();
    let wheel = put(&mut state, P0, "Wheel of Fortune", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, wheel, &[]);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 7);
    assert_eq!(hand_len(&state, P1), 7);
    assert_eq!(state.players[1].graveyard.len(), 0);
}

#[test]
fn wheel_of_fortune_ends_the_game_when_the_first_draw_wins_it() {
    // Both players would win on their draw from an empty library (Jace,
    // Wielder of Mysteries); the caster draws first and the game is over.
    let mut state = game(&[]);
    for player in [P0, P1] {
        let jace = put(&mut state, player, "Jace, Wielder of Mysteries", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(jace, Zone::Battlefield),
        );
    }
    let wheel = put(&mut state, P0, "Wheel of Fortune", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, wheel, &[]);
    let mut over = None;
    for _ in 0..40 {
        match next(&mut state) {
            Decision::GameOver { winner } => {
                over = Some(winner);
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => act(&mut state, first_answer(other)),
        }
    }
    assert_eq!(over, Some(Some(P0)));
}

#[test]
fn wheel_of_fortune_draws_can_be_replaced_by_dredge() {
    let mut state = plains();
    let wheel = put(&mut state, P0, "Wheel of Fortune", Zone::Hand);
    let imp = put(&mut state, P1, "Stinkweed Imp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, wheel, &[]);
    // The Wheel discards the Imp, so its Dredge 5 may replace a draw.
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            player,
            legal_targets,
            can_finish: true,
            ..
        }) => {
            assert_eq!(player, P1);
            assert_eq!(legal_targets, vec![Target::Object(imp)]);
            act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
        }
        other => panic!("expected Dredge prompt for the discarded Stinkweed Imp, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, imp), Zone::Hand);
    assert_eq!(hand_len(&state, P1), 7, "the Imp and six draws");
    assert_eq!(hand_len(&state, P0), 7);
    assert_eq!(library_len(&state, P1), 9, "five milled, six drawn");
}

// ---- Lingering Souls -------------------------------------------------------------

#[test]
fn lingering_souls_makes_two_flying_spirits_and_has_flashback() {
    let mut state = plains();
    let souls = put(&mut state, P0, "Lingering Souls", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, souls, &[]);
    settled(&mut state);
    let spirits = named(&state, P0, "White Flying Spirit Token");
    assert_eq!(spirits.len(), 2);
    for &spirit in &spirits {
        assert_eq!(power_toughness(&state, spirit), (1, 1));
        assert!(keyword(&state, spirit, Keywords::FLYING));
        assert!(state.objects.get(spirit).v4.is_token);
    }
    assert_eq!(zone(&state, souls), Zone::Graveyard);

    // Flashback {1}{B}: not without black mana.
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 0);
    assert!(!castable(&mut state, souls));
    state.players[0].mana_pool = [0; 6];
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, souls, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "White Flying Spirit Token").len(), 4);
    assert_eq!(zone(&state, souls), Zone::Exile, "flashback exiles it");
    assert!(!castable(&mut state, souls));
}

#[test]
fn lingering_souls_tokens_belong_to_the_caster_of_the_flashback() {
    let mut state = plains();
    let souls = put(&mut state, P1, "Lingering Souls", Zone::Graveyard);
    set_active(&mut state, P1, P1);
    pool(&mut state, P1, &[ManaColor::B], 1);
    cast(&mut state, souls, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P1, "White Flying Spirit Token").len(), 2);
    assert_eq!(named(&state, P0, "White Flying Spirit Token").len(), 0);
    assert_eq!(zone(&state, souls), Zone::Exile);
}

// ---- No More Lies ----------------------------------------------------------------

#[test]
fn no_more_lies_exiles_a_spell_its_controller_cannot_pay_for() {
    let mut state = plains();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let lies = put(&mut state, P0, "No More Lies", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    opponent_casts(&mut state, bolt, &[Target::Player(P0)]);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 0);
    cast(&mut state, lies, &[Target::Object(bolt)]);
    settle_paying(&mut state, true);
    assert_eq!(
        zone(&state, bolt),
        Zone::Exile,
        "exiled, not in a graveyard"
    );
    assert_eq!(life(&state, P0), 20, "the Bolt was countered");
    assert_eq!(zone(&state, lies), Zone::Graveyard);
    assert!(state.players[1].graveyard.is_empty());
}

#[test]
fn no_more_lies_lets_its_controller_pay_three_to_keep_the_spell() {
    let mut state = plains();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let lies = put(&mut state, P0, "No More Lies", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 3);
    opponent_casts(&mut state, bolt, &[Target::Player(P0)]);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 0);
    cast(&mut state, lies, &[Target::Object(bolt)]);
    settle_paying(&mut state, true);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "paid, so it resolved");
    assert_eq!(life(&state, P0), 17);
    assert_eq!(state.players[1].mana_pool, [0; 6], "the {{3}} was paid");
}

#[test]
fn no_more_lies_exiles_the_spell_when_its_controller_declines_to_pay() {
    let mut state = plains();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let lies = put(&mut state, P0, "No More Lies", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 3);
    opponent_casts(&mut state, bolt, &[Target::Player(P0)]);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 0);
    cast(&mut state, lies, &[Target::Object(bolt)]);
    settle_paying(&mut state, false);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn no_more_lies_counters_any_spell_and_a_flashback_spell_stays_exiled() {
    let mut state = plains();
    // A creature spell.
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Hand);
    let lies = put(&mut state, P0, "No More Lies", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B, ManaColor::B], 1);
    opponent_casts(&mut state, specter, &[]);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 0);
    cast(&mut state, lies, &[Target::Object(specter)]);
    settle_paying(&mut state, true);
    assert_eq!(zone(&state, specter), Zone::Exile);

    // A spell cast with flashback is exiled either way.
    let mut state = plains();
    let souls = put(&mut state, P1, "Lingering Souls", Zone::Graveyard);
    let lies = put(&mut state, P0, "No More Lies", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::B], 1);
    opponent_casts(&mut state, souls, &[]);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 0);
    cast(&mut state, lies, &[Target::Object(souls)]);
    settle_paying(&mut state, true);
    assert_eq!(zone(&state, souls), Zone::Exile);
    assert!(named(&state, P1, "White Flying Spirit Token").is_empty());
}

#[test]
fn no_more_lies_does_nothing_when_its_target_spell_is_already_gone() {
    let mut state = plains();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let lies = put(&mut state, P0, "No More Lies", Zone::Hand);
    let second = put(&mut state, P0, "No More Lies", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    opponent_casts(&mut state, bolt, &[Target::Player(P0)]);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::U, ManaColor::W, ManaColor::U],
        0,
    );
    // The second No More Lies resolves first and exiles the Bolt; the first
    // then has no legal target and does nothing.
    cast(&mut state, lies, &[Target::Object(bolt)]);
    cast(&mut state, second, &[Target::Object(bolt)]);
    settle_paying(&mut state, true);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(zone(&state, lies), Zone::Graveyard);
    assert_eq!(zone(&state, second), Zone::Graveyard);
}

// ---- Traumatic Critique ----------------------------------------------------------

#[test]
fn traumatic_critique_deals_x_damage_then_draws_two_and_discards_one() {
    let mut state = plains();
    let critique = put(&mut state, P0, "Traumatic Critique", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 3);
    cast_x(&mut state, critique, 3, &[Target::Player(P1)]);
    settle_first(&mut state);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(hand_len(&state, P0), 1, "drew two, discarded one");
    assert_eq!(library_len(&state, P0), 18);
    // The critique and the discarded card.
    assert_eq!(state.players[0].graveyard.len(), 2);
    assert_eq!(
        state.players[0].mana_pool, [0; 6],
        "X, {{U}} and {{R}} paid"
    );
}

#[test]
fn traumatic_critique_with_x_zero_still_draws_and_discards() {
    let mut state = plains();
    let critique = put(&mut state, P0, "Traumatic Critique", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 0);
    cast_x(&mut state, critique, 0, &[Target::Player(P1)]);
    settle_first(&mut state);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(hand_len(&state, P0), 1);
    assert_eq!(state.players[0].graveyard.len(), 2);
}

#[test]
fn traumatic_critique_x_damage_hits_creatures_and_the_caster_chooses_the_discard() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let keep = put(&mut state, P0, "Mountain", Zone::Hand);
    let critique = put(&mut state, P0, "Traumatic Critique", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 2);
    cast_x(&mut state, critique, 2, &[Target::Object(specter)]);
    // The discard is the caster's choice among the hand, which holds the
    // Mountain and the two drawn Plains.
    let mut hand_at_discard = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::Discard {
            player,
            count,
            choices,
            ..
        } => {
            assert_eq!((player, count), (P0, 1));
            hand_at_discard = choices.clone();
            Action::Discard(vec![choices
                .into_iter()
                .find(|&card| card != keep)
                .unwrap()])
        }
        other => first_answer(other),
    });
    assert_eq!(
        zone(&state, specter),
        Zone::Graveyard,
        "two damage to a 2/2"
    );
    assert_eq!(hand_at_discard.len(), 3);
    assert!(hand_at_discard.contains(&keep));
    assert!(hand_contains(&state, P0, keep));
    assert_eq!(hand_len(&state, P0), 2);
}

#[test]
fn traumatic_critique_needs_red_and_blue_and_does_nothing_if_its_target_is_gone() {
    let mut state = plains();
    let critique = put(&mut state, P0, "Traumatic Critique", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(!castable(&mut state, critique), "needs {{R}} as well");

    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast_x(&mut state, critique, 2, &[Target::Object(specter)]);
    pass_priority(&mut state);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    // With its only target gone the whole spell is countered on resolution:
    // nothing is drawn or discarded.
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(library_len(&state, P0), 20);
    assert_eq!(zone(&state, critique), Zone::Graveyard);
}

// ---- Kolaghan's Command ----------------------------------------------------------

/// The mode index of each pair of Kolaghan's Command's modes, in printed
/// order: return a creature card (A), discard (B), destroy an artifact (C),
/// 2 damage (D).
const KC_AB: u8 = 0;
const KC_AC: u8 = 1;
const KC_AD: u8 = 2;
const KC_BC: u8 = 3;
const KC_BD: u8 = 4;
const KC_CD: u8 = 5;

/// P0's hand holds Kolaghan's Command with {1}{B}{R} floating.
fn kolaghans_command_in_hand(state: &mut GameState) -> ObjectId {
    let command = put(state, P0, "Kolaghan's Command", Zone::Hand);
    pool(state, P0, &[ManaColor::B, ManaColor::R], 1);
    command
}

/// The modes offered when casting `spell`.
fn offered_modes(state: &GameState, spell: ObjectId) -> Vec<u8> {
    let mut probe = state.clone();
    act(&mut probe, Action::CastSpell(spell));
    match next(&mut probe) {
        Decision::ChooseSpellMode { legal_modes, .. } => legal_modes,
        // A single legal mode is chosen silently.
        Decision::ChooseTargets { .. } => {
            vec![probe
                .engine
                .pending_cast
                .as_ref()
                .and_then(|cast| cast.mode_chosen)
                .expect("a chosen mode")]
        }
        other => panic!("expected the mode choice, got {other:?}"),
    }
}

#[test]
fn kolaghans_command_returns_a_creature_card_and_makes_a_player_discard() {
    let mut state = plains();
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Graveyard);
    let land = put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P1, "Mountain", Zone::Hand);
    put(&mut state, P1, "Island", Zone::Hand);
    let command = kolaghans_command_in_hand(&mut state);
    // Only creature cards of your own graveyard are legal for the first.
    assert!(offered_modes(&state, command).contains(&KC_AB));
    cast_mode(
        &mut state,
        command,
        KC_AB,
        &[Target::Object(specter), Target::Player(P1)],
    );
    settle_first(&mut state);
    assert!(hand_contains(&state, P0, specter));
    assert_eq!(zone(&state, land), Zone::Graveyard);
    assert_eq!(hand_len(&state, P1), 1, "P1 discarded a card");
    assert_eq!(state.players[1].graveyard.len(), 1);
    assert_eq!(zone(&state, command), Zone::Graveyard);
}

#[test]
fn kolaghans_command_returns_a_creature_card_and_destroys_an_artifact() {
    let mut state = plains();
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Graveyard);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let command = kolaghans_command_in_hand(&mut state);
    // The opponent's graveyard creature is not legal, nor is a non-artifact.
    let foe = put(&mut state, P1, "Hypnotic Specter", Zone::Graveyard);
    act(&mut state, Action::CastSpell(command));
    match next(&mut state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&KC_AC));
            act(&mut state, Action::ChooseSpellMode(KC_AC));
        }
        other => panic!("{other:?}"),
    }
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(specter)));
            assert!(!legal_targets.contains(&Target::Object(foe)));
            act(&mut state, Action::ChooseTarget(Target::Object(specter)));
        }
        other => panic!("{other:?}"),
    }
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(ring)));
            assert!(!legal_targets.contains(&Target::Object(elves)));
            act(&mut state, Action::ChooseTarget(Target::Object(ring)));
        }
        other => panic!("{other:?}"),
    }
    settle_first(&mut state);
    assert!(hand_contains(&state, P0, specter));
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
}

#[test]
fn kolaghans_command_returns_a_creature_card_and_deals_two_to_any_target() {
    let mut state = plains();
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Graveyard);
    let foe = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_AD,
        &[Target::Object(specter), Target::Object(foe)],
    );
    settle_first(&mut state);
    assert!(hand_contains(&state, P0, specter));
    assert_eq!(zone(&state, foe), Zone::Graveyard, "two damage to a 2/2");

    let mut state = plains();
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Graveyard);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_AD,
        &[Target::Object(specter), Target::Player(P1)],
    );
    settle_first(&mut state);
    assert!(hand_contains(&state, P0, specter));
    assert_eq!(life(&state, P1), 18);
}

#[test]
fn kolaghans_command_makes_a_player_discard_and_destroys_an_artifact() {
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    put(&mut state, P1, "Mountain", Zone::Hand);
    put(&mut state, P1, "Island", Zone::Hand);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_BC,
        &[Target::Player(P1), Target::Object(ring)],
    );
    settle_first(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(hand_len(&state, P1), 1);
}

#[test]
fn kolaghans_command_may_discard_and_damage_the_same_player() {
    let mut state = plains();
    put(&mut state, P1, "Mountain", Zone::Hand);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_BD,
        &[Target::Player(P1), Target::Player(P1)],
    );
    settle_first(&mut state);
    assert_eq!(hand_len(&state, P1), 0);
    assert_eq!(life(&state, P1), 18);

    // Targeting yourself for the discard is allowed too: the caster chooses
    // nothing, the target player picks their own card.
    let mut state = plains();
    put(&mut state, P0, "Mountain", Zone::Hand);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_BD,
        &[Target::Player(P0), Target::Player(P1)],
    );
    settle_first(&mut state);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(life(&state, P1), 18);
}

#[test]
fn kolaghans_command_resolves_its_modes_in_printed_order() {
    // The discard is printed before "destroy target artifact" and before the
    // damage: when the target player chooses their card, the later mode has
    // not happened yet.
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let mountain = put(&mut state, P1, "Mountain", Zone::Hand);
    let island = put(&mut state, P1, "Island", Zone::Hand);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_BC,
        &[Target::Player(P1), Target::Object(ring)],
    );
    let choice = settle(&mut state).expect("the target player chooses a card");
    assert!(
        matches!(&choice, Decision::ChooseEffectTargets {
            player, legal_targets, .. }
            if *player == P1 && legal_targets.len() == 2
                && legal_targets.contains(&Target::Object(mountain))
                && legal_targets.contains(&Target::Object(island))),
        "{choice:?}"
    );
    assert_eq!(zone(&state, ring), Zone::Battlefield, "not destroyed yet");
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(island)),
    );
    settle_first(&mut state);
    assert_eq!(zone(&state, island), Zone::Graveyard);
    assert_eq!(zone(&state, mountain), Zone::Hand);
    assert_eq!(zone(&state, ring), Zone::Graveyard);

    let mut state = plains();
    put(&mut state, P1, "Mountain", Zone::Hand);
    put(&mut state, P1, "Island", Zone::Hand);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_BD,
        &[Target::Player(P1), Target::Player(P1)],
    );
    let choice = settle(&mut state).expect("the target player chooses a card");
    assert!(matches!(&choice, Decision::ChooseEffectTargets { player, .. } if *player == P1));
    assert_eq!(life(&state, P1), 20, "not damaged yet");
    settle_first(&mut state);
    assert_eq!(hand_len(&state, P1), 1);
    assert_eq!(life(&state, P1), 18);

    // A creature card returned first is in hand when the same player
    // discards.
    let mut state = plains();
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Graveyard);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_AB,
        &[Target::Object(specter), Target::Player(P0)],
    );
    settle_first(&mut state);
    assert_eq!(
        zone(&state, specter),
        Zone::Graveyard,
        "returned, then discarded as the only card in hand"
    );
    assert_eq!(hand_len(&state, P0), 0);
}

#[test]
fn kolaghans_command_destroys_an_artifact_creature_and_hits_it_for_two() {
    let mut state = plains();
    let wurm = put(&mut state, P1, "Wurmcoil Engine", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_CD,
        &[Target::Object(ring), Target::Object(wurm)],
    );
    settle_first(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(state.objects.get(wurm).damage, 2);

    // Each slot may name the same artifact creature.
    let mut state = plains();
    let wurm = put(&mut state, P1, "Wurmcoil Engine", Zone::Battlefield);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_CD,
        &[Target::Object(wurm), Target::Object(wurm)],
    );
    settle_first(&mut state);
    assert_eq!(zone(&state, wurm), Zone::Graveyard);
}

#[test]
fn kolaghans_command_offers_only_the_pairs_it_can_target() {
    let mut state = plains();
    let command = kolaghans_command_in_hand(&mut state);
    // No creature card in your graveyard, no artifact: only discard and
    // damage, which need only players.
    assert_eq!(offered_modes(&state, command), vec![KC_BD]);

    put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let mut modes = offered_modes(&state, command);
    modes.sort_unstable();
    assert_eq!(modes, vec![KC_BC, KC_BD, KC_CD]);

    put(&mut state, P0, "Hypnotic Specter", Zone::Graveyard);
    let mut modes = offered_modes(&state, command);
    modes.sort_unstable();
    assert_eq!(modes, vec![KC_AB, KC_AC, KC_AD, KC_BC, KC_BD, KC_CD]);

    // Instant speed.
    state.step = Step::Upkeep;
    assert!(castable(&mut state, command));
}

#[test]
fn kolaghans_command_still_does_its_other_mode_when_one_target_is_gone() {
    // The artifact is destroyed in response: the discard still happens (each
    // chosen mode checks its own target on resolution).
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    put(&mut state, P1, "Mountain", Zone::Hand);
    put(&mut state, P1, "Island", Zone::Hand);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_BC,
        &[Target::Player(P1), Target::Object(ring)],
    );
    pass_priority(&mut state);
    let grudge = put(&mut state, P1, "Ancient Grudge", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 1);
    cast(&mut state, grudge, &[Target::Object(ring)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(hand_len(&state, P1), 1, "the discard still happened");
}

#[test]
fn kolaghans_command_does_nothing_when_both_targets_are_gone() {
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let foe = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let command = kolaghans_command_in_hand(&mut state);
    cast_mode(
        &mut state,
        command,
        KC_CD,
        &[Target::Object(ring), Target::Object(foe)],
    );
    pass_priority(&mut state);
    let grudge = put(&mut state, P1, "Ancient Grudge", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R, ManaColor::R], 1);
    cast(&mut state, grudge, &[Target::Object(ring)]);
    cast(&mut state, bolt, &[Target::Object(foe)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, foe), Zone::Graveyard);
    assert_eq!(zone(&state, command), Zone::Graveyard);
}

// ---- Helpers of the second half --------------------------------------------------

const R: ManaColor = ManaColor::R;
const G: ManaColor = ManaColor::G;
const B: ManaColor = ManaColor::B;
const U: ManaColor = ManaColor::U;

/// Casts `spell`, choosing `targets` and then finishing an "up to" or "any
/// number of" target selection.
fn cast_picking(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    for target in targets {
        choose_target(state, *target);
    }
    if let Decision::ChooseTargets {
        can_finish: true, ..
    } = next(state)
    {
        act(state, Action::FinishEffectSelection);
    }
    cast_finished(state);
}

/// Lets a cast whose last choice was made finish (costs paid, spell put on
/// the stack): its caster then holds priority.
fn cast_finished(state: &mut GameState) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { .. }),
        "the cast completes"
    );
}

/// The lands `P0`'s priority window offers to play.
fn land_drops(state: &mut GameState) -> Vec<ObjectId> {
    match next(state) {
        Decision::CastSpellOrPass { land_drops, .. } => land_drops,
        other => panic!("expected priority, got {other:?}"),
    }
}

/// How many play permissions from exile `object` has.
fn permissions(state: &GameState, object: ObjectId) -> usize {
    state
        .engine
        .exile_play_permissions
        .iter()
        .filter(|permission| permission.object == object)
        .count()
}

/// Settles, answering each card selection with the next of `picks`.
fn settle_picking_cards(state: &mut GameState, picks: &[ObjectId]) {
    let mut picks = picks.to_vec();
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if !picks.is_empty() => {
            let pick = picks.remove(0);
            assert!(
                legal_targets.contains(&Target::Object(pick)),
                "{pick:?} not in {legal_targets:?}"
            );
            Action::ChooseEffectTarget(Target::Object(pick))
        }
        Decision::ChooseEffectTargets {
            selected_count: 1, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert!(picks.is_empty(), "unused picks {picks:?}");
}

/// Deals `amount` damage from `source` to `object`.
fn damage(state: &mut GameState, source: ObjectId, object: ObjectId, amount: i32) {
    event::propose_and_commit(
        state,
        ProposedEvent::damage(source, Target::Object(object), amount),
    );
}

/// Removes `object` from the game's zones it is in, to exile.
fn exile(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
}

/// Plays on to P0's next turn (through P1's whole turn), stopping in its
/// first main phase.
fn to_next_turn_of_p0(state: &mut GameState) {
    pass_until(state, |state| state.active_player == P1);
    pass_until(state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
}

// ---- Dead // Gone ------------------------------------------------------------------

const DEAD: u8 = 0;
const GONE: u8 = 1;

#[test]
fn dead_deals_two_damage_to_a_creature_at_instant_speed() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let card = put(&mut state, P0, "Dead // Gone", Zone::Hand);
    pool(&mut state, P0, &[R], 0);

    // Only Dead is affordable, so its half is chosen without asking, and a
    // creature is its only kind of target.
    let legal = cast_targets(&state, card);
    assert!(legal.contains(&Target::Object(specter)));
    assert!(legal.contains(&Target::Object(wall)));
    assert!(!legal
        .iter()
        .any(|target| matches!(target, Target::Player(_))));
    state.step = Step::Upkeep;
    assert!(castable(&mut state, card), "an instant");
    state.step = Step::Main1;

    cast(&mut state, card, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(
        zone(&state, card),
        Zone::Graveyard,
        "a split card is not exiled"
    );
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));

    // Two damage does not kill a 0/5.
    let card = put(&mut state, P0, "Dead // Gone", Zone::Hand);
    pool(&mut state, P0, &[R], 0);
    cast_mode(&mut state, card, DEAD, &[Target::Object(wall)]);
    settled(&mut state);
    assert_eq!(zone(&state, wall), Zone::Battlefield);
    assert_eq!(state.objects.get(wall).damage, 2);
}

#[test]
fn dead_does_nothing_when_its_creature_is_gone() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let card = put(&mut state, P0, "Dead // Gone", Zone::Hand);
    pool(&mut state, P0, &[R], 0);
    cast(&mut state, card, &[Target::Object(specter)]);
    cast_finished(&mut state);
    exile(&mut state, specter);
    settled(&mut state);
    assert_eq!(zone(&state, card), Zone::Graveyard);
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 20));
}

#[test]
fn gone_returns_a_creature_you_dont_control_to_its_owners_hand() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Wall of Roots", Zone::Battlefield);
    let foe = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let card = put(&mut state, P0, "Dead // Gone", Zone::Hand);
    pool(&mut state, P0, &[R], 2);
    assert!(castable(&mut state, card));
    act(&mut state, Action::CastSpell(card));
    match next(&mut state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&DEAD) && legal_modes.contains(&GONE));
        }
        other => panic!("expected the half to cast, got {other:?}"),
    }
    act(&mut state, Action::ChooseSpellMode(GONE));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(foe)));
            assert!(
                !legal_targets.contains(&Target::Object(mine)),
                "a creature you control is not a target"
            );
            assert!(!legal_targets
                .iter()
                .any(|target| matches!(target, Target::Player(_))));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(foe)));
    settled(&mut state);
    assert_eq!(zone(&state, foe), Zone::Hand);
    assert!(hand_contains(&state, P1, foe), "its owner's hand");
    assert_eq!(zone(&state, mine), Zone::Battlefield);
    assert_eq!(
        zone(&state, card),
        Zone::Graveyard,
        "a split card is not exiled"
    );
    assert_eq!(state.players[0].mana_pool, [0; 6], "{{2}}{{R}} paid");
}

#[test]
fn gone_does_nothing_when_its_creature_is_gone_and_needs_all_three_mana() {
    // {R}{1} pays for Dead only: no half is asked for, and it is Dead's.
    let mut state = plains();
    put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let card = put(&mut state, P0, "Dead // Gone", Zone::Hand);
    pool(&mut state, P0, &[R], 1);
    act(&mut state, Action::CastSpell(card));
    assert!(
        matches!(next(&mut state), Decision::ChooseTargets { .. }),
        "only Dead is affordable"
    );

    let mut state = plains();
    let foe = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let card = put(&mut state, P0, "Dead // Gone", Zone::Hand);
    pool(&mut state, P0, &[R], 2);
    cast_mode(&mut state, card, GONE, &[Target::Object(foe)]);
    cast_finished(&mut state);
    exile(&mut state, foe);
    settled(&mut state);
    assert_eq!(zone(&state, card), Zone::Graveyard);
    assert_eq!(zone(&state, foe), Zone::Exile);
}

// ---- Delayed Blast Fireball --------------------------------------------------------

#[test]
fn delayed_blast_fireball_deals_two_to_each_opponent_and_each_creature_they_control() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Wall of Roots", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let fireball = put(&mut state, P0, "Delayed Blast Fireball", Zone::Hand);
    pool(&mut state, P0, &[R, R], 1);
    cast(&mut state, fireball, &[]);
    settled(&mut state);
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 18));
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(zone(&state, wall), Zone::Battlefield);
    assert_eq!(state.objects.get(wall).damage, 2);
    assert_eq!(
        state.objects.get(mine).damage,
        0,
        "its own creatures are safe"
    );
    assert_eq!(zone(&state, fireball), Zone::Graveyard);
}

#[test]
fn delayed_blast_fireball_foretold_and_cast_from_exile_deals_five() {
    let mut state = plains();
    let mine = put(&mut state, P0, "Hypnotic Specter", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let fireball = put(&mut state, P0, "Delayed Blast Fireball", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    assert!(
        !activatable(&mut state, fireball, 0),
        "foretelling costs {{2}}"
    );
    pool(&mut state, P0, &[], 1);
    act(&mut state, Action::ActivateAbility(fireball, 0));
    assert!(state.stack.is_empty(), "a special action");
    assert_eq!(zone(&state, fireball), Zone::Exile);
    assert_eq!(
        state
            .objects
            .get(fireball)
            .v4
            .face_down_v1
            .map(|f| f.lookers),
        Some(1)
    );
    pool(&mut state, P0, &[R, R], 4);
    assert!(
        !castable(&mut state, fireball),
        "not the turn it was foretold"
    );

    // P0's next turn: castable for {4}{R}{R}, never for its mana cost.
    to_next_turn_of_p0(&mut state);
    pool(&mut state, P0, &[R, R], 1);
    assert!(
        !castable(&mut state, fireball),
        "{{4}}{{R}}{{R}} is not yet paid"
    );
    pool(&mut state, P0, &[], 3);
    cast(&mut state, fireball, &[]);
    settled(&mut state);
    assert_eq!((life(&state, P0), life(&state, P1)), (20, 15));
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(
        zone(&state, wall),
        Zone::Graveyard,
        "five damage kills a 0/5"
    );
    assert_eq!(state.objects.get(mine).damage, 0);
    assert_eq!(zone(&state, fireball), Zone::Graveyard);
}

#[test]
fn delayed_blast_fireball_is_foretold_only_during_its_owners_turn() {
    let mut state = plains();
    let fireball = put(&mut state, P1, "Delayed Blast Fireball", Zone::Hand);
    pool(&mut state, P1, &[], 2);
    pass_until(&mut state, |state| state.priority_player == P1);
    let (_, abilities) = priority(&mut state);
    assert!(!abilities.contains(&(fireball, 0)));
}

// ---- Flashback -----------------------------------------------------------------------

#[test]
fn flashback_lets_an_instant_or_sorcery_be_cast_from_the_graveyard_for_its_mana_cost() {
    let mut state = plains();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Hypnotic Specter", Zone::Graveyard);
    put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let flashback = put(&mut state, P0, "Flashback", Zone::Hand);
    pool(&mut state, P0, &[R], 0);

    // Only an instant or sorcery card in your own graveyard.
    assert_eq!(cast_targets(&state, flashback), vec![Target::Object(bolt)]);
    assert!(!castable(&mut state, bolt), "no flashback yet");
    cast(&mut state, flashback, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, flashback), Zone::Graveyard);
    assert!(!castable(&mut state, bolt), "{{R}} is not in the pool");

    // The flashback cost is its mana cost, {R}; then it is exiled.
    pool(&mut state, P0, &[R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(zone(&state, bolt), Zone::Exile, "flashback exiles it");
}

#[test]
fn flashback_costs_a_larger_mana_cost_in_full_and_lasts_only_this_turn() {
    let mut state = plains();
    let wheel = put(&mut state, P0, "Wheel of Fortune", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let flashback = put(&mut state, P0, "Flashback", Zone::Hand);
    pool(&mut state, P0, &[R], 0);
    cast(&mut state, flashback, &[Target::Object(wheel)]);
    settled(&mut state);
    pool(&mut state, P0, &[R], 0);
    assert!(
        !castable(&mut state, wheel),
        "{{2}}{{R}} is its flashback cost"
    );
    pool(&mut state, P0, &[], 2);
    assert!(castable(&mut state, wheel));
    assert!(
        !castable(&mut state, bolt),
        "only the targeted card gains flashback"
    );

    // Next turn the grant is gone.
    state.players[0].mana_pool = [0; 6];
    to_next_turn_of_p0(&mut state);
    pool(&mut state, P0, &[R], 2);
    assert!(!castable(&mut state, wheel), "until end of turn only");
    assert_eq!(zone(&state, wheel), Zone::Graveyard);
}

#[test]
fn flashback_does_nothing_when_its_card_is_gone() {
    let mut state = plains();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let flashback = put(&mut state, P0, "Flashback", Zone::Hand);
    pool(&mut state, P0, &[R], 0);
    cast(&mut state, flashback, &[Target::Object(bolt)]);
    cast_finished(&mut state);
    exile(&mut state, bolt);
    settled(&mut state);
    pool(&mut state, P0, &[R], 0);
    assert_eq!(zone(&state, flashback), Zone::Graveyard);
    assert!(!castable(&mut state, bolt), "it is in exile, not castable");
}

// ---- Impractical Joke ----------------------------------------------------------------

#[test]
fn impractical_joke_deals_three_damage_to_a_creature_or_planeswalker() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, karn), Some(5));

    let joke = put(&mut state, P0, "Impractical Joke", Zone::Hand);
    pool(&mut state, P0, &[R], 0);
    let legal = cast_targets(&state, joke);
    for target in [specter, wall, karn] {
        assert!(legal.contains(&Target::Object(target)), "{target:?}");
    }
    assert!(!legal
        .iter()
        .any(|target| matches!(target, Target::Player(_))));
    cast_picking(&mut state, joke, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(zone(&state, joke), Zone::Graveyard);

    let joke = put(&mut state, P0, "Impractical Joke", Zone::Hand);
    pool(&mut state, P0, &[R], 0);
    cast_picking(&mut state, joke, &[Target::Object(karn)]);
    settled(&mut state);
    assert_eq!(loyalty(&state, karn), Some(2));

    // Three damage does not kill a 0/5; a sorcery.
    let joke = put(&mut state, P0, "Impractical Joke", Zone::Hand);
    pool(&mut state, P0, &[R], 0);
    cast_picking(&mut state, joke, &[Target::Object(wall)]);
    settled(&mut state);
    assert_eq!(zone(&state, wall), Zone::Battlefield);
    assert_eq!(state.objects.get(wall).damage, 3);
    let joke = put(&mut state, P0, "Impractical Joke", Zone::Hand);
    pool(&mut state, P0, &[R], 0);
    state.step = Step::Upkeep;
    assert!(!castable(&mut state, joke));
}

#[test]
fn impractical_joke_may_have_no_target_and_still_stops_prevention() {
    let mut state = plains();
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    activate(&mut state, mother, 0, &[Target::Object(elves)]);
    settle_option(&mut state, 3);
    damage(&mut state, pup, elves, 1);
    assert_eq!(
        state.objects.get(elves).damage,
        0,
        "protection from red prevents the damage"
    );

    // No target: "up to one". Damage can't be prevented this turn.
    let joke = put(&mut state, P0, "Impractical Joke", Zone::Hand);
    pool(&mut state, P0, &[R], 0);
    // A creature with protection from red is not a legal target either.
    assert!(!cast_targets(&state, joke).contains(&Target::Object(elves)));
    cast_picking(&mut state, joke, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, joke), Zone::Graveyard);
    damage(&mut state, pup, elves, 1);
    assert_eq!(
        state.objects.get(elves).damage,
        1,
        "protection no longer prevents it this turn"
    );
}

#[test]
fn impractical_joke_with_its_target_gone_does_not_stop_prevention() {
    let mut state = plains();
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let other = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    activate(&mut state, mother, 0, &[Target::Object(elves)]);
    settle_option(&mut state, 3);
    let joke = put(&mut state, P0, "Impractical Joke", Zone::Hand);
    pool(&mut state, P0, &[R], 0);
    cast_picking(&mut state, joke, &[Target::Object(pup)]);
    exile(&mut state, pup);
    settled(&mut state);
    assert_eq!(zone(&state, joke), Zone::Graveyard);
    damage(&mut state, other, elves, 1);
    assert_eq!(
        state.objects.get(elves).damage,
        0,
        "a Joke with no legal target never resolves"
    );
}

#[test]
fn impractical_joke_stops_a_fog_from_preventing_combat_damage_this_turn() {
    for joke_first in [false, true] {
        let mut state = plains();
        let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
        let fog = put(&mut state, P1, "Fog", Zone::Hand);
        if joke_first {
            let joke = put(&mut state, P0, "Impractical Joke", Zone::Hand);
            pool(&mut state, P0, &[R], 0);
            cast_picking(&mut state, joke, &[]);
            settled(&mut state);
        }
        // Mana for the Fog comes from a land: a mana pool empties between
        // steps.
        put(&mut state, P1, "Forest", Zone::Battlefield);
        let mut fogged = false;
        attack_with(&mut state, vec![pup], |_, decision| match decision {
            Decision::CastSpellOrPass {
                player: P1,
                castable_spells,
                ..
            } if !fogged && castable_spells.contains(&fog) => {
                fogged = true;
                Some(Action::CastSpell(fog))
            }
            _ => None,
        });
        assert!(fogged, "Fog was cast");
        assert_eq!(
            life(&state, P1),
            if joke_first { 18 } else { 20 },
            "Joke first: {joke_first}"
        );
    }
}

// ---- Escape to the Wilds -------------------------------------------------------------

fn escape_library() -> GameState {
    let mut library = vec![
        "Mountain",
        "Forest",
        "Lightning Bolt",
        "Wall of Roots",
        "Plains",
    ];
    library.extend(["Island"; 15]);
    game(&library)
}

#[test]
fn escape_to_the_wilds_exiles_five_and_allows_an_additional_land_this_turn() {
    let mut state = escape_library();
    let top = state.players[0].library[..5].to_vec();
    let (mountain, forest, bolt, wall, plains_card) = (top[0], top[1], top[2], top[3], top[4]);
    let hand_land = put(&mut state, P0, "Swamp", Zone::Hand);
    let escape = put(&mut state, P0, "Escape to the Wilds", Zone::Hand);
    pool(&mut state, P0, &[R, G], 3);
    cast(&mut state, escape, &[]);
    settled(&mut state);
    for card in &top {
        assert_eq!(zone(&state, *card), Zone::Exile);
    }
    assert_eq!(library_len(&state, P0), 15);
    assert_eq!(zone(&state, escape), Zone::Graveyard);

    // One land from the hand, one exiled land: two this turn, never three.
    let drops = land_drops(&mut state);
    assert!(drops.contains(&hand_land) && drops.contains(&mountain));
    assert!(drops.contains(&forest) && drops.contains(&plains_card));
    act(&mut state, Action::PlayLand(hand_land));
    settled(&mut state);
    let drops = land_drops(&mut state);
    assert!(drops.contains(&mountain), "an additional land play");
    act(&mut state, Action::PlayLand(mountain));
    settled(&mut state);
    assert!(
        land_drops(&mut state).is_empty(),
        "only one additional land"
    );
    assert_eq!(zone(&state, mountain), Zone::Battlefield);

    // The exiled spells are cast this turn for their costs.
    pool(&mut state, P0, &[R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);

    // Next turn: only the ordinary land play, still the exiled cards.
    to_next_turn_of_p0(&mut state);
    assert_eq!(permissions(&state, wall), 1);
    let drops = land_drops(&mut state);
    assert!(drops.contains(&forest) && drops.contains(&plains_card));
    act(&mut state, Action::PlayLand(forest));
    settled(&mut state);
    assert!(
        land_drops(&mut state).is_empty(),
        "the additional land play was for one turn"
    );
    pool(&mut state, P0, &[G], 1);
    cast(&mut state, wall, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, wall), Zone::Battlefield);
}

#[test]
fn escape_to_the_wilds_cards_may_be_played_until_the_end_of_your_next_turn() {
    let mut state = escape_library();
    let top = state.players[0].library[..5].to_vec();
    let bolt = top[2];
    let escape = put(&mut state, P0, "Escape to the Wilds", Zone::Hand);
    pool(&mut state, P0, &[R, G], 3);
    cast(&mut state, escape, &[]);
    settled(&mut state);
    assert_eq!(permissions(&state, bolt), 1);

    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(permissions(&state, bolt), 1, "through the opponent's turn");
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    assert_eq!(permissions(&state, bolt), 1, "through your next turn");
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(permissions(&state, bolt), 0, "gone after your next turn");
    assert_eq!(zone(&state, bolt), Zone::Exile);
}

#[test]
fn escape_to_the_wilds_exiles_fewer_cards_from_a_short_library() {
    let mut state = game(&["Mountain", "Forest"]);
    let escape = put(&mut state, P0, "Escape to the Wilds", Zone::Hand);
    pool(&mut state, P0, &[R, G], 3);
    cast(&mut state, escape, &[]);
    settled(&mut state);
    assert_eq!(library_len(&state, P0), 0);
    assert_eq!(life(&state, P0), 20);
}

// ---- Expressive Iteration ------------------------------------------------------------

#[test]
fn expressive_iteration_puts_one_in_hand_one_on_the_bottom_and_exiles_one_playable_this_turn() {
    let mut state = game(&[
        "Mountain",
        "Lightning Bolt",
        "Island",
        "Plains",
        "Plains",
        "Plains",
    ]);
    let top = state.players[0].library[..3].to_vec();
    let (mountain, bolt, island) = (top[0], top[1], top[2]);
    let spell = put(&mut state, P0, "Expressive Iteration", Zone::Hand);
    pool(&mut state, P0, &[U, R], 0);
    cast(&mut state, spell, &[]);
    settle_picking_cards(&mut state, &[bolt, island]);
    assert!(hand_contains(&state, P0, bolt));
    assert_eq!(zone(&state, island), Zone::Library);
    assert_eq!(
        state.players[0].library.last(),
        Some(&island),
        "on the bottom"
    );
    assert_eq!(library_len(&state, P0), 4);
    assert_eq!(zone(&state, mountain), Zone::Exile);
    assert_eq!(zone(&state, spell), Zone::Graveyard);

    // The exiled land may be played this turn.
    assert!(land_drops(&mut state).contains(&mountain));
    act(&mut state, Action::PlayLand(mountain));
    settled(&mut state);
    assert_eq!(zone(&state, mountain), Zone::Battlefield);
}

#[test]
fn expressive_iteration_exiled_spell_is_cast_this_turn_only() {
    let mut state = game(&[
        "Mountain",
        "Lightning Bolt",
        "Wall of Roots",
        "Plains",
        "Plains",
        "Plains",
    ]);
    let top = state.players[0].library[..3].to_vec();
    let (mountain, bolt, wall) = (top[0], top[1], top[2]);
    let spell = put(&mut state, P0, "Expressive Iteration", Zone::Hand);
    pool(&mut state, P0, &[U, R], 0);
    cast(&mut state, spell, &[]);
    settle_picking_cards(&mut state, &[mountain, bolt]);
    assert!(hand_contains(&state, P0, mountain));
    assert_eq!(state.players[0].library.last(), Some(&bolt));
    assert_eq!(zone(&state, wall), Zone::Exile);

    // Cast from exile for its mana cost, {1}{G}.
    assert!(!castable(&mut state, wall), "no mana");
    pool(&mut state, P0, &[G], 1);
    cast(&mut state, wall, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, wall), Zone::Battlefield);

    // An exiled card not played is lost with the turn.
    let mut state = game(&[
        "Mountain",
        "Lightning Bolt",
        "Wall of Roots",
        "Plains",
        "Plains",
        "Plains",
    ]);
    let top = state.players[0].library[..3].to_vec();
    let spell = put(&mut state, P0, "Expressive Iteration", Zone::Hand);
    pool(&mut state, P0, &[U, R], 0);
    cast(&mut state, spell, &[]);
    settle_picking_cards(&mut state, &[top[0], top[1]]);
    assert_eq!(permissions(&state, top[2]), 1);
    to_next_turn_of_p0(&mut state);
    assert_eq!(permissions(&state, top[2]), 0);
    assert_eq!(zone(&state, top[2]), Zone::Exile);
}

#[test]
fn expressive_iteration_with_fewer_than_three_cards() {
    // Two cards: one to the hand, the other to the bottom, none exiled.
    let mut state = game(&["Mountain", "Island"]);
    let top = state.players[0].library.clone();
    let spell = put(&mut state, P0, "Expressive Iteration", Zone::Hand);
    pool(&mut state, P0, &[U, R], 0);
    cast(&mut state, spell, &[]);
    settle_picking_cards(&mut state, &[top[1]]);
    assert!(hand_contains(&state, P0, top[1]));
    assert_eq!(state.players[0].library, vec![top[0]]);
    assert!(state.exile.is_empty());

    // One card: it goes to the hand.
    let mut state = game(&["Mountain"]);
    let top = state.players[0].library.clone();
    let spell = put(&mut state, P0, "Expressive Iteration", Zone::Hand);
    pool(&mut state, P0, &[U, R], 0);
    cast(&mut state, spell, &[]);
    settle_first(&mut state);
    assert!(hand_contains(&state, P0, top[0]));
    assert_eq!(library_len(&state, P0), 0);
}

// ---- Fire Covenant -------------------------------------------------------------------

#[test]
fn fire_covenant_pays_life_and_divides_that_much_damage_among_creatures() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let mine = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let covenant = put(&mut state, P0, "Fire Covenant", Zone::Hand);
    pool(&mut state, P0, &[B, R], 1);

    // Any creature, either side, never a player.
    let legal = cast_targets(&state, covenant);
    for creature in [specter, elves, wall, mine] {
        assert!(legal.contains(&Target::Object(creature)));
    }
    assert!(!legal
        .iter()
        .any(|target| matches!(target, Target::Player(_))));

    // X is the number of damage assigned: 2 + 2 + 1.
    let picks = [specter, specter, elves, elves, wall].map(Target::Object);
    cast_picking(&mut state, covenant, &picks);
    assert_eq!(life(&state, P0), 15, "pay X life as the spell is cast");
    assert_eq!(state.stack.len(), 1);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, wall), Zone::Battlefield);
    assert_eq!(state.objects.get(wall).damage, 1);
    assert_eq!(state.objects.get(mine).damage, 0);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(zone(&state, covenant), Zone::Graveyard);
}

#[test]
fn fire_covenant_may_hit_one_creature_with_all_of_it_or_none() {
    let mut state = plains();
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let covenant = put(&mut state, P0, "Fire Covenant", Zone::Hand);
    pool(&mut state, P0, &[B, R], 1);
    let picks = [wall; 5].map(Target::Object);
    cast_picking(&mut state, covenant, &picks);
    assert_eq!(life(&state, P0), 15);
    settled(&mut state);
    assert_eq!(zone(&state, wall), Zone::Graveyard, "5 damage kills a 0/5");

    // Any number includes none: X is zero and no life is paid.
    let survivor = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let covenant = put(&mut state, P0, "Fire Covenant", Zone::Hand);
    pool(&mut state, P0, &[B, R], 1);
    cast_picking(&mut state, covenant, &[]);
    assert_eq!(life(&state, P0), 15);
    settled(&mut state);
    assert_eq!(zone(&state, covenant), Zone::Graveyard);
    assert_eq!(state.objects.get(survivor).damage, 0);
}

#[test]
fn fire_covenant_cannot_pay_more_life_than_the_caster_has() {
    let mut state = plains();
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    state.players[0].life = 3;
    let covenant = put(&mut state, P0, "Fire Covenant", Zone::Hand);
    pool(&mut state, P0, &[B, R], 1);
    act(&mut state, Action::CastSpell(covenant));
    for _ in 0..3 {
        match next(&mut state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(wall)));
            }
            other => panic!("expected another pick, got {other:?}"),
        }
        act(&mut state, Action::ChooseTarget(Target::Object(wall)));
    }
    // Three life, three picks: no fourth.
    if let Decision::ChooseTargets {
        legal_targets,
        can_finish,
        ..
    } = next(&mut state)
    {
        assert!(legal_targets.is_empty(), "{legal_targets:?}");
        assert!(can_finish);
    }
}

#[test]
fn fire_covenant_deals_the_rest_when_one_target_is_gone() {
    let mut state = plains();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let covenant = put(&mut state, P0, "Fire Covenant", Zone::Hand);
    pool(&mut state, P0, &[B, R], 1);
    cast_picking(
        &mut state,
        covenant,
        &[Target::Object(specter), Target::Object(elves)],
    );
    exile(&mut state, elves);
    settled(&mut state);
    assert_eq!(life(&state, P0), 18, "the life was paid anyway");
    assert_eq!(state.objects.get(specter).damage, 1);

    // Every target gone: nothing happens.
    let covenant = put(&mut state, P0, "Fire Covenant", Zone::Hand);
    pool(&mut state, P0, &[B, R], 1);
    cast_picking(&mut state, covenant, &[Target::Object(specter)]);
    exile(&mut state, specter);
    settled(&mut state);
    assert_eq!(life(&state, P0), 17);
    assert_eq!(zone(&state, covenant), Zone::Graveyard);
}

// ---- Pillage the Bog -----------------------------------------------------------------

fn pillage_library() -> GameState {
    let mut library = vec![
        "Mountain",
        "Island",
        "Swamp",
        "Forest",
        "Lightning Bolt",
        "Plains",
    ];
    library.extend(["Plains"; 14]);
    game(&library)
}

#[test]
fn pillage_the_bog_looks_at_twice_your_lands_takes_one_and_buries_the_rest() {
    let mut state = pillage_library();
    put(&mut state, P0, "Plains", Zone::Battlefield);
    put(&mut state, P0, "Plains", Zone::Battlefield);
    put(&mut state, P1, "Plains", Zone::Battlefield);
    let top = state.players[0].library[..6].to_vec();
    let pillage = put(&mut state, P0, "Pillage the Bog", Zone::Hand);
    pool(&mut state, P0, &[B, G], 0);
    cast(&mut state, pillage, &[]);
    // X is 4: the top four, of which the third goes to the hand.
    settle_picking_cards(&mut state, &[top[2]]);
    assert!(hand_contains(&state, P0, top[2]));
    assert_eq!(library_len(&state, P0), 19);
    assert_eq!(
        state.players[0].library[0], top[4],
        "the fifth card is on top"
    );
    let mut bottom = state.players[0]
        .library
        .iter()
        .rev()
        .take(3)
        .copied()
        .collect::<Vec<_>>();
    bottom.sort();
    let mut expected = vec![top[0], top[1], top[3]];
    expected.sort();
    assert_eq!(bottom, expected, "the rest on the bottom, in some order");
    assert_eq!(zone(&state, pillage), Zone::Graveyard);
}

#[test]
fn pillage_the_bog_with_no_lands_looks_at_nothing() {
    let mut state = pillage_library();
    let top = state.players[0].library[..2].to_vec();
    let pillage = put(&mut state, P0, "Pillage the Bog", Zone::Hand);
    pool(&mut state, P0, &[B, G], 0);
    cast(&mut state, pillage, &[]);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 0);
    assert_eq!(library_len(&state, P0), 20);
    assert_eq!(state.players[0].library[..2], top[..]);
}

#[test]
fn pillage_the_bog_in_a_short_library_looks_at_every_card() {
    let mut state = game(&["Mountain", "Island", "Swamp"]);
    put(&mut state, P0, "Plains", Zone::Battlefield);
    put(&mut state, P0, "Plains", Zone::Battlefield);
    put(&mut state, P0, "Plains", Zone::Battlefield);
    let top = state.players[0].library.clone();
    let pillage = put(&mut state, P0, "Pillage the Bog", Zone::Hand);
    pool(&mut state, P0, &[B, G], 0);
    cast(&mut state, pillage, &[]);
    settle_picking_cards(&mut state, &[top[1]]);
    assert!(hand_contains(&state, P0, top[1]));
    assert_eq!(library_len(&state, P0), 2);
}

#[test]
fn pillage_the_bog_can_be_plotted_and_cast_for_free_on_a_later_turn() {
    let mut state = pillage_library();
    put(&mut state, P0, "Plains", Zone::Battlefield);
    let pillage = put(&mut state, P0, "Pillage the Bog", Zone::Hand);
    pool(&mut state, P0, &[B, G], 1);
    match next(&mut state) {
        Decision::CastSpellOrPass { plot_actions, .. } => {
            assert!(plot_actions.contains(&pillage), "Plot {{1}}{{B}}{{G}}");
        }
        other => panic!("expected priority, got {other:?}"),
    }
    act(&mut state, Action::PlotSpell(pillage));
    assert_eq!(zone(&state, pillage), Zone::Exile);
    assert_eq!(state.players[0].mana_pool, [0; 6], "{{1}}{{B}}{{G}} paid");
    assert!(
        !castable(&mut state, pillage),
        "not the turn it was plotted"
    );

    to_next_turn_of_p0(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert!(castable(&mut state, pillage), "free on a later turn");
    let looked = state.players[0].library[..2].to_vec();
    cast(&mut state, pillage, &[]);
    settle_picking_cards(&mut state, &[looked[1]]);
    assert!(hand_contains(&state, P0, looked[1]));
    assert_eq!(zone(&state, pillage), Zone::Graveyard);
}

// ---- Vicious Rivalry -------------------------------------------------------------------

#[test]
fn vicious_rivalry_pays_x_life_and_destroys_artifacts_and_creatures_with_that_mana_value_or_less() {
    let mut state = plains();
    let my_ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let my_wall = put(&mut state, P0, "Wall of Roots", Zone::Battlefield);
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let land = put(&mut state, P1, "Plains", Zone::Battlefield);
    let rivalry = put(&mut state, P0, "Vicious Rivalry", Zone::Hand);
    pool(&mut state, P0, &[B, G], 2);
    cast_x(&mut state, rivalry, 2, &[]);
    assert_eq!(life(&state, P0), 18, "pay X life as the spell is cast");
    settled(&mut state);
    for destroyed in [my_ring, my_wall, pup, elves] {
        assert_eq!(zone(&state, destroyed), Zone::Graveyard, "{destroyed:?}");
    }
    assert_eq!(zone(&state, specter), Zone::Battlefield, "mana value 3");
    assert_eq!(
        zone(&state, land),
        Zone::Battlefield,
        "not an artifact or creature"
    );
    assert_eq!(zone(&state, rivalry), Zone::Graveyard);
}

#[test]
fn vicious_rivalry_with_x_zero_destroys_only_mana_value_zero_permanents() {
    let mut state = plains();
    let spirit = token(&mut state, P1, "White Flying Spirit Token");
    let marit_lage = token(&mut state, P1, "Marit Lage Token");
    let pup = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let rivalry = put(&mut state, P0, "Vicious Rivalry", Zone::Hand);
    pool(&mut state, P0, &[B, G], 2);
    cast_x(&mut state, rivalry, 0, &[]);
    assert_eq!(life(&state, P0), 20, "no life for X = 0");
    settled(&mut state);
    assert_eq!(zone(&state, spirit), Zone::Graveyard);
    assert_eq!(
        zone(&state, marit_lage),
        Zone::Battlefield,
        "indestructible"
    );
    assert_eq!(zone(&state, pup), Zone::Battlefield, "mana value 1");
}

#[test]
fn vicious_rivalry_cannot_announce_more_x_than_its_casters_life() {
    let mut state = plains();
    state.players[0].life = 3;
    let rivalry = put(&mut state, P0, "Vicious Rivalry", Zone::Hand);
    pool(&mut state, P0, &[B, G], 9);
    assert!(castable(&mut state, rivalry));
    act(&mut state, Action::CastSpell(rivalry));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 4, "X from 0 to 3");
        }
        other => panic!("expected the X announcement, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(2));
    cast_finished(&mut state);
    assert_eq!(life(&state, P0), 1, "X = 2 of 3 life");
    settled(&mut state);
}

#[test]
fn vicious_rivalry_is_a_sorcery() {
    let mut state = plains();
    let rivalry = put(&mut state, P0, "Vicious Rivalry", Zone::Hand);
    pool(&mut state, P0, &[B, G], 2);
    assert!(castable(&mut state, rivalry));
    state.step = Step::Upkeep;
    assert!(!castable(&mut state, rivalry));
}

// ---- Public-session consumer games --------------------------------------------------

/// The casts of this wave's cards seen in the observed actions.
#[derive(Default)]
struct C1cEvents {
    casts: std::collections::BTreeMap<&'static str, usize>,
}

impl C1cEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let Some(card_id) = semantic["source"]["card_db_id"].as_u64() else {
            return;
        };
        let name = CARD_DEFS[card_id as usize].name;
        if semantic["action_kind"].as_str() != Some("cast_spell") {
            return;
        }
        if let Some(card) = C1C_CARDS.iter().find(|card| **card == name) {
            *self.casts.entry(*card).or_default() += 1;
        }
    }
}

#[test]
fn c1c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let burn_and_souls = deck(&[
        ("Plateau", 4),
        ("Badlands", 3),
        ("Scrubland", 3),
        ("Mountain", 5),
        ("Plains", 3),
        ("Swamp", 2),
        ("Flame Slash", 3),
        ("Sear", 3),
        ("Tarfire", 3),
        ("Wheel of Fortune", 3),
        ("Lingering Souls", 4),
        ("Mine Collapse", 3),
        ("Kolaghan's Command", 4),
        ("Sol Ring", 3),
        ("Jackal Pup", 6),
        ("Hypnotic Specter", 4),
        ("Mother of Runes", 3),
    ]);
    let izzet_control = deck(&[
        ("Volcanic Island", 4),
        ("Tundra", 3),
        ("Island", 5),
        ("Mountain", 5),
        ("Plains", 3),
        ("No More Lies", 4),
        ("Traumatic Critique", 4),
        ("Lightning Bolt", 4),
        ("Mana Leak", 3),
        ("Jackal Pup", 6),
        ("Mother of Runes", 5),
    ]);

    let jund_rivalry = deck(&[
        ("Badlands", 4),
        ("Bayou", 4),
        ("Taiga", 4),
        ("Swamp", 1),
        ("Forest", 1),
        ("Mountain", 2),
        ("Pillage the Bog", 4),
        ("Vicious Rivalry", 4),
        ("Fire Covenant", 4),
        ("Escape to the Wilds", 4),
        ("Impractical Joke", 4),
        ("Dead // Gone", 4),
        ("Delayed Blast Fireball", 4),
        ("Jackal Pup", 4),
        ("Goblin Piledriver", 4),
        ("Hypnotic Specter", 2),
        ("Sol Ring", 2),
    ]);
    let izzet_iteration = deck(&[
        ("Volcanic Island", 4),
        ("Island", 4),
        ("Mountain", 6),
        ("Expressive Iteration", 4),
        ("Flashback", 4),
        ("Lightning Bolt", 4),
        ("Tarfire", 3),
        ("Mana Leak", 3),
        ("Jackal Pup", 4),
        ("Goblin Piledriver", 4),
        ("Sol Ring", 3),
    ]);

    let mut totals = C1cEvents::default();
    let mut games = Vec::new();
    for (pair, first_seed) in [
        (
            [burn_and_souls.clone(), izzet_control.clone()],
            0x4331_4300_u64,
        ),
        ([izzet_control, burn_and_souls], 0x4331_4310),
        ([jund_rivalry.clone(), izzet_iteration.clone()], 0x4331_4320),
        ([izzet_iteration, jund_rivalry], 0x4331_4330),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                games.push((pair.clone(), seed, policy));
            }
        }
    }
    // The games run on worker threads (`in_parallel`); the totals add their
    // casts in this loop's order.
    let casts = in_parallel(&games, |(pair, seed, policy)| {
        let mut events = C1cEvents::default();
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

    for card in C1C_CARDS {
        assert!(
            totals.casts.get(card).copied().unwrap_or(0) > 0,
            "{card} never cast"
        );
    }
}
