//! Vintage Cube wave C9b: native rules and public-session tests for the
//! Figure of Destiny, Figure of Fable, Hexdrinker, Nadu, Serra Paragon, and Swift Reconfiguration.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code, unused_imports)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec,
    CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C9B_CARDS: [&str; 6] = [
    "Figure of Destiny",
    "Figure of Fable",
    "Hexdrinker",
    "Nadu, Winged Wisdom",
    "Serra Paragon",
    "Swift Reconfiguration",
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
        0x4355_4245_0000 + 0x3962,
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
fn c9b_cards_are_full() {
    for name in C9B_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
    assert!(def("Nadu, Winged Wisdom").keywords.has(Keywords::FLYING));
    assert!(def("Nadu, Winged Wisdom")
        .supertypes
        .contains(&Supertype::Legendary));
    assert!(def("Serra Paragon").keywords.has(Keywords::FLYING));
    assert!(def("Swift Reconfiguration").keywords.has(Keywords::FLASH));
    assert_eq!(def("Figure of Destiny").activated_abilities.len(), 3);
    assert_eq!(def("Figure of Fable").activated_abilities.len(), 3);
    assert_eq!(def("Hexdrinker").activated_abilities.len(), 1);
}

#[test]
fn c9b_empty_state_omits_serde_and_hashes_zero_bytes() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let plain = plains();
    assert!(plain.eternal_v1.is_none());

    let hash_of = |state: &GameState| {
        let mut h = DefaultHasher::new();
        state.hash(&mut h);
        h.finish()
    };

    let mut with_empty_eternal = plain.clone();
    with_empty_eternal.eternal_v1 = Some(Default::default());
    let json = serde_json::to_value(&with_empty_eternal).expect("serialize state");
    assert!(
        json["eternal_v1"].get("c9b").is_none(),
        "empty C9bStateV1 must skip serialization: {}",
        json["eternal_v1"]
    );

    // An empty C9bStateV1 hashes zero bytes on its own.
    let empty_c9b = mtg_kernel::eternal_cube_c9b_v1::C9bStateV1::default();
    assert!(empty_c9b.is_empty());
    let mut h_empty = DefaultHasher::new();
    let h_raw = DefaultHasher::new();
    empty_c9b.hash(&mut h_empty);
    assert_eq!(h_empty.finish(), h_raw.finish());

    let before = hash_of(&plain);
    assert_eq!(before, hash_of(&plain));
}

fn land_drops(state: &mut GameState) -> Vec<ObjectId> {
    match next(state) {
        Decision::CastSpellOrPass { land_drops, .. } => land_drops,
        other => panic!("expected priority, got {other:?}"),
    }
}

fn crew_with(state: &mut GameState, vehicle: ObjectId, ability_index: u8, crew: &[ObjectId]) {
    assert!(
        activatable(state, vehicle, ability_index),
        "{} crew ability {ability_index} not activatable",
        state.objects.get(vehicle).name
    );
    act(state, Action::ActivateAbility(vehicle, ability_index));
    for &member in crew {
        match next(state) {
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                assert!(
                    legal_targets.contains(&Target::Object(member)),
                    "{member:?} not in crew candidates {legal_targets:?}"
                );
                act(state, Action::ChooseEffectTarget(Target::Object(member)));
            }
            other => panic!("expected crew member choice, got {other:?}"),
        }
    }
    match next(state) {
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => act(state, Action::FinishEffectSelection),
        other => panic!("expected finishable crew prompt, got {other:?}"),
    }
}

fn pass_to_player_main1(state: &mut GameState, player: PlayerId) {
    let start_turn = state.turn;
    let start_active = state.active_player;
    pass_until(state, |s| {
        s.active_player == player
            && s.step == Step::Main1
            && (s.turn != start_turn || s.active_player != start_active)
            && s.stack.is_empty()
            && s.engine.pending_triggers.is_empty()
    });
}

// ---- Figure of Destiny ---------------------------------------------------------

#[test]
fn figure_of_destiny_hybrid_tiers_resolution_check_counters_and_layer6_retention() {
    let mut state = plains();
    let fig = put(&mut state, P0, "Figure of Destiny", Zone::Hand);
    // Cast with {R} hybrid mana.
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, fig, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, fig), Zone::Battlefield);
    assert_eq!(power_toughness(&state, fig), (1, 1));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Kithkin));
    assert!(!engine::has_effective_subtype(&state, fig, Subtype::Spirit));

    // Tier 2 ("If this creature is a Spirit") CAN be activated while not a Spirit,
    // and resolves as a no-op because the check happens at resolution.
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::R, ManaColor::W],
        0,
    );
    activate(&mut state, fig, 1, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, fig), (1, 1));
    assert!(!engine::has_effective_subtype(
        &state,
        fig,
        Subtype::Warrior
    ));

    // Stack tier 2 first, then respond with tier 1: tier 1 resolves first (makes it
    // a 2/2 Spirit), then tier 2 resolves and sees Spirit -> 4/4 Kithkin Spirit Warrior.
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::R, ManaColor::W, ManaColor::W],
        0,
    );
    activate(&mut state, fig, 1, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    activate(&mut state, fig, 0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, fig), (4, 4));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Kithkin));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Spirit));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Warrior));
    assert!(!engine::has_effective_subtype(&state, fig, Subtype::Avatar));

    // Add a +1/+1 counter before tier 3: Layer 7b sets base 8/8, counter adds on top -> 9/9.
    state.objects.get_mut(fig).counters.plus1_plus1 = 1;
    assert_eq!(power_toughness(&state, fig), (5, 5));

    pool(
        &mut state,
        P0,
        &[
            ManaColor::R,
            ManaColor::W,
            ManaColor::R,
            ManaColor::W,
            ManaColor::R,
            ManaColor::W,
        ],
        0,
    );
    activate(&mut state, fig, 2, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, fig), (9, 9));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Kithkin));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Spirit));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Warrior));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Avatar));
    assert!(keyword(&state, fig, Keywords::FLYING));
    assert!(keyword(&state, fig, Keywords::FIRST_STRIKE));

    // Activating tier 1 AFTER tier 3 resets base P/T to 2/2 (+1/+1 counter => 3/3)
    // and subtypes to Kithkin Spirit, while KEEPING flying and first strike (CR 613.1f).
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, fig, 0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, fig), (3, 3));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Kithkin));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Spirit));
    assert!(!engine::has_effective_subtype(
        &state,
        fig,
        Subtype::Warrior
    ));
    assert!(!engine::has_effective_subtype(&state, fig, Subtype::Avatar));
    assert!(keyword(&state, fig, Keywords::FLYING));
    assert!(keyword(&state, fig, Keywords::FIRST_STRIKE));
}

