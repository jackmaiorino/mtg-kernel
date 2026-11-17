//! Vintage Cube wave C4a: native rules and public-session tests for the
//! spell-cast, draw and copy trigger creatures.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, CostComponent, Keywords, Subtype,
    Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_lurrus_v1::{add_companions, companion_condition_met};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1, LimitedJsonlServerV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};
use serde_json::{json, Value};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C4A_CARDS: [&str; 8] = [
    "Displacer Kitten",
    "Faerie Mastermind",
    "Ledger Shredder",
    "Forensic Gadgeteer",
    "Lutri, the Spellchaser",
    "Caustic Bronco",
    "Jacked Rabbit",
    "Bristly Bill, Spine Sower",
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
        0x4355_4245_0000 + 0x3461,
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

// ---- Helpers of this wave ------------------------------------------------------------

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

fn counters(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.plus1_plus1
}

/// Removes `object` from its zone to exile, as an effect that responds would.
fn exile(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Exile));
}

fn clues(state: &GameState, player: PlayerId) -> usize {
    named(state, player, "Clue Token").len()
}

fn hand_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

fn library_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].library.len()
}

/// A first main phase whose libraries hold `top` above nineteen Plains.
fn game_top(top: &str) -> GameState {
    let mut library = vec!["Plains"; 20];
    library[0] = top;
    game(&library)
}

