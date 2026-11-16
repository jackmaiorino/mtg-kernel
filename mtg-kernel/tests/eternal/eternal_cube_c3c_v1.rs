//! Vintage Cube wave C3c: native rules and public-session tests for the
//! artifact utilities and their tokens.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C3C_CARDS: [&str; 10] = [
    "Lightning Greaves",
    "Pyrite Spellbomb",
    "Shorikai, Genesis Engine",
    "The Echoverse Fulcrum",
    "Retrofitter Foundry",
    "Thopter Foundry",
    "Relic of Sauron",
    "Currency Converter",
    "Staff of the Storyteller",
    "The Mightstone and Weakstone",
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
        0x4355_4245_0000 + 0x3363,
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
    // Finish staging the activation, so its costs are paid.
    next(state);
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
fn c3c_cards_are_full() {
    for name in C3C_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
    for name in [
        "Servo Token",
        "Thopter Token",
        "Blue Thopter Token",
        "Construct Token",
        "Rogue Token",
    ] {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
    }
}

#[test]
fn c3c_cards_have_their_printed_characteristics() {
    for (name, cost, abilities) in [
        ("Lightning Greaves", 2u8, 1usize),
        ("Pyrite Spellbomb", 1, 2),
        ("The Echoverse Fulcrum", 2, 1),
        ("Retrofitter Foundry", 1, 4),
        ("Relic of Sauron", 4, 1),
        ("Currency Converter", 1, 2),
        ("The Mightstone and Weakstone", 5, 0),
    ] {
        let card = def(name);
        assert_eq!(card.types, &[CardType::Artifact], "{name}");
        assert_eq!(card.cost.generic, cost, "{name}");
        assert!(card.cost.pips.is_empty(), "{name}");
        assert_eq!(card.mana_value, u16::from(cost), "{name}");
        assert_eq!(card.activated_abilities.len(), abilities, "{name}");
        assert!(!card.is_land && card.power.is_none(), "{name}");
    }
    assert!(def("Lightning Greaves")
        .subtypes
        .contains(&Subtype::Equipment));
    let greaves = def("Lightning Greaves")
        .equipment
        .expect("an Equipment profile");
    for keywords in [
        greaves.controller_turn_keywords,
        greaves.other_turn_keywords,
    ] {
        assert!(keywords.has(Keywords::HASTE) && keywords.has(Keywords::SHROUD));
        assert!(!keywords.has(Keywords::HEXPROOF));
    }
    assert_eq!((greaves.power_delta, greaves.toughness_delta), (0, 0));

    for name in ["The Echoverse Fulcrum", "The Mightstone and Weakstone"] {
        assert_eq!(def(name).supertypes, &[Supertype::Legendary], "{name}");
    }
    assert_eq!(
        def("The Mightstone and Weakstone").subtypes,
        &[Subtype::Powerstone]
    );
    assert!(!Subtype::Powerstone.is_creature_type());

    let shorikai = def("Shorikai, Genesis Engine");
    assert_eq!(shorikai.types, &[CardType::Artifact]);
    assert_eq!(shorikai.subtypes, &[Subtype::Vehicle]);
    assert_eq!(shorikai.supertypes, &[Supertype::Legendary]);
    assert_eq!((shorikai.cost.generic, shorikai.cost.pips.len()), (2, 2));
    assert_eq!(shorikai.mana_value, 4);
    assert_eq!((shorikai.power, shorikai.toughness), (Some(8), Some(8)));
    assert_eq!(shorikai.colors, &[ManaColor::W, ManaColor::U]);
    assert_eq!(shorikai.activated_abilities.len(), 2);

    let foundry = def("Thopter Foundry");
    assert_eq!(foundry.types, &[CardType::Artifact]);
    assert_eq!((foundry.cost.generic, foundry.cost.pips.len()), (0, 2));
    assert_eq!(foundry.mana_value, 2);
    assert!(foundry.colors.contains(&ManaColor::W));
    assert!(foundry.colors.contains(&ManaColor::U));
    assert!(foundry.colors.contains(&ManaColor::B));
    assert_eq!(foundry.activated_abilities.len(), 1);

    let staff = def("Staff of the Storyteller");
    assert_eq!(staff.types, &[CardType::Artifact]);
    assert_eq!((staff.cost.generic, staff.cost.pips.len()), (1, 1));
    assert_eq!(staff.colors, &[ManaColor::W]);
    assert_eq!(staff.activated_abilities.len(), 1);

    let relic = def("Relic of Sauron");
    assert_eq!(
        relic.mana_ability_choices,
        &[ManaColor::U, ManaColor::B, ManaColor::R]
    );
    assert_eq!(relic.additional_mana_abilities.len(), 3);
    assert!(relic.mana_ability_def.is_some());
    assert_eq!(
        def("The Mightstone and Weakstone").mana_ability_choices,
        &[ManaColor::C]
    );
}

#[test]
fn c3c_tokens_have_their_printed_characteristics() {
    for (name, power, colors, subtype) in [
        ("Servo Token", 1, &[][..], Subtype::Servo),
        ("Thopter Token", 1, &[][..], Subtype::Thopter),
        (
            "Blue Thopter Token",
            1,
            &[ManaColor::U][..],
            Subtype::Thopter,
        ),
        ("Construct Token", 4, &[][..], Subtype::Construct),
    ] {
        let token = def(name);
        assert_eq!(
            token.types,
            &[CardType::Artifact, CardType::Creature],
            "{name}"
        );
        assert_eq!(token.subtypes, &[subtype], "{name}");
        assert_eq!(
            (token.power, token.toughness),
            (Some(power), Some(power)),
            "{name}"
        );
        assert_eq!(token.colors, colors, "{name}");
        assert!(subtype.is_creature_type(), "{name}");
    }
    for name in ["Thopter Token", "Blue Thopter Token"] {
        assert!(def(name).keywords.has(Keywords::FLYING), "{name}");
    }
    assert!(!def("Servo Token").keywords.has(Keywords::FLYING));
    assert!(!def("Construct Token").keywords.has(Keywords::FLYING));
    let rogue = def("Rogue Token");
    assert_eq!(rogue.types, &[CardType::Creature]);
    assert_eq!(rogue.subtypes, &[Subtype::Rogue]);
    assert_eq!(rogue.colors, &[ManaColor::B]);
    assert_eq!((rogue.power, rogue.toughness), (Some(2), Some(2)));
}

// ---- Helpers of this wave ------------------------------------------------------------

fn tap(state: &mut GameState, object: ObjectId) {
    state.objects.get_mut(object).tapped = true;
}

