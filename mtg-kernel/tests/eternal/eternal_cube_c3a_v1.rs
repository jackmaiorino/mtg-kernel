//! Vintage Cube wave C3a: native rules and public-session tests for the
//! cheap creatures with existing analogs.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, AltCostCondition, CardCapability, CardDef, CardType, Keywords, Subtype,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

/// The cards of this wave that are finished (Full, with tests below).
/// Guardian Scalelord is not in this wave: it needs a granted until-end-of-turn attack trigger.
const C3A_CARDS: [&str; 10] = [
    "Auriok Salvagers",
    "Bloodbraid Challenger",
    "Death-Greeter's Champion",
    "Embereth Shieldbreaker",
    "Formidable Speaker",
    "Iron-Shield Elf",
    "Luminarch Aspirant",
    "Giver of Runes",
    "Harvester of Misery",
    "Devoted Druid",
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
        0x4355_4245_0000 + 0x3361,
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

/// Advances until the ability or spell just put forward is on the stack with
/// its costs paid (the engine finishes a pending activation or cast as it
/// next advances). Answers any other choice with the first legal answer.
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
fn c3a_cards_are_full() {
    for name in C3A_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c3a_cards_have_their_printed_characteristics() {
    let colored = Pip::Colored;
    let (w, r, g, b) = (ManaColor::W, ManaColor::R, ManaColor::G, ManaColor::B);
    for (name, generic, pips, subtypes, colors, power, toughness) in [
        (
            "Auriok Salvagers",
            3,
            vec![colored(w)],
            vec![Subtype::Human, Subtype::Soldier],
            vec![w],
            2,
            4,
        ),
        (
            "Bloodbraid Challenger",
            3,
            vec![colored(r), colored(g)],
            vec![Subtype::Elf, Subtype::Berserker],
            vec![r, g],
            4,
            3,
        ),
        (
            "Death-Greeter's Champion",
            2,
            vec![colored(r)],
            vec![Subtype::Human, Subtype::Warrior],
            vec![r],
            2,
            1,
        ),
        (
            "Embereth Shieldbreaker",
            1,
            vec![colored(r)],
            vec![Subtype::Human, Subtype::Knight],
            vec![r],
            2,
            1,
        ),
        (
            "Formidable Speaker",
            2,
            vec![colored(g)],
            vec![Subtype::Elf, Subtype::Druid],
            vec![g],
            2,
            4,
        ),
        (
            "Iron-Shield Elf",
            1,
            vec![colored(b)],
            vec![Subtype::Elf, Subtype::Warrior],
            vec![b],
            3,
            1,
        ),
        (
            "Luminarch Aspirant",
            1,
            vec![colored(w)],
            vec![Subtype::Human, Subtype::Cleric],
            vec![w],
            1,
            1,
        ),
        (
            "Giver of Runes",
            0,
            vec![colored(w)],
            vec![Subtype::Kor, Subtype::Cleric],
            vec![w],
            1,
            2,
        ),
        (
            "Harvester of Misery",
            3,
            vec![colored(b), colored(b)],
            vec![Subtype::Spirit],
            vec![b],
            5,
            4,
        ),
        (
            "Devoted Druid",
            1,
            vec![colored(g)],
            vec![Subtype::Elf, Subtype::Druid],
            vec![g],
            0,
            2,
        ),
    ] {
        let card = def(name);
        assert_eq!(card.types, &[CardType::Creature], "{name}");
        assert_eq!(card.cost.generic, generic, "{name}");
        assert_eq!(card.cost.x_count, 0, "{name}");
        assert_eq!(card.cost.pips, pips.as_slice(), "{name}");
        assert_eq!(
            usize::from(card.mana_value),
            usize::from(generic) + pips.len(),
            "{name}"
        );
        assert_eq!(card.subtypes, subtypes.as_slice(), "{name}");
        assert_eq!(card.colors.len(), colors.len(), "{name}");
        for color in colors {
            assert!(card.colors.contains(&color), "{name} is {color:?}");
        }
        assert_eq!(
            (card.power, card.toughness),
            (Some(power), Some(toughness)),
            "{name}"
        );
        assert!(!card.is_land && !card.is_token, "{name}");
    }

    for (name, keyword) in [
        ("Bloodbraid Challenger", Keywords::HASTE),
        ("Death-Greeter's Champion", Keywords::DOUBLE_STRIKE),
        ("Harvester of Misery", Keywords::MENACE),
    ] {
        assert!(def(name).keywords.has(keyword), "{name}");
    }
    // No other card of the wave has a printed evergreen keyword.
    for name in [
        "Auriok Salvagers",
        "Embereth Shieldbreaker",
        "Formidable Speaker",
        "Iron-Shield Elf",
        "Luminarch Aspirant",
        "Giver of Runes",
        "Devoted Druid",
    ] {
        for keyword in [
            Keywords::HASTE,
            Keywords::DOUBLE_STRIKE,
            Keywords::MENACE,
            Keywords::INDESTRUCTIBLE,
            Keywords::FLYING,
        ] {
            assert!(!def(name).keywords.has(keyword), "{name}");
        }
    }

    // Activated abilities (a mana ability is not one of them).
    for name in [
        "Auriok Salvagers",
        "Formidable Speaker",
        "Iron-Shield Elf",
        "Giver of Runes",
        "Harvester of Misery",
        "Devoted Druid",
    ] {
        let abilities = def(name).activated_abilities;
        assert_eq!(abilities.len(), 1, "{name}");
        assert_eq!(
            abilities[0].activation_zone,
            if name == "Harvester of Misery" {
                Zone::Hand
            } else {
                Zone::Battlefield
            },
            "{name}"
        );
    }
    for name in [
        "Bloodbraid Challenger",
        "Death-Greeter's Champion",
        "Embereth Shieldbreaker",
        "Luminarch Aspirant",
    ] {
        assert!(def(name).activated_abilities.is_empty(), "{name}");
    }

    // Dash {3}{R}, escape, and the Adventure.
    let dash = def("Death-Greeter's Champion")
        .alt_cost
        .expect("the dash cost");
    assert_eq!(dash.condition, AltCostCondition::DashV1);
    assert!(def("Bloodbraid Challenger").escape.is_some());
    assert!(def("Death-Greeter's Champion").escape.is_none());
    let adventure = def("Embereth Shieldbreaker")
        .adventure
        .as_ref()
        .expect("Battle Display");
    assert_eq!(adventure.name, "Battle Display");
    assert_eq!(adventure.types, &[CardType::Sorcery]);
    assert_eq!(
        (adventure.cost.generic, adventure.cost.pips),
        (0, &[Pip::Colored(r)][..])
    );
    for name in C3A_CARDS {
        assert_eq!(
            def(name).adventure.is_some(),
            name == "Embereth Shieldbreaker",
            "{name}"
        );
    }
}

// ---- Helpers of this wave ------------------------------------------------------------

/// Removes `object` from its zone to exile, as an effect that responds would.
fn exile(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
}

/// Deals `amount` damage from `source` to `object`.
fn damage(state: &mut GameState, source: ObjectId, object: ObjectId, amount: i32) {
    event::propose_and_commit(
        state,
        ProposedEvent::damage(source, Target::Object(object), amount),
    );
}

/// Plays on to P0's end step, after everything the beginning of it set off.
fn end_step(state: &mut GameState) {
    pass_until(state, |state| {
        state.step == Step::End
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
    });
}

/// Plays on to the beginning of P1's turn, when this turn's effects are over.
fn to_opponents_upkeep(state: &mut GameState) {
    pass_until(state, |state| {
        state.active_player == P1 && state.step == Step::Upkeep
    });
}

fn counters(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.plus1_plus1
}

/// Settles a cast of Death-Greeter's Champion: answers its cast mode (dash
/// or not) and its backup target.
fn settle_champion(state: &mut GameState, dash: bool, target: ObjectId) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(if dash {
            CastMode::Alternative
        } else {
            CastMode::Normal
        }),
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target)) =>
        {
            Action::ChooseTarget(Target::Object(target))
        }
        other => first_answer(other),
    });
}