/// Settles, choosing `object` when a trigger or spell asks for a target that
/// offers it, and declining an optional target otherwise.
fn settle_declining_targets(state: &mut GameState) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn c4a_cards_are_full() {
    for name in C4A_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn the_new_creature_types_have_their_stable_ids() {
    for (subtype, stable_id) in [
        (Subtype::Vedalken, 600),
        (Subtype::Horse, 601),
        (Subtype::Mount, 602),
    ] {
        assert_eq!(subtype.stable_id(), stable_id);
        assert!(subtype.is_creature_type(), "{subtype:?} is a creature type");
    }
}

#[test]
fn c4a_cards_have_their_printed_characteristics() {
    let colored = Pip::Colored;
    let (w, u, b, r, g) = (
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    );
    for (name, generic, pips, subtypes, colors, power, toughness, legendary) in [
        (
            "Displacer Kitten",
            3,
            vec![colored(u)],
            vec![Subtype::Cat, Subtype::Beast],
            vec![u],
            2,
            2,
            false,
        ),
        (
            "Faerie Mastermind",
            1,
            vec![colored(u)],
            vec![Subtype::Faerie, Subtype::Rogue],
            vec![u],
            2,
            1,
            false,
        ),
        (
            "Ledger Shredder",
            1,
            vec![colored(u)],
            vec![Subtype::Bird, Subtype::Advisor],
            vec![u],
            1,
            3,
            false,
        ),
        (
            "Forensic Gadgeteer",
            2,
            vec![colored(u)],
            vec![Subtype::Vedalken, Subtype::Artificer, Subtype::Detective],
            vec![u],
            2,
            3,
            false,
        ),
        (
            "Lutri, the Spellchaser",
            1,
            vec![Pip::Hybrid(u, r), Pip::Hybrid(u, r)],
            vec![Subtype::Elemental, Subtype::Otter],
            vec![u, r],
            3,
            2,
            true,
        ),
        (
            "Caustic Bronco",
            1,
            vec![colored(b)],
            vec![Subtype::Snake, Subtype::Horse, Subtype::Mount],
            vec![b],
            2,
            2,
            false,
        ),
        (
            "Jacked Rabbit",
            1,
            vec![colored(w)],
            vec![Subtype::Rabbit, Subtype::Warrior],
            vec![w],
            1,
            2,
            false,
        ),
        (
            "Bristly Bill, Spine Sower",
            1,
            vec![colored(g)],
            vec![Subtype::Plant, Subtype::Druid],
            vec![g],
            2,
            2,
            true,
        ),
    ] {
        let card = def(name);
        assert_eq!(card.types, &[CardType::Creature], "{name}");
        assert_eq!(card.cost.generic, generic, "{name}");
        assert_eq!(
            card.cost.x_count,
            u8::from(name == "Jacked Rabbit"),
            "{name}"
        );
        assert_eq!(card.cost.pips.len(), pips.len(), "{name}");
        for pip in &pips {
            assert!(card.cost.pips.contains(pip), "{name} lacks {pip:?}");
        }
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
        assert_eq!(
            card.supertypes,
            if legendary {
                &[Supertype::Legendary][..]
            } else {
                &[][..]
            },
            "{name}"
        );
        assert!(!card.is_land && !card.is_token, "{name}");
    }

    // Printed evergreen keywords: no other card of the wave has any.
    for (name, flash, flying) in [
        ("Faerie Mastermind", true, true),
        ("Ledger Shredder", false, true),
        ("Lutri, the Spellchaser", true, false),
        ("Displacer Kitten", false, false),
        ("Forensic Gadgeteer", false, false),
        ("Caustic Bronco", false, false),
        ("Jacked Rabbit", false, false),
        ("Bristly Bill, Spine Sower", false, false),
    ] {
        assert_eq!(def(name).keywords.has(Keywords::FLASH), flash, "{name}");
        assert_eq!(def(name).keywords.has(Keywords::FLYING), flying, "{name}");
        assert!(!def(name).keywords.has(Keywords::HASTE), "{name}");
    }

    // Activated abilities: Faerie Mastermind's draw, Bristly Bill's doubling,
    // Caustic Bronco's Saddle and Lutri's companion action from exile.
    for (name, zone) in [
        ("Faerie Mastermind", Zone::Battlefield),
        ("Bristly Bill, Spine Sower", Zone::Battlefield),
        ("Caustic Bronco", Zone::Battlefield),
        ("Lutri, the Spellchaser", Zone::Exile),
    ] {
        let abilities = def(name).activated_abilities;
        assert_eq!(abilities.len(), 1, "{name}");
        assert_eq!(abilities[0].activation_zone, zone, "{name}");
    }
    for name in [
        "Displacer Kitten",
        "Ledger Shredder",
        "Forensic Gadgeteer",
        "Jacked Rabbit",
    ] {
        assert!(def(name).activated_abilities.is_empty(), "{name}");
    }
}

// ---- Displacer Kitten ----------------------------------------------------------------

#[test]
fn displacer_kitten_flickers_a_nonland_permanent_when_you_cast_a_noncreature_spell() {
    let mut state = plains();
    let kitten = put(&mut state, P0, "Displacer Kitten", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let land = put(&mut state, P0, "Plains", Zone::Battlefield);
    let foe = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    state.objects.get_mut(inspector).counters.plus1_plus1 = 1;
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let before = state.objects.get(inspector).zone_change_count;

    cast(&mut state, bolt, &[Target::Player(P1)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(can_finish, "up to one target");
            assert!(
                legal_targets.contains(&Target::Object(kitten)),
                "it may flicker itself"
            );
            assert!(legal_targets.contains(&Target::Object(inspector)));
            assert!(
                !legal_targets.contains(&Target::Object(land)),
                "a nonland permanent"
            );
            assert!(
                !legal_targets.contains(&Target::Object(foe)),
                "a permanent you control"
            );
            Action::ChooseTarget(Target::Object(inspector))
        }
        other => first_answer(other),
    });

    // Exiled and returned: a new object without its counter, whose enters
    // trigger investigated, and the cast spell resolved after all of that.
    assert_eq!(zone(&state, inspector), Zone::Battlefield);
    assert!(state.objects.get(inspector).zone_change_count > before);
    assert_eq!(counters(&state, inspector), 0);
    assert_eq!(power_toughness(&state, inspector), (1, 2));
    assert_eq!(clues(&state, P0), 1);
    assert_eq!(zone(&state, kitten), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(life(&state, P1), 17);
}

#[test]
fn displacer_kitten_may_choose_no_target_flicker_itself_or_flicker_a_token() {
    // "Up to one": no target, nothing happens.
    let mut state = plains();
    put(&mut state, P0, "Displacer Kitten", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let before = state.objects.get(inspector).zone_change_count;
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settle_declining_targets(&mut state);
    assert_eq!(state.objects.get(inspector).zone_change_count, before);
    assert_eq!(clues(&state, P0), 0);
    assert_eq!(life(&state, P1), 17);

    // It may target itself.
    let mut state = plains();
    let kitten = put(&mut state, P0, "Displacer Kitten", Zone::Battlefield);
    state.objects.get_mut(kitten).counters.plus1_plus1 = 2;
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let before = state.objects.get(kitten).zone_change_count;
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settle_choosing(&mut state, kitten);
    assert_eq!(zone(&state, kitten), Zone::Battlefield);
    assert!(state.objects.get(kitten).zone_change_count > before);
    assert_eq!(counters(&state, kitten), 0);
    assert_eq!(life(&state, P1), 17);

    // A token returns to no battlefield: it ceases to exist in exile.
    let mut state = plains();
    put(&mut state, P0, "Displacer Kitten", Zone::Battlefield);
    let clue = token(&mut state, P0, "Clue Token");
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settle_choosing(&mut state, clue);
    assert!(!state.players[0].battlefield.contains(&clue));
    assert_eq!(clues(&state, P0), 0);
}

#[test]
fn displacer_kitten_triggers_only_for_your_noncreature_spells() {
    // Your creature spell: nothing triggers.
    let mut state = plains();
    put(&mut state, P0, "Displacer Kitten", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, inspector, &[]);
    priority(&mut state);
    assert_eq!(state.stack.len(), 1, "only the creature spell");
    assert!(state.engine.pending_triggers.is_empty());

    // The opponent's noncreature spell: nothing triggers.
    let mut state = plains();
    put(&mut state, P0, "Displacer Kitten", Zone::Battlefield);
    put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    opponent_bolts(&mut state, Target::Player(P0));
    assert_eq!(state.stack.len(), 1, "only the bolt");
    assert!(state.engine.pending_triggers.is_empty());
}

#[test]
fn displacer_kitten_does_nothing_when_its_target_has_left() {
    let mut state = plains();
    put(&mut state, P0, "Displacer Kitten", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(inspector)));
            act(&mut state, Action::ChooseTarget(Target::Object(inspector)));
        }
        other => panic!("expected the trigger's target, got {other:?}"),
    }
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 2, "the spell and the trigger");
    // In response the Inspector leaves.
    exile(&mut state, inspector);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Exile, "it did not return");
    assert_eq!(clues(&state, P0), 0);
    assert_eq!(life(&state, P1), 17);
}

// ---- Forensic Gadgeteer --------------------------------------------------------------

#[test]
fn forensic_gadgeteer_investigates_when_you_cast_an_artifact_spell() {
    let mut state = plains();
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(clues(&state, P0), 0, "a noncreature, nonartifact spell");

    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
    assert_eq!(clues(&state, P0), 1, "an artifact spell investigates");
}

