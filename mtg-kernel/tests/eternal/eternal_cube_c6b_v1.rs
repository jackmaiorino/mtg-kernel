//! Vintage Cube wave C6b: native rules and public-session tests for the
//! discard counts, dies copies and graveyard plays.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
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
const URSINE: &str = "Ursine Monstrosity";

const C6B_CARDS: [&str; 7] = [
    "Seasoned Cryomancer",
    "Seasoned Pyromancer",
    "Sphinx of False Conclusions",
    "Metamorphosis Fanatic",
    "Night Nurse, Healer of Heroes",
    "Six",
    "Ursine Monstrosity",
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
        0x4355_4245_0000 + 0x3662,
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
        Decision::ChooseTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
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
fn c6b_cards_are_full() {
    for name in C6B_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn ursine_printed_characteristics_and_pyrogoyf_unimplemented_boundary() {
    let card = def(URSINE);
    assert_eq!(card.cost.generic, 2);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(card.mana_value, 3);
    assert_eq!(card.types, &[CardType::Creature]);
    assert_eq!(card.subtypes, &[Subtype::Bear, Subtype::Mutant]);
    assert_eq!(card.colors, &[ManaColor::G]);
    assert!(card.supertypes.is_empty());
    assert_eq!((card.power, card.toughness), (Some(3), Some(3)));
    assert_eq!(card.keywords, Keywords::TRAMPLE);
    assert!(!card.is_token);
    assert!(!def("Pyrogoyf").is_executable());
    assert_eq!(
        (def("Pyrogoyf").power, def("Pyrogoyf").toughness),
        (Some(0), Some(1))
    );
}

fn cast_ursine(state: &mut GameState) -> ObjectId {
    let source = put(state, P0, URSINE, Zone::Hand);
    pool(state, P0, &[ManaColor::G], 2);
    cast(state, source, &[]);
    settled(state);
    assert_eq!(zone(state, source), Zone::Battlefield);
    source
}

#[test]
fn ursine_mills_then_fixes_own_card_types_and_expires_on_a_real_turn() {
    let mut state = game(&["Baleful Strix", "Plains", "Plains", "Plains", "Plains"]);
    let top = state.players[0].library[0];
    put(&mut state, P0, "Tarfire", Zone::Graveyard);
    put(&mut state, P1, "Rancor", Zone::Graveyard);
    let source = cast_ursine(&mut state);
    state.objects.get_mut(source).summoning_sick = false;
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    assert_eq!(zone(&state, top), Zone::Graveyard);
    assert_eq!(state.players[0].library.len(), 4);
    // Kindred, instant, artifact, creature. The opponent's enchantment does not count.
    assert_eq!(power_toughness(&state, source), (7, 7));
    assert!(keyword(&state, source, Keywords::INDESTRUCTIBLE));
    assert!(keyword(&state, source, Keywords::TRAMPLE));
    put(&mut state, P0, "Ponder", Zone::Graveyard);
    assert_eq!(power_toughness(&state, source), (7, 7), "boost is fixed");
    assert!(engine::step(&mut state, Action::DeclareAttackers(Vec::new())).is_err());
    act(&mut state, Action::DeclareAttackers(vec![source]));
    pass_until(&mut state, |state| state.step == Step::Main2);
    assert_eq!(life(&state, P1), 13);
    assert_eq!(
        power_toughness(&state, source),
        (7, 7),
        "lasts beyond combat"
    );
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert_eq!(power_toughness(&state, source), (3, 3));
    assert!(!keyword(&state, source, Keywords::INDESTRUCTIBLE));
    let library = state.players[0].library.clone();
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    assert_eq!(
        state.players[0].library, library,
        "no opponent-combat trigger"
    );
    assert_eq!(power_toughness(&state, source), (3, 3));
}

#[test]
fn ursine_tramples_and_survives_lethal_damage_during_its_combat() {
    let mut state = plains();
    let source = cast_ursine(&mut state);
    state.objects.get_mut(source).summoning_sick = false;
    let blocker = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    assert_eq!(power_toughness(&state, source), (4, 4));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(blocker, Target::Object(source), 10),
    );
    assert!(matches!(
        next(&mut state),
        Decision::DeclareAttackers { .. }
    ));
    assert_eq!(zone(&state, source), Zone::Battlefield);
    act(&mut state, Action::DeclareAttackers(vec![source]));
    run_until(
        &mut state,
        |state| state.step == Step::Main2,
        |_, decision| {
            if matches!(decision, Decision::DeclareBlockers { .. }) {
                Some(Action::DeclareBlockers(vec![(blocker, source)]))
            } else {
                None
            }
        },
    );
    assert_eq!(zone(&state, source), Zone::Battlefield);
    assert_eq!(zone(&state, blocker), Zone::Graveyard);
    assert_eq!(life(&state, P1), 17, "three trample damage over a 1/1");
}

#[test]
fn ursine_only_requires_an_attack_when_able_and_still_mills_when_sick_or_tapped() {
    for tapped in [false, true] {
        let mut state = plains();
        let source = cast_ursine(&mut state);
        state.objects.get_mut(source).tapped = tapped;
        state.objects.get_mut(source).summoning_sick = !tapped;
        pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
        assert_eq!(state.players[0].library.len(), 19);
        assert_eq!(power_toughness(&state, source), (4, 4));
        assert!(keyword(&state, source, Keywords::INDESTRUCTIBLE));
        act(&mut state, Action::DeclareAttackers(Vec::new()));
    }
}

#[test]
fn ursine_requirement_selects_the_player_even_when_a_planeswalker_is_available() {
    let mut state = plains();
    let source = cast_ursine(&mut state);
    state.objects.get_mut(source).summoning_sick = false;
    let walker = put(&mut state, P1, "Oko, Thief of Crowns", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(walker, Zone::Battlefield),
    );
    settled(&mut state);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![source]));
    match next(&mut state) {
        Decision::ChooseAttackTarget {
            attacker,
            candidates,
            ..
        } => {
            assert_eq!(attacker, source);
            assert_eq!(candidates, vec![Target::Player(P1)]);
        }
        other => panic!("expected attack target, got {other:?}"),
    }
    assert!(engine::step(
        &mut state,
        Action::ChooseAttackTarget(Target::Object(walker))
    )
    .is_err());
    act(&mut state, Action::ChooseAttackTarget(Target::Player(P1)));
    pass_until(&mut state, |state| state.step == Step::Main2);
    assert_eq!(life(&state, P1), 16);
    assert_eq!(
        mtg_kernel::planeswalker_v1::loyalty(&state, walker),
        Some(4)
    );
}

#[test]
fn ursine_mill_survives_source_removal_but_boost_does_not_follow_a_blink() {
    for return_source in [false, true] {
        let mut state = plains();
        let source = cast_ursine(&mut state);
        pass_until(&mut state, |state| {
            state.step == Step::BeginCombat && !state.stack.is_empty()
        });
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
        if return_source {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
        }
        settled(&mut state);
        assert_eq!(state.players[0].library.len(), 19);
        assert_eq!(
            zone(&state, source),
            if return_source {
                Zone::Battlefield
            } else {
                Zone::Exile
            }
        );
        assert_eq!(power_toughness(&state, source), (3, 3));
        assert!(!keyword(&state, source, Keywords::INDESTRUCTIBLE));
        if return_source {
            state.objects.get_mut(source).summoning_sick = false;
        }
        pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
        act(&mut state, Action::DeclareAttackers(Vec::new()));
    }
}

#[test]
fn ursine_replaced_mill_and_empty_library_still_grant_indestructible_without_a_boost() {
    for empty in [false, true] {
        let mut state = if empty { game(&[]) } else { plains() };
        let top = state.players[0].library.first().copied();
        if !empty {
            put(&mut state, P1, "Dauthi Voidwalker", Zone::Battlefield);
        }
        let source = cast_ursine(&mut state);
        pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
        assert!(state.players[0].graveyard.is_empty());
        if let Some(top) = top {
            assert_eq!(zone(&state, top), Zone::Exile);
        }
        assert_eq!(power_toughness(&state, source), (3, 3));
        assert!(keyword(&state, source, Keywords::INDESTRUCTIBLE));
        act(&mut state, Action::DeclareAttackers(Vec::new()));
    }
}

#[derive(Default)]
struct C6bEvents {
    casts: [usize; C6B_CARDS.len()],
}