fn untap(state: &mut GameState, object: ObjectId) {
    state.objects.get_mut(object).tapped = false;
}

fn is_tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
}

fn charge(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.charge
}

fn hand_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

fn library_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].library.len()
}

fn on_battlefield(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].battlefield.contains(&object)
}

/// The targets P0 may choose first for `source`'s ability `index`.
fn ability_targets(state: &GameState, source: ObjectId, index: u8) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(activatable(&mut probe, source, index));
    act(&mut probe, Action::ActivateAbility(source, index));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Settles, discarding `card` whenever a discard is asked and answering each
/// effect option with `option`.
fn settle_discarding(state: &mut GameState, card: ObjectId, option: u16) {
    settle_with(state, |_, decision| match decision {
        Decision::Discard { choices, .. } => {
            assert!(choices.contains(&card), "{card:?} not in {choices:?}");
            Action::Discard(vec![card])
        }
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
        other => first_answer(other),
    });
}

/// Activates the Vehicle's crew ability `index`, tapping `members`.
fn crew(state: &mut GameState, vehicle: ObjectId, index: u8, members: &[ObjectId]) {
    assert!(activatable(state, vehicle, index), "crew not activatable");
    act(state, Action::ActivateAbility(vehicle, index));
    for &member in members {
        match next(state) {
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(member)));
                act(state, Action::ChooseEffectTarget(Target::Object(member)));
            }
            other => panic!("expected a crew choice, got {other:?}"),
        }
    }
    match next(state) {
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => act(state, Action::FinishEffectSelection),
        other => panic!("expected the crew to be complete, got {other:?}"),
    }
    settled(state);
}

// ---- Lightning Greaves ---------------------------------------------------------------

#[test]
fn lightning_greaves_costs_two_and_equips_for_free_granting_haste_and_shroud() {
    let mut state = plains();
    let greaves = put(&mut state, P0, "Lightning Greaves", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, greaves, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, greaves), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);

    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let bystander = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(elves).summoning_sick = true;
    assert!(!keyword(&state, elves, Keywords::HASTE));
    assert!(!keyword(&state, elves, Keywords::SHROUD));
    activate(&mut state, greaves, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert!(keyword(&state, elves, Keywords::HASTE));
    assert!(keyword(&state, elves, Keywords::SHROUD));
    assert!(!keyword(&state, bystander, Keywords::HASTE));
    assert!(!keyword(&state, bystander, Keywords::SHROUD));
    assert_eq!(power_toughness(&state, elves), (1, 1), "no stat change");
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "equip costs nothing");

    // Both on its controller's turn and on the opponent's.
    state.active_player = P1;
    assert!(keyword(&state, elves, Keywords::HASTE));
    assert!(keyword(&state, elves, Keywords::SHROUD));
    state.active_player = P0;

    // Haste lets the summoning-sick creature use its {T} ability.
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(elves));
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
}

#[test]
fn lightning_greaves_moves_between_creatures_and_the_old_one_loses_both() {
    let mut state = plains();
    let greaves = put(&mut state, P0, "Lightning Greaves", Zone::Battlefield);
    let first = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let second = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    activate(&mut state, greaves, 0, &[Target::Object(first)]);
    settled(&mut state);
    assert!(keyword(&state, first, Keywords::SHROUD));
    // Shroud stops the Equip ability itself from choosing the first creature
    // again, but the second one is a legal target.
    assert!(!ability_targets(&state, greaves, 0).contains(&Target::Object(first)));
    activate(&mut state, greaves, 0, &[Target::Object(second)]);
    settled(&mut state);
    assert!(!keyword(&state, first, Keywords::HASTE));
    assert!(!keyword(&state, first, Keywords::SHROUD));
    assert!(keyword(&state, second, Keywords::HASTE));
    assert!(keyword(&state, second, Keywords::SHROUD));
}

#[test]
fn lightning_greaves_shroud_stops_every_spell_and_ability_from_targeting() {
    let mut state = plains();
    let greaves = put(&mut state, P0, "Lightning Greaves", Zone::Battlefield);
    let shuko = put(&mut state, P0, "Shuko", Zone::Battlefield);
    let shrouded = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let plain = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    activate(&mut state, greaves, 0, &[Target::Object(shrouded)]);
    settled(&mut state);

    // The controller's own targeted abilities cannot choose it (Shuko's
    // Equip) while the other creature is a legal target.
    let equip_targets = ability_targets(&state, shuko, 0);
    assert!(!equip_targets.contains(&Target::Object(shrouded)));
    assert!(equip_targets.contains(&Target::Object(plain)));

    // Nor the controller's spells.
    let own_bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    let own = cast_targets(&state, own_bolt);
    assert!(!own.contains(&Target::Object(shrouded)));
    assert!(own.contains(&Target::Object(plain)));
    assert!(own.contains(&Target::Player(P1)));

    // Nor the opponent's.
    state.priority_player = P1;
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    let theirs = cast_targets(&state, bolt);
    assert!(!theirs.contains(&Target::Object(shrouded)));
    assert!(theirs.contains(&Target::Object(plain)));
    assert!(theirs.contains(&Target::Player(P0)));
}

#[test]
fn lightning_greaves_equips_only_as_a_sorcery() {
    let mut state = plains();
    let greaves = put(&mut state, P0, "Lightning Greaves", Zone::Battlefield);
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(activatable(&mut state, greaves, 0));
    opponent_bolts(&mut state, Target::Player(P0));
    assert!(!state.stack.is_empty());
    assert!(
        !activatable(&mut state, greaves, 0),
        "a spell is on the stack"
    );

    // The Equipment itself keeps no shroud.
    let mut state = plains();
    let greaves = put(&mut state, P0, "Lightning Greaves", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    activate(&mut state, greaves, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert!(!keyword(&state, greaves, Keywords::SHROUD));
    assert!(!keyword(&state, greaves, Keywords::HASTE));
}

// ---- Pyrite Spellbomb ----------------------------------------------------------------

#[test]
fn pyrite_spellbomb_deals_two_damage_to_any_target_and_is_sacrificed() {
    let mut state = plains();
    let bomb = put(&mut state, P0, "Pyrite Spellbomb", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, bomb, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bomb), Zone::Battlefield);

    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, bomb, 0, &[Target::Player(P1)]);
    assert_eq!(zone(&state, bomb), Zone::Graveyard, "sacrificed as a cost");
    assert_eq!(life(&state, P1), 20, "the ability waits on the stack");
    settled(&mut state);
    assert_eq!(life(&state, P1), 18);

    // A creature takes it too.
    let mut state = plains();
    let bomb = put(&mut state, P0, "Pyrite Spellbomb", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, bomb, 0, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(life(&state, P1), 20);

    // Damage to oneself is a legal target.
    let mut state = plains();
    let bomb = put(&mut state, P0, "Pyrite Spellbomb", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, bomb, 0, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 18);
}

#[test]
fn pyrite_spellbomb_draws_a_card_for_one_generic() {
    let mut state = plains();
    let bomb = put(&mut state, P0, "Pyrite Spellbomb", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, bomb, 0), "{{R}} needs red mana");
    assert!(activatable(&mut state, bomb, 1));
    activate(&mut state, bomb, 1, &[]);
    assert_eq!(zone(&state, bomb), Zone::Graveyard, "sacrificed as a cost");
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 1);
    assert_eq!(library_len(&state, P0), 19);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);

    // Red mana pays the generic cost too, and no mana pays for neither.
    let mut state = plains();
    let bomb = put(&mut state, P0, "Pyrite Spellbomb", Zone::Battlefield);
    assert!(!activatable(&mut state, bomb, 0));
    assert!(!activatable(&mut state, bomb, 1));
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(activatable(&mut state, bomb, 0));
    assert!(activatable(&mut state, bomb, 1));
}

