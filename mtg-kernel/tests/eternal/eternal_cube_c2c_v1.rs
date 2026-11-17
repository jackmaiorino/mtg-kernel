//! Vintage Cube wave C2c: native rules and public-session tests for the
//! mana artifacts and Fastbond.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C2C_CARDS: [&str; 11] = [
    "Talisman of Creativity",
    "Talisman of Curiosity",
    "Talisman of Dominance",
    "Talisman of Progress",
    "Mind Stone",
    "Basalt Monolith",
    "Candelabra of Tawnos",
    "Coalition Relic",
    "Fastbond",
    "Everflowing Chalice",
    "Pentad Prism",
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
        0x4355_4245_0000 + 0x3263,
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

/// Advances until the ability just activated is on the stack with its costs
/// paid (the engine finishes a pending activation as it next advances).
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

/// Answers the as-enters choices of permanents the test put onto the
/// battlefield directly (Valgavoth's Lair's color), with the first option.
fn answer_entry_choices(state: &mut GameState) {
    while let Decision::ChooseEffectOption { .. } = next(state) {
        act(state, Action::ChooseEffectOption(0));
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
fn c2c_cards_are_full() {
    for name in C2C_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c2c_cards_have_their_printed_characteristics() {
    for (name, cost) in [
        ("Talisman of Creativity", 2u8),
        ("Talisman of Curiosity", 2),
        ("Talisman of Dominance", 2),
        ("Talisman of Progress", 2),
        ("Mind Stone", 2),
        ("Basalt Monolith", 3),
        ("Candelabra of Tawnos", 1),
        ("Coalition Relic", 3),
        ("Everflowing Chalice", 0),
        ("Pentad Prism", 2),
    ] {
        let card = def(name);
        assert_eq!(card.types, &[CardType::Artifact], "{name}");
        assert_eq!(card.cost.generic, cost, "{name}");
        assert!(card.cost.pips.is_empty(), "{name}");
        assert_eq!(card.mana_value, u16::from(cost), "{name}");
        assert!(!card.is_land, "{name}");
    }
    let fastbond = def("Fastbond");
    assert_eq!(fastbond.types, &[CardType::Enchantment]);
    assert_eq!((fastbond.cost.generic, fastbond.cost.pips.len()), (0, 1));
    assert_eq!(fastbond.mana_value, 1);

    for name in [
        "Talisman of Creativity",
        "Talisman of Curiosity",
        "Talisman of Dominance",
        "Talisman of Progress",
    ] {
        let card = def(name);
        assert_eq!(card.mana_ability_choices, &[ManaColor::C], "{name}");
        assert_eq!(card.additional_mana_abilities.len(), 2, "{name}");
        for ability in card.additional_mana_abilities {
            assert_eq!(ability.ability.controller_damage, 1, "{name}");
            assert_eq!(ability.colors.len(), 1, "{name}");
        }
    }
    assert_eq!(def("Mind Stone").activated_abilities.len(), 1);
    assert_eq!(def("Basalt Monolith").activated_abilities.len(), 1);
    assert_eq!(def("Candelabra of Tawnos").activated_abilities.len(), 9);
    assert_eq!(def("Coalition Relic").activated_abilities.len(), 1);
    assert_eq!(def("Coalition Relic").mana_ability_choices.len(), 5);
    let prism = def("Pentad Prism");
    assert!(prism.mana_ability_choices.is_empty());
    assert_eq!(prism.additional_mana_abilities.len(), 1);
    assert_eq!(prism.additional_mana_abilities[0].colors.len(), 5);
    assert!(def("Everflowing Chalice").mana_ability_def.is_some());
    let kick = mtg_kernel::eternal_cube_c2c_v1::multikicker_cost("Everflowing Chalice")
        .expect("multikicker cost");
    assert_eq!((kick.generic, kick.pips.len(), kick.x_count), (2, 0, 0));
    assert!(mtg_kernel::eternal_cube_c2c_v1::multikicker_cost("Mind Stone").is_none());
}

// ---- Helpers of this wave ------------------------------------------------------------

/// Plays on to the beginning of P0's next first main phase, where it stops
/// before any player acts.
fn to_next_main(state: &mut GameState) {
    let turn = state.turn;
    pass_until(state, |state| {
        state.turn > turn && state.active_player == P0 && state.step == Step::Main1
    });
}

fn tap(state: &mut GameState, object: ObjectId) {
    state.objects.get_mut(object).tapped = true;
}

fn is_tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
}

fn charge(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.charge
}

/// How many 1-damage events `source` dealt to `victim` so far.
fn damage_events(state: &GameState, source: ObjectId, victim: PlayerId) -> usize {
    state
        .engine
        .event_history
        .iter()
        .filter(|event| {
            matches!(
                event,
                CommittedEvent::Damage { source: dealer, target: Target::Player(player), amount: 1 }
                    if *dealer == source && *player == victim
            )
        })
        .count()
}

/// Whether P0 may play `land` from hand right now.
fn can_play_land(state: &GameState, land: ObjectId) -> bool {
    let mut probe = state.clone();
    next(&mut probe);
    engine::step(&mut probe, Action::PlayLand(land)).is_ok()
}

// ---- Talismans -----------------------------------------------------------------------

const TALISMANS: [(&str, [ManaColor; 2]); 4] = [
    ("Talisman of Creativity", [ManaColor::U, ManaColor::R]),
    ("Talisman of Curiosity", [ManaColor::G, ManaColor::U]),
    ("Talisman of Dominance", [ManaColor::U, ManaColor::B]),
    ("Talisman of Progress", [ManaColor::W, ManaColor::U]),
];

#[test]
fn talismans_tap_for_colorless_freely_and_deal_one_damage_for_a_color() {
    for (name, colors) in TALISMANS {
        let mut state = plains();
        let talisman = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        act(
            &mut state,
            Action::ActivateManaAbilityChoice(talisman, ManaColor::C),
        );
        assert_eq!(mana(&state, P0, ManaColor::C), 1, "{name}");
        assert_eq!(life(&state, P0), 20, "{name}");
        assert!(is_tapped(&state, talisman), "{name}");
        // Tapped, it makes no more mana of any kind.
        for color in [ManaColor::C, colors[0], colors[1]] {
            assert!(
                engine::step(
                    &mut state,
                    Action::ActivateManaAbilityChoice(talisman, color)
                )
                .is_err(),
                "{name} tapped"
            );
        }

        for color in colors {
            let mut state = plains();
            let talisman = put(&mut state, P0, name, Zone::Battlefield);
            next(&mut state);
            act(
                &mut state,
                Action::ActivateManaAbilityChoice(talisman, color),
            );
            assert_eq!(mana(&state, P0, color), 1, "{name}");
            assert_eq!(mana(&state, P0, ManaColor::C), 0, "{name}");
            assert_eq!(life(&state, P0), 19, "{name}");
            assert_eq!(life(&state, P1), 20, "{name}");
            assert!(is_tapped(&state, talisman), "{name}");
            assert_eq!(
                damage_events(&state, talisman, P0),
                1,
                "{name}: the Talisman deals the damage"
            );
        }

        // A color it does not print is refused.
        let other = [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
        .into_iter()
        .find(|color| !colors.contains(color))
        .unwrap();
        let mut state = plains();
        let talisman = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        assert!(
            engine::step(
                &mut state,
                Action::ActivateManaAbilityChoice(talisman, other)
            )
            .is_err(),
            "{name}"
        );
    }
}

#[test]
fn talismans_are_cast_for_two_and_pay_a_spell_with_their_colors() {
    for (name, colors) in TALISMANS {
        let mut state = plains();
        let talisman = put(&mut state, P0, name, Zone::Hand);
        pool(&mut state, P0, &[], 2);
        cast(&mut state, talisman, &[]);
        assert_eq!(zone(&state, talisman), Zone::Stack, "{name}");
        settled(&mut state);
        assert_eq!(zone(&state, talisman), Zone::Battlefield, "{name}");
        assert_eq!(mana(&state, P0, ManaColor::C), 0, "{name}");

        // Its red mana pays for Lightning Bolt ({R}), at 1 damage to P0.
        if colors.contains(&ManaColor::R) {
            let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
            next(&mut state);
            act(
                &mut state,
                Action::ActivateManaAbilityChoice(talisman, ManaColor::R),
            );
            cast(&mut state, bolt, &[Target::Player(P1)]);
            settled(&mut state);
            assert_eq!(life(&state, P1), 17, "{name}");
            assert_eq!(life(&state, P0), 19, "{name}");
        }
    }
}

// ---- Mind Stone ----------------------------------------------------------------------

#[test]
fn mind_stone_taps_for_colorless_and_cycles_for_one() {
    let mut state = game(&["Island", "Plains", "Plains"]);
    let stone = put(&mut state, P0, "Mind Stone", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, stone, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, stone), Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(stone));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
    assert!(is_tapped(&state, stone));

    // "{1}, {T}, Sacrifice this artifact: Draw a card."
    let mut state = game(&["Island", "Plains", "Plains"]);
    let top = state.players[0].library[0];
    let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, stone, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, stone), Zone::Graveyard);
    assert!(hand_contains(&state, P0, top), "drew the top card");
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "the {{1}} was paid");

    // Without the {1}, or tapped, it cannot cycle.
    let mut state = plains();
    let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
    assert!(!activatable(&mut state, stone, 0), "no mana for the cost");
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, stone, 0));
    tap(&mut state, stone);
    assert!(!activatable(&mut state, stone, 0), "it is tapped");
}