/// Settles a cast of Bloodbraid Challenger with the library holding a land,
/// then a Lightning Bolt: the Bolt is cascaded into and cast at P1.
fn settle_cascade_into_bolt(state: &mut GameState, bolt: ObjectId) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2, "cast or decline");
            Action::ChooseEffectOption(0)
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets == vec![Target::Object(bolt)] =>
        {
            Action::ChooseEffectTarget(Target::Object(bolt))
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
}

// ---- Auriok Salvagers --------------------------------------------------------------

#[test]
fn auriok_salvagers_returns_an_artifact_card_with_mana_value_one_or_less() {
    let mut state = plains();
    let salvagers = put(&mut state, P0, "Auriok Salvagers", Zone::Battlefield);
    let star = put(&mut state, P0, "Chromatic Star", Zone::Graveyard);
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Graveyard);
    let sphere = put(&mut state, P0, "Sphere of Resistance", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Chromatic Star", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W], 1);

    let mut probe = state.clone();
    act(&mut probe, Action::ActivateAbility(salvagers, 0));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(star)));
            assert!(
                legal_targets.contains(&Target::Object(bauble)),
                "mana value 0"
            );
            assert!(
                !legal_targets.contains(&Target::Object(sphere)),
                "mana value 2"
            );
            assert!(
                !legal_targets.contains(&Target::Object(bolt)),
                "not an artifact"
            );
            assert!(
                !legal_targets.contains(&Target::Object(theirs)),
                "your graveyard"
            );
            assert_eq!(legal_targets.len(), 2);
        }
        other => panic!("expected the target, got {other:?}"),
    }

    activate(&mut state, salvagers, 0, &[Target::Object(star)]);
    assert_eq!(
        zone(&state, star),
        Zone::Graveyard,
        "not before it resolves"
    );
    settled(&mut state);
    assert_eq!(zone(&state, star), Zone::Hand);
    assert!(hand_contains(&state, P0, star));
    assert_eq!(zone(&state, bauble), Zone::Graveyard);
    assert_eq!(zone(&state, theirs), Zone::Graveyard);
    assert_eq!(mana(&state, P0, ManaColor::W), 0, "{{1}}{{W}} was paid");
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn auriok_salvagers_needs_a_legal_target_and_checks_it_on_resolution() {
    // Only a mana value 2 artifact and a nonartifact card: no target, no
    // activation.
    let mut state = plains();
    let salvagers = put(&mut state, P0, "Auriok Salvagers", Zone::Battlefield);
    put(&mut state, P0, "Sphere of Resistance", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(!activatable(&mut state, salvagers, 0));

    // Without the mana it can't be activated either.
    let mut state = plains();
    let salvagers = put(&mut state, P0, "Auriok Salvagers", Zone::Battlefield);
    put(&mut state, P0, "Chromatic Star", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(!activatable(&mut state, salvagers, 0));

    // The target leaves the graveyard in response: nothing is returned.
    let mut state = plains();
    let salvagers = put(&mut state, P0, "Auriok Salvagers", Zone::Battlefield);
    let star = put(&mut state, P0, "Chromatic Star", Zone::Graveyard);
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W], 1);
    activate(&mut state, salvagers, 0, &[Target::Object(star)]);
    on_the_stack(&mut state);
    exile(&mut state, star);
    settled(&mut state);
    assert_eq!(zone(&state, star), Zone::Exile);
    assert_eq!(zone(&state, bauble), Zone::Graveyard);
    assert!(state.players[0].hand.is_empty());

    // It can be activated again, once for each card.
    let mut state = plains();
    let salvagers = put(&mut state, P0, "Auriok Salvagers", Zone::Battlefield);
    let star = put(&mut state, P0, "Chromatic Star", Zone::Graveyard);
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    activate(&mut state, salvagers, 0, &[Target::Object(star)]);
    settled(&mut state);
    activate(&mut state, salvagers, 0, &[Target::Object(bauble)]);
    settled(&mut state);
    assert!(hand_contains(&state, P0, star));
    assert!(hand_contains(&state, P0, bauble));
}