#[test]
fn figure_of_destiny_dress_down_timestamps_and_noncreature_subtype_preservation() {
    // 1. Dress Down entering AFTER tier 3 removes Flying and First Strike while
    //    keeping 8/8 base P/T and creature subtypes.
    let mut state = plains();
    let fig = put(&mut state, P0, "Figure of Destiny", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, fig, 0, &[]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::R; 3], 0);
    activate(&mut state, fig, 1, &[]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::W; 6], 0);
    activate(&mut state, fig, 2, &[]);
    settled(&mut state);
    assert!(keyword(&state, fig, Keywords::FLYING));
    assert!(keyword(&state, fig, Keywords::FIRST_STRIKE));

    let dd = put(&mut state, P1, "Dress Down", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::U], 1);
    cast(&mut state, dd, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, fig), (8, 8));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Avatar));
    assert!(!keyword(&state, fig, Keywords::FLYING));
    assert!(!keyword(&state, fig, Keywords::FIRST_STRIKE));
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!activatable(&mut state, fig, 0));

    // Next turn after Dress Down sacrifices at end step: Flying and First Strike return.
    pass_to_player_main1(&mut state, P1);
    assert!(keyword(&state, fig, Keywords::FLYING));
    assert!(keyword(&state, fig, Keywords::FIRST_STRIKE));

    // 2. Activate tier 3, opponent flashes Dress Down in response: tier 3 resolves
    //    with a LATER Layer 6 timestamp than Dress Down -> keeps Flying and First Strike!
    let mut state = plains();
    let fig = put(&mut state, P0, "Figure of Destiny", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, fig, 0, &[]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::R; 3], 0);
    activate(&mut state, fig, 1, &[]);
    settled(&mut state);

    pool(&mut state, P0, &[ManaColor::W; 6], 0);
    activate(&mut state, fig, 2, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let dd = put(&mut state, P1, "Dress Down", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 1);
    cast(&mut state, dd, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, dd), Zone::Battlefield);
    assert_eq!(power_toughness(&state, fig), (8, 8));
    assert!(
        keyword(&state, fig, Keywords::FLYING),
        "later tier 3 grant beats earlier Dress Down"
    );
    assert!(keyword(&state, fig, Keywords::FIRST_STRIKE));

    // 3. Preserves non-creature subtype (Vehicle) when crewed and activated later.
    let mut state = plains();
    let fig = put(&mut state, P0, "Figure of Destiny", Zone::Battlefield);
    let helper = put(&mut state, P0, "Questing Beast", Zone::Battlefield);
    let reconfig = put(&mut state, P0, "Swift Reconfiguration", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, reconfig, &[Target::Object(fig)]);
    settled(&mut state);
    assert!(!engine::object_has_type(&state, fig, CardType::Creature));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Vehicle));
    // Questing Beast has power 4; add Elvish Mystic (power 1) to meet Crew 5.
    let elf = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    crew_with(&mut state, fig, 253, &[helper, elf]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, fig, CardType::Creature));
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, fig, 0, &[]);
    settled(&mut state);
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Vehicle));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Kithkin));
    assert!(engine::has_effective_subtype(&state, fig, Subtype::Spirit));
    assert_eq!(power_toughness(&state, fig), (2, 2));
}

// ---- Figure of Fable -----------------------------------------------------------