// ---- Basalt Monolith -----------------------------------------------------------------

#[test]
fn basalt_monolith_taps_for_three_does_not_untap_and_untaps_for_three() {
    let mut state = plains();
    let monolith = put(&mut state, P0, "Basalt Monolith", Zone::Battlefield);
    let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(monolith));
    assert_eq!(mana(&state, P0, ManaColor::C), 3);
    assert!(is_tapped(&state, monolith));

    // "{3}: Untap this artifact." works while it is tapped, then it taps
    // again for three more.
    activate(&mut state, monolith, 0, &[]);
    settled(&mut state);
    assert!(!is_tapped(&state, monolith));
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "the untap cost {{3}}");
    act(&mut state, Action::ActivateManaAbility(monolith));
    assert_eq!(mana(&state, P0, ManaColor::C), 3);
    assert!(is_tapped(&state, monolith));

    // Through the next untap step it stays tapped, while another artifact
    // untaps.
    act(&mut state, Action::ActivateManaAbility(stone));
    assert!(is_tapped(&state, stone));
    to_next_main(&mut state);
    assert!(is_tapped(&state, monolith), "doesn't untap during untap");
    assert!(!is_tapped(&state, stone), "Mind Stone untaps");

    pool(&mut state, P0, &[], 3);
    activate(&mut state, monolith, 0, &[]);
    settled(&mut state);
    assert!(!is_tapped(&state, monolith));
}