// ---- Bloodbraid Challenger ---------------------------------------------------------

#[test]
fn bloodbraid_challenger_cascades_when_cast_and_has_haste() {
    let mut state = game(&["Island", "Lightning Bolt", "Plains", "Plains"]);
    let island = state.players[0].library[0];
    let bolt = state.players[0].library[1];
    let challenger = put(&mut state, P0, "Bloodbraid Challenger", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 3);
    cast(&mut state, challenger, &[]);
    settle_cascade_into_bolt(&mut state, bolt);
    assert_eq!(life(&state, P1), 17, "the Bolt was cast");
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, challenger), Zone::Battlefield);
    assert_eq!(
        state.players[0].library.last(),
        Some(&island),
        "the land went to the bottom"
    );
    assert!(state.exile.is_empty());
    assert_eq!(power_toughness(&state, challenger), (4, 3));
    assert_eq!(counters(&state, challenger), 0);
    state.objects.get_mut(challenger).summoning_sick = true;
    assert!(keyword(&state, challenger, Keywords::HASTE));
}

#[test]
fn bloodbraid_challenger_may_decline_a_cascade_and_skips_cards_that_cost_too_much() {
    // The cascade may be declined: the Bolt goes to the bottom with the land.
    let mut state = game(&["Island", "Lightning Bolt", "Plains", "Plains"]);
    let bolt = state.players[0].library[1];
    let challenger = put(&mut state, P0, "Bloodbraid Challenger", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 3);
    cast(&mut state, challenger, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(life(&state, P1), 20);
    assert_eq!(zone(&state, bolt), Zone::Library);
    assert_eq!(zone(&state, challenger), Zone::Battlefield);
    assert!(state.exile.is_empty());

    // A card that costs {5} or more is not a cascade hit: the next one is.
    let mut state = game(&["Gurmag Angler", "Lightning Bolt", "Plains", "Plains"]);
    let angler = state.players[0].library[0];
    let bolt = state.players[0].library[1];
    let challenger = put(&mut state, P0, "Bloodbraid Challenger", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 3);
    cast(&mut state, challenger, &[]);
    settle_cascade_into_bolt(&mut state, bolt);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(zone(&state, angler), Zone::Library, "on the bottom");
}

#[test]
fn bloodbraid_challenger_escapes_by_exiling_three_other_cards_and_cascades_again() {
    let mut state = game(&["Island", "Lightning Bolt", "Plains", "Plains"]);
    let island = state.players[0].library[0];
    let bolt = state.players[0].library[1];
    let challenger = put(&mut state, P0, "Bloodbraid Challenger", Zone::Graveyard);
    let others = [
        put(&mut state, P0, "Plains", Zone::Graveyard),
        put(&mut state, P0, "Plains", Zone::Graveyard),
        put(&mut state, P0, "Plains", Zone::Graveyard),
    ];
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 3);
    cast(&mut state, challenger, &[]);
    settle_cascade_into_bolt(&mut state, bolt);
    assert_eq!(zone(&state, challenger), Zone::Battlefield, "escaped");
    for other in others {
        assert_eq!(zone(&state, other), Zone::Exile);
    }
    assert_eq!(state.exile.len(), 3);
    assert_eq!(life(&state, P1), 17, "cascade triggers on an escape cast");
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.players[0].library.last(), Some(&island));
    // Unlike Uro, it is not sacrificed, and it enters without counters.
    assert_eq!(counters(&state, challenger), 0);
    assert_eq!(power_toughness(&state, challenger), (4, 3));
}

#[test]
fn bloodbraid_challenger_cannot_escape_without_three_other_cards_or_the_mana() {
    let mut state = plains();
    let challenger = put(&mut state, P0, "Bloodbraid Challenger", Zone::Graveyard);
    put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P0, "Plains", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 3);
    assert!(!castable(&mut state, challenger), "two other cards");

    let mut state = plains();
    let challenger = put(&mut state, P0, "Bloodbraid Challenger", Zone::Graveyard);
    for _ in 0..3 {
        put(&mut state, P0, "Plains", Zone::Graveyard);
    }
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 2);
    assert!(!castable(&mut state, challenger), "only {{2}}{{R}}{{G}}");
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, challenger));
}