#[test]
fn figure_of_fable_tiers_replace_subtypes_and_protection_from_opponents_debt() {
    let mut state = plains();
    let fable = put(&mut state, P0, "Figure of Fable", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, fable, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, fable), (1, 1));
    assert!(engine::has_effective_subtype(
        &state,
        fable,
        Subtype::Kithkin
    ));

    // Tier 1: Kithkin Scout 2/3.
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, fable, 0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, fable), (2, 3));
    assert!(engine::has_effective_subtype(
        &state,
        fable,
        Subtype::Kithkin
    ));
    assert!(engine::has_effective_subtype(&state, fable, Subtype::Scout));

    // Tier 2: Kithkin Soldier 4/5 (replaces Scout!).
    pool(&mut state, P0, &[ManaColor::G, ManaColor::W], 1);
    activate(&mut state, fable, 1, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, fable), (4, 5));
    assert!(engine::has_effective_subtype(
        &state,
        fable,
        Subtype::Kithkin
    ));
    assert!(engine::has_effective_subtype(
        &state,
        fable,
        Subtype::Soldier
    ));
    assert!(!engine::has_effective_subtype(
        &state,
        fable,
        Subtype::Scout
    ));

    // Attach controller's own Rancor, then on P1's turn attach P1's Rancor,
    // then return to P0's turn for tier 3.
    let own_rancor = put(&mut state, P0, "Rancor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, own_rancor, &[Target::Object(fable)]);
    settled(&mut state);
    assert!(state.objects.get(fable).attachments.contains(&own_rancor));

    pass_to_player_main1(&mut state, P1);
    let opp_rancor = put(&mut state, P1, "Rancor", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::G], 0);
    cast(&mut state, opp_rancor, &[Target::Object(fable)]);
    settled(&mut state);
    assert!(state.objects.get(fable).attachments.contains(&opp_rancor));
    pass_to_player_main1(&mut state, P0);

    // Tier 3: Kithkin Avatar 7/8 (+2/+0 from own Rancor => 9/8) + protection from
    // each of your opponents! Opponent's Rancor falls off to graveyard in SBA while
    // controller's own Rancor stays attached.
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::W, ManaColor::G],
        3,
    );
    activate(&mut state, fable, 2, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, fable), (9, 8));
    assert!(engine::has_effective_subtype(
        &state,
        fable,
        Subtype::Kithkin
    ));
    assert!(engine::has_effective_subtype(
        &state,
        fable,
        Subtype::Avatar
    ));
    assert!(!engine::has_effective_subtype(
        &state,
        fable,
        Subtype::Soldier
    ));
    assert!(
        state.objects.get(fable).attachments.contains(&own_rancor),
        "controller's own Aura stays attached"
    );
    assert!(
        !state.objects.get(fable).attachments.contains(&opp_rancor),
        "opponent's Aura is removed by protection from opponents"
    );

    // Targeting: opponent's Lightning Bolt & Walking Ballista CANNOT target fable;
    // controller's own Lightning Bolt CAN target fable.
    let own_bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(cast_targets(&state, own_bolt).contains(&Target::Object(fable)));

    let opp_bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let opp_ballista = put(&mut state, P1, "Walking Ballista", Zone::Battlefield);
    state.objects.get_mut(opp_ballista).counters.plus1_plus1 = 1;
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(!cast_targets(&state, opp_bolt).contains(&Target::Object(fable)));
    assert!(activatable(&mut state, opp_ballista, 1));
    act(&mut state, Action::ActivateAbility(opp_ballista, 1));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                !legal_targets.contains(&Target::Object(fable)),
                "opponent's ability cannot target Figure of Fable"
            );
            act(&mut state, Action::ChooseTarget(Target::Player(P0)));
        }
        other => panic!("expected target choice, got {other:?}"),
    }
    settled(&mut state);

    // Blocking: opponent's creature CANNOT block Figure of Fable.
    let _opp_blocker = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    assert!(
        legal_blockers(&mut state, fable).is_empty(),
        "opponent's creatures cannot block Figure of Fable"
    );
}

#[test]
fn figure_of_fable_dynamic_controller_and_unpreventable_damage() {
    let mut state = plains();
    let fable = put(&mut state, P0, "Figure of Fable", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    activate(&mut state, fable, 0, &[]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::W], 1);
    activate(&mut state, fable, 1, &[]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::G; 3], 3);
    activate(&mut state, fable, 2, &[]);
    settled(&mut state);

    // Untargeted opponent damage (e.g. opponent's damage event) is prevented,
    // UNLESS Flaring Pain makes damage unpreventable!
    let opp_bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let prevented =
        engine::damage_is_prevented_by_protection(&state, opp_bolt, Target::Object(fable));
    assert!(prevented, "damage from opponent's source is prevented");

    // Cast Flaring Pain from P1, then check damage from opponent's source actually lands.
    let flaring = put(&mut state, P1, "Flaring Pain", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 1);
    cast(&mut state, flaring, &[]);
    settled(&mut state);
    // Opponent's Questing Beast combat damage (and Flaring Pain) bypasses prevention.
    let opp_source = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    mtg_kernel::event::propose_and_commit(
        &mut state,
        mtg_kernel::event::ProposedEvent::damage(opp_source, Target::Object(fable), 3),
    );
    assert_eq!(
        state.objects.get(fable).damage,
        3,
        "Flaring Pain overrides protection damage prevention"
    );

    // Dynamic relative to current controller: move control of fable to P1.
    // Now P1 CAN target fable, and P0 CANNOT target fable!
    state.players[P0.index()]
        .battlefield
        .retain(|&o| o != fable);
    state.players[P1.index()].battlefield.push(fable);
    state.objects.get_mut(fable).controller = P1;

    state.priority_player = P1;
    let p1_bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(
        cast_targets(&state, p1_bolt).contains(&Target::Object(fable)),
        "new controller P1 can now target Figure of Fable"
    );
    state.priority_player = P0;
    let p0_bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(
        !cast_targets(&state, p0_bolt).contains(&Target::Object(fable)),
        "former controller P0 is now an opponent and cannot target Figure of Fable"
    );
}

// ---- Hexdrinker ----------------------------------------------------------------