impl C6bEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        if semantic["action_kind"].as_str() != Some("cast_spell") {
            return;
        }
        let Some(card) = semantic["source"]["card_db_id"]
            .as_u64()
            .and_then(|id| usize::try_from(id).ok())
            .and_then(|id| CARD_DEFS.get(id))
        else {
            return;
        };
        if let Some(index) = C6B_CARDS.iter().position(|name| *name == card.name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c6b_decks_finish_and_replay_through_public_sessions_both_seats() {
    let spells = deck(&[
        ("Volcanic Island", 20),
        ("Seasoned Cryomancer", 6),
        ("Seasoned Pyromancer", 6),
        ("Sphinx of False Conclusions", 4),
        ("Mulldrifter", 4),
    ]);
    let permanents = deck(&[
        ("Bayou", 10),
        ("Savannah", 6),
        ("Scrubland", 4),
        ("Metamorphosis Fanatic", 6),
        ("Night Nurse, Healer of Heroes", 6),
        ("Six", 4),
        ("Ursine Monstrosity", 4),
        ("Llanowar Elves", 4),
    ]);
    let mut events = C6bEvents::default();
    // 2 seats x 3 seeds x 2 policies x (original + replay) = 24 games total.
    for pair in [[spells.clone(), permanents.clone()], [permanents, spells]] {
        for seed in 0x4336_4200_u64..0x4336_4203 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action);
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, casts) in C6B_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} was never cast in a public session");
    }
}

const NURSE: &str = "Night Nurse, Healer of Heroes";
const SIX: &str = "Six";

#[test]
fn nurse_and_six_printed_characteristics() {
    for (name, generic, color, subtypes, pt, keywords) in [
        (
            NURSE,
            1,
            ManaColor::W,
            vec![Subtype::Human, Subtype::Doctor, Subtype::Hero],
            (2, 1),
            Keywords::FLASH | Keywords::LIFELINK,
        ),
        (
            SIX,
            2,
            ManaColor::G,
            vec![Subtype::Treefolk],
            (2, 4),
            Keywords::REACH,
        ),
    ] {
        let card = def(name);
        assert_eq!(card.cost.generic, generic);
        assert_eq!(card.cost.pips, &[Pip::Colored(color)]);
        assert_eq!(card.mana_value, u16::from(generic) + 1);
        assert_eq!(card.types, &[CardType::Creature]);
        assert_eq!(card.subtypes, subtypes.as_slice());
        assert_eq!(
            card.supertypes,
            &[mtg_kernel::card_def::Supertype::Legendary]
        );
        assert_eq!(card.colors, &[color]);
        assert_eq!((card.power, card.toughness), (Some(pt.0), Some(pt.1)));
        assert_eq!(card.keywords, keywords);
        assert!(!card.is_token);
    }
    assert_eq!(Subtype::Doctor as u16, 710);
}

fn cast_nurse(state: &mut GameState, source: ObjectId) {
    pool(state, P0, &[ManaColor::W], 1);
    cast(state, source, &[]);
}

fn cast_six(state: &mut GameState) -> ObjectId {
    let source = put(state, P0, SIX, Zone::Hand);
    pool(state, P0, &[ManaColor::G], 2);
    cast(state, source, &[]);
    settled(state);
    source
}

#[test]
fn nurse_returns_each_permanent_type_from_anywhere_and_only_eligible_cards() {
    for (name, origin) in [
        ("Plains", Zone::Hand),
        ("Gurmag Angler", Zone::Battlefield),
        ("Lotus Petal", Zone::Library),
        ("Rancor", Zone::Exile),
        ("Wrenn and Six", Zone::Hand),
    ] {
        let mut state = plains();
        let nurse = put(&mut state, P0, NURSE, Zone::Hand);
        let target = put(&mut state, P0, name, origin);
        let old = put(&mut state, P0, "Plains", Zone::Graveyard);
        let instant = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        let theirs = put(&mut state, P1, "Plains", Zone::Hand);
        let token = token(&mut state, P0, "1/1 red Elemental Token");
        for object in [target, instant, theirs, token] {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(object, Zone::Graveyard),
            );
        }
        cast_nurse(&mut state, nurse);
        let mut choices = 0;
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseTargets {
                legal_targets,
                can_finish,
                ..
            } => {
                choices += 1;
                assert!(!can_finish, "Night Nurse is mandatory");
                assert_eq!(legal_targets, vec![Target::Object(target)]);
                Action::ChooseTarget(Target::Object(target))
            }
            other => first_answer(other),
        });
        assert_eq!(choices, 1);
        assert_eq!(zone(&state, target), Zone::Hand);
        assert_eq!(zone(&state, nurse), Zone::Battlefield);
        assert_eq!(zone(&state, old), Zone::Graveyard);
    }
}

#[test]
fn nurse_can_return_a_countered_permanent_spell() {
    let mut state = plains();
    let nurse = put(&mut state, P0, NURSE, Zone::Hand);
    let petal = put(&mut state, P0, "Lotus Petal", Zone::Hand);
    cast(&mut state, petal, &[]);
    on_the_stack(&mut state);
    act(&mut state, Action::Pass);
    let counter = put(&mut state, P1, "Counterspell", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U; 2], 0);
    cast(&mut state, counter, &[Target::Object(petal)]);
    settled(&mut state);
    assert_eq!(zone(&state, petal), Zone::Graveyard);
    cast_nurse(&mut state, nurse);
    settle_choosing(&mut state, petal);
    assert_eq!(zone(&state, petal), Zone::Hand);
}

#[test]
fn nurse_flash_lifelink_and_no_eligible_target() {
    let mut state = plains();
    state.active_player = P1;
    let nurse = put(&mut state, P0, NURSE, Zone::Hand);
    pool(&mut state, P0, &[], 2);
    assert!(!castable(&mut state, nurse), "white mana is required");
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, nurse, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, nurse), (2, 1));
    assert!(keyword(&state, nurse, Keywords::FLASH));
    assert!(keyword(&state, nurse, Keywords::LIFELINK));
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    let before = life(&state, P0);
    attack_with(&mut state, vec![nurse], |_, _| None);
    assert_eq!(life(&state, P0), before + 2);
    assert_eq!(life(&state, P1), 18);
}

#[test]
fn nurse_uses_actual_turn_and_latest_graveyard_incarnation() {
    let mut state = plains();
    let nurse = put(&mut state, P0, NURSE, Zone::Hand);
    let old = put(&mut state, P0, "Plains", Zone::Hand);
    let renewed = put(&mut state, P0, "Island", Zone::Hand);
    for object in [old, renewed] {
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(object, Zone::Graveyard),
        );
    }
    let round = state.turn_key_v1();
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_eq!(state.turn_key_v1(), round, "two turns share one round key");
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(renewed, Zone::Exile));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(renewed, Zone::Graveyard),
    );
    act(&mut state, Action::Pass);
    cast_nurse(&mut state, nurse);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(renewed)]);
            Action::ChooseTarget(Target::Object(renewed))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, old), Zone::Graveyard);
    assert_eq!(zone(&state, renewed), Zone::Hand);
}

#[test]
fn nurse_target_leaving_or_returning_fizzles_but_source_leaving_does_not() {
    for (blink_target, remove_source) in [(false, false), (true, false), (false, true)] {
        let mut state = plains();
        let nurse = put(&mut state, P0, NURSE, Zone::Hand);
        let target = put(&mut state, P0, "Plains", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(target, Zone::Graveyard),
        );
        cast_nurse(&mut state, nurse);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseTargets { .. })
        ));
        act(&mut state, Action::ChooseTarget(Target::Object(target)));
        on_the_stack(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(if remove_source { nurse } else { target }, Zone::Exile),
        );
        if blink_target {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Graveyard),
            );
        }
        let mut replay = state.clone();
        settled(&mut state);
        settled(&mut replay);
        assert_eq!(state, replay);
        assert_eq!(
            zone(&state, target),
            if remove_source {
                Zone::Hand
            } else if blink_target {
                Zone::Graveyard
            } else {
                Zone::Exile
            }
        );
    }
}

#[test]
fn six_attacks_mills_three_and_may_take_only_one_of_those_lands() {
    for (attacks, take) in [(false, false), (true, false), (true, true)] {
        let mut state = game(&["Plains", "Lightning Bolt", "Island", "Forest"]);
        let milled = state.players[0].library[..3].to_vec();
        let old = put(&mut state, P0, "Forest", Zone::Graveyard);
        let six = cast_six(&mut state);
        assert_eq!(state.players[0].library.len(), 4, "no entry mill");
        state.objects.get_mut(six).summoning_sick = false;
        let mut saw_land_choice = false;
        attack_with(
            &mut state,
            if attacks { vec![six] } else { Vec::new() },
            |s, decision| {
                if let Decision::ChooseEffectTargets {
                    legal_targets,
                    can_finish: true,
                    ..
                } = decision
                {
                    saw_land_choice = true;
                    assert_eq!(s.players[0].library.len(), 1);
                    assert!(!legal_targets.contains(&Target::Object(old)));
                    assert!(!legal_targets.contains(&Target::Object(milled[1])));
                    assert_eq!(legal_targets.len(), 2);
                    return Some(if take {
                        Action::ChooseEffectTarget(Target::Object(milled[2]))
                    } else {
                        Action::FinishEffectSelection
                    });
                }
                None
            },
        );
        assert_eq!(saw_land_choice, attacks);
        assert_eq!(state.players[0].library.len(), if attacks { 1 } else { 4 });
        assert_eq!(
            zone(&state, milled[2]),
            if take {
                Zone::Hand
            } else if attacks {
                Zone::Graveyard
            } else {
                Zone::Library
            }
        );
        assert_eq!(zone(&state, old), Zone::Graveyard);
        assert_eq!(life(&state, P1), if attacks { 18 } else { 20 });
    }
}