#[test]
fn forensic_gadgeteer_makes_artifact_abilities_cost_one_less_but_never_below_one_mana() {
    // Without the Gadgeteer a Clue needs {2}.
    let mut state = plains();
    let clue = token(&mut state, P0, "Clue Token");
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, clue, 0));
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, clue, 0));

    // With it, {1} does, and the mana is spent for the cheaper cost.
    let mut state = plains();
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let clue = token(&mut state, P0, "Clue Token");
    let hand = hand_len(&state, P0);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, clue, 0, &[]);
    on_the_stack(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "{{1}} paid the ability");
    settled(&mut state);
    assert!(!state.players[0].battlefield.contains(&clue), "sacrificed");
    assert_eq!(hand_len(&state, P0), hand + 1, "and it drew a card");

    // A cost of one mana is not reduced: the {1} ability stays {1}, and the
    // colored {U} ability is not touched.
    let mut state = plains();
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let bomb = put(&mut state, P0, "Aether Spellbomb", Zone::Battlefield);
    let draw = def("Aether Spellbomb")
        .activated_abilities
        .iter()
        .position(|ability| {
            ability.cost.iter().any(|component| {
                matches!(component, CostComponent::Mana(cost) if cost.generic == 1 && cost.pips.is_empty())
            })
        })
        .expect("the {1} ability");
    let draw = u8::try_from(draw).unwrap();
    assert!(!activatable(&mut state, bomb, draw), "it still costs {{1}}");
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, bomb, draw));

    // Each Gadgeteer takes {1} more off a bigger cost: Walking Ballista's
    // {4} becomes {3}, then {2}.
    let mut state = plains();
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let ballista = put(&mut state, P0, "Walking Ballista", Zone::Battlefield);
    state.objects.get_mut(ballista).counters.plus1_plus1 = 2;
    pool(&mut state, P0, &[], 2);
    assert!(!activatable(&mut state, ballista, 0), "{{3}} with two mana");
    pool(&mut state, P0, &[], 1);
    assert!(
        activatable(&mut state, ballista, 0),
        "{{3}} with three mana"
    );
    let mut state = plains();
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let ballista = put(&mut state, P0, "Walking Ballista", Zone::Battlefield);
    state.objects.get_mut(ballista).counters.plus1_plus1 = 2;
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, ballista, 0), "{{2}} with one mana");
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, ballista, 0), "{{2}} with two mana");
    activate(&mut state, ballista, 0, &[]);
    settled(&mut state);
    assert_eq!(counters(&state, ballista), 3);
}

#[test]
fn forensic_gadgeteer_reduces_only_artifacts_you_control() {
    // The opponent's Clue still costs {2}.
    let mut state = plains();
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let clue = token(&mut state, P1, "Clue Token");
    state.priority_player = P1;
    pool(&mut state, P1, &[], 1);
    assert!(!activatable(&mut state, clue, 0));
    pool(&mut state, P1, &[], 1);
    assert!(activatable(&mut state, clue, 0));

    // A creature that is not an artifact costs what it printed: Faerie
    // Mastermind's {3}{U}.
    let mut state = plains();
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let mastermind = put(&mut state, P0, "Faerie Mastermind", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U], 2);
    assert!(!activatable(&mut state, mastermind, 0));
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, mastermind, 0));
}

#[test]
fn forensic_gadgeteer_reduces_an_artifact_mana_ability_that_has_a_mana_cost() {
    let any_black = |state: &mut GameState, source: ObjectId| {
        engine::step(
            state,
            Action::ActivateManaAbilityChoice(source, ManaColor::B),
        )
    };

    // Without the Gadgeteer Energy Refractor's "{2}: Add one mana of any
    // color" needs two mana.
    let mut state = plains();
    let refractor = put(&mut state, P0, "Energy Refractor", Zone::Battlefield);
    next(&mut state);
    pool(&mut state, P0, &[], 1);
    assert!(any_black(&mut state, refractor).is_err(), "{{2}} with one");
    pool(&mut state, P0, &[], 1);
    any_black(&mut state, refractor).expect("{2} with two");
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);

    // With one, a single mana pays it and only that mana is spent.
    let mut state = plains();
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let reduced = put(&mut state, P0, "Energy Refractor", Zone::Battlefield);
    next(&mut state);
    pool(&mut state, P0, &[], 1);
    any_black(&mut state, reduced).expect("{1} with the Gadgeteer");
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "one mana was charged");

    // The reduction never takes the cost below one mana.
    let mut state = plains();
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let floor = put(&mut state, P0, "Energy Refractor", Zone::Battlefield);
    next(&mut state);
    assert!(any_black(&mut state, floor).is_err(), "never free");
    pool(&mut state, P0, &[], 1);
    any_black(&mut state, floor).expect("{1} with two Gadgeteers");
    assert_eq!(mana(&state, P0, ManaColor::C), 0);

    // An opponent's Gadgeteer does not help.
    let mut state = plains();
    put(&mut state, P1, "Forensic Gadgeteer", Zone::Battlefield);
    let foreign = put(&mut state, P0, "Energy Refractor", Zone::Battlefield);
    next(&mut state);
    pool(&mut state, P0, &[], 1);
    assert!(any_black(&mut state, foreign).is_err(), "{{2}} with one");

    // A mana ability that costs {1} is not reduced (Prophetic Prism).
    let mut state = plains();
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let prism = put(&mut state, P0, "Prophetic Prism", Zone::Battlefield);
    next(&mut state);
    assert!(any_black(&mut state, prism).is_err(), "{{1}} stays {{1}}");
    pool(&mut state, P0, &[], 1);
    any_black(&mut state, prism).expect("{1}");
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

/// Casts Anointed Peacekeeper for P0 and names `name`: its two-step choice
/// takes the group of the name's first letter, then the name.
fn peacekeeper_naming(state: &mut GameState, name: &str) {
    let keeper = put(state, P0, "Anointed Peacekeeper", Zone::Hand);
    pool(state, P0, &[ManaColor::W], 2);
    cast(state, keeper, &[]);
    let (group, index) = mtg_kernel::eternal_vintage_v1::nameable_definitions()
        .iter()
        .enumerate()
        .find_map(|(group, ids)| {
            ids.iter()
                .position(|&id| CARD_DEFS[usize::from(id)].object_name == name)
                .map(|index| (group, index))
        })
        .unwrap_or_else(|| panic!("{name} is not nameable"));
    match settle(state) {
        Some(Decision::ChooseEffectOption { player, .. }) => {
            assert_eq!(player, P0);
            act(state, Action::ChooseEffectOption(u16::try_from(group).unwrap()));
        }
        other => panic!("expected the letter, got {other:?}"),
    }
    if let Some(Decision::ChooseEffectOption { .. }) = settle(state) {
        act(state, Action::ChooseEffectOption(u16::try_from(index).unwrap()));
    }
    settled(state);
    assert_eq!(zone(state, keeper), Zone::Battlefield);
}