#[test]
fn hexdrinker_level_up_sorcery_speed_instants_and_everything() {
    let mut state = plains();
    let hex = put(&mut state, P0, "Hexdrinker", Zone::Battlefield);
    assert_eq!(power_toughness(&state, hex), (2, 1));
    assert!(!keyword(&state, hex, Keywords::PROTECTION_FROM_EVERYTHING));

    // Level up {1} is sorcery-speed only: NOT activatable on opponent's turn or
    // with a spell on the stack.
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, hex, 0));
    let _bolt = opponent_bolts(&mut state, Target::Player(P0));
    assert!(
        !activatable(&mut state, hex, 0),
        "Level up cannot be activated while stack is non-empty"
    );
    settled(&mut state);

    // Level up 3 times -> LEVEL 3-7: 4/4, Protection from instants.
    for _ in 0..3 {
        pool(&mut state, P0, &[], 1);
        activate(&mut state, hex, 0, &[]);
        settled(&mut state);
    }
    assert_eq!(power_toughness(&state, hex), (4, 4));
    assert!(!keyword(&state, hex, Keywords::PROTECTION_FROM_EVERYTHING));

    // Protection from instants blocks opponent's instant (Lightning Bolt) AND
    // controller's own instant (Ephemerate)!
    let own_eph = put(&mut state, P0, "Ephemerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(
        !castable(&mut state, own_eph),
        "own instant Ephemerate cannot target Hexdrinker at level 3"
    );

    let opp_bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(!cast_targets(&state, opp_bolt).contains(&Target::Object(hex)));

    // Sorcery (Chain Lightning) AND creature ability (Walking Ballista) CAN target
    // and damage Hexdrinker at LEVEL 3-7!
    state.priority_player = P0;
    let chain = put(&mut state, P0, "Chain Lightning", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(cast_targets(&state, chain).contains(&Target::Object(hex)));

    let ballista = put(&mut state, P1, "Walking Ballista", Zone::Battlefield);
    state.objects.get_mut(ballista).counters.plus1_plus1 = 1;
    state.priority_player = P1;
    activate(&mut state, ballista, 1, &[Target::Object(hex)]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(hex).damage,
        1,
        "activated ability deals damage through protection from instants"
    );

    // Attach Rancor and Jitte before reaching level 8.
    state.priority_player = P0;
    let rancor = put(&mut state, P0, "Rancor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, rancor, &[Target::Object(hex)]);
    settled(&mut state);
    let jitte = put(&mut state, P0, "Umezawa's Jitte", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, jitte, 3, &[Target::Object(hex)]);
    settled(&mut state);
    assert!(state.objects.get(hex).attachments.contains(&rancor));
    assert!(state.objects.get(hex).attachments.contains(&jitte));

    // Level up 5 more times -> LEVEL 8+: 6/6, Protection from everything!
    for _ in 0..5 {
        pool(&mut state, P0, &[], 1);
        activate(&mut state, hex, 0, &[]);
        settled(&mut state);
    }
    assert_eq!(power_toughness(&state, hex), (6, 6));
    assert!(keyword(&state, hex, Keywords::PROTECTION_FROM_EVERYTHING));
    // Protection from everything detaches own Equipment and removes own Aura in SBA!
    assert!(!state.objects.get(hex).attachments.contains(&jitte));
    assert!(!state.objects.get(hex).attachments.contains(&rancor));
    assert_eq!(zone(&state, jitte), Zone::Battlefield);
    // Sorcery and ability can no longer target Hexdrinker!
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!cast_targets(&state, chain).contains(&Target::Object(hex)));
    // Opponent cannot block Hexdrinker!
    let _opp_blocker = put(&mut state, P1, "Elvish Mystic", Zone::Battlefield);
    assert!(legal_blockers(&mut state, hex).is_empty());
}

#[test]
fn hexdrinker_innkeepers_talent_dress_down_and_departure_pruning() {
    // 1. Innkeeper's Talent at Level 3 doubles each Level up from 1 to 2 counters.
    let mut state = plains();
    let hex = put(&mut state, P0, "Hexdrinker", Zone::Battlefield);
    let talent = put(&mut state, P0, "Innkeeper's Talent", Zone::Battlefield);
    // Level Innkeeper's Talent to 2 ({G}) and then 3 ({3}{G}).
    pool(&mut state, P0, &[ManaColor::G], 0);
    activate(&mut state, talent, 0, &[]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::G], 3);
    activate(&mut state, talent, 1, &[]);
    settled(&mut state);

    // Two level-ups = 4 level counters -> LEVEL 3-7 (4/4)!
    pool(&mut state, P0, &[], 2);
    activate(&mut state, hex, 0, &[]);
    settled(&mut state);
    activate(&mut state, hex, 0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, hex), (4, 4));
    // Two more level-ups = 8 level counters -> LEVEL 8+ (6/6 + protection from everything)!
    pool(&mut state, P0, &[], 2);
    activate(&mut state, hex, 0, &[]);
    settled(&mut state);
    activate(&mut state, hex, 0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, hex), (6, 6));
    assert!(keyword(&state, hex, Keywords::PROTECTION_FROM_EVERYTHING));

    // 2. Dress Down suppresses level bands (reverts to 2/1, loses protection) while
    //    keeping level counters so Hexdrinker recovers after Dress Down leaves.
    //    Here Innkeeper's Talent Level 1+3 is still active on the battlefield, so
    //    during P0's combat while Dress Down removes protection from everything,
    //    Innkeeper's Talent puts 2 +1/+1 counters on Hexdrinker -> 8/8 after
    //    Dress Down leaves!
    let dd = put(&mut state, P1, "Dress Down", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::U], 1);
    cast(&mut state, dd, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, hex), (2, 1));
    assert!(!keyword(&state, hex, Keywords::PROTECTION_FROM_EVERYTHING));
    pass_to_player_main1(&mut state, P1);
    assert_eq!(state.objects.get(hex).counters.plus1_plus1, 2);
    assert_eq!(
        power_toughness(&state, hex),
        (8, 8),
        "level counters survived Dress Down (6/6) plus 2 +1/+1 counters from Talent during combat"
    );
    assert!(keyword(&state, hex, Keywords::PROTECTION_FROM_EVERYTHING));

    // And without Innkeeper's Talent, Dress Down leaves Hexdrinker at clean 6/6.
    let mut clean = plains();
    let hex_clean = put(&mut clean, P0, "Hexdrinker", Zone::Battlefield);
    for _ in 0..8 {
        pool(&mut clean, P0, &[], 1);
        activate(&mut clean, hex_clean, 0, &[]);
        settled(&mut clean);
    }
    let dd_clean = put(&mut clean, P1, "Dress Down", Zone::Hand);
    clean.priority_player = P1;
    pool(&mut clean, P1, &[ManaColor::U], 1);
    cast(&mut clean, dd_clean, &[]);
    settled(&mut clean);
    assert_eq!(power_toughness(&clean, hex_clean), (2, 1));
    assert!(!keyword(
        &clean,
        hex_clean,
        Keywords::PROTECTION_FROM_EVERYTHING
    ));
    pass_to_player_main1(&mut clean, P1);
    assert_eq!(
        power_toughness(&clean, hex_clean),
        (6, 6),
        "level counters survived Dress Down and restore LEVEL 8+ once Dress Down leaves"
    );
    assert!(keyword(
        &clean,
        hex_clean,
        Keywords::PROTECTION_FROM_EVERYTHING
    ));

    // 3. Departure prunes level counters: while Dress Down suppresses protection,
    //    Ephemerate flickers Hexdrinker -> returns at level 0 (2/1) even after
    //    Dress Down sacrifices at end step.
    let mut state = plains();
    let hex = put(&mut state, P0, "Hexdrinker", Zone::Battlefield);
    for _ in 0..8 {
        pool(&mut state, P0, &[], 1);
        activate(&mut state, hex, 0, &[]);
        settled(&mut state);
    }
    assert_eq!(power_toughness(&state, hex), (6, 6));
    let dd = put(&mut state, P1, "Dress Down", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::U], 1);
    cast(&mut state, dd, &[]);
    settled(&mut state);
    state.priority_player = P0;
    let eph = put(&mut state, P0, "Ephemerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, eph, &[Target::Object(hex)]);
    settled(&mut state);
    pass_to_player_main1(&mut state, P1);
    assert_eq!(
        power_toughness(&state, hex),
        (2, 1),
        "flickered Hexdrinker lost all level counters"
    );
    assert!(!keyword(&state, hex, Keywords::PROTECTION_FROM_EVERYTHING));
}