#[test]
fn basalt_monolith_untap_costs_exactly_three_and_an_opponents_never_untaps() {
    let mut state = plains();
    let monolith = put(&mut state, P0, "Basalt Monolith", Zone::Battlefield);
    tap(&mut state, monolith);
    pool(&mut state, P0, &[], 2);
    assert!(!activatable(&mut state, monolith, 0), "two is not three");
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, monolith, 0));

    // P1's own untap step leaves P1's tapped Monolith tapped too.
    let mut state = plains();
    let theirs = put(&mut state, P1, "Basalt Monolith", Zone::Battlefield);
    tap(&mut state, theirs);
    to_next_main(&mut state);
    assert!(is_tapped(&state, theirs), "never untaps, even on its turns");
}

// ---- Candelabra of Tawnos ------------------------------------------------------------

/// The lands P0 names for a Candelabra ability are named in object order, so
/// the announcements below list them ascending.
fn sorted(mut lands: Vec<ObjectId>) -> Vec<Target> {
    lands.sort_unstable();
    lands.into_iter().map(Target::Object).collect()
}

#[test]
fn candelabra_untaps_x_target_lands_of_either_player_tapped_or_not() {
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    let mine: Vec<ObjectId> = (0..3)
        .map(|_| put(&mut state, P0, "Plains", Zone::Battlefield))
        .collect();
    let theirs = put(&mut state, P1, "Plains", Zone::Battlefield);
    tap(&mut state, mine[0]);
    tap(&mut state, mine[1]);
    tap(&mut state, theirs);
    pool(&mut state, P0, &[], 2);
    // Targets are announced as the ability is activated.
    let targets = sorted(vec![mine[0], theirs]);
    activate(&mut state, candelabra, 2, &targets);
    on_the_stack(&mut state);
    assert!(is_tapped(&state, candelabra), "{{T}} is part of the cost");
    assert!(
        is_tapped(&state, mine[0]),
        "nothing untaps before it resolves"
    );
    settled(&mut state);
    assert!(!is_tapped(&state, mine[0]));
    assert!(!is_tapped(&state, theirs));
    assert!(is_tapped(&state, mine[1]), "not a target");

    // An untapped land may be targeted too; it just stays untapped.
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    let first = put(&mut state, P0, "Plains", Zone::Battlefield);
    let second = put(&mut state, P1, "Plains", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, candelabra, 2, &sorted(vec![first, second]));
    settled(&mut state);
    assert!(is_tapped(&state, candelabra));
    assert!(!is_tapped(&state, first));
    assert!(!is_tapped(&state, second));
}