#[test]
fn six_mill_empty_short_no_land_and_public_replacement_destinations() {
    for (library, leyline) in [
        (vec![], false),
        (vec!["Lightning Bolt"], false),
        (vec!["Plains"], false),
        (vec!["Plains", "Island", "Lightning Bolt"], true),
    ] {
        let mut state = game(&library);
        if leyline {
            put(&mut state, P1, "Leyline of the Void", Zone::Battlefield);
        }
        let cards = state.players[0].library.clone();
        let six = cast_six(&mut state);
        state.objects.get_mut(six).summoning_sick = false;
        attack_with(&mut state, vec![six], |_, decision| {
            if let Decision::ChooseEffectTargets {
                legal_targets,
                can_finish: true,
                ..
            } = decision
            {
                return Some(Action::ChooseEffectTarget(legal_targets[0]));
            }
            None
        });
        assert!(state.players[0].library.is_empty());
        for (index, card) in cards.iter().enumerate() {
            assert_eq!(
                zone(&state, *card),
                if index == 0 && library[0] == "Plains" {
                    Zone::Hand
                } else if leyline {
                    Zone::Exile
                } else {
                    Zone::Graveyard
                }
            );
        }
    }
}

#[test]
fn six_retrace_requires_land_and_mana_and_returns_permanents_to_battlefield() {
    let mut state = plains();
    cast_six(&mut state);
    let nurse = put(&mut state, P0, NURSE, Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(!castable(&mut state, nurse), "no land to discard");
    let nonland = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    assert!(!castable(&mut state, nurse), "nonlands cannot pay retrace");
    let land = put(&mut state, P0, "Plains", Zone::Hand);
    let spare = put(&mut state, P0, "Island", Zone::Hand);
    cast(&mut state, nurse, &[]);
    match next(&mut state) {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&land) && candidates.contains(&spare));
            assert!(!candidates.contains(&nonland));
        }
        other => panic!("{other:?}"),
    }
    act(&mut state, Action::ChooseCostTarget(land));
    on_the_stack(&mut state);
    assert_eq!(zone(&state, land), Zone::Graveyard);
    assert_eq!(mana(&state, P0, ManaColor::W), 0);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    // Night Nurse can recover the land just discarded to pay its retrace.
    settle_choosing(&mut state, land);
    assert_eq!(zone(&state, land), Zone::Hand);
    assert_eq!(zone(&state, nurse), Zone::Battlefield);
    assert_eq!(zone(&state, spare), Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(nurse, Zone::Graveyard),
    );
    assert!(!castable(&mut state, nurse), "retrace still needs mana");
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, nurse, &[]);
    settle_choosing(&mut state, land);
    assert_eq!(
        zone(&state, nurse),
        Zone::Battlefield,
        "retrace can be repeated"
    );
}

#[test]
fn six_retrace_respects_types_controller_turn_timing_and_lost_abilities() {
    let mut state = plains();
    let six = cast_six(&mut state);
    let creature = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let flash = put(&mut state, P0, NURSE, Zone::Graveyard);
    let artifact = put(&mut state, P0, "Lotus Petal", Zone::Graveyard);
    let enchantment = put(&mut state, P0, "Exploration", Zone::Graveyard);
    let walker = put(&mut state, P0, "Wrenn and Six", Zone::Graveyard);
    let instant = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let sorcery = put(&mut state, P0, "Ponder", Zone::Graveyard);
    let land = put(&mut state, P0, "Plains", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Lotus Petal", Zone::Graveyard);
    put(&mut state, P0, "Plains", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::W, ManaColor::R, ManaColor::U],
        5,
    );
    for card in [creature, flash, artifact, enchantment, walker] {
        assert!(castable(&mut state, card));
    }
    for card in [instant, sorcery, land, theirs] {
        assert!(!castable(&mut state, card));
    }
    let dress = put(&mut state, P1, "Dress Down", Zone::Battlefield);
    assert!(!castable(&mut state, creature));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(dress, Zone::Graveyard),
    );
    assert!(castable(&mut state, creature));
    pass_until(&mut state, |s| s.step == Step::BeginCombat);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::W], 2);
    assert!(
        !castable(&mut state, creature),
        "normal timing still applies"
    );
    assert!(castable(&mut state, flash), "flash works during own combat");
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    act(&mut state, Action::Pass);
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(
        !castable(&mut state, flash),
        "no retrace on another player's turn"
    );
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(castable(&mut state, flash));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(six, Zone::Hand));
    assert!(
        !castable(&mut state, flash),
        "permission ends with its source"
    );
}

#[test]
fn six_reach_blocks_flying() {
    let mut state = plains();
    state.active_player = P1;
    state.priority_player = P1;
    let six = put(&mut state, P1, SIX, Zone::Hand);
    pool(&mut state, P1, &[ManaColor::G], 2);
    cast(&mut state, six, &[]);
    settled(&mut state);
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.step == Step::Main1
    });
    let flyer = put(
        &mut state,
        P0,
        "Sphinx of False Conclusions",
        Zone::Battlefield,
    );
    let ground = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![flyer]));
    run_until(&mut state, |s| s.step == Step::DeclareBlockers, |_, _| None);
    match next(&mut state) {
        Decision::DeclareBlockers { legal_blockers, .. } => {
            let blockers = &legal_blockers
                .iter()
                .find(|(id, _)| *id == flyer)
                .unwrap()
                .1;
            assert!(blockers.contains(&six));
            assert!(!blockers.contains(&ground));
        }
        other => panic!("{other:?}"),
    }
    act(&mut state, Action::DeclareBlockers(vec![(six, flyer)]));
    pass_until(&mut state, |s| s.step == Step::Main2);
    assert_eq!(life(&state, P1), 20);
    assert_eq!(zone(&state, flyer), Zone::Graveyard);
    assert_eq!(zone(&state, six), Zone::Graveyard);
}

#[test]
fn six_retraces_all_supported_nonland_permanent_types() {
    for (name, colors, generic) in [
        ("Llanowar Elves", vec![ManaColor::G], 0),
        ("Lotus Petal", vec![], 0),
        ("Exploration", vec![ManaColor::G], 0),
        ("Wrenn and Six", vec![ManaColor::R, ManaColor::G], 0),
    ] {
        let mut state = plains();
        cast_six(&mut state);
        let card = put(&mut state, P0, name, Zone::Graveyard);
        let land = put(&mut state, P0, "Plains", Zone::Hand);
        pool(&mut state, P0, &colors, generic);
        cast(&mut state, card, &[]);
        settle_choosing(&mut state, land);
        assert_eq!(zone(&state, card), Zone::Battlefield, "{name}");
        assert_eq!(zone(&state, land), Zone::Graveyard, "{name}");
    }
}

#[test]
fn six_cast_remains_legal_after_source_leaves_and_a_countered_card_can_be_retraced_again() {
    for remove_source in [false, true] {
        let mut state = plains();
        let six = cast_six(&mut state);
        let card = put(&mut state, P0, "Lotus Petal", Zone::Graveyard);
        let land = put(&mut state, P0, "Plains", Zone::Hand);
        cast(&mut state, card, &[]);
        assert!(matches!(
            next(&mut state),
            Decision::ChooseCostTargets { .. }
        ));
        act(&mut state, Action::ChooseCostTarget(land));
        on_the_stack(&mut state);
        if remove_source {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(six, Zone::Exile));
            let mut replay = state.clone();
            settled(&mut state);
            settled(&mut replay);
            assert_eq!(state, replay);
            assert_eq!(zone(&state, card), Zone::Battlefield);
        } else {
            act(&mut state, Action::Pass);
            let counter = put(&mut state, P1, "Counterspell", Zone::Hand);
            pool(&mut state, P1, &[ManaColor::U; 2], 0);
            cast(&mut state, counter, &[Target::Object(card)]);
            settled(&mut state);
            assert_eq!(zone(&state, card), Zone::Graveyard);
            assert!(
                !castable(&mut state, card),
                "the old discard cannot pay a new cast"
            );
            let second_land = put(&mut state, P0, "Forest", Zone::Hand);
            cast(&mut state, card, &[]);
            settle_choosing(&mut state, second_land);
            assert_eq!(zone(&state, card), Zone::Battlefield);
            assert_eq!(zone(&state, second_land), Zone::Graveyard);
        }
    }
}