// ---- Death-Greeter's Champion ------------------------------------------------------

#[test]
fn death_greeters_champion_backs_up_another_creature_with_double_strike() {
    let mut state = plains();
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let champion = put(&mut state, P0, "Death-Greeter's Champion", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, champion, &[]);
    settle_champion(&mut state, false, pup);
    assert_eq!(zone(&state, champion), Zone::Battlefield);
    assert_eq!(counters(&state, pup), 1);
    assert_eq!(power_toughness(&state, pup), (3, 2));
    assert!(keyword(&state, pup, Keywords::DOUBLE_STRIKE), "granted");
    assert_eq!(counters(&state, champion), 0);
    assert_eq!(power_toughness(&state, champion), (2, 1));
    assert!(
        keyword(&state, champion, Keywords::DOUBLE_STRIKE),
        "printed"
    );

    // A normal cast is not returned at the end step; the grant ends with the
    // turn and the counter stays.
    end_step(&mut state);
    assert_eq!(zone(&state, champion), Zone::Battlefield);
    to_opponents_upkeep(&mut state);
    assert_eq!(counters(&state, pup), 1);
    assert!(
        !keyword(&state, pup, Keywords::DOUBLE_STRIKE),
        "until end of turn"
    );
    assert!(keyword(&state, champion, Keywords::DOUBLE_STRIKE));
}

#[test]
fn death_greeters_champion_backs_up_itself_or_an_opponents_creature() {
    let mut state = plains();
    let foe = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let champion = put(&mut state, P0, "Death-Greeter's Champion", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, champion, &[]);
    settle_champion(&mut state, false, champion);
    assert_eq!(counters(&state, champion), 1);
    assert_eq!(power_toughness(&state, champion), (3, 2));
    assert_eq!(counters(&state, foe), 0);
    assert!(!keyword(&state, foe, Keywords::DOUBLE_STRIKE));

    // Any creature may be the target, an opponent's too.
    let mut state = plains();
    let foe = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let champion = put(&mut state, P0, "Death-Greeter's Champion", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, champion, &[]);
    settle_champion(&mut state, false, foe);
    assert_eq!(counters(&state, foe), 1);
    assert!(keyword(&state, foe, Keywords::DOUBLE_STRIKE));
    assert_eq!(counters(&state, champion), 0);
}

#[test]
fn death_greeters_champion_backup_does_nothing_when_its_target_is_gone() {
    let mut state = plains();
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let champion = put(&mut state, P0, "Death-Greeter's Champion", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, champion, &[]);
    let chosen = std::cell::Cell::new(false);
    run_until(
        &mut state,
        |state| chosen.get() && !state.stack.is_empty(),
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(pup)) =>
            {
                chosen.set(true);
                Some(Action::ChooseTarget(Target::Object(pup)))
            }
            _ => None,
        },
    );
    exile(&mut state, pup);
    settled(&mut state);
    assert_eq!(zone(&state, pup), Zone::Exile);
    assert_eq!(
        counters(&state, champion),
        0,
        "the counter is not redirected"
    );
    assert_eq!(power_toughness(&state, champion), (2, 1));
}

#[test]
fn death_greeters_champion_dashes_with_haste_and_returns_at_the_end_step() {
    let mut state = plains();
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let champion = put(&mut state, P0, "Death-Greeter's Champion", Zone::Hand);
    // {3}{R} is a dash; {2}{R} would be a normal cast.
    pool(&mut state, P0, &[ManaColor::R], 3);
    cast(&mut state, champion, &[]);
    settle_champion(&mut state, true, pup);
    assert_eq!(zone(&state, champion), Zone::Battlefield);
    state.objects.get_mut(champion).summoning_sick = true;
    assert!(keyword(&state, champion, Keywords::HASTE), "dashed");
    // The backup trigger still happens for a dashed cast.
    assert_eq!(counters(&state, pup), 1);
    assert!(keyword(&state, pup, Keywords::DOUBLE_STRIKE));
    end_step(&mut state);
    assert!(
        hand_contains(&state, P0, champion),
        "returned to its owner's hand at the beginning of the next end step"
    );
    assert_eq!(zone(&state, pup), Zone::Battlefield);

    // Cast normally it has no haste.
    let mut state = plains();
    let champion = put(&mut state, P0, "Death-Greeter's Champion", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, champion, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, champion), Zone::Battlefield);
    state.objects.get_mut(champion).summoning_sick = true;
    assert!(!keyword(&state, champion, Keywords::HASTE), "not dashed");
}

// ---- Embereth Shieldbreaker // Battle Display -------------------------------------