#[test]
fn pyrite_spellbomb_with_a_vanished_target_still_costs_the_sacrifice() {
    let mut state = plains();
    let bomb = put(&mut state, P0, "Pyrite Spellbomb", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, bomb, 0, &[Target::Object(specter)]);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(specter, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, bomb), Zone::Graveyard);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);
    assert_eq!(life(&state, P1), 20, "the damage had no one to hit");
}

// ---- Shorikai, Genesis Engine --------------------------------------------------------

#[test]
fn shorikai_draws_two_discards_one_and_makes_a_pilot() {
    let mut state = plains();
    let shorikai = put(
        &mut state,
        P0,
        "Shorikai, Genesis Engine",
        Zone::Battlefield,
    );
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, shorikai, 0, &[]);
    assert!(is_tapped(&state, shorikai), "{{T}} is a cost");
    settle_discarding(&mut state, bolt, 0);
    assert_eq!(library_len(&state, P0), 18, "two cards drawn");
    assert_eq!(hand_len(&state, P0), 2, "three in hand, one discarded");
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    let pilots = named(&state, P0, "Pilot Token");
    assert_eq!(pilots.len(), 1);
    assert_eq!(power_toughness(&state, pilots[0]), (1, 1));
    assert!(state.objects.get(pilots[0]).v4.is_token);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert!(!activatable(&mut state, shorikai, 0), "it is tapped");
}

#[test]
fn shorikai_loot_needs_its_mana_and_works_with_an_empty_hand() {
    let mut state = plains();
    let shorikai = put(
        &mut state,
        P0,
        "Shorikai, Genesis Engine",
        Zone::Battlefield,
    );
    assert!(!activatable(&mut state, shorikai, 0), "{{1}} is unpaid");
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, shorikai, 0));

    // With nothing in hand, the two drawn cards leave one after the discard.
    activate(&mut state, shorikai, 0, &[]);
    settle_first(&mut state);
    assert_eq!(hand_len(&state, P0), 1);
    assert_eq!(state.players[0].graveyard.len(), 1);
    assert_eq!(named(&state, P0, "Pilot Token").len(), 1);
}

#[test]
fn shorikai_crews_with_total_power_eight_and_each_pilot_counts_as_three() {
    let mut state = plains();
    let shorikai = put(
        &mut state,
        P0,
        "Shorikai, Genesis Engine",
        Zone::Battlefield,
    );
    assert!(!engine::object_has_type(
        &state,
        shorikai,
        CardType::Creature
    ));
    let first = token(&mut state, P0, "Pilot Token");
    let second = token(&mut state, P0, "Pilot Token");
    assert!(!activatable(&mut state, shorikai, 1), "two Pilots are 6");
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    assert!(activatable(&mut state, shorikai, 1), "3 + 3 + 2");
    // A power-1 creature instead of the Pup is one short.
    let mut short = state.clone();
    put(&mut short, P0, "Llanowar Elves", Zone::Battlefield);
    short.objects.get_mut(pup).tapped = true;
    assert!(!activatable(&mut short, shorikai, 1), "3 + 3 + 1");

    // Crew 8 with the Pilots and the Pup.
    state.objects.get_mut(first).summoning_sick = true;
    crew(&mut state, shorikai, 1, &[first, second, pup]);
    assert!(is_tapped(&state, first));
    assert!(is_tapped(&state, second));
    assert!(is_tapped(&state, pup));
    assert!(engine::object_has_type(
        &state,
        shorikai,
        CardType::Creature
    ));
    assert!(engine::object_has_type(
        &state,
        shorikai,
        CardType::Artifact
    ));
    assert_eq!(power_toughness(&state, shorikai), (8, 8));
    assert_eq!(
        power_toughness(&state, first),
        (1, 1),
        "the bonus is for crewing only"
    );
}

#[test]
fn shorikai_three_pilots_crew_it_and_the_crewed_vehicle_is_summoning_sick() {
    let mut state = plains();
    let shorikai = put(
        &mut state,
        P0,
        "Shorikai, Genesis Engine",
        Zone::Battlefield,
    );
    state.objects.get_mut(shorikai).summoning_sick = true;
    let pilots: Vec<ObjectId> = (0..3)
        .map(|_| token(&mut state, P0, "Pilot Token"))
        .collect();
    pool(&mut state, P0, &[], 1);
    // Not a creature yet, so summoning sickness does not stop its {T} ability.
    assert!(activatable(&mut state, shorikai, 0));
    crew(&mut state, shorikai, 1, &pilots);
    assert!(engine::object_has_type(
        &state,
        shorikai,
        CardType::Creature
    ));
    // A creature that came under its controller's control this turn cannot
    // pay {T}.
    assert!(!activatable(&mut state, shorikai, 0));
}

// ---- The Echoverse Fulcrum -----------------------------------------------------------

#[test]
fn echoverse_fulcrum_loots_when_it_enters() {
    let mut state = plains();
    let fulcrum = put(&mut state, P0, "The Echoverse Fulcrum", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, fulcrum, &[]);
    settle_discarding(&mut state, bolt, 0);
    assert_eq!(zone(&state, fulcrum), Zone::Battlefield);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(library_len(&state, P0), 19, "one card drawn");
    assert_eq!(hand_len(&state, P0), 1, "the drawn card stays");
}