#[test]
fn six_attack_survives_source_leaving_and_replays_its_optional_choice() {
    let mut state = game(&["Plains", "Lightning Bolt", "Island", "Forest"]);
    let land = state.players[0].library[0];
    let six = cast_six(&mut state);
    state.objects.get_mut(six).summoning_sick = false;
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![six]));
    on_the_stack(&mut state);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(six, Zone::Exile));
    loop {
        let choice = settle(&mut state).expect("mill order or optional land selection");
        if matches!(
            choice,
            Decision::ChooseEffectTargets {
                can_finish: true,
                ..
            }
        ) {
            break;
        }
        act(&mut state, first_answer(choice));
    }
    let mut replay = state.clone();
    for game in [&mut state, &mut replay] {
        act(game, Action::ChooseEffectTarget(Target::Object(land)));
        settled(game);
        assert_eq!(zone(game, land), Zone::Hand);
        assert_eq!(game.players[0].library.len(), 1);
    }
    assert_eq!(state, replay);
}

#[test]
fn six_grant_belongs_to_its_controller_in_either_seat() {
    for player in [P0, P1] {
        let mut state = plains();
        state.active_player = player;
        state.priority_player = player;
        let six = put(&mut state, player, SIX, Zone::Hand);
        pool(&mut state, player, &[ManaColor::G], 2);
        cast(&mut state, six, &[]);
        settled(&mut state);
        let card = put(&mut state, player, "Lotus Petal", Zone::Graveyard);
        let land = put(&mut state, player, "Plains", Zone::Hand);
        let opponent_card = put(
            &mut state,
            player.opponent(),
            "Lotus Petal",
            Zone::Graveyard,
        );
        cast(&mut state, card, &[]);
        settle_choosing(&mut state, land);
        assert_eq!(zone(&state, card), Zone::Battlefield);
        assert_eq!(zone(&state, land), Zone::Graveyard);
        assert_eq!(zone(&state, opponent_card), Zone::Graveyard);
    }
}

#[test]
fn six_retrace_can_pay_a_mana_only_alternative_cost_in_addition_to_discarding_land() {
    for evoke in [false, true] {
        let mut state = plains();
        cast_six(&mut state);
        let card = put(&mut state, P0, "Mulldrifter", Zone::Graveyard);
        let land = put(&mut state, P0, "Plains", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U], if evoke { 2 } else { 4 });
        cast(&mut state, card, &[]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseCastMode { options, .. } => {
                let mode = if evoke {
                    engine::CastMode::Alternative
                } else {
                    engine::CastMode::Normal
                };
                assert!(options.contains(&mode));
                Action::ChooseCastMode(mode)
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                assert_eq!(candidates, vec![land]);
                Action::ChooseCostTarget(land)
            }
            other => first_answer(other),
        });
        assert_eq!(zone(&state, land), Zone::Graveyard);
        assert_eq!(
            zone(&state, card),
            if evoke {
                Zone::Graveyard
            } else {
                Zone::Battlefield
            }
        );
        assert_eq!(state.players[0].hand.len(), 2);
        assert_eq!(state.players[0].library.len(), 18);
    }
}

#[test]
fn nurse_ignores_replaced_graveyard_entries_and_works_in_either_seat() {
    for player in [P0, P1] {
        let mut state = plains();
        state.active_player = player;
        state.priority_player = player;
        let nurse = put(&mut state, player, NURSE, Zone::Hand);
        let leyline = put(
            &mut state,
            player.opponent(),
            "Leyline of the Void",
            Zone::Battlefield,
        );
        let replaced = put(&mut state, player, "Plains", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(replaced, Zone::Graveyard),
        );
        assert_eq!(zone(&state, replaced), Zone::Exile);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(leyline, Zone::Exile));
        let actual = put(&mut state, player, "Island", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(actual, Zone::Graveyard),
        );
        pool(&mut state, player, &[ManaColor::W], 1);
        cast(&mut state, nurse, &[]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert_eq!(legal_targets, vec![Target::Object(actual)]);
                Action::ChooseTarget(Target::Object(actual))
            }
            other => first_answer(other),
        });
        assert_eq!(zone(&state, actual), Zone::Hand);
        assert_eq!(zone(&state, replaced), Zone::Exile);
    }
}

const ELEMENTAL: &str = "1/1 red Elemental Token";

fn on_the_stack(state: &mut GameState) {
    for _ in 0..20 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => return,
            other => act(state, first_answer(other)),
        }
    }
    panic!("pending action did not finish");
}

#[test]
fn seasoned_characteristics_and_graveyard_costs() {
    for (name, color, subtype) in [
        ("Seasoned Cryomancer", ManaColor::U, Subtype::Wizard),
        ("Seasoned Pyromancer", ManaColor::R, Subtype::Shaman),
    ] {
        let card = def(name);
        assert_eq!(card.cost.generic, 1);
        assert_eq!(card.cost.pips, &[Pip::Colored(color), Pip::Colored(color)]);
        assert_eq!(card.mana_value, 3);
        assert_eq!(card.types, &[CardType::Creature]);
        assert_eq!(card.subtypes, &[Subtype::Human, subtype]);
        assert_eq!(card.colors, &[color]);
        assert!(card.supertypes.is_empty());
        assert_eq!((card.power, card.toughness), (Some(2), Some(2)));
        assert_eq!(card.keywords, Keywords::NONE);
    }
    let token_def = def(ELEMENTAL);
    assert_eq!(token_def.types, &[CardType::Creature]);
    assert_eq!(token_def.subtypes, &[Subtype::Elemental]);
    assert_eq!(token_def.colors, &[ManaColor::R]);
    assert_eq!((token_def.power, token_def.toughness), (Some(1), Some(1)));
    assert_eq!(token_def.keywords, Keywords::NONE);
}

#[test]
fn pyromancer_discards_before_drawing_and_counts_only_nonlands() {
    for nonlands in 0..=2 {
        let mut state = plains();
        let source = put(&mut state, P0, "Seasoned Pyromancer", Zone::Hand);
        let discarded: Vec<_> = (0..2)
            .map(|index| {
                put(
                    &mut state,
                    P0,
                    if index < nonlands {
                        "Lightning Bolt"
                    } else {
                        "Island"
                    },
                    Zone::Hand,
                )
            })
            .collect();
        pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 1);
        cast(&mut state, source, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, source), Zone::Battlefield);
        assert_eq!(state.players[0].hand.len(), 2);
        assert_eq!(state.players[0].library.len(), 18);
        assert!(state.players[0]
            .hand
            .iter()
            .all(|&card| state.objects.get(card).card_def == id("Plains")));
        assert!(discarded
            .iter()
            .all(|&card| zone(&state, card) == Zone::Graveyard));
        let tokens = named(&state, P0, ELEMENTAL);
        assert_eq!(tokens.len(), nonlands);
        for object in tokens {
            assert_eq!(power_toughness(&state, object), (1, 1));
            assert!(state.objects.get(object).v4.is_token);
            assert!(state.objects.get(object).summoning_sick);
        }
        assert!(state
            .eternal_v1
            .as_ref()
            .is_none_or(|eternal| eternal.c6b.discarded_nonlands.is_empty()));
    }
}

#[test]
fn pyromancer_empty_or_short_hand_still_draws_two() {
    for hand in [vec![], vec!["Island"], vec!["Lightning Bolt"]] {
        let mut state = plains();
        let source = put(&mut state, P0, "Seasoned Pyromancer", Zone::Hand);
        for name in &hand {
            put(&mut state, P0, name, Zone::Hand);
        }
        pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 1);
        cast(&mut state, source, &[]);
        settled(&mut state);
        assert_eq!(state.players[0].hand.len(), 2);
        assert_eq!(state.players[0].graveyard.len(), hand.len());
        assert_eq!(
            named(&state, P0, ELEMENTAL).len(),
            usize::from(hand.contains(&"Lightning Bolt"))
        );
    }
}