// ---- Nadu, Winged Wisdom -------------------------------------------------------

#[test]
fn nadu_triggers_on_any_target_reveals_land_or_hand_not_draw_and_caps_twice() {
    // Library top-first: Plains (land -> battlefield), Lightning Bolt (nonland -> hand),
    // Forest (land -> battlefield), Mountain (land -> battlefield).
    let mut state = game(&[
        "Plains",
        "Lightning Bolt",
        "Forest",
        "Mountain",
        "Island",
        "Swamp",
    ]);
    let nadu = put(&mut state, P0, "Nadu, Winged Wisdom", Zone::Battlefield);
    let elf = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let shuko = put(&mut state, P0, "Shuko", Zone::Battlefield);
    // Opponent has Orcish Bowmasters and Narset, Parter of Veils: Nadu puts
    // nonland into hand (NOT a draw!), so neither may trigger or block it.
    let _bowmasters = put(&mut state, P1, "Orcish Bowmasters", Zone::Battlefield);
    let _narset = put(&mut state, P1, "Narset, Parter of Veils", Zone::Battlefield);

    let top_land = state.players[P0.index()].library[0];
    let top_bolt = state.players[P0.index()].library[1];
    let third_land = state.players[P0.index()].library[2];
    let hand_before = state.players[P0.index()].hand.len();

    // Target 1 on elf with Shuko Equip {0} -> top card Plains enters battlefield untapped.
    activate(&mut state, shuko, 0, &[Target::Object(elf)]);
    settled(&mut state);
    assert_eq!(zone(&state, top_land), Zone::Battlefield);
    assert!(!state.objects.get(top_land).tapped);

    // Target 2 on elf with Shuko Equip {0} -> top card Lightning Bolt goes to hand
    // without triggering Bowmasters or being blocked by Narset!
    activate(&mut state, shuko, 0, &[Target::Object(elf)]);
    settled(&mut state);
    assert_eq!(zone(&state, top_bolt), Zone::Hand);
    assert_eq!(state.players[P0.index()].hand.len(), hand_before + 1);
    assert_eq!(life(&state, P0), 20, "Bowmasters did not trigger on Nadu");

    // Target 3 on same elf this turn -> cap of 2 reached, NO trigger!
    activate(&mut state, shuko, 0, &[Target::Object(elf)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, third_land),
        Zone::Library,
        "third targeting of same creature does not trigger Nadu"
    );

    // Targeting Nadu itself gets its OWN independent 2 triggers (and opponent's spell
    // triggers it too!).
    let _opp_bolt = opponent_bolts(&mut state, Target::Object(nadu));
    settled(&mut state);
    assert_eq!(
        zone(&state, third_land),
        Zone::Battlefield,
        "opponent targeting Nadu triggers Nadu's own 2/turn budget"
    );
}