#[test]
fn echoverse_fulcrum_destroys_all_creatures_exiling_itself_as_a_sorcery() {
    let mut state = plains();
    let fulcrum = put(&mut state, P0, "The Echoverse Fulcrum", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let ent = put(&mut state, P1, "Generous Ent", Zone::Battlefield);
    let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
    // No other mana source may pay for it.
    tap(&mut state, stone);
    tap(&mut state, elves);
    pool(&mut state, P0, &[], 4);
    assert!(
        !activatable(&mut state, fulcrum, 0),
        "{{5}} needs five mana"
    );
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, fulcrum, 0));
    activate(&mut state, fulcrum, 0, &[]);
    assert_eq!(zone(&state, fulcrum), Zone::Exile, "exiled as a cost");
    assert_eq!(zone(&state, elves), Zone::Battlefield, "not yet");
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(zone(&state, ent), Zone::Graveyard);
    assert_eq!(zone(&state, stone), Zone::Battlefield, "only creatures");
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn echoverse_fulcrum_activates_only_as_a_sorcery_and_needs_to_be_untapped() {
    let mut state = plains();
    let fulcrum = put(&mut state, P0, "The Echoverse Fulcrum", Zone::Battlefield);
    pool(&mut state, P0, &[], 5);
    assert!(activatable(&mut state, fulcrum, 0));
    opponent_bolts(&mut state, Target::Player(P0));
    assert!(
        !activatable(&mut state, fulcrum, 0),
        "a spell is on the stack"
    );

    let mut state = plains();
    let fulcrum = put(&mut state, P0, "The Echoverse Fulcrum", Zone::Battlefield);
    tap(&mut state, fulcrum);
    pool(&mut state, P0, &[], 5);
    assert!(!activatable(&mut state, fulcrum, 0), "{{T}} is a cost");
}

// ---- Retrofitter Foundry -------------------------------------------------------------

#[test]
fn retrofitter_foundry_makes_a_servo() {
    let mut state = plains();
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    assert!(!activatable(&mut state, foundry, 1), "{{2}} needs two mana");
    pool(&mut state, P0, &[], 1);
    activate(&mut state, foundry, 1, &[]);
    assert!(is_tapped(&state, foundry));
    settled(&mut state);
    let servos = named(&state, P0, "Servo Token");
    assert_eq!(servos.len(), 1);
    assert_eq!(power_toughness(&state, servos[0]), (1, 1));
    assert!(engine::object_has_type(
        &state,
        servos[0],
        CardType::Artifact
    ));
    assert!(engine::object_has_type(
        &state,
        servos[0],
        CardType::Creature
    ));
    assert!(engine::has_effective_subtype(
        &state,
        servos[0],
        Subtype::Servo
    ));
    assert!(!keyword(&state, servos[0], Keywords::FLYING));
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert!(!activatable(&mut state, foundry, 1), "it is tapped");
}

#[test]
fn retrofitter_foundry_trades_a_servo_for_a_flying_thopter() {
    let mut state = plains();
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    assert!(
        !activatable(&mut state, foundry, 2),
        "no Servo to sacrifice"
    );
    let servo = token(&mut state, P0, "Servo Token");
    assert!(activatable(&mut state, foundry, 2));
    activate(&mut state, foundry, 2, &[]);
    assert!(!on_battlefield(&state, P0, servo), "sacrificed as a cost");
    assert!(is_tapped(&state, foundry));
    settled(&mut state);
    let thopters = named(&state, P0, "Thopter Token");
    assert_eq!(
        thopters.len(),
        1,
        "the sacrificed Servo is not the new token"
    );
    assert!(named(&state, P0, "Servo Token").is_empty());
    assert_eq!(power_toughness(&state, thopters[0]), (1, 1));
    assert!(keyword(&state, thopters[0], Keywords::FLYING));
    assert!(engine::object_has_type(
        &state,
        thopters[0],
        CardType::Artifact
    ));
    assert!(engine::has_effective_subtype(
        &state,
        thopters[0],
        Subtype::Thopter
    ));
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn retrofitter_foundry_sacrifices_only_a_servo_for_a_thopter() {
    let mut state = plains();
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    let first = token(&mut state, P0, "Servo Token");
    let second = token(&mut state, P0, "Servo Token");
    let thopter = token(&mut state, P0, "Thopter Token");
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    act(&mut state, Action::ActivateAbility(foundry, 2));
    match next(&mut state) {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&first));
            assert!(candidates.contains(&second));
            assert!(!candidates.contains(&thopter));
            assert!(!candidates.contains(&elves));
            assert!(!candidates.contains(&foundry));
        }
        other => panic!("expected the sacrifice, got {other:?}"),
    }
    act(&mut state, Action::ChooseCostTarget(second));
    settled(&mut state);
    assert!(on_battlefield(&state, P0, first));
    assert!(!on_battlefield(&state, P0, second));
    assert_eq!(named(&state, P0, "Thopter Token").len(), 2);
}

#[test]
fn retrofitter_foundry_trades_a_thopter_for_a_four_four_construct() {
    let mut state = plains();
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    assert!(
        !activatable(&mut state, foundry, 3),
        "no Thopter to sacrifice"
    );
    let servo = token(&mut state, P0, "Servo Token");
    assert!(
        !activatable(&mut state, foundry, 3),
        "a Servo is not a Thopter"
    );
    let thopter = token(&mut state, P0, "Thopter Token");
    assert!(
        activatable(&mut state, foundry, 3),
        "{{T}} and the Thopter only"
    );
    activate(&mut state, foundry, 3, &[]);
    assert!(!on_battlefield(&state, P0, thopter));
    assert!(is_tapped(&state, foundry));
    settled(&mut state);
    assert!(on_battlefield(&state, P0, servo));
    let constructs = named(&state, P0, "Construct Token");
    assert_eq!(constructs.len(), 1);
    assert_eq!(power_toughness(&state, constructs[0]), (4, 4));
    assert!(engine::object_has_type(
        &state,
        constructs[0],
        CardType::Artifact
    ));
    assert!(engine::object_has_type(
        &state,
        constructs[0],
        CardType::Creature
    ));
    assert!(engine::has_effective_subtype(
        &state,
        constructs[0],
        Subtype::Construct
    ));
    assert!(!keyword(&state, constructs[0], Keywords::FLYING));

    // Thopter Foundry's blue Thopter is a Thopter too.
    let mut state = plains();
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    let blue = token(&mut state, P0, "Blue Thopter Token");
    activate(&mut state, foundry, 3, &[]);
    settled(&mut state);
    assert!(!on_battlefield(&state, P0, blue));
    assert_eq!(named(&state, P0, "Construct Token").len(), 1);
}

