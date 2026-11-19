//! Vintage Cube wave C6b: native rules and public-session tests for the
//! discard counts, dies copies and graveyard plays.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C6B_CARDS: [&str; 2] = ["Seasoned Cryomancer", "Seasoned Pyromancer"];

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