#[test]
fn candelabra_needs_exactly_x_distinct_lands_to_be_activated() {
    // X = 0 needs no land and only taps it.
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    assert!(activatable(&mut state, candelabra, 0));
    assert!(!activatable(&mut state, candelabra, 1), "no land to target");
    activate(&mut state, candelabra, 0, &[]);
    settled(&mut state);
    assert!(is_tapped(&state, candelabra));

    // Two lands: X = 2 is offered, X = 3 is not even with {3} to pay.
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    let first = put(&mut state, P0, "Plains", Zone::Battlefield);
    let second = put(&mut state, P1, "Plains", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    assert!(activatable(&mut state, candelabra, 1));
    assert!(activatable(&mut state, candelabra, 2));
    assert!(!activatable(&mut state, candelabra, 3), "only two lands");
    assert!(!activatable(&mut state, candelabra, 8), "only two lands");
    // The announcement takes each land once, and the second pick follows the
    // first.
    act(&mut state, Action::ActivateAbility(candelabra, 2));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(first)]);
        }
        other => panic!("expected the first land, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(first)));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(second)]);
        }
        other => panic!("expected the second land, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(second)));
    settled(&mut state);
    assert!(is_tapped(&state, candelabra));

    // The cost is X and {T}: too little mana, or a tapped Candelabra, is not
    // enough.
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    for _ in 0..3 {
        put(&mut state, P0, "Plains", Zone::Battlefield);
    }
    for land in state.players[0].battlefield.clone() {
        if land != candelabra {
            tap(&mut state, land);
        }
    }
    pool(&mut state, P0, &[], 2);
    assert!(activatable(&mut state, candelabra, 2));
    assert!(!activatable(&mut state, candelabra, 3), "X = 3 needs {{3}}");
    tap(&mut state, candelabra);
    assert!(!activatable(&mut state, candelabra, 0), "it is tapped");
    assert!(!activatable(&mut state, candelabra, 2), "it is tapped");
}

#[test]
fn candelabra_cannot_target_hexproof_or_shroud_lands() {
    // An opponent's hexproof land (Valgavoth's Lair) is not a legal target,
    // so it does not count toward X.
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    let plains_a = put(&mut state, P0, "Plains", Zone::Battlefield);
    let plains_b = put(&mut state, P0, "Plains", Zone::Battlefield);
    let lair = put(&mut state, P1, "Valgavoth's Lair", Zone::Battlefield);
    answer_entry_choices(&mut state);
    assert!(keyword(&state, lair, Keywords::HEXPROOF));
    pool(&mut state, P0, &[], 3);
    assert!(activatable(&mut state, candelabra, 2));
    assert!(
        !activatable(&mut state, candelabra, 3),
        "the Lair is hexproof"
    );
    // Naming it is refused as well.
    act(&mut state, Action::ActivateAbility(candelabra, 2));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(lair)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(plains_a)));
    choose_target(&mut state, Target::Object(plains_b));
    settled(&mut state);

    // Hexproof does not stop its own controller.
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    let own_lair = put(&mut state, P0, "Valgavoth's Lair", Zone::Battlefield);
    answer_entry_choices(&mut state);
    tap(&mut state, own_lair);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, candelabra, 1, &[Target::Object(own_lair)]);
    settled(&mut state);
    assert!(!is_tapped(&state, own_lair));

    // Shroud (Sterling Grove's, on the Lair) stops everyone, its controller
    // too.
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    let own_lair = put(&mut state, P0, "Valgavoth's Lair", Zone::Battlefield);
    answer_entry_choices(&mut state);
    let grove = put(&mut state, P0, "Sterling Grove", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(grove, Zone::Battlefield),
    );
    assert!(keyword(&state, own_lair, Keywords::SHROUD));
    pool(&mut state, P0, &[], 1);
    assert!(
        !activatable(&mut state, candelabra, 1),
        "the only land has shroud"
    );
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    act(&mut state, Action::ActivateAbility(candelabra, 1));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(island)]);
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