#[test]
fn retrofitter_foundry_untaps_for_three_even_in_response_to_its_own_ability() {
    let mut state = plains();
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    tap(&mut state, foundry);
    pool(&mut state, P0, &[], 2);
    assert!(
        !activatable(&mut state, foundry, 0),
        "{{3}} needs three mana"
    );
    pool(&mut state, P0, &[], 1);
    activate(&mut state, foundry, 0, &[]);
    settled(&mut state);
    assert!(!is_tapped(&state, foundry));
    assert_eq!(mana(&state, P0, ManaColor::C), 0);

    // Tap it for a Servo, untap it in response, then use it again.
    let mut state = plains();
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    pool(&mut state, P0, &[], 5);
    activate(&mut state, foundry, 1, &[]);
    assert!(is_tapped(&state, foundry));
    assert!(activatable(&mut state, foundry, 0), "no {{T}} in its cost");
    assert!(!activatable(&mut state, foundry, 1), "it is tapped");
    activate(&mut state, foundry, 0, &[]);
    settled(&mut state);
    assert!(!is_tapped(&state, foundry));
    assert_eq!(named(&state, P0, "Servo Token").len(), 1);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, foundry, 1, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Servo Token").len(), 2);
}

// ---- Thopter Foundry -----------------------------------------------------------------

#[test]
fn thopter_foundry_casts_for_white_or_black_and_blue() {
    for hybrid in [ManaColor::W, ManaColor::B] {
        let mut state = plains();
        let foundry = put(&mut state, P0, "Thopter Foundry", Zone::Hand);
        pool(&mut state, P0, &[hybrid, ManaColor::U], 0);
        cast(&mut state, foundry, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, foundry), Zone::Battlefield, "{hybrid:?}");
    }
    let mut state = plains();
    let foundry = put(&mut state, P0, "Thopter Foundry", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::U], 0);
    assert!(
        !castable(&mut state, foundry),
        "red is neither white nor black"
    );
    let mut state = plains();
    let foundry = put(&mut state, P0, "Thopter Foundry", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(!castable(&mut state, foundry), "the blue pip needs blue");
}

#[test]
fn thopter_foundry_sacrifices_a_nontoken_artifact_for_a_thopter_and_a_life() {
    let mut state = plains();
    let foundry = put(&mut state, P0, "Thopter Foundry", Zone::Battlefield);
    let stone = put(&mut state, P0, "Mind Stone", Zone::Battlefield);
    let treasure = token(&mut state, P0, "Treasure Token");
    let servo = token(&mut state, P0, "Servo Token");
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    act(&mut state, Action::ActivateAbility(foundry, 0));
    match next(&mut state) {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&stone));
            assert!(
                candidates.contains(&foundry),
                "the Foundry is a nontoken artifact"
            );
            assert!(!candidates.contains(&treasure), "a token");
            assert!(!candidates.contains(&servo), "an artifact token");
            assert!(!candidates.contains(&elves), "not an artifact");
        }
        other => panic!("expected the sacrifice, got {other:?}"),
    }
    act(&mut state, Action::ChooseCostTarget(stone));
    // The activation is finished (its costs paid) as the game next advances.
    next(&mut state);
    assert_eq!(zone(&state, stone), Zone::Graveyard, "sacrificed as a cost");
    settled(&mut state);
    assert_eq!(life(&state, P0), 21);
    let thopters = named(&state, P0, "Blue Thopter Token");
    assert_eq!(thopters.len(), 1);
    assert_eq!(power_toughness(&state, thopters[0]), (1, 1));
    assert!(keyword(&state, thopters[0], Keywords::FLYING));
    assert!(engine::object_has_type(
        &state,
        thopters[0],
        CardType::Artifact
    ));
    assert!(engine::object_has_type(
        &state,
        thopters[0],
        CardType::Creature
    ));
    assert!(engine::has_effective_subtype(
        &state,
        thopters[0],
        Subtype::Thopter
    ));
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(zone(&state, foundry), Zone::Battlefield);
}

