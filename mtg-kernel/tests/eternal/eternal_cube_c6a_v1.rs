//! Vintage Cube wave C6a: native rules and public-session tests for the
//! discard, exile and impulse-draw trigger creatures.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C6A_CARDS: [&str; 1] = ["Ivora, Insatiable Heir"];

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
        0x4355_4245_0000 + 0x3661,
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

// ---- Ivora ---------------------------------------------------------------------

#[test]
fn c6a_cards_are_full_with_printed_characteristics() {
    let card = def(C6A_CARDS[0]);
    assert_eq!(card.capability, CardCapability::Full);
    assert!(card.is_executable());
    assert_eq!(card.cost.generic, 1);
    assert_eq!(card.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(card.cost.x_count, 0);
    assert_eq!(card.types, &[CardType::Creature]);
    assert_eq!(card.supertypes, &[Supertype::Legendary]);
    assert_eq!(card.subtypes, &[Subtype::Vampire, Subtype::Warrior]);
    assert_eq!((card.power, card.toughness), (Some(1), Some(1)));
    assert!(card.keywords.has(Keywords::TRAMPLE));
}

fn cast_ivora(state: &mut GameState, player: PlayerId) -> ObjectId {
    state.priority_player = player;
    let creature = put(state, player, C6A_CARDS[0], Zone::Hand);
    pool(state, player, &[ManaColor::R], 1);
    cast(state, creature, &[]);
    settled(state);
    assert_eq!(zone(state, creature), Zone::Battlefield);
    creature
}

fn on_the_stack(state: &mut GameState) {
    loop {
        match next(state) {
            Decision::CastSpellOrPass { .. } => return,
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()));
            }
            other => act(state, first_answer(other)),
        }
    }
}

fn cycle(state: &mut GameState, player: PlayerId) {
    state.priority_player = player;
    let hollow = put(state, player, "Hollow One", Zone::Hand);
    pool(state, player, &[], 2);
    activate(state, hollow, 0, &[]);
    settled(state);
    assert_eq!(zone(state, hollow), Zone::Graveyard);
}

fn hymn(state: &mut GameState, target: PlayerId) {
    state.priority_player = state.active_player;
    let caster = state.active_player;
    let spell = put(state, caster, "Hymn to Tourach", Zone::Hand);
    pool(state, caster, &[ManaColor::B, ManaColor::B], 0);
    cast(state, spell, &[Target::Player(target)]);
    settled(state);
}

#[test]
fn ivora_creates_blood_on_entry_and_combat_damage_and_blood_discards_before_drawing() {
    let mut state = plains();
    let ivora = cast_ivora(&mut state, P0);
    let blood = named(&state, P0, "Blood Token")[0];
    assert_eq!(named(&state, P0, "Blood Token").len(), 1);
    let discard = put(&mut state, P0, "Mountain", Zone::Hand);
    assert!(!activatable(&mut state, blood, 0), "requires one mana");
    pool(&mut state, P0, &[], 1);
    let drawn = state.players[0].library[0];
    activate(&mut state, blood, 0, &[]);
    on_the_stack(&mut state);
    assert_ne!(zone(&state, blood), Zone::Battlefield);
    assert_eq!(zone(&state, discard), Zone::Graveyard);
    assert_eq!(zone(&state, drawn), Zone::Library, "draw has not resolved");
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    settled(&mut state);
    assert_eq!(zone(&state, drawn), Zone::Hand);
    assert_eq!(power_toughness(&state, ivora), (2, 2));
    state.objects.get_mut(ivora).summoning_sick = false;
    attack_with(&mut state, vec![ivora], |_, _| None);
    assert_eq!(named(&state, P0, "Blood Token").len(), 1);
    assert_eq!(life(&state, P1), 18);
    let turn = state.turn;
    pass_until(&mut state, |s| {
        s.active_player == P0 && s.turn != turn && s.step == Step::Main1
    });
    attack_with(&mut state, vec![ivora], |_, _| None);
    assert_eq!(named(&state, P0, "Blood Token").len(), 2);
    assert_eq!(life(&state, P1), 16);
}

#[test]
fn ivora_blood_cannot_activate_without_a_discardable_card_or_when_tapped() {
    let mut state = plains();
    cast_ivora(&mut state, P0);
    let blood = named(&state, P0, "Blood Token")[0];
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, blood, 0), "empty hand");
    put(&mut state, P0, "Mountain", Zone::Hand);
    event::propose_and_commit(&mut state, ProposedEvent::tap(blood));
    settled(&mut state);
    assert!(!activatable(&mut state, blood, 0), "tapped Blood");
}

#[test]
fn ivora_tramples_over_a_blocker_and_does_not_make_blood_for_only_creature_damage() {
    for growing in [false, true] {
        let mut state = plains();
        let ivora = cast_ivora(&mut state, P0);
        let blocker = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
        if growing {
            put(&mut state, P0, "Mountain", Zone::Hand);
            put(&mut state, P0, "Plains", Zone::Hand);
            hymn(&mut state, P0);
        }
        state.objects.get_mut(ivora).summoning_sick = false;
        attack_with(&mut state, vec![ivora], |_, decision| match decision {
            Decision::DeclareBlockers { .. } => {
                Some(Action::DeclareBlockers(vec![(blocker, ivora)]))
            }
            Decision::ChooseCombatDamageRange { .. } => {
                Some(Action::ChooseCombatDamageRange { upper_half: false })
            }
            _ => None,
        });
        assert_eq!(
            named(&state, P0, "Blood Token").len(),
            if growing { 2 } else { 1 }
        );
        assert_eq!(life(&state, P1), if growing { 18 } else { 20 });
        assert_eq!(zone(&state, blocker), Zone::Graveyard);
    }
}