#[test]
fn nadu_flicker_resets_cap_empty_library_noop_and_dress_down_suppresses() {
    let mut state = game(&["Plains"; 20]);
    let nadu = put(&mut state, P0, "Nadu, Winged Wisdom", Zone::Battlefield);
    let elf = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let shuko = put(&mut state, P0, "Shuko", Zone::Battlefield);

    // Exhaust elf's 2 triggers.
    activate(&mut state, shuko, 0, &[Target::Object(elf)]);
    settled(&mut state);
    activate(&mut state, shuko, 0, &[Target::Object(elf)]);
    settled(&mut state);
    let lib_before = state.players[P0.index()].library.len();
    activate(&mut state, shuko, 0, &[Target::Object(elf)]);
    settled(&mut state);
    assert_eq!(state.players[P0.index()].library.len(), lib_before);

    // Flicker elf with Ephemerate -> elf is a new incarnation, gets 2 fresh triggers!
    let eph1 = put(&mut state, P0, "Ephemerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, eph1, &[Target::Object(elf)]);
    settled(&mut state);
    let lib_after_flicker = state.players[P0.index()].library.len();
    activate(&mut state, shuko, 0, &[Target::Object(elf)]);
    settled(&mut state);
    assert_eq!(
        state.players[P0.index()].library.len(),
        lib_after_flicker - 1,
        "flickering creature resets its 2 triggers"
    );

    // Exhaust elf's 2nd trigger, then flicker Nadu itself -> resets elf's triggers again!
    activate(&mut state, shuko, 0, &[Target::Object(elf)]);
    settled(&mut state);
    let eph2 = put(&mut state, P0, "Ephemerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, eph2, &[Target::Object(nadu)]);
    settled(&mut state);
    let lib_after_nadu_flicker = state.players[P0.index()].library.len();
    activate(&mut state, shuko, 0, &[Target::Object(elf)]);
    settled(&mut state);
    assert_eq!(
        state.players[P0.index()].library.len(),
        lib_after_nadu_flicker - 1,
        "flickering Nadu resets every creature's triggers"
    );

    // Dress Down suppresses Nadu's grant.
    let dd = put(&mut state, P1, "Dress Down", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::U], 1);
    cast(&mut state, dd, &[]);
    settled(&mut state);
    state.priority_player = P0;
    let lib_under_dd = state.players[P0.index()].library.len();
    activate(&mut state, shuko, 0, &[Target::Object(elf)]);
    settled(&mut state);
    assert_eq!(
        state.players[P0.index()].library.len(),
        lib_under_dd,
        "Dress Down suppresses Nadu's granted trigger"
    );

    // Empty library resolves cleanly as a no-op (not a draw, so no deck-out).
    let mut state = game(&[]);
    let nadu = put(&mut state, P0, "Nadu, Winged Wisdom", Zone::Battlefield);
    let shuko = put(&mut state, P0, "Shuko", Zone::Battlefield);
    activate(&mut state, shuko, 0, &[Target::Object(nadu)]);
    settled(&mut state);
    assert!(state.players[P0.index()].library.is_empty());
    assert!(state.players[P0.index()].hand.is_empty());
}

// ---- Serra Paragon -------------------------------------------------------------

#[test]
fn serra_paragon_shared_once_per_turn_land_and_spell_and_dies_trigger() {
    let mut state = plains();
    let paragon = put(&mut state, P0, "Serra Paragon", Zone::Battlefield);
    let _exp = put(&mut state, P0, "Exploration", Zone::Battlefield);
    let waste1 = put(&mut state, P0, "Wasteland", Zone::Graveyard);
    let waste2 = put(&mut state, P0, "Wasteland", Zone::Graveyard);
    let elf = put(&mut state, P0, "Elvish Mystic", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let beast = put(&mut state, P0, "Questing Beast", Zone::Graveyard);

    // Instant (Lightning Bolt) and MV 4 permanent (Questing Beast) are NEVER castable
    // via Serra Paragon, even with plenty of mana.
    pool(
        &mut state,
        P0,
        &[ManaColor::R, ManaColor::G, ManaColor::G],
        4,
    );
    assert!(!castable(&mut state, bolt));
    assert!(!castable(&mut state, beast));
    assert!(castable(&mut state, elf));
    assert!(land_drops(&mut state).contains(&waste1));

    // Play Wasteland #1 from graveyard: consumes BOTH a land drop AND Serra Paragon's
    // single once-per-turn budget! Neither Wasteland #2 (even with Exploration) nor
    // Elvish Mystic can be played/cast via this Serra Paragon this turn.
    play_land(&mut state, waste1);
    assert_eq!(zone(&state, waste1), Zone::Battlefield);
    assert!(!land_drops(&mut state).contains(&waste2));
    assert!(!castable(&mut state, elf));

    // Destroy opponent's nonbasic land with Wasteland #1 -> Wasteland #1 goes from
    // battlefield to graveyard -> Serra Paragon's granted dies trigger exiles
    // Wasteland #1 and gains P0 2 life!
    let opp_plateau = put(&mut state, P1, "Plateau", Zone::Battlefield);
    let life_before = life(&state, P0);
    activate(&mut state, waste1, 0, &[Target::Object(opp_plateau)]);
    settled(&mut state);
    assert_eq!(zone(&state, opp_plateau), Zone::Graveyard);
    assert_eq!(
        zone(&state, waste1),
        Zone::Exile,
        "land played via Serra Paragon is exiled when put into graveyard"
    );
    assert_eq!(life(&state, P0), life_before + 2);

    // Flicker Serra Paragon on the same turn -> fresh incarnation grants a new
    // once-per-turn use! Now Elvish Mystic IS castable from graveyard!
    let eph = put(&mut state, P0, "Ephemerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, eph, &[Target::Object(paragon)]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::G], 0);
    assert!(castable(&mut state, elf));
    cast(&mut state, elf, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elf), Zone::Battlefield);

    // Destroy Serra Paragon BEFORE Elvish Mystic dies: Elvish Mystic STILL carries
    // its granted dies trigger -> exiles itself and gains 2 life when Bolted!
    let kill_paragon = put(&mut state, P1, "Swords to Plowshares", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, kill_paragon, &[Target::Object(paragon)]);
    settled(&mut state);
    assert_eq!(zone(&state, paragon), Zone::Exile);

    let life_before_elf_dies = life(&state, P0);
    let _bolt = opponent_bolts(&mut state, Target::Object(elf));
    settled(&mut state);
    assert_eq!(
        zone(&state, elf),
        Zone::Exile,
        "granted dies trigger survives Serra Paragon leaving"
    );
    assert_eq!(life(&state, P0), life_before_elf_dies + 2);
}

#[test]
fn serra_paragon_edge_cases_cr609_3_dress_down_opponent_turn_and_x_cap() {
    // 1. Not active on opponent's turn (even for a Flash permanent like Swift Reconfiguration).
    let mut state = plains();
    let _paragon = put(&mut state, P0, "Serra Paragon", Zone::Battlefield);
    let _reconfig = put(&mut state, P0, "Swift Reconfiguration", Zone::Graveyard);
    let _target = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    pass_to_player_main1(&mut state, P1);
    // Pass priority from P1 to P0 on P1's turn.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(
        !castable(&mut state, _reconfig),
        "Serra Paragon only allows casting on your own turn"
    );

    // 2. CR 609.3: if the permanent is exiled from graveyard before the dies trigger
    //    resolves (e.g. Relic of Progenitus), controller STILL gains 2 life!
    let mut state = plains();
    let _paragon = put(&mut state, P0, "Serra Paragon", Zone::Battlefield);
    let elf = put(&mut state, P0, "Elvish Mystic", Zone::Graveyard);
    let relic = put(&mut state, P1, "Relic of Progenitus", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elf, &[]);
    settled(&mut state);
    let life_before = life(&state, P0);
    let _bolt = opponent_bolts(&mut state, Target::Object(elf));
    // Resolve Bolt (elf dies, Serra Paragon dies trigger goes on stack).
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    // Now P1 activates Relic of Progenitus ability 1 ({1}, Exile Relic: exile all graveyards)
    // in response to Serra Paragon's dies trigger!
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    pool(&mut state, P1, &[], 1);
    activate(&mut state, relic, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elf), Zone::Exile);
    assert_eq!(
        life(&state, P0),
        life_before + 2,
        "CR 609.3: gain 2 life even when card already left graveyard"
    );

    // 3. Ability removal (Dress Down) before dying suppresses the granted dies trigger.
    let mut state = plains();
    let _paragon = put(&mut state, P0, "Serra Paragon", Zone::Battlefield);
    let elf = put(&mut state, P0, "Elvish Mystic", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elf, &[]);
    settled(&mut state);
    let dd = put(&mut state, P1, "Dress Down", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::U], 1);
    cast(&mut state, dd, &[]);
    settled(&mut state);
    let life_before = life(&state, P0);
    let _bolt = opponent_bolts(&mut state, Target::Object(elf));
    settled(&mut state);
    assert_eq!(
        zone(&state, elf),
        Zone::Graveyard,
        "Dress Down removed the granted dies trigger so elf stays in graveyard"
    );
    assert_eq!(life(&state, P0), life_before);

    // 4. X-spell mana value cap (Stonecoil Serpent {X} cast via Serra Paragon caps X <= 3).
    let mut state = plains();
    let _paragon = put(&mut state, P0, "Serra Paragon", Zone::Battlefield);
    let serpent = put(&mut state, P0, "Stonecoil Serpent", Zone::Graveyard);
    pool(&mut state, P0, &[], 6);
    assert!(castable(&mut state, serpent));
    act(&mut state, Action::CastSpell(serpent));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            // Options are X = 0, 1, 2, 3 (4 options total), NOT up to 6!
            assert_eq!(
                option_count, 4,
                "Serra Paragon caps Stonecoil Serpent X at 3 (options 0..=3)"
            );
            act(&mut state, Action::ChooseEffectOption(3));
        }
        other => panic!("expected X choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, serpent), Zone::Battlefield);
    assert_eq!(power_toughness(&state, serpent), (3, 3));
}