#[test]
fn thopter_foundry_may_sacrifice_itself_and_a_token_never_pays() {
    let mut state = plains();
    let foundry = put(&mut state, P0, "Thopter Foundry", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    // The Foundry is the only nontoken artifact, so it is chosen for the cost.
    activate(&mut state, foundry, 0, &[]);
    assert_eq!(zone(&state, foundry), Zone::Graveyard, "sacrificed itself");
    settled(&mut state);
    assert_eq!(named(&state, P0, "Blue Thopter Token").len(), 1);
    assert_eq!(life(&state, P0), 21);

    // Artifact tokens alone give nothing to sacrifice but the Foundry.
    let mut state = plains();
    let foundry = put(&mut state, P0, "Thopter Foundry", Zone::Battlefield);
    let thopter = token(&mut state, P0, "Thopter Token");
    pool(&mut state, P0, &[], 1);
    activate(&mut state, foundry, 0, &[]);
    settled(&mut state);
    assert!(on_battlefield(&state, P0, thopter), "the token stays");
    assert_eq!(zone(&state, foundry), Zone::Graveyard);

    // The {1} must be paid.
    let mut state = plains();
    let foundry = put(&mut state, P0, "Thopter Foundry", Zone::Battlefield);
    assert!(!activatable(&mut state, foundry, 0));
}

// ---- Relic of Sauron -----------------------------------------------------------------

#[test]
fn relic_of_sauron_taps_for_two_mana_of_one_color() {
    for color in [ManaColor::U, ManaColor::B, ManaColor::R] {
        let mut state = plains();
        let relic = put(&mut state, P0, "Relic of Sauron", Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(relic, color));
        assert_eq!(mana(&state, P0, color), 2, "{color:?}");
        for other in [ManaColor::W, ManaColor::G, ManaColor::C] {
            assert_eq!(mana(&state, P0, other), 0);
        }
        assert!(is_tapped(&state, relic));
        assert!(
            state.stack.is_empty(),
            "a mana ability never uses the stack"
        );
        assert!(
            engine::step(&mut state, Action::ActivateManaAbilityChoice(relic, color)).is_err(),
            "it is tapped"
        );
    }
}

#[test]
fn relic_of_sauron_taps_for_each_mixed_pair() {
    // Each mixed pair is its own bundle, named by the color its action
    // carries that the pair does not make.
    for (key, pair) in [
        (ManaColor::W, [ManaColor::U, ManaColor::B]),
        (ManaColor::G, [ManaColor::U, ManaColor::R]),
        (ManaColor::C, [ManaColor::B, ManaColor::R]),
    ] {
        let mut state = plains();
        let relic = put(&mut state, P0, "Relic of Sauron", Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbilityChoice(relic, key));
        for color in [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
            ManaColor::C,
        ] {
            let expected = u8::from(pair.contains(&color));
            assert_eq!(mana(&state, P0, color), expected, "{key:?} makes {color:?}");
        }
        assert!(is_tapped(&state, relic));
        assert!(state.stack.is_empty());
    }
}

#[test]
fn relic_of_sauron_mana_pays_for_spells_and_loots_for_three() {
    // RR pays a Bolt and leaves the second red mana floating.
    let mut state = plains();
    let relic = put(&mut state, P0, "Relic of Sauron", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(relic, ManaColor::R),
    );
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(
        mana(&state, P0, ManaColor::R),
        1,
        "the other red mana floats"
    );

    // {3}, {T}: draw two, discard one.
    let mut state = plains();
    let relic = put(&mut state, P0, "Relic of Sauron", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    assert!(!activatable(&mut state, relic, 0), "{{3}} needs three mana");
    pool(&mut state, P0, &[], 1);
    activate(&mut state, relic, 0, &[]);
    assert!(is_tapped(&state, relic));
    settle_discarding(&mut state, bolt, 0);
    assert_eq!(library_len(&state, P0), 18);
    assert_eq!(hand_len(&state, P0), 2, "three in hand, one discarded");
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

// ---- Currency Converter --------------------------------------------------------------

/// P0 controls a Currency Converter and holds `cards` (in this order) in
/// hand; the library is plains.
fn converter_game(cards: &[&str]) -> (GameState, ObjectId, Vec<ObjectId>) {
    let mut state = plains();
    let converter = put(&mut state, P0, "Currency Converter", Zone::Battlefield);
    let hand = cards
        .iter()
        .map(|&name| put(&mut state, P0, name, Zone::Hand))
        .collect();
    (state, converter, hand)
}

/// Loots with the Converter, discarding `card`, and answers its "may exile"
/// with `option` (0 exiles, 1 declines).
fn loot_discarding(state: &mut GameState, converter: ObjectId, card: ObjectId, option: u16) {
    pool(state, P0, &[], 2);
    activate(state, converter, 0, &[]);
    settle_discarding(state, card, option);
}

#[test]
fn currency_converter_exiles_a_discarded_card_and_puts_a_land_back_for_a_treasure() {
    let (mut state, converter, hand) = converter_game(&["Plains"]);
    let plains_card = hand[0];
    loot_discarding(&mut state, converter, plains_card, 0);
    assert_eq!(zone(&state, plains_card), Zone::Exile);
    assert!(is_tapped(&state, converter), "the loot taps it");
    assert_eq!(library_len(&state, P0), 19, "the loot drew a card");
    assert_eq!(hand_len(&state, P0), 1);
    assert!(state.stack.is_empty());

    // "{T}: Put a card exiled with this artifact into its owner's graveyard.
    // If it's a land card, create a Treasure token."
    untap(&mut state, converter);
    activate(&mut state, converter, 1, &[]);
    assert!(is_tapped(&state, converter));
    settled(&mut state);
    assert_eq!(zone(&state, plains_card), Zone::Graveyard);
    assert_eq!(named(&state, P0, "Treasure Token").len(), 1);
    assert!(named(&state, P0, "Rogue Token").is_empty());

    // Nothing is left exiled with it.
    untap(&mut state, converter);
    activate(&mut state, converter, 1, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Treasure Token").len(), 1);
    assert!(named(&state, P0, "Rogue Token").is_empty());
}

#[test]
fn currency_converter_puts_a_nonland_card_back_for_a_two_two_rogue() {
    let (mut state, converter, hand) = converter_game(&["Lightning Bolt"]);
    let bolt = hand[0];
    loot_discarding(&mut state, converter, bolt, 0);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    untap(&mut state, converter);
    activate(&mut state, converter, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "its owner's graveyard");
    let rogues = named(&state, P0, "Rogue Token");
    assert_eq!(rogues.len(), 1);
    assert_eq!(power_toughness(&state, rogues[0]), (2, 2));
    assert!(engine::has_effective_subtype(
        &state,
        rogues[0],
        Subtype::Rogue
    ));
    assert!(named(&state, P0, "Treasure Token").is_empty());
}

#[test]
fn currency_converter_may_decline_to_exile() {
    let (mut state, converter, hand) = converter_game(&["Lightning Bolt"]);
    let bolt = hand[0];
    loot_discarding(&mut state, converter, bolt, 1);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "declined");
    untap(&mut state, converter);
    activate(&mut state, converter, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert!(named(&state, P0, "Rogue Token").is_empty());
    assert!(named(&state, P0, "Treasure Token").is_empty());
}

#[test]
fn currency_converter_lets_you_choose_among_the_cards_exiled_with_it() {
    let (mut state, converter, hand) = converter_game(&["Lightning Bolt", "Plains"]);
    let (bolt, land) = (hand[0], hand[1]);
    loot_discarding(&mut state, converter, bolt, 0);
    untap(&mut state, converter);
    loot_discarding(&mut state, converter, land, 0);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(zone(&state, land), Zone::Exile);

    // Two cards are linked: the choice lists them in the order exiled, and
    // choosing the land makes a Treasure.
    untap(&mut state, converter);
    activate(&mut state, converter, 1, &[]);
    let mut asked = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 2);
            asked += 1;
            Action::ChooseEffectOption(1)
        }
        other => first_answer(other),
    });
    assert_eq!(asked, 1);
    assert_eq!(zone(&state, land), Zone::Graveyard);
    assert_eq!(zone(&state, bolt), Zone::Exile, "the other stays exiled");
    assert_eq!(named(&state, P0, "Treasure Token").len(), 1);

    // The Bolt is still linked: the next activation takes it unasked.
    untap(&mut state, converter);
    activate(&mut state, converter, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(named(&state, P0, "Rogue Token").len(), 1);
}

#[test]
fn currency_converter_links_cards_to_the_converter_that_exiled_them() {
    let (mut state, converter, hand) = converter_game(&["Lightning Bolt"]);
    let bolt = hand[0];
    loot_discarding(&mut state, converter, bolt, 0);
    let second = put(&mut state, P0, "Currency Converter", Zone::Battlefield);
    activate(&mut state, second, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Exile, "not this Converter's card");
    assert!(named(&state, P0, "Rogue Token").is_empty());
    untap(&mut state, converter);
    activate(&mut state, converter, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(named(&state, P0, "Rogue Token").len(), 1);
}

#[test]
fn currency_converter_triggers_for_cycling_and_not_for_the_opponents_discards() {
    // Cycling discards the card.
    let (mut state, _converter, hand) = converter_game(&["Hollow One"]);
    let hollow = hand[0];
    pool(&mut state, P0, &[], 2);
    activate(&mut state, hollow, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, hollow), Zone::Exile);

    // A Converter's controller is the only one who triggers it: P1's
    // Converter ignores P0's discard.
    let mut state = plains();
    put(&mut state, P1, "Currency Converter", Zone::Battlefield);
    let relic = put(&mut state, P0, "Relic of Sauron", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    activate(&mut state, relic, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::Discard { .. } => Action::Discard(vec![bolt]),
        Decision::ChooseEffectOption { .. } => panic!("P1's Converter must not trigger"),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
}

#[test]
fn currency_converter_triggers_once_per_converter_for_a_discard() {
    let (mut state, converter, hand) = converter_game(&["Lightning Bolt"]);
    let bolt = hand[0];
    let second = put(&mut state, P0, "Currency Converter", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, converter, 0, &[]);
    let mut asked = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::Discard { .. } => Action::Discard(vec![bolt]),
        Decision::ChooseEffectOption { .. } => {
            asked += 1;
            Action::ChooseEffectOption(0)
        }
        other => first_answer(other),
    });
    // Both triggered, but the card could be exiled only once.
    assert_eq!(asked, 1);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    // It is linked to exactly one of the two Converters.
    untap(&mut state, converter);
    activate(&mut state, converter, 1, &[]);
    activate(&mut state, second, 1, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(named(&state, P0, "Rogue Token").len(), 1);
}

// ---- Staff of the Storyteller --------------------------------------------------------

#[test]
fn staff_of_the_storyteller_makes_a_spirit_and_counts_its_own_token() {
    let mut state = plains();
    let staff = put(&mut state, P0, "Staff of the Storyteller", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, staff, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, staff), Zone::Battlefield);
    let spirits = named(&state, P0, "White Flying Spirit Token");
    assert_eq!(spirits.len(), 1);
    assert_eq!(power_toughness(&state, spirits[0]), (1, 1));
    assert!(keyword(&state, spirits[0], Keywords::FLYING));
    assert_eq!(
        charge(&state, staff),
        1,
        "its own Spirit is a creature token"
    );
}

#[test]
fn staff_of_the_storyteller_counts_each_creature_token_event_once() {
    // One event creating two tokens is one counter.
    let mut state = plains();
    let staff = put(
        &mut state,
        P0,
        "Staff of the Storyteller",
        Zone::Battlefield,
    );
    let souls = put(&mut state, P0, "Lingering Souls", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, souls, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "White Flying Spirit Token").len(), 2);
    assert_eq!(charge(&state, staff), 1);

    // A Servo from a Foundry is another event.
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, foundry, 1, &[]);
    settled(&mut state);
    assert_eq!(charge(&state, staff), 2);
    assert_eq!(named(&state, P0, "Servo Token").len(), 1);
}