/// The floor is checked against the total cost (601.2f): Anointed
/// Peacekeeper's {2} tax is added first, then the Gadgeteer takes {1} off.
#[test]
fn forensic_gadgeteer_reduces_the_total_after_an_activation_tax() {
    // Pyrite Spellbomb's "{1}, Sacrifice: Draw a card" named by the
    // Peacekeeper costs {3}; with the Gadgeteer, {2}.
    let draw = def("Pyrite Spellbomb")
        .activated_abilities
        .iter()
        .position(|ability| {
            ability.cost.iter().any(|component| {
                matches!(component, CostComponent::Mana(cost) if cost.generic == 1 && cost.pips.is_empty())
            })
        })
        .expect("the {1} ability");
    let draw = u8::try_from(draw).unwrap();
    let mut state = plains();
    peacekeeper_naming(&mut state, "Pyrite Spellbomb");
    let bomb = put(&mut state, P0, "Pyrite Spellbomb", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    assert!(!activatable(&mut state, bomb, draw), "{{3}} with two mana");
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    assert!(activatable(&mut state, bomb, draw), "{{2}} with two mana");
    let hand = hand_len(&state, P0);
    activate(&mut state, bomb, draw, &[]);
    on_the_stack(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "{{2}} was paid");
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), hand + 1);

    // An ability with no mana cost is taxed to {2}, which the Gadgeteer
    // reduces to {1}: Mishra's Bauble's "{T}, Sacrifice".
    let mut state = plains();
    peacekeeper_naming(&mut state, "Mishra's Bauble");
    put(&mut state, P0, "Forensic Gadgeteer", Zone::Battlefield);
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Battlefield);
    assert!(!activatable(&mut state, bauble, 0), "never free");
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, bauble, 0), "{{1}} with one mana");
}

// ---- Faerie Mastermind ---------------------------------------------------------------

#[test]
fn faerie_mastermind_draws_when_an_opponent_draws_their_second_card_each_turn() {
    let mut state = plains();
    let mastermind = put(&mut state, P0, "Faerie Mastermind", Zone::Battlefield);
    let (own, foe) = (hand_len(&state, P0), hand_len(&state, P1));

    // {3}{U}: each player draws a card. The opponent's first card.
    pool(&mut state, P0, &[ManaColor::U], 3);
    activate(&mut state, mastermind, 0, &[]);
    on_the_stack(&mut state);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), own + 1);
    assert_eq!(hand_len(&state, P1), foe + 1);
    assert_eq!(state.players[1].draws_this_turn, 1);

    // Their second card: the trigger draws a card for you.
    pool(&mut state, P0, &[ManaColor::U], 3);
    activate(&mut state, mastermind, 0, &[]);
    on_the_stack(&mut state);
    settled(&mut state);
    assert_eq!(state.players[1].draws_this_turn, 2);
    assert_eq!(hand_len(&state, P1), foe + 2);
    assert_eq!(hand_len(&state, P0), own + 3, "two draws and the trigger");

    // Their third card: not their second, so no trigger.
    pool(&mut state, P0, &[ManaColor::U], 3);
    activate(&mut state, mastermind, 0, &[]);
    on_the_stack(&mut state);
    settled(&mut state);
    assert_eq!(hand_len(&state, P1), foe + 3);
    assert_eq!(hand_len(&state, P0), own + 4);
}

#[test]
fn faerie_mastermind_counts_the_draw_step_and_ignores_your_own_draws() {
    let mut state = plains();
    let mastermind = put(&mut state, P0, "Faerie Mastermind", Zone::Battlefield);
    // The opponent's turn: their draw step draws the first card (no trigger).
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert_eq!(state.players[1].draws_this_turn, 1);
    let (own, foe) = (hand_len(&state, P0), hand_len(&state, P1));

    // The activated draw is their second card of the turn.
    state.priority_player = P0;
    pool(&mut state, P0, &[ManaColor::U], 3);
    activate(&mut state, mastermind, 0, &[]);
    on_the_stack(&mut state);
    settled(&mut state);
    assert_eq!(state.players[1].draws_this_turn, 2);
    assert_eq!(hand_len(&state, P1), foe + 1);
    assert_eq!(
        hand_len(&state, P0),
        own + 2,
        "the ability's draw and the trigger"
    );
}

// ---- Ledger Shredder -----------------------------------------------------------------

/// Settles a connive: discards `discard` when it is offered.
fn settle_connive_discarding(state: &mut GameState, discard: ObjectId) {
    settle_choosing(state, discard);
}

#[test]
fn ledger_shredder_connives_when_you_cast_your_second_spell_each_turn() {
    // A land is drawn and discarded: no counter.
    let mut state = plains();
    let shredder = put(&mut state, P0, "Ledger Shredder", Zone::Battlefield);
    let first = put(&mut state, P0, "Mox Pearl", Zone::Hand);
    let second = put(&mut state, P0, "Mox Pearl", Zone::Hand);
    let third = put(&mut state, P0, "Mox Pearl", Zone::Hand);
    cast(&mut state, first, &[]);
    priority(&mut state);
    assert_eq!(state.stack.len(), 1, "the first spell does not trigger");
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 2);

    let drawn = state.players[0].library[0];
    cast(&mut state, second, &[]);
    priority(&mut state);
    assert_eq!(state.stack.len(), 2, "the spell and the Shredder's trigger");
    assert_eq!(
        state.stack.last().map(|item| item.kind),
        Some(StackItemKind::TriggeredAbility)
    );
    settle_connive_discarding(&mut state, drawn);
    assert_eq!(zone(&state, drawn), Zone::Graveyard);
    assert_eq!(
        power_toughness(&state, shredder),
        (1, 3),
        "a land: no counter"
    );
    assert_eq!(hand_len(&state, P0), 1);

    // The third spell does not trigger.
    cast(&mut state, third, &[]);
    priority(&mut state);
    assert_eq!(state.stack.len(), 1);
    settled(&mut state);
    assert_eq!(power_toughness(&state, shredder), (1, 3));
}