#[test]
fn pyromancer_controller_selects_exactly_two_from_original_hand() {
    let mut state = plains();
    let source = put(&mut state, P0, "Seasoned Pyromancer", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let island = put(&mut state, P0, "Island", Zone::Hand);
    let keep = put(&mut state, P0, "Mountain", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 1);
    cast(&mut state, source, &[]);
    let choice = settle(&mut state).expect("choose discards before drawing");
    assert!(matches!(
        choice,
        Decision::ChooseEffectTargets {
            player: P0,
            can_finish: false,
            ..
        }
    ));
    assert_eq!(state.players[0].library.len(), 20);
    act(&mut state, Action::ChooseEffectTarget(Target::Object(bolt)));
    choose_target(&mut state, Target::Object(island));
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(zone(&state, island), Zone::Graveyard);
    assert!(hand_contains(&state, P0, keep));
    assert_eq!(state.players[0].hand.len(), 3);
    assert_eq!(named(&state, P0, ELEMENTAL).len(), 1);
}

#[test]
fn cryomancer_draws_first_then_targets_up_to_the_nonland_discard_count() {
    for count in 0..=2 {
        let library = match count {
            0 => ["Plains", "Island", "Plains", "Plains"],
            1 => ["Lightning Bolt", "Island", "Plains", "Plains"],
            _ => ["Lightning Bolt", "Llanowar Elves", "Plains", "Plains"],
        };
        let mut state = game(&library);
        let source = put(&mut state, P0, "Seasoned Cryomancer", Zone::Hand);
        let own = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
        let enemy = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
        let protected = put(&mut state, P1, "Slippery Bogle", Zone::Battlefield);
        pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
        cast(&mut state, source, &[]);
        if count == 0 {
            settled(&mut state);
        } else {
            let choice = settle(&mut state).expect("reflexive target choice");
            assert!(matches!(choice, Decision::ChooseTargets {
                player: P0, remaining, can_finish: true, ref legal_targets, ..
            } if usize::from(remaining) == count
                && legal_targets.contains(&Target::Object(own))
                && legal_targets.contains(&Target::Object(source))
                && legal_targets.contains(&Target::Object(enemy))
                && !legal_targets.contains(&Target::Object(protected))));
            assert_eq!(state.players[0].hand.len(), 0);
            assert_eq!(state.players[0].graveyard.len(), 2);
            act(&mut state, Action::ChooseTarget(Target::Object(own)));
            if count == 2 {
                match next(&mut state) {
                    Decision::ChooseTargets { legal_targets, .. } => {
                        assert!(!legal_targets.contains(&Target::Object(own)));
                        assert!(legal_targets.contains(&Target::Object(enemy)));
                    }
                    other => panic!("second target: {other:?}"),
                }
                act(&mut state, Action::ChooseTarget(Target::Object(enemy)));
            }
            on_the_stack(&mut state);
            assert_eq!(state.stack.len(), 1, "separate reflexive trigger");
            assert!(
                !state.objects.get(own).tapped,
                "response window before tapping"
            );
            settled(&mut state);
        }
        assert_eq!(state.players[0].library.len(), 2);
        assert_eq!(state.objects.get(own).tapped, count > 0);
        assert_eq!(state.objects.get(own).counters.stun, i16::from(count > 0));
        assert_eq!(state.objects.get(enemy).tapped, count == 2);
        assert_eq!(
            state.objects.get(enemy).counters.stun,
            i16::from(count == 2)
        );
    }
}

#[test]
fn cryomancer_can_choose_zero_or_fewer_targets_and_stun_prevents_next_untap() {
    for chosen in 0..=1 {
        let mut state = game(&[
            "Lightning Bolt",
            "Llanowar Elves",
            "Plains",
            "Plains",
            "Plains",
            "Plains",
        ]);
        let source = put(&mut state, P0, "Seasoned Cryomancer", Zone::Hand);
        let victim = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
        state.objects.get_mut(victim).tapped = true;
        pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
        cast(&mut state, source, &[]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseTargets {
                can_finish: true,
                ..
            })
        ));
        if chosen == 1 {
            act(&mut state, Action::ChooseTarget(Target::Object(victim)));
            assert!(matches!(
                next(&mut state),
                Decision::ChooseTargets {
                    can_finish: true,
                    ..
                }
            ));
        }
        act(&mut state, Action::FinishEffectSelection);
        settled(&mut state);
        assert_eq!(state.objects.get(victim).counters.stun, chosen);
        pass_until(&mut state, |s| {
            s.active_player == P1 && s.step == Step::Main1
        });
        assert_eq!(state.objects.get(victim).tapped, chosen == 1);
        assert_eq!(state.objects.get(victim).counters.stun, 0);
    }
}

#[test]
fn cryomancer_chooses_discards_from_cards_just_drawn() {
    let mut state = game(&["Lightning Bolt", "Llanowar Elves", "Plains", "Plains"]);
    let drawn = state.players[0].library[..2].to_vec();
    let source = put(&mut state, P0, "Seasoned Cryomancer", Zone::Hand);
    let keep = put(&mut state, P0, "Mountain", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    cast(&mut state, source, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { player: P0, .. })
    ));
    assert_eq!(state.players[0].library.len(), 2);
    for card in &drawn {
        choose_target(&mut state, Target::Object(*card));
    }
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { remaining: 2, .. })
    ));
    act(&mut state, Action::FinishEffectSelection);
    settled(&mut state);
    assert_eq!(state.players[0].hand, vec![keep]);
    assert!(drawn
        .iter()
        .all(|&card| zone(&state, card) == Zone::Graveyard));
}

#[test]
fn cryomancer_reflexive_trigger_rechecks_vanished_and_changed_incarnations() {
    for remove_both in [false, true] {
        let mut state = game(&["Lightning Bolt", "Llanowar Elves", "Plains", "Plains"]);
        let source = put(&mut state, P0, "Seasoned Cryomancer", Zone::Hand);
        let first = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
        let second = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
        pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
        cast(&mut state, source, &[]);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseTargets { .. })
        ));
        act(&mut state, Action::ChooseTarget(Target::Object(first)));
        choose_target(&mut state, Target::Object(second));
        on_the_stack(&mut state);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(first, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(first, Zone::Battlefield),
        );
        if remove_both {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(second, Zone::Graveyard),
            );
        }
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        settled(&mut state);
        assert!(!state.objects.get(first).tapped);
        assert_eq!(state.objects.get(first).counters.stun, 0);
        assert_eq!(
            state.objects.get(second).counters.stun,
            i16::from(!remove_both)
        );
        assert_eq!(state.objects.get(second).tapped, !remove_both);
    }
}

#[test]
fn seasoned_graveyard_abilities_exile_as_cost_work_at_instant_speed_and_only_in_graveyard() {
    for (name, color) in [
        ("Seasoned Cryomancer", ManaColor::U),
        ("Seasoned Pyromancer", ManaColor::R),
    ] {
        for wrong_zone in [Zone::Hand, Zone::Battlefield, Zone::Exile] {
            let mut state = plains();
            let card = put(&mut state, P0, name, wrong_zone);
            pool(&mut state, P0, &[color, color], 3);
            assert!(!activatable(&mut state, card, 0));
        }
        let mut wrong_colors = plains();
        let unpaid = put(&mut wrong_colors, P0, name, Zone::Graveyard);
        pool(&mut wrong_colors, P0, &[color], 4);
        assert!(
            !activatable(&mut wrong_colors, unpaid, 0),
            "two colored pips required"
        );
        let mut state = plains();
        state.active_player = P1;
        state.priority_player = P0;
        let card = put(&mut state, P0, name, Zone::Graveyard);
        pool(&mut state, P0, &[color, color], 2);
        assert!(!activatable(&mut state, card, 0), "five mana required");
        pool(&mut state, P0, &[], 1);
        activate(&mut state, card, 0, &[]);
        on_the_stack(&mut state);
        assert_eq!(zone(&state, card), Zone::Exile);
        assert_eq!(mana(&state, P0, color), 0);
        assert_eq!(mana(&state, P0, ManaColor::C), 0);
        assert!(state.players[0].hand.is_empty());
        assert!(named(&state, P0, ELEMENTAL).is_empty());
        settled(&mut state);
        if color == ManaColor::U {
            assert_eq!(state.players[0].hand.len(), 2);
            assert_eq!(state.players[0].library.len(), 18);
        } else {
            assert_eq!(named(&state, P0, ELEMENTAL).len(), 2);
            assert_eq!(state.players[0].library.len(), 20);
        }
        pool(&mut state, P0, &[color, color], 3);
        assert!(
            !activatable(&mut state, card, 0),
            "cannot reuse exiled source"
        );
    }
}