#[test]
fn candelabra_untaps_only_the_targets_that_are_still_legal() {
    // One of two targets leaves the battlefield in response: the other still
    // untaps.
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    let kept = put(&mut state, P0, "Plains", Zone::Battlefield);
    let lost = put(&mut state, P1, "Plains", Zone::Battlefield);
    tap(&mut state, kept);
    tap(&mut state, lost);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, candelabra, 2, &sorted(vec![kept, lost]));
    on_the_stack(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(lost, Zone::Graveyard),
    );
    settled(&mut state);
    assert!(!is_tapped(&state, kept), "the legal target untaps");
    assert_eq!(zone(&state, lost), Zone::Graveyard);

    // Every target gone: it does nothing, and its costs stay paid.
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    let first = put(&mut state, P0, "Plains", Zone::Battlefield);
    let second = put(&mut state, P1, "Plains", Zone::Battlefield);
    let other = put(&mut state, P0, "Plains", Zone::Battlefield);
    tap(&mut state, first);
    tap(&mut state, second);
    tap(&mut state, other);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, candelabra, 2, &sorted(vec![first, second]));
    on_the_stack(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(first, Zone::Graveyard),
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(second, Zone::Graveyard),
    );
    settled(&mut state);
    assert!(is_tapped(&state, other), "not a target, so it stays tapped");
    assert!(is_tapped(&state, candelabra));
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn candelabra_untaps_up_to_eight_target_lands() {
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    let lands: Vec<ObjectId> = (0..8)
        .map(|_| put(&mut state, P0, "Plains", Zone::Battlefield))
        .collect();
    for &land in &lands {
        tap(&mut state, land);
    }
    pool(&mut state, P0, &[], 8);
    assert!(activatable(&mut state, candelabra, 8));
    activate(&mut state, candelabra, 8, &sorted(lands.clone()));
    settled(&mut state);
    assert!(lands.iter().all(|&land| !is_tapped(&state, land)));
    assert!(is_tapped(&state, candelabra));

    // Seven lands are too few for X = 8.
    let mut state = plains();
    let candelabra = put(&mut state, P0, "Candelabra of Tawnos", Zone::Battlefield);
    for _ in 0..7 {
        put(&mut state, P0, "Plains", Zone::Battlefield);
    }
    pool(&mut state, P0, &[], 8);
    assert!(activatable(&mut state, candelabra, 7));
    assert!(!activatable(&mut state, candelabra, 8));
}

// ---- Coalition Relic -----------------------------------------------------------------

#[test]
fn coalition_relic_taps_for_one_mana_of_any_color() {
    for color in [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ] {
        let mut state = plains();
        let relic = put(&mut state, P0, "Coalition Relic", Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(relic, color));
        assert_eq!(mana(&state, P0, color), 1);
        assert!(is_tapped(&state, relic));
        assert_eq!(charge(&state, relic), 0);
    }
}

#[test]
fn coalition_relic_puts_a_charge_counter_on_itself_by_tapping() {
    let mut state = plains();
    let relic = put(&mut state, P0, "Coalition Relic", Zone::Battlefield);
    activate(&mut state, relic, 0, &[]);
    settled(&mut state);
    assert_eq!(charge(&state, relic), 1);
    assert!(is_tapped(&state, relic));
    assert!(!activatable(&mut state, relic, 0), "it is tapped");
}

#[test]
fn coalition_relic_releases_its_counters_as_colors_you_choose() {
    let mut state = plains();
    let relic = put(&mut state, P0, "Coalition Relic", Zone::Battlefield);
    state.objects.get_mut(relic).counters.charge = 2;
    to_next_main(&mut state);
    assert!(!state.stack.is_empty(), "the trigger waits on the stack");
    // Option i of the color choice is W, U, B, R, G in that order.
    let mut answers = [2u16, 3].into_iter();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 5);
            Action::ChooseEffectOption(answers.next().expect("two colors"))
        }
        other => first_answer(other),
    });
    assert_eq!(answers.next(), None, "one choice per counter removed");
    assert_eq!(charge(&state, relic), 0);
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    assert_eq!(mana(&state, P0, ManaColor::W), 0);

    // The same color may be chosen for each counter.
    let mut state = plains();
    let relic = put(&mut state, P0, "Coalition Relic", Zone::Battlefield);
    state.objects.get_mut(relic).counters.charge = 3;
    to_next_main(&mut state);
    settle_option(&mut state, 4);
    assert_eq!(mana(&state, P0, ManaColor::G), 3);
    assert_eq!(charge(&state, relic), 0);
}

#[test]
fn coalition_relic_with_no_counters_adds_nothing_but_a_counter_put_in_response_counts() {
    let mut state = plains();
    put(&mut state, P0, "Coalition Relic", Zone::Battlefield);
    to_next_main(&mut state);
    assert!(!state.stack.is_empty(), "it triggers with no counters too");
    settled(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6]);

    let mut state = plains();
    let relic = put(&mut state, P0, "Coalition Relic", Zone::Battlefield);
    to_next_main(&mut state);
    assert!(!state.stack.is_empty());
    // In response, tap it for a counter; the counter resolves first and the
    // trigger then removes it.
    activate(&mut state, relic, 0, &[]);
    settle_option(&mut state, 0);
    assert_eq!(charge(&state, relic), 0);
    assert_eq!(mana(&state, P0, ManaColor::W), 1);
}