#[test]
fn ledger_shredder_grows_when_connive_discards_a_nonland_card() {
    let mut state = game_top("Llanowar Elves");
    let shredder = put(&mut state, P0, "Ledger Shredder", Zone::Battlefield);
    let first = put(&mut state, P0, "Mox Pearl", Zone::Hand);
    let second = put(&mut state, P0, "Mox Pearl", Zone::Hand);
    cast(&mut state, first, &[]);
    settled(&mut state);
    let drawn = state.players[0].library[0];
    assert_eq!(
        CARD_DEFS[state.objects.get(drawn).card_def as usize].name,
        "Llanowar Elves"
    );
    cast(&mut state, second, &[]);
    settle_connive_discarding(&mut state, drawn);
    assert_eq!(zone(&state, drawn), Zone::Graveyard);
    assert_eq!(counters(&state, shredder), 1);
    assert_eq!(power_toughness(&state, shredder), (2, 4));
}

/// P1 casts a Lightning Bolt at `target` and keeps priority, leaving it on the
/// stack.
fn opponent_casts_bolt(state: &mut GameState, target: Target) {
    state.priority_player = P1;
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    cast(state, bolt, &[target]);
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
}

#[test]
fn ledger_shredder_also_connives_for_the_opponents_second_spell() {
    let mut state = game_top("Llanowar Elves");
    let shredder = put(&mut state, P0, "Ledger Shredder", Zone::Battlefield);
    opponent_casts_bolt(&mut state, Target::Player(P0));
    assert_eq!(state.stack.len(), 1, "their first spell");
    opponent_casts_bolt(&mut state, Target::Player(P0));
    assert_eq!(state.stack.len(), 3, "two bolts and the trigger");
    assert_eq!(
        state.stack.last().map(|item| item.kind),
        Some(StackItemKind::TriggeredAbility)
    );
    // Its controller (not the caster) draws and discards.
    let drawn = state.players[0].library[0];
    settle_connive_discarding(&mut state, drawn);
    assert_eq!(counters(&state, shredder), 1);
    assert_eq!(life(&state, P0), 14);
}

// ---- Jacked Rabbit -------------------------------------------------------------------

/// Casts Jacked Rabbit for `x` with exactly the mana it needs and lets it
/// resolve with every choice but X answered first.
fn cast_rabbit(state: &mut GameState, rabbit: ObjectId, x: u16) {
    pool(
        state,
        P0,
        &[ManaColor::W],
        u8::try_from(x).unwrap().saturating_add(1),
    );
    cast(state, rabbit, &[]);
    settle_option(state, x);
}

#[test]
fn jacked_rabbit_enters_with_x_counters_and_draws_only_at_five_or_more() {
    for (x, draws) in [(0_u16, false), (2, false), (4, false), (5, true), (6, true)] {
        let mut state = plains();
        let rabbit = put(&mut state, P0, "Jacked Rabbit", Zone::Hand);
        let library = library_len(&state, P0);
        cast_rabbit(&mut state, rabbit, x);
        assert_eq!(zone(&state, rabbit), Zone::Battlefield, "X = {x}");
        assert_eq!(counters(&state, rabbit), i32::from(x), "X = {x}");
        assert_eq!(
            power_toughness(&state, rabbit),
            (1 + i32::from(x), 2 + i32::from(x)),
            "X = {x}"
        );
        assert_eq!(mana(&state, P0, ManaColor::C), 0, "X = {x} paid {{X}}{{1}}");
        assert_eq!(
            library_len(&state, P0),
            library - usize::from(draws),
            "X = {x}: draws only at five or more"
        );
    }
}

#[test]
fn jacked_rabbit_ravenous_draw_needs_the_cast_x_not_the_counters() {
    // Put onto the battlefield without being cast: X is 0, no draw.
    let mut state = plains();
    let rabbit = put(&mut state, P0, "Jacked Rabbit", Zone::Hand);
    let library = library_len(&state, P0);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(rabbit, Zone::Battlefield),
    );
    state.objects.get_mut(rabbit).counters.plus1_plus1 = 6;
    priority(&mut state);
    assert_eq!(library_len(&state, P0), library);
    assert!(state.stack.is_empty());
}

#[test]
fn jacked_rabbit_makes_a_rabbit_for_each_point_of_power_when_it_attacks() {
    let mut state = plains();
    let rabbit = put(&mut state, P0, "Jacked Rabbit", Zone::Battlefield);
    state.objects.get_mut(rabbit).counters.plus1_plus1 = 2;
    attack_with(&mut state, vec![rabbit], |_, _| None);
    let rabbits = named(&state, P0, "Rabbit Token");
    assert_eq!(rabbits.len(), 3, "power 3: three tokens");
    for &token in &rabbits {
        assert_eq!(power_toughness(&state, token), (1, 1));
        assert!(!state.objects.get(token).tapped, "untapped, not attacking");
        assert!(def("Rabbit Token").subtypes.contains(&Subtype::Rabbit));
    }
    assert_eq!(
        life(&state, P1),
        17,
        "the Rabbit itself dealt combat damage"
    );

    // A 1/2 makes one.
    let mut state = plains();
    let rabbit = put(&mut state, P0, "Jacked Rabbit", Zone::Battlefield);
    attack_with(&mut state, vec![rabbit], |_, _| None);
    assert_eq!(named(&state, P0, "Rabbit Token").len(), 1);
}

#[test]
fn jacked_rabbit_uses_its_last_known_power_when_it_has_left() {
    let mut state = plains();
    let rabbit = put(&mut state, P0, "Jacked Rabbit", Zone::Battlefield);
    state.objects.get_mut(rabbit).counters.plus1_plus1 = 2;
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![rabbit]));
    run_until(&mut state, |state| !state.stack.is_empty(), |_, _| None);
    assert_eq!(state.stack.len(), 1, "the attack trigger");
    exile(&mut state, rabbit);
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
    assert_eq!(zone(&state, rabbit), Zone::Exile);
    assert_eq!(named(&state, P0, "Rabbit Token").len(), 3);
}