#[test]
fn seasoned_draws_offer_competing_replacements_and_preserve_discard_counts() {
    for (name, color, graveyard) in [
        ("Seasoned Cryomancer", ManaColor::U, false),
        ("Seasoned Pyromancer", ManaColor::R, false),
        ("Seasoned Cryomancer", ManaColor::U, true),
    ] {
        for win in [false, true] {
            let mut state = game(&[]);
            put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
            let jace = put(&mut state, P0, "Jace, Wielder of Mysteries", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(jace, Zone::Battlefield),
            );
            let source = put(
                &mut state,
                P0,
                name,
                if graveyard {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                },
            );
            let discarded = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
            pool(
                &mut state,
                P0,
                &[color, color],
                if graveyard { 3 } else { 1 },
            );
            if graveyard {
                activate(&mut state, source, 0, &[]);
            } else {
                cast(&mut state, source, &[]);
            }
            let choice = settle(&mut state).expect("Jace competes with Hullbreacher");
            assert!(
                matches!(choice, Decision::ChooseEffectTargets { player: P0, can_finish: true, ref legal_targets, .. }
                if legal_targets.as_slice() == [Target::Object(jace)])
            );
            assert!(
                named(&state, P0, ELEMENTAL).is_empty(),
                "tokens follow draw"
            );
            assert_eq!(
                zone(&state, discarded),
                if color == ManaColor::R {
                    Zone::Graveyard
                } else {
                    Zone::Hand
                }
            );
            // A suspended draw must preserve Pyromancer's count and the
            // exact unchanged root frames through a state round trip.
            let snapshot = serde_json::to_string(&state).expect("serialize draw choice");
            state = serde_json::from_str(&snapshot).expect("restore draw choice");
            if win {
                act(&mut state, Action::ChooseEffectTarget(Target::Object(jace)));
                assert!(matches!(
                    next(&mut state),
                    Decision::GameOver { winner: Some(P0) }
                ));
            } else {
                let mut replacements = 0;
                settle_with(&mut state, |_, decision| {
                    if matches!(decision, Decision::ChooseEffectTargets { .. }) {
                        replacements += 1;
                        Action::FinishEffectSelection
                    } else {
                        first_answer(decision)
                    }
                });
                assert_eq!(replacements, 2);
                assert_eq!(named(&state, P1, "Treasure Token").len(), 2);
                assert_eq!(
                    named(&state, P0, ELEMENTAL).len(),
                    usize::from(color == ManaColor::R)
                );
                assert_eq!(
                    zone(&state, discarded),
                    if graveyard {
                        Zone::Hand
                    } else {
                        Zone::Graveyard
                    }
                );
            }
        }
    }
}

#[test]
fn seasoned_count_discards_replaced_by_exile() {
    for (name, color) in [
        ("Seasoned Cryomancer", ManaColor::U),
        ("Seasoned Pyromancer", ManaColor::R),
    ] {
        let mut state = plains();
        put(&mut state, P1, "Rest in Peace", Zone::Battlefield);
        let source = put(&mut state, P0, name, Zone::Hand);
        let discard = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        pool(&mut state, P0, &[color, color], 1);
        cast(&mut state, source, &[]);
        let mut stun_targets = 0;
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(discard)) =>
            {
                Action::ChooseEffectTarget(Target::Object(discard))
            }
            Decision::ChooseTargets { remaining, .. } => {
                stun_targets = remaining;
                Action::ChooseTarget(Target::Object(source))
            }
            other => first_answer(other),
        });
        assert_eq!(zone(&state, discard), Zone::Exile);
        if color == ManaColor::U {
            assert_eq!(stun_targets, 1);
            assert!(state.objects.get(source).tapped);
            assert_eq!(state.objects.get(source).counters.stun, 1);
        } else {
            assert_eq!(named(&state, P0, ELEMENTAL).len(), 1);
        }
    }
}

#[test]
fn cryomancer_with_replaced_draws_and_empty_hand_has_no_reflexive_trigger() {
    let mut state = plains();
    put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
    let source = put(&mut state, P0, "Seasoned Cryomancer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    cast(&mut state, source, &[]);
    settled(&mut state);
    assert!(state.players[0].hand.is_empty());
    assert!(state.players[0].graveyard.is_empty());
    assert_eq!(state.players[0].library.len(), 20);
    assert_eq!(named(&state, P1, "Treasure Token").len(), 2);
    assert!(!state.objects.get(source).tapped);
    assert_eq!(state.objects.get(source).counters.stun, 0);
}

#[test]
fn seasoned_entry_effect_survives_source_leaving_before_resolution() {
    for (name, color) in [
        ("Seasoned Cryomancer", ManaColor::U),
        ("Seasoned Pyromancer", ManaColor::R),
    ] {
        let mut state = plains();
        let source = put(&mut state, P0, name, Zone::Hand);
        let discard = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        let creature = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
        pool(&mut state, P0, &[color, color], 1);
        cast(&mut state, source, &[]);
        on_the_stack(&mut state);
        // Resolve the creature spell, then stop at the ETB's response window.
        run_until(
            &mut state,
            |s| zone(s, source) == Zone::Battlefield && !s.stack.is_empty(),
            |_, _| None,
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(discard)) =>
            {
                Action::ChooseEffectTarget(Target::Object(discard))
            }
            Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(creature)),
            other => first_answer(other),
        });
        assert_eq!(zone(&state, discard), Zone::Graveyard);
        assert_eq!(state.players[0].library.len(), 18);
        if color == ManaColor::U {
            assert_eq!(state.objects.get(creature).counters.stun, 1);
            assert!(state.objects.get(creature).tapped);
            assert_eq!(state.players[0].hand.len(), 1);
        } else {
            assert_eq!(named(&state, P0, ELEMENTAL).len(), 1);
            assert_eq!(state.players[0].hand.len(), 2);
        }
    }
}

const SPHINX: &str = "Sphinx of False Conclusions";

fn cast_sphinx(state: &mut GameState) -> ObjectId {
    let source = put(state, P0, SPHINX, Zone::Hand);
    pool(state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(state, source, &[]);
    settled(state);
    source
}

#[test]
fn sphinx_characteristics_and_flash_on_opponents_turn() {
    let card = def(SPHINX);
    assert_eq!(card.cost.generic, 2);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::U); 2]);
    assert_eq!(card.mana_value, 4);
    assert_eq!(card.types, &[CardType::Creature]);
    assert_eq!(card.subtypes, &[Subtype::Sphinx, Subtype::Illusion]);
    assert_eq!(card.colors, &[ManaColor::U]);
    assert!(card.supertypes.is_empty());
    assert_eq!((card.power, card.toughness), (Some(4), Some(2)));
    assert_eq!(card.keywords, Keywords::FLASH | Keywords::FLYING);

    let mut state = plains();
    state.active_player = P1;
    let source = put(&mut state, P0, SPHINX, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    assert!(!castable(&mut state, source), "two blue pips required");
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, source, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, source), Zone::Battlefield);
    assert_eq!(power_toughness(&state, source), (4, 2));
    assert!(keyword(&state, source, Keywords::FLASH));
    assert!(keyword(&state, source, Keywords::FLYING));
    assert!(state.players[0].hand.is_empty(), "no entry loot");
    assert_eq!(state.players[0].library.len(), 20);
}

#[test]
fn sphinx_attacks_draw_then_mandatory_discard_and_no_attack_means_no_loot() {
    for attacks in [false, true] {
        let mut state = plains();
        let source = cast_sphinx(&mut state);
        let keep = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        state.objects.get_mut(source).summoning_sick = false;
        let drawn = state.players[0].library[0];
        let mut discards = 0;
        attack_with(
            &mut state,
            if attacks { vec![source] } else { Vec::new() },
            |s, decision| {
                if let Decision::Discard {
                    player,
                    count,
                    choices,
                } = decision
                {
                    assert_eq!(*player, P0);
                    assert_eq!(*count, 1);
                    assert!(choices.contains(&drawn), "draw precedes discard");
                    assert!(choices.contains(&keep));
                    assert_eq!(s.players[0].library.len(), 19);
                    discards += 1;
                    Some(Action::Discard(vec![drawn]))
                } else {
                    None
                }
            },
        );
        assert_eq!(discards, usize::from(attacks));
        assert_eq!(state.players[0].hand, vec![keep]);
        assert_eq!(
            state.players[0].library.len(),
            if attacks { 19 } else { 20 }
        );
        assert_eq!(
            zone(&state, drawn),
            if attacks {
                Zone::Graveyard
            } else {
                Zone::Library
            }
        );
        assert_eq!(life(&state, P1), if attacks { 16 } else { 20 });
    }
}

#[test]
fn sphinx_attack_loot_survives_source_leaving_and_draws_with_empty_hand() {
    let mut state = plains();
    let source = cast_sphinx(&mut state);
    state.objects.get_mut(source).summoning_sick = false;
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![source]));
    on_the_stack(&mut state);
    assert_eq!(state.stack.len(), 1);
    let drawn = state.players[0].library[0];
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
    settle_first(&mut state);
    assert_eq!(zone(&state, drawn), Zone::Graveyard);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(state.players[0].library.len(), 19);
    assert!(named(&state, P0, SPHINX).is_empty());
}

#[test]
fn sphinx_flying_excludes_ground_blockers() {
    let mut state = plains();
    let source = cast_sphinx(&mut state);
    state.objects.get_mut(source).summoning_sick = false;
    let ground = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let flyer = put(&mut state, P1, "Birds of Paradise", Zone::Battlefield);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![source]));
    settle_first(&mut state);
    for _ in 0..40 {
        match next(&mut state) {
            Decision::DeclareBlockers { legal_blockers, .. } => {
                let blockers = &legal_blockers
                    .iter()
                    .find(|(object, _)| *object == source)
                    .unwrap()
                    .1;
                assert!(!blockers.contains(&ground));
                assert!(blockers.contains(&flyer));
                return;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => act(&mut state, first_answer(other)),
        }
    }
    panic!("no blockers decision");
}