#[test]
fn coalition_relic_triggers_only_for_its_controller_in_the_first_main_phase() {
    // P1's Relic with counters does nothing as P0's first main phase begins
    // (the game is taken from P1's second main phase to P0's next turn).
    let mut state = plains();
    let theirs = put(&mut state, P1, "Coalition Relic", Zone::Battlefield);
    state.objects.get_mut(theirs).counters.charge = 2;
    state.active_player = P1;
    state.priority_player = P1;
    state.step = Step::Main2;
    to_next_main(&mut state);
    settled(&mut state);
    assert_eq!(charge(&state, theirs), 2);
    assert_eq!(state.players[1].mana_pool, [0; 6]);
    assert_eq!(state.players[0].mana_pool, [0; 6]);

    // The beginning of the second main phase does not trigger it.
    let mut state = plains();
    let relic = put(&mut state, P0, "Coalition Relic", Zone::Battlefield);
    state.objects.get_mut(relic).counters.charge = 1;
    pass_until(&mut state, |state| state.step == Step::Main2);
    assert_eq!(charge(&state, relic), 1);
}

// ---- Fastbond ------------------------------------------------------------------------

#[test]
fn fastbond_is_cast_for_green() {
    let mut state = plains();
    let fastbond = put(&mut state, P0, "Fastbond", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, fastbond, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, fastbond), Zone::Battlefield);
}

#[test]
fn fastbond_lets_you_play_any_number_of_lands_and_hurts_after_the_first() {
    let mut state = plains();
    let fastbond = put(&mut state, P0, "Fastbond", Zone::Battlefield);
    let lands: Vec<ObjectId> = (0..5)
        .map(|_| put(&mut state, P0, "Plains", Zone::Hand))
        .collect();
    for (index, &land) in lands.iter().enumerate() {
        assert!(can_play_land(&state, land), "land {index} is playable");
        play_land(&mut state, land);
        assert_eq!(zone(&state, land), Zone::Battlefield);
        assert_eq!(
            life(&state, P0),
            20 - i32::try_from(index).unwrap(),
            "the first land is free, each later one costs 1 life"
        );
        assert_eq!(damage_events(&state, fastbond, P0), index);
    }
    assert_eq!(life(&state, P1), 20);
}

#[test]
fn without_fastbond_only_one_land_is_played_and_only_its_controller_gets_more() {
    let mut state = plains();
    let first = put(&mut state, P0, "Plains", Zone::Hand);
    let second = put(&mut state, P0, "Plains", Zone::Hand);
    assert!(can_play_land(&state, first));
    play_land(&mut state, first);
    assert!(!can_play_land(&state, second), "one land a turn");

    // An opponent's Fastbond does not give P0 more land plays.
    let mut state = plains();
    put(&mut state, P1, "Fastbond", Zone::Battlefield);
    let first = put(&mut state, P0, "Plains", Zone::Hand);
    let second = put(&mut state, P0, "Plains", Zone::Hand);
    play_land(&mut state, first);
    assert!(!can_play_land(&state, second), "it is not P0's Fastbond");
    assert_eq!(life(&state, P0), 20);
    assert_eq!(life(&state, P1), 20);
}

#[test]
fn fastbond_ignores_lands_that_enter_without_being_played() {
    let mut state = game(&["Forest", "Island", "Plains", "Plains"]);
    let fastbond = put(&mut state, P0, "Fastbond", Zone::Battlefield);
    let fetch = put(&mut state, P0, "Misty Rainforest", Zone::Battlefield);
    let first = put(&mut state, P0, "Plains", Zone::Hand);
    let second = put(&mut state, P0, "Plains", Zone::Hand);
    play_land(&mut state, first);
    assert_eq!(life(&state, P0), 20);

    // "Pay 1 life, Sacrifice this land: Search for a Forest or Island card":
    // the land it finds is put onto the battlefield, not played.
    let found = state.players[0].library[0];
    activate(&mut state, fetch, 0, &[]);
    let mut chosen = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. }
            if !chosen && legal_targets.contains(&Target::Object(found)) =>
        {
            chosen = true;
            Action::ChooseEffectTarget(Target::Object(found))
        }
        other => first_answer(other),
    });
    assert!(chosen, "the search offered the land");
    assert_eq!(zone(&state, found), Zone::Battlefield);
    assert_eq!(
        life(&state, P0),
        19,
        "only the fetch land's own life payment"
    );
    assert_eq!(damage_events(&state, fastbond, P0), 0);
    assert_eq!(state.players[0].lands_played_this_turn, 1);

    // The next land played is the second one played this turn.
    play_land(&mut state, second);
    assert_eq!(life(&state, P0), 18);
    assert_eq!(damage_events(&state, fastbond, P0), 1);
}

// ---- Pentad Prism --------------------------------------------------------------------