// ---- Bristly Bill, Spine Sower -------------------------------------------------------

#[test]
fn bristly_bill_puts_a_counter_on_any_target_creature_when_a_land_enters() {
    let mut state = plains();
    let bill = put(
        &mut state,
        P0,
        "Bristly Bill, Spine Sower",
        Zone::Battlefield,
    );
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let foe = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(forest));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(bill)), "itself");
            assert!(legal_targets.contains(&Target::Object(elves)));
            assert!(
                legal_targets.contains(&Target::Object(foe)),
                "any creature, not only yours"
            );
            assert!(!legal_targets.contains(&Target::Object(forest)));
            Action::ChooseTarget(Target::Object(foe))
        }
        other => first_answer(other),
    });
    assert_eq!(counters(&state, foe), 1);
    assert_eq!(power_toughness(&state, foe), (3, 2));
    assert_eq!(counters(&state, elves), 0);
    assert_eq!(counters(&state, bill), 0);

    // It may grow itself.
    let mut state = plains();
    let bill = put(
        &mut state,
        P0,
        "Bristly Bill, Spine Sower",
        Zone::Battlefield,
    );
    let forest = put(&mut state, P0, "Forest", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(forest));
    settle_choosing(&mut state, bill);
    assert_eq!(power_toughness(&state, bill), (3, 3));
}

#[test]
fn bristly_bill_does_not_trigger_for_an_opponents_land_or_a_nonland() {
    let mut state = plains();
    put(
        &mut state,
        P0,
        "Bristly Bill, Spine Sower",
        Zone::Battlefield,
    );
    let foe_land = put(&mut state, P1, "Forest", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(foe_land, Zone::Battlefield),
    );
    priority(&mut state);
    assert!(state.stack.is_empty(), "a land the opponent controls");
    let ring = put(&mut state, P0, "Mox Pearl", Zone::Hand);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Battlefield, "a nonland permanent");
}

#[test]
fn bristly_bill_doubles_the_counters_on_each_creature_you_control() {
    let mut state = plains();
    let bill = put(
        &mut state,
        P0,
        "Bristly Bill, Spine Sower",
        Zone::Battlefield,
    );
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    let foe = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    state.objects.get_mut(bill).counters.plus1_plus1 = 1;
    state.objects.get_mut(elves).counters.plus1_plus1 = 3;
    state.objects.get_mut(foe).counters.plus1_plus1 = 2;
    // The Elves would pay a fifth mana from their own ability; keep them tapped
    // so that only the pool counts.
    state.objects.get_mut(elves).tapped = true;
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    assert!(
        !activatable(&mut state, bill, 0),
        "{{3}}{{G}}{{G}} needs five"
    );
    pool(&mut state, P0, &[], 1);
    activate(&mut state, bill, 0, &[]);
    on_the_stack(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::G), 0);
    settled(&mut state);
    assert_eq!(counters(&state, bill), 2);
    assert_eq!(counters(&state, elves), 6);
    assert_eq!(power_toughness(&state, elves), (7, 7));
    assert_eq!(counters(&state, mother), 0, "no counters double to none");
    assert_eq!(counters(&state, foe), 2, "only creatures you control");
}

// ---- Lutri, the Spellchaser ----------------------------------------------------------

/// P0 casts a Lightning Bolt at P1 and then, in response, Lutri. Answers
/// Lutri's trigger by naming the Bolt and returns the state with the copy's
/// choice of target open, with the number of options it offers.
fn lutri_copy_choice() -> (GameState, ObjectId, ObjectId, u16) {
    let mut state = plains();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let lutri = put(&mut state, P0, "Lutri, the Spellchaser", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::U, ManaColor::U],
        1,
    );
    cast(&mut state, bolt, &[Target::Player(P1)]);
    cast(&mut state, lutri, &[]);
    // Both players pass: Lutri resolves and its trigger asks for a target.
    match settle(&mut state) {
        Some(Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        }) => {
            assert_eq!(legal_targets, vec![Target::Object(bolt)]);
            assert!(!can_finish, "the target is not optional");
        }
        other => panic!("expected the trigger's target, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(bolt)));
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { option_count, .. }) => {
            (state, bolt, lutri, option_count)
        }
        other => panic!("expected the copy's target choice, got {other:?}"),
    }
}

#[test]
fn lutri_copies_its_spell_and_may_keep_the_target() {
    let (mut state, bolt, lutri, options) = lutri_copy_choice();
    // Keep it, give it to you, or give it to the only creature.
    assert_eq!(options, 3);
    act(&mut state, Action::ChooseEffectOption(0));
    settled(&mut state);
    assert_eq!(life(&state, P1), 14, "the bolt and its copy");
    assert_eq!(life(&state, P0), 20);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, lutri), Zone::Battlefield);
}