#[test]
fn ivora_ignores_noncombat_player_damage_and_another_creatures_combat_damage() {
    let mut state = plains();
    let ivora = cast_ivora(&mut state, P0);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::damage(ivora, Target::Player(P1), 1),
    );
    settled(&mut state);
    assert_eq!(named(&state, P0, "Blood Token").len(), 1);
    assert_eq!(life(&state, P1), 19);
    let bear = put(&mut state, P0, "Grizzly Bears", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, bear, &[]);
    settled(&mut state);
    assert_eq!(
        named(&state, P0, "Blood Token").len(),
        1,
        "another creature entering"
    );
    state.objects.get_mut(bear).summoning_sick = false;
    attack_with(&mut state, vec![bear], |_, _| None);
    assert_eq!(named(&state, P0, "Blood Token").len(), 1);
    assert_eq!(life(&state, P1), 17);
}

#[test]
fn ivora_counts_each_discarded_card_but_not_an_empty_hand_or_opponents_discard() {
    for player in [P0, P1] {
        let mut state = plains();
        state.active_player = player;
        state.priority_player = player;
        let ivora = cast_ivora(&mut state, player);
        let cards = [
            put(&mut state, player, "Mountain", Zone::Hand),
            put(&mut state, player, "Plains", Zone::Hand),
        ];
        hymn(&mut state, player);
        assert!(cards
            .into_iter()
            .all(|card| zone(&state, card) == Zone::Graveyard));
        assert_eq!(power_toughness(&state, ivora), (3, 3));
        hymn(&mut state, player);
        assert_eq!(
            power_toughness(&state, ivora),
            (3, 3),
            "empty hand is no discard"
        );
        put(&mut state, player.opponent(), "Mountain", Zone::Hand);
        put(&mut state, player.opponent(), "Plains", Zone::Hand);
        hymn(&mut state, player.opponent());
        assert_eq!(
            power_toughness(&state, ivora),
            (3, 3),
            "opponent's discard is ignored"
        );
    }
}

#[test]
fn ivora_counts_discard_self_costs_for_its_current_controller_only() {
    let mut state = plains();
    let ivora = cast_ivora(&mut state, P0);
    cycle(&mut state, P0);
    assert_eq!(power_toughness(&state, ivora), (2, 2));
    cycle(&mut state, P1);
    assert_eq!(power_toughness(&state, ivora), (2, 2));
}

#[test]
fn ivora_counter_does_not_follow_a_source_that_left_and_optionally_returned() {
    for returns in [false, true] {
        let mut state = plains();
        let ivora = cast_ivora(&mut state, P0);
        let hollow = put(&mut state, P0, "Hollow One", Zone::Hand);
        pool(&mut state, P0, &[], 2);
        activate(&mut state, hollow, 0, &[]);
        on_the_stack(&mut state);
        assert_eq!(zone(&state, hollow), Zone::Graveyard);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(ivora, Zone::Hand));
        if returns {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(ivora, Zone::Battlefield),
            );
        }
        settled(&mut state);
        assert_eq!(state.objects.get(ivora).counters.plus1_plus1, 0);
        assert_eq!(
            named(&state, P0, "Blood Token").len(),
            if returns { 2 } else { 1 }
        );
    }
}

#[test]
fn ivora_triggers_once_for_a_madness_discard_even_though_the_card_goes_to_exile() {
    let mut state = plains();
    let ivora = cast_ivora(&mut state, P0);
    let blood = named(&state, P0, "Blood Token")[0];
    let temper = put(&mut state, P0, "Fiery Temper", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, blood, 0, &[]);
    let mut madness = 0;
    settle_with(&mut state, |state, decision| match decision {
        Decision::ChooseMadnessCast { card, .. } => {
            assert_eq!(card, temper);
            assert_eq!(zone(state, temper), Zone::Exile);
            madness += 1;
            Action::ChooseMadnessCast(false)
        }
        other => first_answer(other),
    });
    assert_eq!(madness, 1);
    assert_eq!(zone(&state, temper), Zone::Graveyard);
    assert_eq!(power_toughness(&state, ivora), (2, 2));
}

#[test]
fn ivora_counts_each_cleanup_discard_and_ignores_other_hand_zone_changes() {
    let mut state = plains();
    let ivora = cast_ivora(&mut state, P0);
    let moved = put(&mut state, P0, "Mountain", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(moved, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(power_toughness(&state, ivora), (1, 1));
    for _ in 0..9 {
        put(&mut state, P0, "Mountain", Zone::Hand);
    }
    pass_until(&mut state, |s| {
        s.active_player == P1 && s.step == Step::Main1
    });
    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(power_toughness(&state, ivora), (3, 3));
}