#[test]
fn battle_display_destroys_an_artifact_then_the_knight_is_cast_from_exile() {
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    let mine = put(&mut state, P0, "Chromatic Star", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let shieldbreaker = put(&mut state, P0, "Embereth Shieldbreaker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let targets = cast_targets(&state, shieldbreaker);
    assert!(targets.contains(&Target::Object(ring)));
    assert!(
        targets.contains(&Target::Object(strix)),
        "an artifact creature"
    );
    assert!(targets.contains(&Target::Object(mine)), "any artifact");
    assert!(!targets.contains(&Target::Object(elves)), "not an artifact");
    cast(&mut state, shieldbreaker, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, strix), Zone::Battlefield);
    assert_eq!(zone(&state, shieldbreaker), Zone::Exile);
    assert!(state.objects.get(shieldbreaker).v4.on_adventure);

    // The creature half is cast later from exile.
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(castable(&mut state, shieldbreaker));
    cast(&mut state, shieldbreaker, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, shieldbreaker), Zone::Battlefield);
    assert_eq!(power_toughness(&state, shieldbreaker), (2, 1));
    assert!(!state.objects.get(shieldbreaker).v4.on_adventure);
}

#[test]
fn battle_display_and_the_knight_are_two_modes_of_one_card() {
    // With {1}{R} and an artifact to target, both halves can be cast.
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let shieldbreaker = put(&mut state, P0, "Embereth Shieldbreaker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_mode(&mut state, shieldbreaker, 1, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, shieldbreaker), Zone::Exile);

    let mut state = plains();
    put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let shieldbreaker = put(&mut state, P0, "Embereth Shieldbreaker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast_mode(&mut state, shieldbreaker, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, shieldbreaker), Zone::Battlefield);
    assert_eq!(power_toughness(&state, shieldbreaker), (2, 1));
    assert!(!state.objects.get(shieldbreaker).v4.on_adventure);

    // Without an artifact on the battlefield, only the Knight can be cast.
    let mut state = plains();
    let shieldbreaker = put(&mut state, P0, "Embereth Shieldbreaker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(
        !castable(&mut state, shieldbreaker),
        "no artifact, and the Knight costs {{1}}{{R}}"
    );
    pool(&mut state, P0, &[], 1);
    cast(&mut state, shieldbreaker, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, shieldbreaker), Zone::Battlefield);
}

#[test]
fn battle_display_is_a_sorcery_and_does_nothing_without_its_target() {
    let mut state = plains();
    put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let shieldbreaker = put(&mut state, P0, "Embereth Shieldbreaker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    state.step = Step::Upkeep;
    assert!(!castable(&mut state, shieldbreaker), "a sorcery");

    // The artifact leaves in response: Battle Display is countered on
    // resolution, nothing else is destroyed, and the card is put into the
    // graveyard rather than exiled.
    let mut state = plains();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let cage = put(&mut state, P1, "Grafdigger's Cage", Zone::Battlefield);
    let shieldbreaker = put(&mut state, P0, "Embereth Shieldbreaker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, shieldbreaker, &[Target::Object(ring)]);
    on_the_stack(&mut state);
    exile(&mut state, ring);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Exile);
    assert_eq!(zone(&state, cage), Zone::Battlefield);
    assert_eq!(zone(&state, shieldbreaker), Zone::Graveyard);
    assert!(!state.objects.get(shieldbreaker).v4.on_adventure);
}

// ---- Formidable Speaker ------------------------------------------------------------

#[test]
fn formidable_speaker_discards_a_card_to_search_for_and_reveal_a_creature() {
    let mut state = game(&["Island", "Llanowar Elves", "Jackal Pup", "Plains"]);
    let island = state.players[0].library[0];
    let elves = state.players[0].library[1];
    let speaker = put(&mut state, P0, "Formidable Speaker", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, speaker, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(bolt)) =>
        {
            assert_eq!(
                legal_targets,
                vec![Target::Object(bolt)],
                "the cards in hand"
            );
            Action::ChooseEffectTarget(Target::Object(bolt))
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(elves)) =>
        {
            assert!(
                !legal_targets.contains(&Target::Object(island)),
                "a creature card"
            );
            assert_eq!(legal_targets.len(), 2, "the two creature cards");
            Action::ChooseEffectTarget(Target::Object(elves))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, speaker), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "discarded");
    assert!(hand_contains(&state, P0, elves));
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[0].library.len(), 3);
    assert!(
        state
            .known_hand_cards(P1, P0)
            .iter()
            .any(|entry| entry.object == elves),
        "the creature card was revealed"
    );
}

#[test]
fn formidable_speaker_may_decline_the_discard_and_needs_a_card_to_discard() {
    // Declined: no discard, no search.
    let mut state = game(&["Island", "Llanowar Elves", "Jackal Pup", "Plains"]);
    let speaker = put(&mut state, P0, "Formidable Speaker", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, speaker, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.contains(&Target::Object(bolt)) => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bolt), Zone::Hand);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[0].library.len(), 4);
    assert_eq!(zone(&state, speaker), Zone::Battlefield);

    // With no other card in hand there is nothing to discard and nothing to
    // ask.
    let mut state = game(&["Island", "Llanowar Elves", "Jackal Pup", "Plains"]);
    let speaker = put(&mut state, P0, "Formidable Speaker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 2);
    cast(&mut state, speaker, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, speaker), Zone::Battlefield);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.players[0].library.len(), 4);
}