#[test]
fn staff_of_the_storyteller_ignores_noncreature_tokens_and_opponents_tokens() {
    // A Clue is not a creature token.
    let mut state = plains();
    let staff = put(
        &mut state,
        P0,
        "Staff of the Storyteller",
        Zone::Battlefield,
    );
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Clue Token").len(), 1);
    assert_eq!(charge(&state, staff), 0);

    // The opponent's creature tokens are not "you create".
    let mut state = plains();
    let staff = put(
        &mut state,
        P0,
        "Staff of the Storyteller",
        Zone::Battlefield,
    );
    let foundry = put(&mut state, P1, "Retrofitter Foundry", Zone::Battlefield);
    state.priority_player = P1;
    pool(&mut state, P1, &[], 2);
    activate(&mut state, foundry, 1, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P1, "Servo Token").len(), 1);
    assert_eq!(charge(&state, staff), 0);

    // Without a Staff nothing is counted, and a second Staff counts on its own.
    let mut state = plains();
    let first = put(
        &mut state,
        P0,
        "Staff of the Storyteller",
        Zone::Battlefield,
    );
    let second = put(
        &mut state,
        P0,
        "Staff of the Storyteller",
        Zone::Battlefield,
    );
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, foundry, 1, &[]);
    settled(&mut state);
    assert_eq!(charge(&state, first), 1);
    assert_eq!(charge(&state, second), 1);
}

#[test]
fn staff_of_the_storyteller_removes_a_counter_to_draw() {
    let mut state = plains();
    let staff = put(
        &mut state,
        P0,
        "Staff of the Storyteller",
        Zone::Battlefield,
    );
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(!activatable(&mut state, staff, 0), "no story counter");
    state.objects.get_mut(staff).counters.charge = 2;
    assert!(activatable(&mut state, staff, 0));
    activate(&mut state, staff, 0, &[]);
    assert_eq!(charge(&state, staff), 1, "the counter is a cost");
    assert!(is_tapped(&state, staff));
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 1);
    assert_eq!(library_len(&state, P0), 19);
    assert_eq!(mana(&state, P0, ManaColor::W), 0);
    assert!(!activatable(&mut state, staff, 0), "it is tapped");

    // {W} specifically.
    let mut state = plains();
    let staff = put(
        &mut state,
        P0,
        "Staff of the Storyteller",
        Zone::Battlefield,
    );
    state.objects.get_mut(staff).counters.charge = 1;
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!activatable(&mut state, staff, 0), "red is not white");
}

// ---- The Mightstone and Weakstone ----------------------------------------------------

/// Settles, choosing trigger mode `mode` and, when asked, `victim` as the
/// target.
fn settle_mode(state: &mut GameState, mode: u8, victim: Option<ObjectId>) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseTriggerMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&mode), "{mode} not in {legal_modes:?}");
            Action::ChooseTriggerMode(mode)
        }
        Decision::ChooseTargets { legal_targets, .. } => {
            let victim = victim.expect("no target was expected");
            assert!(legal_targets.contains(&Target::Object(victim)));
            Action::ChooseTarget(Target::Object(victim))
        }
        other => first_answer(other),
    });
}

#[test]
fn mightstone_and_weakstone_draws_two_cards_when_that_mode_is_chosen() {
    let mut state = plains();
    let stone = put(&mut state, P0, "The Mightstone and Weakstone", Zone::Hand);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, stone, &[]);
    settle_mode(&mut state, 0, None);
    assert_eq!(zone(&state, stone), Zone::Battlefield);
    assert_eq!(hand_len(&state, P0), 2);
    assert_eq!(library_len(&state, P0), 18);
    assert_eq!(
        zone(&state, elves),
        Zone::Battlefield,
        "the other mode did not run"
    );
    assert_eq!(power_toughness(&state, elves), (1, 1));
}