#[test]
fn pentad_prism_enters_with_a_counter_for_each_color_of_mana_spent() {
    for (colored, colorless, expected) in [
        (vec![ManaColor::W, ManaColor::U], 0u8, 2),
        (vec![ManaColor::W, ManaColor::W], 0, 1),
        (vec![ManaColor::R], 1, 1),
        (Vec::new(), 2, 0),
    ] {
        let mut state = plains();
        let prism = put(&mut state, P0, "Pentad Prism", Zone::Hand);
        pool(&mut state, P0, &colored, colorless);
        cast(&mut state, prism, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, prism), Zone::Battlefield);
        assert_eq!(
            charge(&state, prism),
            expected,
            "{colored:?} and {colorless} colorless"
        );
    }
}

#[test]
fn pentad_prism_removes_a_counter_to_add_one_mana_of_any_color() {
    for color in [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ] {
        let mut state = plains();
        let prism = put(&mut state, P0, "Pentad Prism", Zone::Battlefield);
        state.objects.get_mut(prism).counters.charge = 2;
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(prism, color));
        assert_eq!(mana(&state, P0, color), 1);
        assert_eq!(charge(&state, prism), 1);
        assert!(!is_tapped(&state, prism), "there is no {{T}} in the cost");
        // A second activation in a row, then none left.
        act(&mut state, Action::ActivateManaAbilityChoice(prism, color));
        assert_eq!(mana(&state, P0, color), 2);
        assert_eq!(charge(&state, prism), 0);
        assert!(
            engine::step(&mut state, Action::ActivateManaAbilityChoice(prism, color)).is_err(),
            "no counter to remove"
        );
        assert_eq!(mana(&state, P0, color), 2);
    }
}

#[test]
fn pentad_prism_that_was_not_cast_has_no_counters_and_no_mana() {
    let mut state = plains();
    let prism = put(&mut state, P0, "Pentad Prism", Zone::Battlefield);
    next(&mut state);
    for color in [ManaColor::W, ManaColor::G] {
        assert!(engine::step(&mut state, Action::ActivateManaAbilityChoice(prism, color)).is_err());
    }
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

// ---- Everflowing Chalice -------------------------------------------------------------

/// P0 casts the Chalice in `chalice`, announcing `kicks` multikicker
/// payments; returns the number of counts the cast offered.
fn cast_chalice(state: &mut GameState, chalice: ObjectId, kicks: u16) -> u16 {
    assert!(castable(state, chalice));
    act(state, Action::CastSpell(chalice));
    let counts = match next(state) {
        Decision::ChooseEffectOption { option_count, .. } => option_count,
        other => panic!("expected the kick count, got {other:?}"),
    };
    act(state, Action::ChooseEffectOption(kicks));
    counts
}

#[test]
fn everflowing_chalice_enters_with_a_counter_for_each_kick() {
    for kicks in 0..=2u16 {
        let mut state = plains();
        let chalice = put(&mut state, P0, "Everflowing Chalice", Zone::Hand);
        pool(&mut state, P0, &[], 4);
        let counts = cast_chalice(&mut state, chalice, kicks);
        assert_eq!(counts, 3, "0, 1 or 2 kicks with four mana");
        assert_eq!(
            engine::object_mana_value(&state, chalice),
            0,
            "kicks do not change the mana value"
        );
        settled(&mut state);
        assert_eq!(zone(&state, chalice), Zone::Battlefield);
        assert_eq!(charge(&state, chalice), i32::from(kicks));
        assert_eq!(
            mana(&state, P0, ManaColor::C),
            4 - 2 * u8::try_from(kicks).unwrap(),
            "each kick costs {{2}}"
        );
    }
}

#[test]
fn everflowing_chalice_pays_each_kick_from_lands_and_asks_nothing_when_none_is_payable() {
    // Six lands pay for up to three kicks.
    let mut state = plains();
    let lands: Vec<ObjectId> = (0..6)
        .map(|_| put(&mut state, P0, "Plains", Zone::Battlefield))
        .collect();
    let chalice = put(&mut state, P0, "Everflowing Chalice", Zone::Hand);
    let counts = cast_chalice(&mut state, chalice, 3);
    assert_eq!(counts, 4);
    settled(&mut state);
    assert_eq!(charge(&state, chalice), 3);
    assert!(lands.iter().all(|&land| is_tapped(&state, land)));

    // Nothing to pay with: it is cast with no kicks and no question.
    let mut state = plains();
    let chalice = put(&mut state, P0, "Everflowing Chalice", Zone::Hand);
    assert!(castable(&mut state, chalice), "its cost is {{0}}");
    act(&mut state, Action::CastSpell(chalice));
    settled(&mut state);
    assert_eq!(zone(&state, chalice), Zone::Battlefield);
    assert_eq!(charge(&state, chalice), 0);

    // One mana is not enough for a kick either.
    let mut state = plains();
    let chalice = put(&mut state, P0, "Everflowing Chalice", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, chalice));
    act(&mut state, Action::CastSpell(chalice));
    settled(&mut state);
    assert_eq!(charge(&state, chalice), 0);
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
}