// ---- Swift Reconfiguration -----------------------------------------------------

#[test]
fn swift_reconfiguration_types_subtypes_devoted_druid_sba_and_crew_5() {
    let mut state = plains();
    let nadu = put(&mut state, P0, "Nadu, Winged Wisdom", Zone::Battlefield);
    let rancor = put(&mut state, P0, "Rancor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, rancor, &[Target::Object(nadu)]);
    settled(&mut state);
    let jitte = put(&mut state, P0, "Umezawa's Jitte", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, jitte, 3, &[Target::Object(nadu)]);
    settled(&mut state);

    // Enchant Legendary Creature - Bird Wizard with Swift Reconfiguration.
    let reconfig = put(&mut state, P0, "Swift Reconfiguration", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, reconfig, &[Target::Object(nadu)]);
    settled(&mut state);

    // Layer 4: Artifact ONLY (lost Creature), subtype Vehicle ONLY (lost Bird/Wizard),
    // keeps Legendary supertype and printed U/G colors, stays legally attached!
    assert_eq!(zone(&state, reconfig), Zone::Battlefield);
    assert!(state.objects.get(nadu).attachments.contains(&reconfig));
    assert!(engine::object_has_type(&state, nadu, CardType::Artifact));
    assert!(!engine::object_has_type(&state, nadu, CardType::Creature));
    assert!(engine::has_effective_subtype(
        &state,
        nadu,
        Subtype::Vehicle
    ));
    assert!(!engine::has_effective_subtype(&state, nadu, Subtype::Bird));
    assert!(!engine::has_effective_subtype(
        &state,
        nadu,
        Subtype::Wizard
    ));
    assert!(engine::effective_supertypes(&state, nadu).contains(&Supertype::Legendary));
    assert_ne!(engine::object_color_mask(&state, nadu) & 0x1f, 0);

    // SBA: creature-only Aura (Rancor) fell off and Equipment (Jitte) unattached!
    assert!(!state.objects.get(nadu).attachments.contains(&rancor));
    assert!(!state.objects.get(nadu).attachments.contains(&jitte));
    assert_eq!(zone(&state, jitte), Zone::Battlefield);

    // Crew 5: with only a 4-power creature (Questing Beast), Crew 5 is NOT activatable.
    let beast = put(&mut state, P0, "Questing Beast", Zone::Battlefield);
    assert!(!activatable(&mut state, nadu, 253));
    // Add Elvish Mystic (power 1) -> total power 5 -> Crew 5 IS activatable!
    let elf = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    crew_with(&mut state, nadu, 253, &[beast, elf]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, nadu, CardType::Artifact));
    assert!(engine::object_has_type(&state, nadu, CardType::Creature));
    assert!(engine::has_effective_subtype(
        &state,
        nadu,
        Subtype::Vehicle
    ));
    assert!(
        !engine::has_effective_subtype(&state, nadu, Subtype::Bird),
        "crewing does not restore creature subtypes lost to Swift Reconfiguration"
    );
    assert_eq!(power_toughness(&state, nadu), (3, 4));

    // At cleanup, Crew wears off and Nadu reverts to a noncreature Artifact - Vehicle.
    pass_to_player_main1(&mut state, P1);
    assert!(engine::object_has_type(&state, nadu, CardType::Artifact));
    assert!(!engine::object_has_type(&state, nadu, CardType::Creature));

    // Iconic Vintage Cube Devoted Druid combo: summoning-sick Devoted Druid enchanted
    // by Swift Reconfiguration taps for {G} immediately and untaps repeatedly with
    // -1/-1 counters below 0 toughness without dying!
    let mut state = plains();
    let druid = put(&mut state, P0, "Devoted Druid", Zone::Battlefield);
    state.objects.get_mut(druid).summoning_sick = true;
    let reconfig = put(&mut state, P0, "Swift Reconfiguration", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, reconfig, &[Target::Object(druid)]);
    settled(&mut state);
    assert!(!engine::object_has_type(&state, druid, CardType::Creature));
    for _ in 0..3 {
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(druid));
        activate(&mut state, druid, 0, &[]);
        settled(&mut state);
    }
    assert_eq!(zone(&state, druid), Zone::Battlefield);
    assert_eq!(state.objects.get(druid).counters.minus1_minus1, 3);
    assert_eq!(mana(&state, P0, ManaColor::G), 3);
}