#[test]
fn lutri_copy_may_choose_a_new_target() {
    let (mut state, _, lutri, _) = lutri_copy_choice();
    act(&mut state, Action::ChooseEffectOption(1));
    priority(&mut state);
    let copy = state.stack.last().expect("the copy");
    assert!(
        copy.is_copy && copy.controller == P0,
        "a copy you control, not cast"
    );
    assert_eq!(copy.targets, vec![Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 17, "the copy hit its new target");
    assert_eq!(life(&state, P1), 17, "the original kept its own");
    assert_eq!(zone(&state, lutri), Zone::Battlefield);

    let (mut state, _, lutri, _) = lutri_copy_choice();
    act(&mut state, Action::ChooseEffectOption(2));
    settled(&mut state);
    assert_eq!(
        zone(&state, lutri),
        Zone::Graveyard,
        "three damage to a 3/2"
    );
    assert_eq!(life(&state, P1), 17);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn lutri_needs_an_instant_or_sorcery_spell_you_control_and_only_when_cast() {
    // The opponent's spell is not a legal target: the trigger is removed.
    let mut state = plains();
    let lutri = put(&mut state, P0, "Lutri, the Spellchaser", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    opponent_bolts(&mut state, Target::Player(P0));
    cast(&mut state, lutri, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, lutri), Zone::Battlefield);
    assert_eq!(life(&state, P0), 17, "only their bolt, no copy");

    // Cast with nothing to copy: no trigger either.
    let mut state = plains();
    let lutri = put(&mut state, P0, "Lutri, the Spellchaser", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    cast(&mut state, lutri, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, lutri), Zone::Battlefield);

    // Entering any other way is not "if you cast it": no trigger.
    let mut state = plains();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    priority(&mut state);
    let lutri = put(&mut state, P0, "Lutri, the Spellchaser", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(lutri, Zone::Battlefield),
    );
    priority(&mut state);
    assert_eq!(state.stack.len(), 1, "only the bolt: Lutri did not trigger");
}

#[test]
fn lutri_trigger_does_nothing_when_the_spell_has_been_countered() {
    let (mut state, bolt, _) = {
        let mut state = plains();
        let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        let lutri = put(&mut state, P0, "Lutri, the Spellchaser", Zone::Hand);
        pool(
            &mut state,
            P0,
            &[ManaColor::R, ManaColor::U, ManaColor::U],
            1,
        );
        cast(&mut state, bolt, &[Target::Player(P1)]);
        cast(&mut state, lutri, &[]);
        (state, bolt, lutri)
    };
    match settle(&mut state) {
        Some(Decision::ChooseTargets { .. }) => {
            act(&mut state, Action::ChooseTarget(Target::Object(bolt)));
        }
        other => panic!("expected the trigger's target, got {other:?}"),
    }
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    assert_eq!(state.stack.len(), 2, "the bolt and the trigger");
    // P0 passes; P1 counters the Bolt before the trigger resolves.
    act(&mut state, Action::Pass);
    let counterspell = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, counterspell, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "countered");
    assert!(state.stack.is_empty(), "no copy of a spell that is gone");
    assert_eq!(life(&state, P1), 20);
    assert_eq!(life(&state, P0), 20);
}

#[test]
fn lutri_is_a_companion_when_every_nonland_card_has_a_different_name() {
    let lutri = id("Lutri, the Spellchaser");
    let ids = |names: &[&str]| names.iter().map(|name| id(name)).collect::<Vec<_>>();
    assert_eq!(
        companion_condition_met(
            lutri,
            &ids(&[
                "Island",
                "Island",
                "Island",
                "Sol Ring",
                "Mox Pearl",
                "Lightning Bolt"
            ])
        ),
        Some(true),
        "lands may repeat"
    );
    assert_eq!(
        companion_condition_met(
            lutri,
            &ids(&["Island", "Sol Ring", "Mox Pearl", "Sol Ring"])
        ),
        Some(false),
        "a nonland card twice"
    );
    assert_eq!(
        companion_condition_met(lutri, &ids(&["Island", "Island"])),
        Some(true)
    );

    // It waits in exile and joins the hand for {3} as a special action.
    let mut state = plains();
    add_companions(&mut state, [Some(lutri), None]);
    let companion = *state.exile.last().unwrap();
    assert_eq!(zone(&state, companion), Zone::Exile);
    pool(&mut state, P0, &[], 2);
    assert!(
        !activatable(&mut state, companion, 0),
        "{{3}} is not payable"
    );
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, companion, 0));
    act(&mut state, Action::ActivateAbility(companion, 0));
    assert_eq!(zone(&state, companion), Zone::Hand);
    assert!(state.stack.is_empty(), "a special action");
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn the_limited_session_accepts_lutri_as_a_companion_only_for_a_deck_of_different_names() {
    let reset = |second: &str, count: u32| {
        json!({"schema_version": 4, "request_type": "reset", "request_id": "reset",
            "decks": [
                {"cards": [{"name": "Island", "count": 40 - count},
                           {"name": second, "count": count}],
                 "companion": "Lutri, the Spellchaser"},
                {"cards": [{"name": "Island", "count": 40}]},
            ],
            "episode_id": 7, "env_seed": 7,
            "max_physical_decisions": 4096, "max_policy_steps": 8192})
        .to_string()
    };
    let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
    let reply: Value = serde_json::from_str(&server.handle_line(&reset("Mox Pearl", 1))).unwrap();
    assert_eq!(reply["response_type"], "decision", "{reply}");

    let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
    let reply: Value = serde_json::from_str(&server.handle_line(&reset("Mox Pearl", 2))).unwrap();
    assert_eq!(reply["response_type"], "error", "{reply}");
    assert_eq!(reply["error"]["code"], "unsupported_deck");
}

// ---- Caustic Bronco ------------------------------------------------------------------

/// Activates Caustic Bronco's Saddle tapping exactly `crew`, and lets it
/// resolve.
fn saddle_with(state: &mut GameState, bronco: ObjectId, crew: &[ObjectId]) {
    assert!(activatable(state, bronco, 0), "Saddle 3 is payable");
    act(state, Action::ActivateAbility(bronco, 0));
    for &member in crew {
        match next(state) {
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                assert!(
                    !legal_targets.contains(&Target::Object(bronco)),
                    "other creatures only"
                );
                assert!(
                    legal_targets.contains(&Target::Object(member)),
                    "{member:?} not among {legal_targets:?}"
                );
                act(state, Action::ChooseEffectTarget(Target::Object(member)));
            }
            other => panic!("expected the saddle selection, got {other:?}"),
        }
    }
    match next(state) {
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => act(state, Action::FinishEffectSelection),
        other => panic!("expected the saddle selection to finish, got {other:?}"),
    }
    on_the_stack(state);
}