#[test]
fn sphinx_dies_copy_is_four_two_without_counters_loots_and_does_not_recurse() {
    let mut state = plains();
    let source = cast_sphinx(&mut state);
    state.objects.get_mut(source).counters.plus1_plus1 = 1;
    opponent_bolts(&mut state, Target::Object(source));
    settled(&mut state);
    assert_eq!(zone(&state, source), Zone::Graveyard);
    let copies = named(&state, P0, SPHINX);
    assert_eq!(copies.len(), 1);
    let copy = copies[0];
    assert!(state.objects.get(copy).v4.is_token);
    assert_eq!(power_toughness(&state, copy), (4, 2));
    assert_eq!(state.objects.get(copy).counters.plus1_plus1, 0);
    assert!(keyword(&state, copy, Keywords::FLASH));
    assert!(keyword(&state, copy, Keywords::FLYING));
    assert_eq!(state.players[0].library.len(), 20, "no entry loot");
    state.objects.get_mut(copy).summoning_sick = false;
    attack_with(&mut state, vec![copy], |_, _| None);
    assert_eq!(state.players[0].library.len(), 19);
    assert!(state.players[0].hand.is_empty());
    assert_eq!(life(&state, P1), 16);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(copy, Zone::Graveyard),
    );
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.is_empty(), "token death creates no trigger");
    assert!(state.engine.pending_triggers.is_empty());
    assert!(named(&state, P0, SPHINX).is_empty());
}

#[test]
fn sphinx_only_copies_on_death_and_respects_replaced_death() {
    for destination in [Zone::Hand, Zone::Exile, Zone::Graveyard] {
        for replaced in [false, true] {
            let mut state = plains();
            let source = cast_sphinx(&mut state);
            if replaced {
                put(&mut state, P1, "Rest in Peace", Zone::Battlefield);
            }
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, destination));
            settled(&mut state);
            assert_eq!(
                named(&state, P0, SPHINX).len(),
                usize::from(destination == Zone::Graveyard && !replaced)
            );
            if destination == Zone::Graveyard && replaced {
                assert_eq!(zone(&state, source), Zone::Exile);
            }
        }
    }
}

#[test]
fn sphinx_death_uses_departure_lki_after_source_changes_zone_again_and_replays() {
    let mut state = plains();
    let source = cast_sphinx(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    on_the_stack(&mut state);
    assert_eq!(state.stack.len(), 1);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
    let snapshot = serde_json::to_string(&state).expect("serialize death trigger");
    let mut replay: GameState = serde_json::from_str(&snapshot).expect("restore death trigger");
    settled(&mut state);
    settled(&mut replay);
    assert_eq!(state, replay);
    assert_eq!(zone(&state, source), Zone::Hand);
    let copies = named(&state, P0, SPHINX);
    assert_eq!(copies.len(), 1);
    assert!(state.objects.get(copies[0]).v4.is_token);
}

#[test]
fn sphinx_death_copy_respects_token_doubling() {
    let mut state = plains();
    put(&mut state, P0, "Doubling Season", Zone::Battlefield);
    let source = cast_sphinx(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(named(&state, P0, SPHINX).len(), 2);
}

#[test]
fn sphinx_replaced_draw_still_discards_as_many_as_possible() {
    for has_card in [false, true] {
        let mut state = plains();
        put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
        let source = cast_sphinx(&mut state);
        let discard = has_card.then(|| put(&mut state, P0, "Lightning Bolt", Zone::Hand));
        state.objects.get_mut(source).summoning_sick = false;
        attack_with(&mut state, vec![source], |_, _| None);
        assert_eq!(state.players[0].library.len(), 20);
        assert!(state.players[0].hand.is_empty());
        assert_eq!(named(&state, P1, "Treasure Token").len(), 1);
        if let Some(discard) = discard {
            assert_eq!(zone(&state, discard), Zone::Graveyard);
        }
    }
}

#[test]
fn sphinx_death_copies_a_copy_using_its_departed_definition() {
    let mut state = plains();
    let source = cast_sphinx(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Graveyard),
    );
    settled(&mut state);
    let first_copy = named(&state, P0, SPHINX)[0];
    let woodland = put(&mut state, P0, "Shifting Woodland", Zone::Battlefield);
    for name in ["Lightning Bolt", "Mind Stone", "Island"] {
        put(&mut state, P0, name, Zone::Graveyard);
    }
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 2);
    activate(&mut state, woodland, 0, &[Target::Object(source)]);
    settled(&mut state);
    assert_eq!(state.objects.get(woodland).card_def, id(SPHINX));
    assert!(!state.objects.get(woodland).v4.is_token);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(woodland, Zone::Graveyard),
    );
    assert_eq!(
        state.objects.get(woodland).card_def,
        id("Shifting Woodland")
    );
    settled(&mut state);
    let copies = named(&state, P0, SPHINX);
    assert_eq!(copies.len(), 2);
    let second_copy = *copies.iter().find(|&&object| object != first_copy).unwrap();
    assert!(state.objects.get(second_copy).v4.is_token);
    assert_eq!(power_toughness(&state, second_copy), (4, 2));
    assert!(keyword(&state, second_copy, Keywords::FLYING));
    assert!(!engine::object_has_type(
        &state,
        second_copy,
        CardType::Land
    ));
}

#[test]
fn sphinx_loot_offers_competing_draw_replacements_and_stops_after_a_win() {
    for win in [false, true] {
        let mut state = game(&[]);
        put(&mut state, P1, "Hullbreacher", Zone::Battlefield);
        let jace = put(&mut state, P0, "Jace, Wielder of Mysteries", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(jace, Zone::Battlefield),
        );
        let source = cast_sphinx(&mut state);
        let discard = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
        state.objects.get_mut(source).summoning_sick = false;
        pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
        act(&mut state, Action::DeclareAttackers(vec![source]));
        let choice = settle(&mut state).expect("competing draw replacements");
        assert!(matches!(choice, Decision::ChooseEffectTargets {
            player: P0, can_finish: true, ref legal_targets, ..
        } if legal_targets.as_slice() == [Target::Object(jace)]));
        assert_eq!(zone(&state, discard), Zone::Hand, "discard follows draw");
        if win {
            act(&mut state, Action::ChooseEffectTarget(Target::Object(jace)));
            assert!(matches!(
                next(&mut state),
                Decision::GameOver { winner: Some(P0) }
            ));
            assert_eq!(zone(&state, discard), Zone::Hand);
        } else {
            act(&mut state, Action::FinishEffectSelection);
            settle_first(&mut state);
            assert_eq!(zone(&state, discard), Zone::Graveyard);
            assert_eq!(named(&state, P1, "Treasure Token").len(), 1);
        }
    }
}

const FANATIC: &str = "Metamorphosis Fanatic";

fn cast_fanatic(state: &mut GameState) -> ObjectId {
    let source = put(state, P0, FANATIC, Zone::Hand);
    pool(state, P0, &[ManaColor::B, ManaColor::B], 4);
    cast(state, source, &[]);
    on_the_stack(state);
    source
}

#[test]
fn fanatic_characteristics_hard_cast_cost_and_lifelink_combat() {
    let card = def(FANATIC);
    assert_eq!(card.cost.generic, 4);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::B); 2]);
    assert_eq!(card.mana_value, 6);
    assert_eq!(card.types, &[CardType::Creature]);
    assert_eq!(card.subtypes, &[Subtype::Human, Subtype::Cleric]);
    assert_eq!(card.colors, &[ManaColor::B]);
    assert!(card.supertypes.is_empty());
    assert_eq!((card.power, card.toughness), (Some(4), Some(4)));
    assert_eq!(card.keywords, Keywords::LIFELINK);
    assert!(!card.is_token);

    let mut state = plains();
    let source = put(&mut state, P0, FANATIC, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    assert!(
        !castable(&mut state, source),
        "miracle is not a hand discount"
    );
    pool(&mut state, P0, &[], 4);
    assert!(
        !castable(&mut state, source),
        "hard cast needs two black pips"
    );
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, source, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, source), Zone::Battlefield);
    assert_eq!(power_toughness(&state, source), (4, 4));
    assert!(keyword(&state, source, Keywords::LIFELINK));
    assert_eq!(state.objects.get(source).v4.lifelink_keyword_counters, 0);
    state.objects.get_mut(source).summoning_sick = false;
    attack_with(&mut state, vec![source], |_, _| None);
    assert_eq!(life(&state, P0), 24);
    assert_eq!(life(&state, P1), 16);
}