/// Multikicker is an additional cost: a Chalice cast without paying its mana
/// cost still announces and pays its kicks (118.9d, 601.2f). Cascade.
#[test]
fn everflowing_chalice_cast_by_cascade_may_still_be_kicked() {
    let mut state = game(&["Island", "Everflowing Chalice", "Plains", "Plains"]);
    let chalice = state.players[0].library[1];
    let challenger = put(&mut state, P0, "Bloodbraid Challenger", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 7);
    cast(&mut state, challenger, &[]);
    let mut asked = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            asked.push(option_count);
            // Cast the Chalice, then announce two kicks.
            Action::ChooseEffectOption(if asked.len() == 1 { 0 } else { 2 })
        }
        Decision::ChooseEffectTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(chalice)) =>
        {
            Action::ChooseEffectTarget(Target::Object(chalice))
        }
        other => first_answer(other),
    });
    assert_eq!(asked, vec![2, 3], "cast or decline, then 0, 1 or 2 kicks");
    assert_eq!(zone(&state, chalice), Zone::Battlefield);
    assert_eq!(zone(&state, challenger), Zone::Battlefield);
    assert_eq!(charge(&state, chalice), 2);
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "two kicks paid {{4}}");
}

/// Omniscience: the mana cost is skipped and the kicks are still paid.
#[test]
fn everflowing_chalice_cast_through_omniscience_may_still_be_kicked() {
    let mut state = plains();
    put(&mut state, P0, "Omniscience", Zone::Battlefield);
    let chalice = put(&mut state, P0, "Everflowing Chalice", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    assert!(castable(&mut state, chalice));
    act(&mut state, Action::CastSpell(chalice));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2, "pay the mana cost or not");
        }
        other => panic!("expected the free-cast choice, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(1));
    match next(&mut state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 3, "0, 1 or 2 kicks with four mana");
        }
        other => panic!("expected the kick count, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(2));
    settled(&mut state);
    assert_eq!(zone(&state, chalice), Zone::Battlefield);
    assert_eq!(charge(&state, chalice), 2);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

/// Bolas's Citadel: life is paid rather than the mana cost, and the kicks
/// are still paid with mana.
#[test]
fn everflowing_chalice_cast_through_bolas_citadel_may_still_be_kicked() {
    let mut state = plains();
    put(&mut state, P0, "Bolas's Citadel", Zone::Battlefield);
    let chalice = put(&mut state, P0, "Everflowing Chalice", Zone::Library);
    pool(&mut state, P0, &[], 4);
    let counts = cast_chalice(&mut state, chalice, 2);
    assert_eq!(counts, 3, "0, 1 or 2 kicks with four mana");
    settled(&mut state);
    assert_eq!(zone(&state, chalice), Zone::Battlefield);
    assert_eq!(charge(&state, chalice), 2);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(life(&state, P0), 20, "its mana value is 0");
}

#[test]
fn everflowing_chalice_taps_for_one_colorless_for_each_counter() {
    for counters in [0, 1, 3] {
        let mut state = plains();
        let chalice = put(&mut state, P0, "Everflowing Chalice", Zone::Battlefield);
        state.objects.get_mut(chalice).counters.charge = counters;
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(chalice));
        assert_eq!(
            i32::from(mana(&state, P0, ManaColor::C)),
            counters,
            "{counters} counters"
        );
        assert!(is_tapped(&state, chalice));
        assert_eq!(charge(&state, chalice), counters, "counters stay");
    }
}

// ---- Public sessions -----------------------------------------------------------------

#[test]
fn c2c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let ramp = deck(&[
        ("Forest", 9),
        ("Mountain", 5),
        ("Llanowar Elves", 4),
        ("Jackal Pup", 4),
        ("Lightning Bolt", 4),
        ("Fastbond", 3),
        ("Talisman of Creativity", 3),
        ("Talisman of Curiosity", 2),
        ("Mind Stone", 3),
        ("Candelabra of Tawnos", 2),
        ("Basalt Monolith", 1),
    ]);
    let rocks = deck(&[
        ("Swamp", 6),
        ("Island", 5),
        ("Plains", 3),
        ("Hypnotic Specter", 6),
        ("Talisman of Dominance", 3),
        ("Talisman of Progress", 3),
        ("Coalition Relic", 3),
        ("Pentad Prism", 3),
        ("Everflowing Chalice", 3),
        ("Mind Stone", 3),
        ("Basalt Monolith", 2),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    for (pair, first_seed) in [
        ([ramp.clone(), rocks.clone()], 0x4332_4300_u64),
        ([rocks, ramp], 0x4332_4310),
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
    for name in C2C_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