#[test]
fn swift_reconfiguration_combat_removal_vehicle_timestamps_and_stony_silence() {
    // 1. Flashing Swift Reconfiguration onto an attacking creature removes it from combat!
    let mut state = plains();
    let attacker = put(&mut state, P0, "Questing Beast", Zone::Battlefield);
    let reconfig = put(&mut state, P1, "Swift Reconfiguration", Zone::Hand);
    pass_until(&mut state, |s| s.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![attacker]));
    // P0 passes in Declare Attackers; P1 adds {W} and flashes Swift Reconfiguration onto attacker.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    pool(&mut state, P1, &[ManaColor::W], 0);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    cast(&mut state, reconfig, &[Target::Object(attacker)]);
    settled(&mut state);
    assert!(
        !state.engine.combat.attackers.contains(&attacker),
        "flashed Swift Reconfiguration removes attacking creature from combat (CR 506.4)"
    );
    pass_until(&mut state, |s| s.step == Step::Main2);
    assert_eq!(
        life(&state, P1),
        20,
        "removed attacker dealt 0 combat damage"
    );

    // 2. Flashing onto an ALREADY-crewed Smuggler's Copter overrides the earlier Crew;
    //    crewing again afterward makes it a creature again.
    let mut state = plains();
    let copter = put(&mut state, P0, "Smuggler's Copter", Zone::Battlefield);
    let elf = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let elf2 = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    // Enchant uncrewed Vehicle is legal, too, and here we crew first then reconfig.
    crew_with(&mut state, copter, 0, &[elf]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, copter, CardType::Creature));

    let reconfig = put(&mut state, P1, "Swift Reconfiguration", Zone::Hand);
    state.priority_player = P1;
    pool(&mut state, P1, &[ManaColor::W], 0);
    cast(&mut state, reconfig, &[Target::Object(copter)]);
    settled(&mut state);
    assert!(
        !engine::object_has_type(&state, copter, CardType::Creature),
        "later Swift Reconfiguration overrides earlier Crew timestamp"
    );
    // Crew again with printed Crew 1 -> later timestamp makes Copter a creature again!
    state.priority_player = P0;
    crew_with(&mut state, copter, 0, &[elf2]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, copter, CardType::Creature));

    // 3. Stony Silence blocks activating Swift Reconfiguration's granted Crew 5.
    let mut state = plains();
    let host = put(&mut state, P0, "Nadu, Winged Wisdom", Zone::Battlefield);
    let _b1 = put(&mut state, P0, "Questing Beast", Zone::Battlefield);
    let _b2 = put(&mut state, P0, "Elvish Mystic", Zone::Battlefield);
    let reconfig = put(&mut state, P0, "Swift Reconfiguration", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, reconfig, &[Target::Object(host)]);
    settled(&mut state);
    assert!(activatable(&mut state, host, 253));
    let _stony = put(&mut state, P1, "Stony Silence", Zone::Battlefield);
    assert!(
        !activatable(&mut state, host, 253),
        "Stony Silence locks artifact Crew 5 granted by Swift Reconfiguration"
    );
}

// ---- Public Sessions -----------------------------------------------------------

#[derive(Default)]
struct C9bEvents {
    casts: [usize; C9B_CARDS.len()],
}

impl C9bEvents {
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
        if let Some(index) = C9B_CARDS.iter().position(|name| *name == card.name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c9b_decks_finish_and_replay_through_public_sessions_both_seats() {
    let naya_figures = deck(&[
        ("Plateau", 8),
        ("Savannah", 8),
        ("Wasteland", 4),
        ("Mox Pearl", 4),
        ("Mox Emerald", 4),
        ("Figure of Destiny", 6),
        ("Figure of Fable", 6),
        ("Serra Paragon", 6),
        ("Swift Reconfiguration", 6),
        ("Lightning Bolt", 4),
    ]);
    let bant_nadu = deck(&[
        ("Tropical Island", 10),
        ("Tundra", 6),
        ("Savannah", 4),
        ("Mox Emerald", 4),
        ("Mox Sapphire", 4),
        ("Hexdrinker", 6),
        ("Nadu, Winged Wisdom", 6),
        ("Shuko", 4),
        ("Elvish Mystic", 4),
        ("Smuggler's Copter", 4),
    ]);
    let mut events = C9bEvents::default();
    // 2 seats x 3 seeds x 2 policies x (original + replay) = 24 games total.
    for pair in [
        [naya_figures.clone(), bant_nadu.clone()],
        [bant_nadu, naya_figures],
    ] {
        for seed in 0x4339_6200_u64..0x4339_6203 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                let played =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action);
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(played.transcript_sha256, replay.transcript_sha256);
            }
        }
    }
    for (name, casts) in C9B_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} was never cast in a public session");
    }
}