#[test]
fn fanatic_reanimates_only_own_creature_with_lifelink_counter_or_chooses_zero() {
    for choose in [false, true] {
        let mut state = plains();
        let creature = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
        put(&mut state, P0, "Plains", Zone::Graveyard);
        put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
        put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
        let source = cast_fanatic(&mut state);
        let mut offered = 0;
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseTargets {
                legal_targets,
                can_finish,
                ..
            } => {
                offered += 1;
                assert_eq!(legal_targets, vec![Target::Object(creature)]);
                assert!(can_finish);
                if choose {
                    Action::ChooseTarget(Target::Object(creature))
                } else {
                    Action::FinishEffectSelection
                }
            }
            other => first_answer(other),
        });
        assert_eq!(offered, 1);
        assert_eq!(zone(&state, source), Zone::Battlefield);
        assert_eq!(
            zone(&state, creature),
            if choose {
                Zone::Battlefield
            } else {
                Zone::Graveyard
            }
        );
        assert_eq!(
            state.objects.get(creature).v4.lifelink_keyword_counters,
            i16::from(choose)
        );
        if choose {
            assert!(keyword(&state, creature, Keywords::LIFELINK));
            assert!(!state.objects.get(creature).tapped);
            assert!(state.objects.get(creature).summoning_sick);
            state.objects.get_mut(creature).summoning_sick = false;
            attack_with(&mut state, vec![creature], |_, _| None);
            assert_eq!(life(&state, P0), 25);
            assert_eq!(life(&state, P1), 15);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(creature, Zone::Hand));
            assert_eq!(state.objects.get(creature).v4.lifelink_keyword_counters, 0);
        }
    }
}

#[test]
fn fanatic_target_leaving_or_blinking_fizzles_without_counter_or_return() {
    for reenter_graveyard in [false, true] {
        let mut state = plains();
        let creature = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
        cast_fanatic(&mut state);
        let choice = settle(&mut state).expect("entry target");
        assert!(matches!(choice, Decision::ChooseTargets { .. }));
        act(&mut state, Action::ChooseTarget(Target::Object(creature)));
        on_the_stack(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Exile),
        );
        if reenter_graveyard {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Graveyard),
            );
        }
        settled(&mut state);
        assert_eq!(
            zone(&state, creature),
            if reenter_graveyard {
                Zone::Graveyard
            } else {
                Zone::Exile
            }
        );
        assert_eq!(state.objects.get(creature).v4.lifelink_keyword_counters, 0);
    }
}

#[test]
fn fanatic_return_survives_source_leaving_doubles_counters_and_replays() {
    let mut state = plains();
    put(&mut state, P0, "Doubling Season", Zone::Battlefield);
    let creature = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    let source = cast_fanatic(&mut state);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseTargets { .. })
    ));
    act(&mut state, Action::ChooseTarget(Target::Object(creature)));
    on_the_stack(&mut state);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Exile));
    let snapshot = serde_json::to_string(&state).expect("serialize reanimation");
    let mut replay: GameState = serde_json::from_str(&snapshot).expect("restore reanimation");
    settled(&mut state);
    settled(&mut replay);
    assert_eq!(state, replay);
    assert_eq!(zone(&state, creature), Zone::Battlefield);
    assert_eq!(state.objects.get(creature).v4.lifelink_keyword_counters, 2);
    assert!(keyword(&state, creature, Keywords::LIFELINK));
}

#[test]
fn fanatic_grafdiggers_cage_stops_return_and_counter() {
    let mut state = plains();
    put(&mut state, P1, "Grafdigger's Cage", Zone::Battlefield);
    let creature = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    cast_fanatic(&mut state);
    settle_choosing(&mut state, creature);
    assert_eq!(zone(&state, creature), Zone::Graveyard);
    assert_eq!(state.objects.get(creature).v4.lifelink_keyword_counters, 0);
}

/// S1l's first-draw setup: collect the external draw before the reveal prompt.
fn fanatic_first_draw(state: &mut GameState, player: PlayerId) -> ObjectId {
    let card = put(state, player, FANATIC, Zone::Library);
    event::propose_and_commit(state, ProposedEvent::draw(player));
    assert!(hand_contains(state, player, card));
    next(state);
    card
}

#[test]
fn fanatic_miracle_casts_for_one_black_on_either_turn_both_seats_then_targets() {
    for owner in [P0, P1] {
        for active in [P0, P1] {
            let mut state = plains();
            state.active_player = active;
            state.priority_player = owner;
            let creature = put(&mut state, owner, "Gurmag Angler", Zone::Graveyard);
            pool(&mut state, owner, &[ManaColor::B], 1);
            let card = fanatic_first_draw(&mut state, owner);
            assert!(
                matches!(next(&mut state), Decision::ChooseEffectOption { player, source, option_count: 2 } if player == owner && source == card)
            );
            act(&mut state, Action::ChooseEffectOption(1));
            let mut paid_choice = 0;
            let mut targets = 0;
            settle_with(&mut state, |s, decision| match decision {
                Decision::ChooseEffectOption {
                    player,
                    option_count: 2,
                    ..
                } if player == owner => {
                    paid_choice += 1;
                    Action::ChooseEffectOption(0)
                }
                Decision::ChooseEffectTargets { legal_targets, .. }
                    if legal_targets == vec![Target::Object(card)] =>
                {
                    Action::ChooseEffectTarget(Target::Object(card))
                }
                Decision::ChooseTargets { legal_targets, .. } => {
                    assert_eq!(
                        zone(s, card),
                        Zone::Battlefield,
                        "targeting follows creature entry"
                    );
                    assert_eq!(legal_targets, vec![Target::Object(creature)]);
                    targets += 1;
                    Action::ChooseTarget(Target::Object(creature))
                }
                other => first_answer(other),
            });
            assert_eq!(paid_choice, 1);
            assert_eq!(targets, 1);
            assert_eq!(zone(&state, card), Zone::Battlefield);
            assert_eq!(zone(&state, creature), Zone::Battlefield);
            assert_eq!(state.objects.get(creature).v4.lifelink_keyword_counters, 1);
            assert_eq!(mana(&state, owner, ManaColor::B), 0);
            assert_eq!(mana(&state, owner, ManaColor::C), 0);
        }
    }
}

#[test]
fn fanatic_miracle_reveal_and_cast_can_be_declined_and_needs_payable_cost() {
    // Reveal decline; cast decline; no generic; no black.
    for (reveal, accept, black, generic) in [
        (false, false, 1, 1),
        (true, false, 1, 1),
        (true, true, 1, 0),
        (true, true, 0, 2),
    ] {
        let mut state = plains();
        pool(&mut state, P0, &vec![ManaColor::B; black], generic);
        let card = fanatic_first_draw(&mut state, P0);
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption { .. }
        ));
        act(&mut state, Action::ChooseEffectOption(u16::from(reveal)));
        let mut offered = false;
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption {
                option_count: 2, ..
            } => {
                offered = true;
                assert!(!accept, "an unpayable cast must not be offered");
                Action::ChooseEffectOption(1)
            }
            other => first_answer(other),
        });
        assert_eq!(offered, reveal && !accept);
        assert_eq!(zone(&state, card), Zone::Hand);
        assert_eq!(mana(&state, P0, ManaColor::B), u8::try_from(black).unwrap());
        assert_eq!(mana(&state, P0, ManaColor::C), generic);
    }
}

#[test]
fn fanatic_second_draw_has_no_miracle_and_real_turn_change_resets_first_draw() {
    let mut state = plains();
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    next(&mut state);
    let second = put(&mut state, P0, FANATIC, Zone::Library);
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    settled(&mut state);
    assert!(hand_contains(&state, P0, second));
    assert!(state.engine.pending_triggers.is_empty());
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    let first = fanatic_first_draw(&mut state, P0);
    assert!(
        matches!(next(&mut state), Decision::ChooseEffectOption { source, .. } if source == first)
    );
    act(&mut state, Action::ChooseEffectOption(0));
    settled(&mut state);
}

#[test]
fn fanatic_miracle_cannot_cast_a_replaced_hand_incarnation() {
    for back_to_hand in [false, true] {
        let mut state = plains();
        pool(&mut state, P0, &[ManaColor::B], 1);
        let card = fanatic_first_draw(&mut state, P0);
        assert!(matches!(
            next(&mut state),
            Decision::ChooseEffectOption { .. }
        ));
        act(&mut state, Action::ChooseEffectOption(1));
        on_the_stack(&mut state);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Exile));
        if back_to_hand {
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Hand));
        }
        settled(&mut state);
        assert_eq!(
            zone(&state, card),
            if back_to_hand {
                Zone::Hand
            } else {
                Zone::Exile
            }
        );
        assert_eq!(mana(&state, P0, ManaColor::B), 1);
        assert_eq!(mana(&state, P0, ManaColor::C), 1);
    }
}