#[test]
fn caustic_bronco_reveals_the_top_card_and_loses_life_equal_to_its_mana_value() {
    let mut state = game_top("Mulldrifter");
    let bronco = put(&mut state, P0, "Caustic Bronco", Zone::Battlefield);
    let top = state.players[0].library[0];
    let library = library_len(&state, P0);
    attack_with(&mut state, vec![bronco], |_, _| None);
    assert!(hand_contains(&state, P0, top), "into the hand, not drawn");
    assert_eq!(library_len(&state, P0), library - 1);
    assert_eq!(life(&state, P0), 15, "Mulldrifter has mana value five");
    assert_eq!(life(&state, P1), 18, "the Bronco's combat damage");

    // A land has mana value zero: no life is lost.
    let mut state = plains();
    let bronco = put(&mut state, P0, "Caustic Bronco", Zone::Battlefield);
    attack_with(&mut state, vec![bronco], |_, _| None);
    assert_eq!(life(&state, P0), 20);
    assert_eq!(hand_len(&state, P0), 1);
}

#[test]
fn caustic_bronco_saddled_makes_each_opponent_lose_the_life_instead() {
    let mut state = game_top("Mulldrifter");
    let bronco = put(&mut state, P0, "Caustic Bronco", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    let foe = put(&mut state, P1, "Jackal Pup", Zone::Battlefield);
    state.objects.get_mut(mother).summoning_sick = true;
    saddle_with(&mut state, bronco, &[pup, mother]);
    assert!(state.objects.get(pup).tapped && state.objects.get(mother).tapped);
    assert!(!state.objects.get(bronco).tapped, "the Mount is not tapped");
    assert!(!state.objects.get(foe).tapped, "only your creatures");
    settled(&mut state);
    attack_with(&mut state, vec![bronco], |_, _| None);
    assert_eq!(life(&state, P0), 20, "saddled: you lose nothing");
    assert_eq!(life(&state, P1), 13, "five from the reveal and two combat");
}

#[test]
fn caustic_bronco_saddle_needs_total_power_three_other_untapped_creatures_and_a_sorcery_window() {
    // One 2-power creature is not enough.
    let mut state = plains();
    let bronco = put(&mut state, P0, "Caustic Bronco", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    assert!(!activatable(&mut state, bronco, 0), "power 2 of 3");
    // The opponent's creatures do not count.
    put(&mut state, P1, "Mother of Runes", Zone::Battlefield);
    assert!(!activatable(&mut state, bronco, 0), "their creature");
    // The Bronco cannot saddle itself, and a tapped creature is no help.
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    state.objects.get_mut(mother).tapped = true;
    assert!(!activatable(&mut state, bronco, 0), "a tapped creature");
    state.objects.get_mut(mother).tapped = false;
    assert!(activatable(&mut state, bronco, 0), "power 3 of 3");

    // Only as a sorcery: not while a spell is on the stack.
    let ring = put(&mut state, P0, "Mox Pearl", Zone::Hand);
    cast(&mut state, ring, &[]);
    assert!(
        !activatable(&mut state, bronco, 0),
        "the stack is not empty"
    );
    settled(&mut state);
    assert!(activatable(&mut state, bronco, 0));
    assert!(!state.objects.get(pup).tapped);
}

#[test]
fn caustic_bronco_is_saddled_only_until_end_of_turn() {
    let mut state = game_top("Mulldrifter");
    let bronco = put(&mut state, P0, "Caustic Bronco", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    saddle_with(&mut state, bronco, &[pup, mother]);
    settled(&mut state);
    // Its controller's next turn: no longer saddled, so it costs them life.
    let turn = state.turn;
    pass_until(&mut state, |state| {
        state.turn > turn && state.active_player == P0 && state.step == Step::Main1
    });
    put(&mut state, P0, "Mulldrifter", Zone::Library);
    let before = life(&state, P0);
    attack_with(&mut state, vec![bronco], |_, _| None);
    assert_eq!(life(&state, P0), before - 5, "not saddled any more");
}

/// Saddle is not crew (702.171a): a Pilot Token "crews Vehicles as though its
/// power were 2 greater", which does not count toward saddling a Mount.
#[test]
fn caustic_bronco_saddle_does_not_count_a_pilot_tokens_crew_bonus() {
    let mut state = plains();
    let bronco = put(&mut state, P0, "Caustic Bronco", Zone::Battlefield);
    token(&mut state, P0, "Pilot Token");
    put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    assert!(!activatable(&mut state, bronco, 0), "power 1 + 1 of 3");

    // The same two creatures crew Esika's Chariot (crew 4): 1 + 2 + 1.
    let chariot = put(&mut state, P0, "Esika's Chariot", Zone::Battlefield);
    let crew = def("Esika's Chariot")
        .activated_abilities
        .iter()
        .position(|ability| {
            ability
                .cost
                .iter()
                .any(|component| matches!(component, CostComponent::Crew(4)))
        })
        .expect("crew 4");
    let crew = u8::try_from(crew).unwrap();
    assert!(activatable(&mut state, chariot, crew), "1 + 2 + 1 of 4");
    assert!(!activatable(&mut state, bronco, 0), "still power 2 of 3");
}

// ---- Public sessions ---------------------------------------------------------------

#[test]
fn c4a_decks_finish_and_replay_through_public_sessions_both_seats() {
    let blue = deck(&[
        ("Island", 16),
        ("Displacer Kitten", 4),
        ("Faerie Mastermind", 4),
        ("Ledger Shredder", 4),
        ("Forensic Gadgeteer", 4),
        ("Lutri, the Spellchaser", 4),
        ("Mind Stone", 4),
    ]);
    let three_colors = deck(&[
        ("Plains", 7),
        ("Swamp", 7),
        ("Forest", 6),
        ("Caustic Bronco", 4),
        ("Jacked Rabbit", 4),
        ("Bristly Bill, Spine Sower", 4),
        ("Llanowar Elves", 4),
        ("Thraben Inspector", 4),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    for (pair, first_seed) in [
        ([blue.clone(), three_colors.clone()], 0x4334_6100_u64),
        ([three_colors, blue], 0x4334_6110),
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
    for name in C4A_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