#[test]
fn formidable_speaker_untaps_another_target_permanent() {
    let mut state = plains();
    let speaker = put(&mut state, P0, "Formidable Speaker", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    state.objects.get_mut(forest).tapped = true;
    state.objects.get_mut(island).tapped = true;
    pool(&mut state, P0, &[], 1);

    let mut probe = state.clone();
    act(&mut probe, Action::ActivateAbility(speaker, 0));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(forest)));
            assert!(
                legal_targets.contains(&Target::Object(island)),
                "any controller"
            );
            assert!(
                !legal_targets.contains(&Target::Object(speaker)),
                "another permanent"
            );
        }
        other => panic!("expected the target, got {other:?}"),
    }

    activate(&mut state, speaker, 0, &[Target::Object(forest)]);
    on_the_stack(&mut state);
    assert!(state.objects.get(speaker).tapped, "{{T}} is a cost");
    settled(&mut state);
    assert!(!state.objects.get(forest).tapped);
    assert!(state.objects.get(island).tapped, "only the target");
    // It is tapped now, so it can't be activated again.
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, speaker, 0));

    // The cost is {1} and {T}: a summoning sick Speaker can't pay it.
    let mut state = plains();
    let speaker = put(&mut state, P0, "Formidable Speaker", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    state.objects.get_mut(forest).tapped = true;
    pool(&mut state, P0, &[], 0);
    assert!(!activatable(&mut state, speaker, 0), "without {{1}}");
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, speaker, 0));
    state.objects.get_mut(speaker).summoning_sick = true;
    assert!(!activatable(&mut state, speaker, 0), "summoning sick");
}

// ---- Iron-Shield Elf ---------------------------------------------------------------

#[test]
fn iron_shield_elf_discards_a_card_to_become_indestructible_and_tap() {
    let mut state = plains();
    let elf = put(&mut state, P0, "Iron-Shield Elf", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Hand);
    let foe = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    assert!(!state.objects.get(elf).tapped);
    activate(&mut state, elf, 0, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, island), Zone::Graveyard, "discarded as a cost");
    assert!(keyword(&state, elf, Keywords::INDESTRUCTIBLE));
    assert!(state.objects.get(elf).tapped, "tapped by the effect");
    // Lethal damage does not destroy it.
    damage(&mut state, foe, elf, 3);
    next(&mut state);
    assert_eq!(zone(&state, elf), Zone::Battlefield);
    assert_eq!(state.objects.get(elf).damage, 3);

    // Without the ability the same damage kills it.
    let mut state = plains();
    let elf = put(&mut state, P0, "Iron-Shield Elf", Zone::Battlefield);
    let foe = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    damage(&mut state, foe, elf, 3);
    next(&mut state);
    assert_eq!(zone(&state, elf), Zone::Graveyard);
}

#[test]
fn iron_shield_elf_needs_a_card_and_can_be_activated_again_while_tapped() {
    // No card to discard: no activation.
    let mut state = plains();
    let elf = put(&mut state, P0, "Iron-Shield Elf", Zone::Battlefield);
    assert!(!activatable(&mut state, elf, 0));

    // Tapping is part of the effect, not a cost: a tapped Elf can do it again.
    put(&mut state, P0, "Island", Zone::Hand);
    put(&mut state, P0, "Plains", Zone::Hand);
    state.objects.get_mut(elf).tapped = true;
    activate(&mut state, elf, 0, &[]);
    settle_first(&mut state);
    assert!(keyword(&state, elf, Keywords::INDESTRUCTIBLE));
    assert!(state.objects.get(elf).tapped);
    activate(&mut state, elf, 0, &[]);
    settle_first(&mut state);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.players[0].graveyard.len(), 2);
    assert!(keyword(&state, elf, Keywords::INDESTRUCTIBLE));

    // It stops being indestructible when the turn ends.
    to_opponents_upkeep(&mut state);
    assert!(!keyword(&state, elf, Keywords::INDESTRUCTIBLE));
}

// ---- Luminarch Aspirant ------------------------------------------------------------

#[test]
fn luminarch_aspirant_grows_a_creature_you_control_at_each_of_your_combats() {
    let mut state = plains();
    let aspirant = put(&mut state, P0, "Luminarch Aspirant", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let foe = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    run_until(
        &mut state,
        |state| state.step == Step::DeclareAttackers,
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(elves)));
                assert!(
                    legal_targets.contains(&Target::Object(aspirant)),
                    "it may target itself"
                );
                assert!(
                    !legal_targets.contains(&Target::Object(foe)),
                    "a creature you control"
                );
                Some(Action::ChooseTarget(Target::Object(elves)))
            }
            _ => None,
        },
    );
    assert_eq!(counters(&state, elves), 1);
    assert_eq!(power_toughness(&state, elves), (2, 2));
    assert_eq!(power_toughness(&state, aspirant), (1, 1));
    assert_eq!(counters(&state, foe), 0);

    // "On your turn": the opponent's combat puts nothing anywhere.
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::End
    });
    assert_eq!(counters(&state, elves), 1);
    assert_eq!(counters(&state, aspirant), 0);
}

#[test]
fn luminarch_aspirant_may_target_itself_and_does_nothing_when_its_target_is_gone() {
    let mut state = plains();
    let aspirant = put(&mut state, P0, "Luminarch Aspirant", Zone::Battlefield);
    run_until(
        &mut state,
        |state| state.step == Step::DeclareAttackers,
        |_, decision| match decision {
            Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Object(aspirant))),
            _ => None,
        },
    );
    assert_eq!(counters(&state, aspirant), 1);
    assert_eq!(power_toughness(&state, aspirant), (2, 2));

    let mut state = plains();
    let aspirant = put(&mut state, P0, "Luminarch Aspirant", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let chosen = std::cell::Cell::new(false);
    run_until(
        &mut state,
        |state| chosen.get() && !state.stack.is_empty(),
        |_, decision| match decision {
            Decision::ChooseTargets { .. } => {
                chosen.set(true);
                Some(Action::ChooseTarget(Target::Object(elves)))
            }
            _ => None,
        },
    );
    exile(&mut state, elves);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    assert_eq!(counters(&state, aspirant), 0, "the target was gone");
    assert_eq!(zone(&state, elves), Zone::Exile);
}