#[test]
fn mightstone_and_weakstone_shrinks_a_target_creature_by_five_until_end_of_turn() {
    // A small creature dies.
    let mut state = plains();
    let stone = put(&mut state, P0, "The Mightstone and Weakstone", Zone::Hand);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, stone, &[]);
    settle_mode(&mut state, 1, Some(specter));
    assert_eq!(zone(&state, specter), Zone::Graveyard);
    assert_eq!(hand_len(&state, P0), 0, "no cards were drawn");

    // A big one just shrinks (and so may a creature of its own controller).
    let mut state = plains();
    let stone = put(&mut state, P0, "The Mightstone and Weakstone", Zone::Hand);
    let ent = put(&mut state, P0, "Generous Ent", Zone::Battlefield);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, stone, &[]);
    settle_mode(&mut state, 1, Some(ent));
    assert_eq!(zone(&state, ent), Zone::Battlefield);
    assert_eq!(power_toughness(&state, ent), (0, 2));
}

#[test]
fn mightstone_and_weakstone_without_a_creature_can_only_draw() {
    let mut state = plains();
    let stone = put(&mut state, P0, "The Mightstone and Weakstone", Zone::Hand);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, stone, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTriggerMode { legal_modes, .. } => {
            assert_eq!(legal_modes, vec![0], "no creature to shrink");
            Action::ChooseTriggerMode(0)
        }
        other => first_answer(other),
    });
    assert_eq!(hand_len(&state, P0), 2);
}

#[test]
fn mightstone_and_weakstone_shrink_fizzles_when_its_target_is_gone() {
    let mut state = plains();
    let stone = put(&mut state, P0, "The Mightstone and Weakstone", Zone::Hand);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let ent = put(&mut state, P1, "Generous Ent", Zone::Battlefield);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, stone, &[]);
    run_until(
        &mut state,
        |state| {
            state
                .stack
                .iter()
                .any(|item| item.source == stone && item.kind == StackItemKind::TriggeredAbility)
        },
        |_, decision| match decision {
            Decision::ChooseTriggerMode { .. } => Some(Action::ChooseTriggerMode(1)),
            Decision::ChooseTargets { .. } => Some(Action::ChooseTarget(Target::Object(specter))),
            _ => None,
        },
    );
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(specter, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, stone), Zone::Battlefield);
    assert_eq!(
        power_toughness(&state, ent),
        (5, 7),
        "no other target was hit"
    );
    assert_eq!(hand_len(&state, P0), 0);
}

#[test]
fn mightstone_and_weakstone_mana_pays_for_artifact_spells_and_abilities_only() {
    // {T}: Add {C}{C}, held as two restricted units, which pay for an
    // artifact spell.
    let mut state = plains();
    let stone = put(
        &mut state,
        P0,
        "The Mightstone and Weakstone",
        Zone::Battlefield,
    );
    let mind_stone = put(&mut state, P0, "Mind Stone", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(stone));
    assert!(is_tapped(&state, stone));
    assert!(
        state.stack.is_empty(),
        "a mana ability never uses the stack"
    );
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(state.players[0].restricted_mana_pool.0.len(), 2);
    assert!(castable(&mut state, mind_stone));
    cast(&mut state, mind_stone, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mind_stone), Zone::Battlefield);
    assert!(state.players[0].restricted_mana_pool.0.is_empty());

    // They cannot pay the generic part of a nonartifact spell, though black
    // mana pays its colored part.
    let mut state = plains();
    let stone = put(
        &mut state,
        P0,
        "The Mightstone and Weakstone",
        Zone::Battlefield,
    );
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 0);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(stone));
    assert_eq!(state.players[0].restricted_mana_pool.0.len(), 2);
    assert!(
        !castable(&mut state, specter),
        "{{1}}{{B}}{{B}} is a nonartifact spell"
    );
}

#[test]
fn mightstone_and_weakstone_mana_does_not_pay_a_nonartifact_spell_even_with_other_mana() {
    let mut state = plains();
    let stone = put(
        &mut state,
        P0,
        "The Mightstone and Weakstone",
        Zone::Battlefield,
    );
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 1);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(stone));
    // The floating {C} pays the generic part; the restricted units stay.
    assert!(castable(&mut state, specter));
    cast(&mut state, specter, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, specter), Zone::Battlefield);
    assert_eq!(state.players[0].restricted_mana_pool.0.len(), 2);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn mightstone_and_weakstone_mana_pays_for_any_activated_ability() {
    let mut state = plains();
    let stone = put(
        &mut state,
        P0,
        "The Mightstone and Weakstone",
        Zone::Battlefield,
    );
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    assert!(!activatable(&mut state, foundry, 1), "no mana yet");
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(stone));
    assert!(
        activatable(&mut state, foundry, 1),
        "{{2}} from the restricted units"
    );
    activate(&mut state, foundry, 1, &[]);
    assert!(
        state.players[0].restricted_mana_pool.0.is_empty(),
        "spent on the ability"
    );
    settled(&mut state);
    assert_eq!(named(&state, P0, "Servo Token").len(), 1);
}

// ---- Public sessions -----------------------------------------------------------------

#[test]
fn c3c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let white_blue = deck(&[
        ("Plains", 8),
        ("Island", 7),
        ("Mind Stone", 2),
        ("Sacred Cat", 3),
        ("Harrier Strix", 3),
        ("Shorikai, Genesis Engine", 3),
        ("Thopter Foundry", 3),
        ("Staff of the Storyteller", 3),
        ("Lightning Greaves", 2),
        ("Retrofitter Foundry", 2),
        ("Currency Converter", 2),
        ("The Echoverse Fulcrum", 2),
        ("The Mightstone and Weakstone", 3),
        ("Relic of Sauron", 2),
        ("Pyrite Spellbomb", 2),
    ]);
    let black_red = deck(&[
        ("Swamp", 6),
        ("Mountain", 6),
        ("Island", 4),
        ("Mind Stone", 2),
        ("Hypnotic Specter", 3),
        ("Jackal Pup", 3),
        ("Lightning Bolt", 3),
        ("Pyrite Spellbomb", 3),
        ("Relic of Sauron", 3),
        ("The Mightstone and Weakstone", 3),
        ("Currency Converter", 2),
        ("Lightning Greaves", 2),
        ("The Echoverse Fulcrum", 2),
        ("Retrofitter Foundry", 2),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    for (pair, first_seed) in [
        ([white_blue.clone(), black_red.clone()], 0x4333_4300_u64),
        ([black_red, white_blue], 0x4333_4310),
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
    for name in C3C_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