// ---- Giver of Runes ----------------------------------------------------------------

/// The options of Giver of Runes' choice: the five colors in WUBRG order,
/// then colorless.
const GIVER_RED: u16 = 3;
const GIVER_COLORLESS: u16 = 5;

#[test]
fn giver_of_runes_gives_another_creature_protection_from_colorless() {
    let mut state = plains();
    let giver = put(&mut state, P0, "Giver of Runes", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let foe = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let golem = put(&mut state, P1, "Phyrexian Dreadnought", Zone::Battlefield);
    let mut probe = state.clone();
    act(&mut probe, Action::ActivateAbility(giver, 0));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(
                legal_targets,
                vec![Target::Object(pup)],
                "another creature you control"
            );
        }
        other => panic!("expected the target, got {other:?}"),
    }

    activate(&mut state, giver, 0, &[Target::Object(pup)]);
    on_the_stack(&mut state);
    assert!(state.objects.get(giver).tapped, "{{T}} is a cost");
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { option_count, .. }) => {
            assert_eq!(option_count, 6, "five colors and colorless");
            act(&mut state, Action::ChooseEffectOption(GIVER_COLORLESS));
        }
        other => panic!("expected the choice, got {other:?}"),
    }
    settled(&mut state);
    // Colorless damage is prevented, colored damage is not.
    damage(&mut state, golem, pup, 5);
    assert_eq!(
        state.objects.get(pup).damage,
        0,
        "protection from colorless"
    );
    damage(&mut state, foe, pup, 1);
    assert_eq!(state.objects.get(pup).damage, 1, "a black source");
    // The state-based actions run on the next advance: the 2/1 dies of it.
    next(&mut state);
    assert_eq!(zone(&state, pup), Zone::Graveyard);
}

#[test]
fn giver_of_runes_gives_protection_from_a_color_of_your_choice() {
    let mut state = plains();
    let giver = put(&mut state, P0, "Giver of Runes", Zone::Battlefield);
    // A 1/2, so that one point of damage does not kill it when the
    // state-based actions run on the next advance.
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let red = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let golem = put(&mut state, P1, "Phyrexian Dreadnought", Zone::Battlefield);
    activate(&mut state, giver, 0, &[Target::Object(inspector)]);
    settle_option(&mut state, GIVER_RED);
    damage(&mut state, red, inspector, 1);
    assert_eq!(
        state.objects.get(inspector).damage,
        0,
        "protection from red"
    );
    damage(&mut state, golem, inspector, 1);
    assert_eq!(
        state.objects.get(inspector).damage,
        1,
        "colorless damage is dealt"
    );

    // The protection ends with the turn (damage wears off with it).
    to_opponents_upkeep(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Battlefield);
    assert_eq!(state.objects.get(inspector).damage, 0);
    damage(&mut state, red, inspector, 1);
    assert_eq!(state.objects.get(inspector).damage, 1);
}

#[test]
fn giver_of_runes_colorless_protection_stops_colorless_targeting_and_equipment() {
    // A colorless source can't target the creature, whoever controls it.
    let mut state = plains();
    let giver = put(&mut state, P0, "Giver of Runes", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let bomb = put(&mut state, P0, "Aether Spellbomb", Zone::Battlefield);
    activate(&mut state, giver, 0, &[Target::Object(pup)]);
    settle_option(&mut state, GIVER_COLORLESS);
    pool(&mut state, P0, &[ManaColor::U], 0);
    let mut probe = state.clone();
    act(&mut probe, Action::ActivateAbility(bomb, 0));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(pup)), "protected");
            assert!(legal_targets.contains(&Target::Object(elves)));
        }
        other => panic!("expected the target, got {other:?}"),
    }

    // A colorless Equipment attached to the creature falls off.
    let mut state = plains();
    let giver = put(&mut state, P0, "Giver of Runes", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let shuko = put(&mut state, P0, "Shuko", Zone::Battlefield);
    activate(&mut state, shuko, 0, &[Target::Object(pup)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, pup), (3, 1), "equipped");
    activate(&mut state, giver, 0, &[Target::Object(pup)]);
    settle_option(&mut state, GIVER_COLORLESS);
    assert_eq!(power_toughness(&state, pup), (2, 1), "unattached");
    assert_eq!(zone(&state, shuko), Zone::Battlefield);
    assert!(state.objects.get(shuko).v4.attached_to.is_none());
}

// ---- Harvester of Misery -----------------------------------------------------------

#[test]
fn harvester_of_misery_gives_every_other_creature_minus_two_minus_two() {
    let mut state = plains();
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    let harvester = put(&mut state, P0, "Harvester of Misery", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 3);
    cast(&mut state, harvester, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, harvester), Zone::Battlefield);
    assert_eq!(
        power_toughness(&state, harvester),
        (5, 4),
        "other creatures"
    );
    for dead in [pup, inspector, specter, strix] {
        assert_eq!(zone(&state, dead), Zone::Graveyard);
    }
    assert_eq!(zone(&state, wall), Zone::Battlefield);
    assert_eq!(power_toughness(&state, wall), (-2, 3));
    assert!(keyword(&state, harvester, Keywords::MENACE));

    // Creatures that enter later are not affected.
    let late = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    assert_eq!(power_toughness(&state, late), (2, 2));
}

#[test]
fn harvester_of_misery_discards_for_minus_two_minus_two_on_a_target_creature() {
    let mut state = plains();
    let harvester = put(&mut state, P0, "Harvester of Misery", Zone::Hand);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 1);
    let mut probe = state.clone();
    act(&mut probe, Action::ActivateAbility(harvester, 0));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(specter)));
            assert!(legal_targets.contains(&Target::Object(wall)));
        }
        other => panic!("expected the target, got {other:?}"),
    }
    activate(&mut state, harvester, 0, &[Target::Object(specter)]);
    on_the_stack(&mut state);
    assert_eq!(
        zone(&state, harvester),
        Zone::Graveyard,
        "discarded as a cost"
    );
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(zone(&state, wall), Zone::Battlefield);

    // A creature that survives has -2/-2 until end of turn.
    let mut state = plains();
    let harvester = put(&mut state, P0, "Harvester of Misery", Zone::Hand);
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 1);
    activate(&mut state, harvester, 0, &[Target::Object(wall)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, wall), (-2, 3));
    to_opponents_upkeep(&mut state);
    assert_eq!(power_toughness(&state, wall), (0, 5), "until end of turn");
}

#[test]
fn harvester_of_misery_ability_works_only_from_the_hand_and_checks_its_target() {
    // On the battlefield it has no such ability.
    let mut state = plains();
    let harvester = put(&mut state, P0, "Harvester of Misery", Zone::Battlefield);
    put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 1);
    assert!(!activatable(&mut state, harvester, 0));

    // It costs {1}{B}.
    let mut state = plains();
    let harvester = put(&mut state, P0, "Harvester of Misery", Zone::Hand);
    put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 0);
    assert!(!activatable(&mut state, harvester, 0));
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, harvester, 0));

    // Its target is gone in response: the card is still discarded.
    let mut state = plains();
    let harvester = put(&mut state, P0, "Harvester of Misery", Zone::Hand);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let other = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 1);
    activate(&mut state, harvester, 0, &[Target::Object(specter)]);
    on_the_stack(&mut state);
    exile(&mut state, specter);
    settled(&mut state);
    assert_eq!(zone(&state, harvester), Zone::Graveyard);
    assert_eq!(power_toughness(&state, other), (2, 2));
}

// ---- Devoted Druid -----------------------------------------------------------------

#[test]
fn devoted_druid_taps_for_green_and_untaps_by_putting_a_minus_one_counter_on_itself() {
    let mut state = plains();
    let druid = put(&mut state, P0, "Devoted Druid", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(druid));
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert!(state.objects.get(druid).tapped);

    // The counter is a cost, put as the ability is activated.
    activate(&mut state, druid, 0, &[]);
    on_the_stack(&mut state);
    assert_eq!(state.objects.get(druid).counters.minus1_minus1, 1);
    assert!(
        state.objects.get(druid).tapped,
        "it untaps as the ability resolves"
    );
    assert_eq!(power_toughness(&state, druid).1, 1);
    settled(&mut state);
    assert!(!state.objects.get(druid).tapped);
    assert_eq!(zone(&state, druid), Zone::Battlefield);

    // It taps for green again.
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(druid));
    assert_eq!(mana(&state, P0, ManaColor::G), 2);

    // The second counter makes it 0/0: it dies, and the ability still
    // resolves without effect.
    activate(&mut state, druid, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, druid), Zone::Graveyard);
}

#[test]
fn devoted_druid_cannot_tap_while_summoning_sick_but_can_untap_itself() {
    let mut state = plains();
    let druid = put(&mut state, P0, "Devoted Druid", Zone::Battlefield);
    state.objects.get_mut(druid).summoning_sick = true;
    next(&mut state);
    assert!(engine::step(&mut state, Action::ActivateManaAbility(druid)).is_err());
    // The untap ability has no {T} in its cost: it can always be activated.
    assert!(activatable(&mut state, druid, 0));
    activate(&mut state, druid, 0, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(druid).counters.minus1_minus1, 1);
    assert!(!state.objects.get(druid).tapped);
}

// ---- Public sessions ---------------------------------------------------------------

#[test]
fn c3a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let white_black = deck(&[
        ("Plains", 8),
        ("Swamp", 8),
        ("Auriok Salvagers", 4),
        ("Luminarch Aspirant", 4),
        ("Giver of Runes", 4),
        ("Iron-Shield Elf", 4),
        ("Harvester of Misery", 4),
        ("Thraben Inspector", 4),
    ]);
    let red_green = deck(&[
        ("Mountain", 8),
        ("Forest", 8),
        ("Bloodbraid Challenger", 4),
        ("Death-Greeter's Champion", 4),
        ("Embereth Shieldbreaker", 4),
        ("Formidable Speaker", 4),
        ("Devoted Druid", 4),
        ("Jackal Pup", 4),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    for (pair, first_seed) in [
        ([white_black.clone(), red_green.clone()], 0x4333_6100_u64),
        ([red_green, white_black], 0x4333_6110),
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
    for name in C3A_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
