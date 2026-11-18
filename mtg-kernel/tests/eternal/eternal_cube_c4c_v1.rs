//! Vintage Cube wave C4c: native rules and public-session tests for the
//! Equipment and Vehicles.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{in_parallel, play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C4C_CARDS: [&str; 10] = [
    "Skullclamp",
    "Sword of the Meek",
    "Umezawa's Jitte",
    "Nettlecyst",
    "Kaldra Compleat",
    "Glimmer Lens",
    "Iron Man Armor",
    "Astrologian's Planisphere",
    "Smuggler's Copter",
    "Esika's Chariot",
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
        0x4355_4245_0000 + 0x3463,
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
fn c4c_cards_are_full() {
    for name in C4C_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
}

#[test]
fn c4c_tokens_are_full() {
    for name in ["Phyrexian Germ Token", "Rebel Token", "Green Cat Token"] {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_token, "{name}");
    }
}

#[test]
fn c4c_equipment_have_their_printed_characteristics() {
    for (name, generic, pips, mana_value, abilities, legendary) in [
        ("Skullclamp", 1u8, 0usize, 1u16, 1usize, false),
        ("Sword of the Meek", 2, 0, 2, 1, false),
        ("Umezawa's Jitte", 2, 0, 2, 4, true),
        ("Nettlecyst", 3, 0, 3, 1, false),
        ("Kaldra Compleat", 7, 0, 7, 1, true),
        ("Glimmer Lens", 1, 1, 2, 1, false),
        ("Iron Man Armor", 3, 0, 3, 2, false),
        ("Astrologian's Planisphere", 1, 1, 2, 1, false),
    ] {
        let card = def(name);
        assert_eq!(card.types, &[CardType::Artifact], "{name}");
        assert_eq!(card.subtypes, &[Subtype::Equipment], "{name}");
        assert_eq!(
            (card.cost.generic, card.cost.pips.len()),
            (generic, pips),
            "{name}"
        );
        assert_eq!(card.mana_value, mana_value, "{name}");
        assert_eq!(card.activated_abilities.len(), abilities, "{name}");
        assert_eq!(card.supertypes.contains(&Supertype::Legendary), legendary);
        assert!(!card.is_land && card.power.is_none(), "{name}");
        assert!(card.equipment.is_some(), "{name}");
    }
    assert_eq!(def("Glimmer Lens").colors, &[ManaColor::W]);
    assert_eq!(def("Astrologian's Planisphere").colors, &[ManaColor::U]);
    for name in [
        "Skullclamp",
        "Sword of the Meek",
        "Umezawa's Jitte",
        "Nettlecyst",
        "Kaldra Compleat",
        "Iron Man Armor",
    ] {
        assert!(def(name).colors.is_empty(), "{name}");
    }

    let deltas = |name: &str| {
        let equipment = def(name).equipment.expect("an Equipment profile");
        (equipment.power_delta, equipment.toughness_delta)
    };
    assert_eq!(deltas("Skullclamp"), (1, -1));
    assert_eq!(deltas("Sword of the Meek"), (1, 2));
    assert_eq!(deltas("Umezawa's Jitte"), (0, 0));
    assert_eq!(deltas("Nettlecyst"), (0, 0));
    assert_eq!(deltas("Kaldra Compleat"), (5, 5));
    assert_eq!(deltas("Glimmer Lens"), (0, 0));
    assert_eq!(deltas("Iron Man Armor"), (2, 1));
    assert_eq!(deltas("Astrologian's Planisphere"), (0, 0));

    let kaldra = def("Kaldra Compleat");
    assert!(
        kaldra.keywords.has(Keywords::INDESTRUCTIBLE),
        "the Equipment"
    );
    let granted = kaldra.equipment.unwrap();
    for keywords in [
        granted.controller_turn_keywords,
        granted.other_turn_keywords,
    ] {
        for keyword in [
            Keywords::FIRST_STRIKE,
            Keywords::TRAMPLE,
            Keywords::INDESTRUCTIBLE,
            Keywords::HASTE,
        ] {
            assert!(keywords.has(keyword));
        }
        assert!(!keywords.has(Keywords::FLYING));
    }
    let armor = def("Iron Man Armor").equipment.unwrap();
    assert!(armor.controller_turn_keywords.has(Keywords::FLYING));
    assert!(armor.other_turn_keywords.has(Keywords::FLYING));
    assert_eq!(
        def("Astrologian's Planisphere")
            .equipment
            .unwrap()
            .add_subtype,
        Some(Subtype::Wizard)
    );
    for name in [
        "Skullclamp",
        "Sword of the Meek",
        "Umezawa's Jitte",
        "Nettlecyst",
        "Glimmer Lens",
        "Astrologian's Planisphere",
    ] {
        let equipment = def(name).equipment.unwrap();
        assert!(
            equipment.controller_turn_keywords == Keywords::NONE
                && equipment.other_turn_keywords == Keywords::NONE,
            "{name}"
        );
    }

    // Iron Man Armor's {2}: a 0/0 Construct Hero artifact creature with flying
    // until end of turn.
    let animation = def("Iron Man Armor").animation.expect("an animation");
    assert!(animation.until_end_of_turn && animation.artifact);
    assert_eq!((animation.power, animation.toughness), (0, 0));
    assert_eq!(animation.subtypes, &[Subtype::Construct, Subtype::Hero]);
    assert!(animation.keywords.has(Keywords::FLYING));
    assert!(animation.colors.is_empty());
}

#[test]
fn c4c_vehicles_have_their_printed_characteristics() {
    let copter = def("Smuggler's Copter");
    assert_eq!(copter.types, &[CardType::Artifact]);
    assert_eq!(copter.subtypes, &[Subtype::Vehicle]);
    assert!(copter.supertypes.is_empty());
    assert_eq!((copter.cost.generic, copter.cost.pips.len()), (2, 0));
    assert_eq!(copter.mana_value, 2);
    assert!(copter.colors.is_empty());
    assert_eq!((copter.power, copter.toughness), (Some(3), Some(3)));
    assert!(copter.keywords.has(Keywords::FLYING));
    assert_eq!(copter.activated_abilities.len(), 1, "Crew 1");

    let chariot = def("Esika's Chariot");
    assert_eq!(chariot.types, &[CardType::Artifact]);
    assert_eq!(chariot.subtypes, &[Subtype::Vehicle]);
    assert_eq!(chariot.supertypes, &[Supertype::Legendary]);
    assert_eq!((chariot.cost.generic, chariot.cost.pips.len()), (3, 1));
    assert_eq!(chariot.mana_value, 4);
    assert_eq!(chariot.colors, &[ManaColor::G]);
    assert_eq!((chariot.power, chariot.toughness), (Some(4), Some(4)));
    assert!(!chariot.keywords.has(Keywords::FLYING));
    assert_eq!(chariot.activated_abilities.len(), 1, "Crew 4");

    // Their printed triggered abilities.
    let trigger_count = |name: &str| mtg_kernel::trigger::triggers_for(id(name)).len();
    assert_eq!(trigger_count("Smuggler's Copter"), 1);
    assert_eq!(trigger_count("Esika's Chariot"), 2);
    assert_eq!(trigger_count("Nettlecyst"), 1, "living weapon");
    assert_eq!(trigger_count("Kaldra Compleat"), 1, "living weapon");
    assert_eq!(trigger_count("Glimmer Lens"), 1, "For Mirrodin!");
    assert_eq!(trigger_count("Iron Man Armor"), 1);
}

#[test]
fn c4c_tokens_have_their_printed_characteristics() {
    for (name, color, subtypes, power) in [
        (
            "Phyrexian Germ Token",
            ManaColor::B,
            &[Subtype::Phyrexian, Subtype::Germ][..],
            0,
        ),
        ("Rebel Token", ManaColor::R, &[Subtype::Rebel][..], 2),
        ("Green Cat Token", ManaColor::G, &[Subtype::Cat][..], 2),
    ] {
        let token = def(name);
        assert_eq!(token.types, &[CardType::Creature], "{name}");
        assert_eq!(token.subtypes, subtypes, "{name}");
        assert_eq!(token.colors, &[color], "{name}");
        assert_eq!(
            (token.power, token.toughness),
            (Some(power), Some(power)),
            "{name}"
        );
        assert_eq!(token.mana_value, 0, "{name}");
    }
    assert_eq!(Subtype::Germ.stable_id(), 620);
    assert_eq!(Subtype::Rebel.stable_id(), 621);
    assert!(Subtype::Germ.is_creature_type() && Subtype::Rebel.is_creature_type());
}

// ---- Helpers of this wave ------------------------------------------------------------

fn tap(state: &mut GameState, object: ObjectId) {
    state.objects.get_mut(object).tapped = true;
}

fn is_tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
}

fn charge(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.charge
}

fn plus_one_counters(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.plus1_plus1
}

fn hand_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

fn library_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].library.len()
}

fn attached_to(state: &GameState, equipment: ObjectId) -> Option<ObjectId> {
    state
        .objects
        .get(equipment)
        .v4
        .attached_to
        .map(|link| link.object)
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

/// Activates `equipment`'s ability `index` at `creature` and lets it resolve.
fn equip_to(state: &mut GameState, equipment: ObjectId, index: u8, creature: ObjectId) {
    activate(state, equipment, index, &[Target::Object(creature)]);
    settled(state);
    assert_eq!(attached_to(state, equipment), Some(creature));
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

/// P1 casts the instant `name` with `colored` and `colorless` mana from its
/// pool and passes, leaving it on the stack with P0 holding priority.
fn opponent_instant(
    state: &mut GameState,
    name: &str,
    colored: &[ManaColor],
    colorless: u8,
    targets: &[Target],
) -> ObjectId {
    state.priority_player = P1;
    let spell = put(state, P1, name, Zone::Hand);
    pool(state, P1, colored, colorless);
    cast(state, spell, targets);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(state, Action::Pass);
    spell
}

// ---- Skullclamp ----------------------------------------------------------------------

#[test]
fn skullclamp_costs_one_equips_for_one_and_gives_plus_one_minus_one() {
    let mut state = plains();
    let clamp = put(&mut state, P0, "Skullclamp", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, clamp, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, clamp), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);

    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let bystander = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    assert!(!activatable(&mut state, clamp, 0), "equip costs {{1}}");
    pool(&mut state, P0, &[], 1);
    equip_to(&mut state, clamp, 0, inspector);
    assert_eq!(power_toughness(&state, inspector), (2, 1));
    assert_eq!(power_toughness(&state, bystander), (1, 2));
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "equip cost {{1}}");

    // Equip moves it: the old creature is whole again.
    pool(&mut state, P0, &[], 1);
    equip_to(&mut state, clamp, 0, bystander);
    assert_eq!(power_toughness(&state, inspector), (1, 2));
    assert_eq!(power_toughness(&state, bystander), (2, 1));
}

#[test]
fn skullclamp_equips_only_as_a_sorcery() {
    let mut state = plains();
    let clamp = put(&mut state, P0, "Skullclamp", Zone::Battlefield);
    put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, clamp, 0));
    opponent_bolts(&mut state, Target::Player(P0));
    assert!(
        !activatable(&mut state, clamp, 0),
        "a spell is on the stack"
    );
}

#[test]
fn skullclamp_on_a_one_toughness_creature_kills_it_and_still_draws_two() {
    let mut state = plains();
    let clamp = put(&mut state, P0, "Skullclamp", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert_eq!(hand_len(&state, P0), 0);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, clamp, 0, &[Target::Object(elves)]);
    settled(&mut state);
    // The 2/0 died as the Equip resolved; the Clamp is left unattached.
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, clamp), Zone::Battlefield);
    assert_eq!(attached_to(&state, clamp), None);
    assert_eq!(hand_len(&state, P0), 2, "it died equipped: draw two");
    assert_eq!(library_len(&state, P0), 18);

    // A token dies too.
    let mut state = plains();
    let clamp = put(&mut state, P0, "Skullclamp", Zone::Battlefield);
    let soldier = token(&mut state, P0, "Soldier Token");
    pool(&mut state, P0, &[], 1);
    activate(&mut state, clamp, 0, &[Target::Object(soldier)]);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 2);
}

#[test]
fn skullclamp_draws_two_when_the_equipped_creature_is_killed() {
    let mut state = plains();
    let clamp = put(&mut state, P0, "Skullclamp", Zone::Battlefield);
    let skyfisher = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    equip_to(&mut state, clamp, 0, skyfisher);
    assert_eq!(power_toughness(&state, skyfisher), (3, 2));
    assert_eq!(hand_len(&state, P0), 0);
    opponent_bolts(&mut state, Target::Object(skyfisher));
    settled(&mut state);
    assert_eq!(zone(&state, skyfisher), Zone::Graveyard);
    assert_eq!(hand_len(&state, P0), 2);
    assert_eq!(library_len(&state, P0), 18);
    assert_eq!(zone(&state, clamp), Zone::Battlefield);
}

#[test]
fn skullclamp_does_not_draw_for_other_creatures_or_a_former_host() {
    let mut state = plains();
    let clamp = put(&mut state, P0, "Skullclamp", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let skyfisher = put(&mut state, P0, "Kor Skyfisher", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, clamp, 0, inspector);
    equip_to(&mut state, clamp, 0, skyfisher);
    // The first host dies after the Clamp moved on.
    opponent_bolts(&mut state, Target::Object(inspector));
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Graveyard);
    assert_eq!(hand_len(&state, P0), 0, "no longer equipped when it died");
    assert_eq!(attached_to(&state, clamp), Some(skyfisher));
}

#[test]
fn skullclamp_does_not_draw_when_the_equipped_creature_is_exiled() {
    let mut state = plains();
    let clamp = put(&mut state, P0, "Skullclamp", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    equip_to(&mut state, clamp, 0, inspector);
    opponent_instant(
        &mut state,
        "Swords to Plowshares",
        &[ManaColor::W],
        0,
        &[Target::Object(inspector)],
    );
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Exile);
    assert_eq!(hand_len(&state, P0), 0, "exile is not dying");
}

#[test]
fn skullclamp_still_draws_when_it_dies_with_the_creature() {
    // Nevinyrral's Disk destroys the Clamp, its host and itself together.
    let mut state = plains();
    let clamp = put(&mut state, P0, "Skullclamp", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let disk = put(&mut state, P0, "Nevinyrral's Disk", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    equip_to(&mut state, clamp, 0, inspector);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, disk, 0, &[]);
    settled(&mut state);
    for object in [clamp, inspector, disk] {
        assert_eq!(zone(&state, object), Zone::Graveyard);
    }
    assert_eq!(hand_len(&state, P0), 2, "the Clamp saw its host die");
}

// ---- Sword of the Meek ---------------------------------------------------------------

#[test]
fn sword_of_the_meek_costs_two_gives_plus_one_plus_two_and_equips_for_two() {
    let mut state = plains();
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, sword, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, sword), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(!activatable(&mut state, sword, 0), "equip costs {{2}}");
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, sword, 0, elves);
    assert_eq!(power_toughness(&state, elves), (2, 3));
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn sword_of_the_meek_returns_attached_to_a_one_one_creature_you_control() {
    let mut state = plains();
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Graveyard);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, sword), Zone::Battlefield);
    assert_eq!(attached_to(&state, sword), Some(elves));
    assert_eq!(power_toughness(&state, elves), (2, 3));
}

#[test]
fn sword_of_the_meek_may_stay_in_the_graveyard() {
    let mut state = plains();
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Graveyard);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settle_option(&mut state, 0);
    assert_eq!(zone(&state, sword), Zone::Graveyard);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(power_toughness(&state, elves), (1, 1));
}

#[test]
fn sword_of_the_meek_ignores_other_creatures_and_the_opponents() {
    // A 2/1 is not a 1/1: no choice is offered.
    let mut state = plains();
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Graveyard);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, pup, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, pup), Zone::Battlefield);
    assert_eq!(zone(&state, sword), Zone::Graveyard);

    // An opponent's 1/1 is not "a 1/1 creature you control".
    let mut state = plains();
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Graveyard);
    state.active_player = P1;
    state.priority_player = P1;
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield);
    assert_eq!(zone(&state, sword), Zone::Graveyard);

    // The Sword must be in its owner's graveyard: in the hand it does nothing.
    let mut state = plains();
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Hand);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, sword), Zone::Hand);
}

#[test]
fn sword_of_the_meek_counts_a_token_that_enters() {
    let mut state = plains();
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Graveyard);
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, foundry, 1, &[]);
    settle_option(&mut state, 1);
    let servos = named(&state, P0, "Servo Token");
    assert_eq!(servos.len(), 1);
    assert_eq!(zone(&state, sword), Zone::Battlefield);
    assert_eq!(attached_to(&state, sword), Some(servos[0]));
    assert_eq!(power_toughness(&state, servos[0]), (2, 3));
}

#[test]
fn sword_of_the_meek_still_returns_when_the_creature_is_gone() {
    let mut state = plains();
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Graveyard);
    let foundry = put(&mut state, P0, "Retrofitter Foundry", Zone::Battlefield);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, foundry, 1, &[]);
    on_the_stack(&mut state);
    act(&mut state, Action::Pass);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    // The Servo exists and the Sword's trigger waits on the stack.
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    assert_eq!(state.stack.len(), 1);
    let servo = named(&state, P0, "Servo Token")[0];
    act(&mut state, Action::Pass);
    cast(&mut state, bolt, &[Target::Object(servo)]);
    settle_option(&mut state, 1);
    assert!(named(&state, P0, "Servo Token").is_empty());
    assert_eq!(zone(&state, sword), Zone::Battlefield, "it returned anyway");
    assert_eq!(
        attached_to(&state, sword),
        None,
        "with nothing to attach to"
    );
}

#[test]
fn sword_of_the_meek_reads_the_power_and_toughness_the_creature_had_as_it_entered() {
    // Nettlecyst's Germ enters as a 0/0 and only becomes a 1/1 when the living
    // weapon attaches the Nettlecyst: it did not enter as a 1/1 creature, so
    // the Sword stays in the graveyard and no choice is offered.
    let mut state = plains();
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Graveyard);
    let cyst = put(&mut state, P0, "Nettlecyst", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    cast(&mut state, cyst, &[]);
    settled(&mut state);
    let germ = named(&state, P0, "Phyrexian Germ Token")[0];
    assert_eq!(attached_to(&state, cyst), Some(germ));
    assert_eq!(power_toughness(&state, germ), (1, 1), "a 1/1 by now");
    assert_eq!(zone(&state, sword), Zone::Graveyard);

    // A creature that really enters as a 1/1 still triggers it.
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, sword), Zone::Battlefield);
    assert_eq!(attached_to(&state, sword), Some(elves));
    assert_eq!(power_toughness(&state, elves), (2, 3));
}

#[test]
fn sword_of_the_meek_and_thopter_foundry_loop() {
    // Sacrifice the Sword to Thopter Foundry; the Thopter is a 1/1, so the
    // Sword returns attached to it and can be sacrificed again.
    let mut state = plains();
    let foundry = put(&mut state, P0, "Thopter Foundry", Zone::Battlefield);
    let sword = put(&mut state, P0, "Sword of the Meek", Zone::Battlefield);
    for _ in 0..2 {
        pool(&mut state, P0, &[], 1);
        activate(&mut state, foundry, 0, &[]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseCostTargets { candidates, .. } if candidates.contains(&sword) => {
                Action::ChooseCostTarget(sword)
            }
            Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
            other => first_answer(other),
        });
        assert_eq!(zone(&state, sword), Zone::Battlefield, "back each time");
    }
    assert_eq!(named(&state, P0, "Blue Thopter Token").len(), 2);
    assert_eq!(life(&state, P0), 22);
}

// ---- Umezawa's Jitte -----------------------------------------------------------------

/// P0 with a Jitte attached to a Jackal Pup (2/1) and P1 with nothing.
fn jitte_game() -> (GameState, ObjectId, ObjectId) {
    let mut state = plains();
    let jitte = put(&mut state, P0, "Umezawa's Jitte", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, jitte, 3, pup);
    (state, jitte, pup)
}

#[test]
fn umezawas_jitte_costs_two_and_equips_for_two() {
    let mut state = plains();
    let jitte = put(&mut state, P0, "Umezawa's Jitte", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, jitte, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, jitte), Zone::Battlefield);
    assert_eq!(charge(&state, jitte), 0);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    assert!(!activatable(&mut state, jitte, 3), "equip costs {{2}}");
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, jitte, 3, pup);
    assert_eq!(power_toughness(&state, pup), (2, 1), "no stat change");
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

#[test]
fn umezawas_jitte_gets_two_counters_when_its_creature_deals_combat_damage() {
    let (mut state, jitte, pup) = jitte_game();
    attack_with(&mut state, vec![pup], |_, _| None);
    assert_eq!(life(&state, P1), 18);
    assert_eq!(charge(&state, jitte), 2);
    assert_eq!(attached_to(&state, jitte), Some(pup));
}

#[test]
fn umezawas_jitte_triggers_for_combat_damage_dealt_to_a_creature_too() {
    let (mut state, jitte, pup) = jitte_game();
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    attack_with(&mut state, vec![pup], |_, decision| match decision {
        Decision::DeclareBlockers { player: P1, .. } => {
            Some(Action::DeclareBlockers(vec![(wall, pup)]))
        }
        _ => None,
    });
    assert_eq!(life(&state, P1), 20, "blocked");
    assert_eq!(charge(&state, jitte), 2);
}

#[test]
fn umezawas_jitte_does_not_trigger_for_other_creatures_or_without_damage() {
    // The Jitte sits unattached while another creature deals the damage.
    let mut state = plains();
    let jitte = put(&mut state, P0, "Umezawa's Jitte", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    attack_with(&mut state, vec![pup], |_, _| None);
    assert_eq!(life(&state, P1), 18);
    assert_eq!(charge(&state, jitte), 0);

    // The equipped creature stays home while another attacks.
    let (mut state, jitte, _pup) = jitte_game();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    attack_with(&mut state, vec![elves], |_, _| None);
    assert_eq!(charge(&state, jitte), 0);

    // The equipped creature dies in the same combat damage step: both deal
    // damage at once, so the Jitte still counts.
    let (mut state, jitte, pup) = jitte_game();
    let wurm = put(&mut state, P1, "Wurmcoil Engine", Zone::Battlefield);
    attack_with(&mut state, vec![pup], |_, decision| match decision {
        Decision::DeclareBlockers { player: P1, .. } => {
            Some(Action::DeclareBlockers(vec![(wurm, pup)]))
        }
        _ => None,
    });
    assert_eq!(zone(&state, pup), Zone::Graveyard);
    assert_eq!(charge(&state, jitte), 2, "it dealt damage as it died");
    assert_eq!(attached_to(&state, jitte), None);
}

#[test]
fn umezawas_jitte_counters_make_an_equipped_creature_bigger_until_end_of_turn() {
    let (mut state, jitte, pup) = jitte_game();
    assert!(!activatable(&mut state, jitte, 0), "no counter to remove");
    state.objects.get_mut(jitte).counters.charge = 2;
    activate(&mut state, jitte, 0, &[]);
    settled(&mut state);
    assert_eq!(charge(&state, jitte), 1);
    assert_eq!(power_toughness(&state, pup), (4, 3));
    activate(&mut state, jitte, 0, &[]);
    settled(&mut state);
    assert_eq!(charge(&state, jitte), 0);
    assert_eq!(power_toughness(&state, pup), (6, 5));
    assert!(!activatable(&mut state, jitte, 0), "out of counters");
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(power_toughness(&state, pup), (2, 1), "until end of turn");
    assert_eq!(charge(&state, jitte), 0);
}

#[test]
fn umezawas_jitte_counters_shrink_a_target_creature_until_end_of_turn() {
    let (mut state, jitte, pup) = jitte_game();
    state.objects.get_mut(jitte).counters.charge = 2;
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let inspector = put(&mut state, P1, "Thraben Inspector", Zone::Battlefield);
    // Any creature is a target, including its own.
    let targets = ability_targets(&state, jitte, 1);
    for creature in [pup, elves, inspector] {
        assert!(targets.contains(&Target::Object(creature)));
    }
    activate(&mut state, jitte, 1, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard, "1/1 gets -1/-1");
    assert_eq!(charge(&state, jitte), 1);
    activate(&mut state, jitte, 1, &[Target::Object(inspector)]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Battlefield, "1/2 survives");
    assert_eq!(power_toughness(&state, inspector), (0, 1));
    assert_eq!(charge(&state, jitte), 0);
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(power_toughness(&state, inspector), (1, 2));
}

#[test]
fn umezawas_jitte_counter_shrink_fizzles_when_its_target_is_gone() {
    let (mut state, jitte, _pup) = jitte_game();
    state.objects.get_mut(jitte).counters.charge = 1;
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    activate(&mut state, jitte, 1, &[Target::Object(elves)]);
    on_the_stack(&mut state);
    assert_eq!(charge(&state, jitte), 0, "the cost is paid up front");
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(elves, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(charge(&state, jitte), 0);
}

#[test]
fn umezawas_jitte_counters_gain_two_life() {
    let (mut state, jitte, _pup) = jitte_game();
    state.objects.get_mut(jitte).counters.charge = 2;
    activate(&mut state, jitte, 2, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 22);
    assert_eq!(charge(&state, jitte), 1);
    activate(&mut state, jitte, 2, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 24);
    assert!(!activatable(&mut state, jitte, 2));
}

#[test]
fn umezawas_jitte_keeps_its_counters_when_it_moves_and_works_unattached() {
    let (mut state, jitte, pup) = jitte_game();
    attack_with(&mut state, vec![pup], |_, _| None);
    assert_eq!(charge(&state, jitte), 2);
    // Main 2 is a main phase: Equip moves the Jitte and its counters stay.
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, jitte, 3, elves);
    assert_eq!(charge(&state, jitte), 2, "counters stay on the Jitte");
    // The modes need no particular creature.
    assert!(activatable(&mut state, jitte, 2));
    assert!(activatable(&mut state, jitte, 1));
}

// ---- Nettlecyst ----------------------------------------------------------------------

#[test]
fn nettlecyst_makes_a_germ_that_gets_plus_one_for_each_artifact_and_enchantment() {
    let mut state = plains();
    let cyst = put(&mut state, P0, "Nettlecyst", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    cast(&mut state, cyst, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, cyst), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    let germs = named(&state, P0, "Phyrexian Germ Token");
    assert_eq!(germs.len(), 1);
    let germ = germs[0];
    assert_eq!(attached_to(&state, cyst), Some(germ), "living weapon");
    assert!(engine::has_effective_subtype(&state, germ, Subtype::Germ));
    assert!(engine::has_effective_subtype(
        &state,
        germ,
        Subtype::Phyrexian
    ));
    assert!(!engine::object_has_type(&state, germ, CardType::Artifact));
    // A 0/0 that lives only because the Nettlecyst itself counts.
    assert_eq!(power_toughness(&state, germ), (1, 1));

    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    assert_eq!(power_toughness(&state, germ), (2, 2));
    put(&mut state, P0, "Mirri's Guile", Zone::Battlefield);
    assert_eq!(power_toughness(&state, germ), (3, 3));
    // An artifact creature counts once.
    put(&mut state, P0, "Wurmcoil Engine", Zone::Battlefield);
    assert_eq!(power_toughness(&state, germ), (4, 4));
    // Other permanents and the opponent's do not count.
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    put(&mut state, P1, "Mirri's Guile", Zone::Battlefield);
    assert_eq!(power_toughness(&state, germ), (4, 4));
    // The bonus is recomputed as the board changes.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(ring, Zone::Graveyard),
    );
    assert_eq!(power_toughness(&state, germ), (3, 3));
}

#[test]
fn nettlecyst_equips_for_two_and_its_old_germ_dies() {
    let mut state = plains();
    let cyst = put(&mut state, P0, "Nettlecyst", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    cast(&mut state, cyst, &[]);
    settled(&mut state);
    let germ = named(&state, P0, "Phyrexian Germ Token")[0];
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(!activatable(&mut state, cyst, 0), "equip costs {{2}}");
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, cyst, 0, elves);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(power_toughness(&state, elves), (2, 2));
    assert!(
        named(&state, P0, "Phyrexian Germ Token").is_empty(),
        "the Germ is 0/0 without it"
    );
    assert!(!on_battlefield_anywhere(&state, germ));
}

fn on_battlefield_anywhere(state: &GameState, object: ObjectId) -> bool {
    [P0, P1]
        .into_iter()
        .any(|player| state.players[player.index()].battlefield.contains(&object))
}

// ---- Kaldra Compleat -----------------------------------------------------------------

/// Kaldra Compleat cast for {7}: its Germ.
fn kaldra_game() -> (GameState, ObjectId, ObjectId) {
    let mut state = plains();
    let kaldra = put(&mut state, P0, "Kaldra Compleat", Zone::Hand);
    pool(&mut state, P0, &[], 7);
    cast(&mut state, kaldra, &[]);
    settled(&mut state);
    let germ = named(&state, P0, "Phyrexian Germ Token")[0];
    (state, kaldra, germ)
}

#[test]
fn kaldra_compleat_makes_a_five_five_germ_with_four_keywords_that_can_attack_at_once() {
    let (mut state, kaldra, germ) = kaldra_game();
    assert_eq!(zone(&state, kaldra), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(attached_to(&state, kaldra), Some(germ));
    assert!(keyword(&state, kaldra, Keywords::INDESTRUCTIBLE));
    assert_eq!(power_toughness(&state, germ), (5, 5));
    for granted in [
        Keywords::FIRST_STRIKE,
        Keywords::TRAMPLE,
        Keywords::INDESTRUCTIBLE,
        Keywords::HASTE,
    ] {
        assert!(keyword(&state, germ, granted));
    }
    assert!(!keyword(&state, germ, Keywords::FLYING));
    // Haste lets the token that arrived this turn attack.
    attack_with(&mut state, vec![germ], |_, _| None);
    assert_eq!(life(&state, P1), 15);
}

#[test]
fn kaldra_compleat_exiles_a_creature_its_creature_damaged_and_it_survived() {
    let (mut state, _kaldra, germ) = kaldra_game();
    let paladin = put(&mut state, P1, "Goliath Paladin", Zone::Battlefield);
    attack_with(&mut state, vec![germ], |_, decision| match decision {
        Decision::DeclareBlockers { player: P1, .. } => {
            Some(Action::DeclareBlockers(vec![(paladin, germ)]))
        }
        _ => None,
    });
    assert_eq!(
        zone(&state, paladin),
        Zone::Exile,
        "3/6 took 5 and was exiled"
    );
    assert_eq!(zone(&state, germ), Zone::Battlefield, "indestructible");
    assert_eq!(
        life(&state, P1),
        20,
        "a blocked creature tramples over nothing"
    );
}

#[test]
fn kaldra_compleat_does_not_exile_a_creature_that_dies_to_the_damage() {
    let (mut state, _kaldra, germ) = kaldra_game();
    let wall = put(&mut state, P1, "Wall of Roots", Zone::Battlefield);
    attack_with(&mut state, vec![germ], |_, decision| match decision {
        Decision::DeclareBlockers { player: P1, .. } => {
            Some(Action::DeclareBlockers(vec![(wall, germ)]))
        }
        _ => None,
    });
    assert_eq!(zone(&state, wall), Zone::Graveyard, "5 damage is lethal");
    assert_eq!(life(&state, P1), 20);
}

#[test]
fn kaldra_compleat_is_indestructible_and_equips_for_seven() {
    let mut state = plains();
    let kaldra = put(&mut state, P0, "Kaldra Compleat", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    opponent_instant(
        &mut state,
        "Disenchant",
        &[ManaColor::W],
        1,
        &[Target::Object(kaldra)],
    );
    settled(&mut state);
    assert_eq!(zone(&state, kaldra), Zone::Battlefield, "indestructible");
    // The Elves would pay a seventh mana from their own ability; keep them
    // tapped so that only the pool counts.
    state.objects.get_mut(elves).tapped = true;
    pool(&mut state, P0, &[], 6);
    assert!(!activatable(&mut state, kaldra, 0), "equip costs {{7}}");
    pool(&mut state, P0, &[], 1);
    equip_to(&mut state, kaldra, 0, elves);
    assert_eq!(power_toughness(&state, elves), (6, 6));
    assert!(keyword(&state, elves, Keywords::FIRST_STRIKE));
    assert!(keyword(&state, elves, Keywords::HASTE));
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
}

// ---- Glimmer Lens --------------------------------------------------------------------

/// A Glimmer Lens equipped to a Jackal Pup, with a Llanowar Elves besides.
fn lens_game() -> (GameState, ObjectId, ObjectId, ObjectId) {
    let mut state = plains();
    let lens = put(&mut state, P0, "Glimmer Lens", Zone::Battlefield);
    let pup = put(&mut state, P0, "Jackal Pup", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(!activatable(&mut state, lens, 0), "equip costs {{1}}{{W}}");
    pool(&mut state, P0, &[ManaColor::W], 1);
    equip_to(&mut state, lens, 0, pup);
    assert_eq!(mana(&state, P0, ManaColor::W), 0);
    (state, lens, pup, elves)
}

#[test]
fn glimmer_lens_costs_two_and_makes_an_equipped_two_two_red_rebel() {
    let mut state = plains();
    let lens = put(&mut state, P0, "Glimmer Lens", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, lens, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, lens), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::W), 0);
    let rebels = named(&state, P0, "Rebel Token");
    assert_eq!(rebels.len(), 1);
    assert_eq!(attached_to(&state, lens), Some(rebels[0]));
    assert_eq!(power_toughness(&state, rebels[0]), (2, 2));
    assert!(engine::has_effective_subtype(
        &state,
        rebels[0],
        Subtype::Rebel
    ));
    assert_eq!(hand_len(&state, P0), 0, "no draw without an attack");
}

#[test]
fn glimmer_lens_draws_when_its_creature_and_another_creature_attack() {
    let (mut state, _lens, pup, elves) = lens_game();
    attack_with(&mut state, vec![pup, elves], |_, _| None);
    assert_eq!(life(&state, P1), 17, "both attacked");
    assert_eq!(hand_len(&state, P0), 1);
    assert_eq!(library_len(&state, P0), 19);
}

#[test]
fn glimmer_lens_draws_once_however_many_creatures_attack_with_it() {
    let (mut state, _lens, pup, elves) = lens_game();
    let third = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    attack_with(&mut state, vec![pup, elves, third], |_, _| None);
    assert_eq!(hand_len(&state, P0), 1);
}

#[test]
fn glimmer_lens_does_not_draw_when_its_creature_attacks_alone() {
    let (mut state, _lens, pup, _elves) = lens_game();
    attack_with(&mut state, vec![pup], |_, _| None);
    assert_eq!(life(&state, P1), 18);
    assert_eq!(hand_len(&state, P0), 0);
}

#[test]
fn glimmer_lens_does_not_draw_when_others_attack_without_its_creature() {
    let (mut state, _lens, _pup, elves) = lens_game();
    let third = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    attack_with(&mut state, vec![elves, third], |_, _| None);
    assert_eq!(hand_len(&state, P0), 0);
}

// ---- Iron Man Armor ------------------------------------------------------------------

#[test]
fn iron_man_armor_attaches_to_a_creature_you_control_as_it_enters() {
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let armor = put(&mut state, P0, "Iron Man Armor", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    cast(&mut state, armor, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, armor), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(attached_to(&state, armor), Some(elves));
    assert_eq!(power_toughness(&state, elves), (3, 2));
    assert!(keyword(&state, elves, Keywords::FLYING));

    // With no creature to attach to, it just enters.
    let mut state = plains();
    let armor = put(&mut state, P0, "Iron Man Armor", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    cast(&mut state, armor, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, armor), Zone::Battlefield);
    assert_eq!(attached_to(&state, armor), None);
}

#[test]
fn iron_man_armor_equips_for_two() {
    let mut state = plains();
    let armor = put(&mut state, P0, "Iron Man Armor", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let other = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    // Untapped Elves would pay the equip cost from their own ability; keep
    // them tapped so that only the pool counts.
    state.objects.get_mut(elves).tapped = true;
    state.objects.get_mut(other).tapped = true;
    assert!(!activatable(&mut state, armor, 1), "equip costs {{2}}");
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, armor, 1, elves);
    assert_eq!(power_toughness(&state, elves), (3, 2));
    assert!(keyword(&state, elves, Keywords::FLYING));
    assert!(!keyword(&state, other, Keywords::FLYING));
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, armor, 1, other);
    assert_eq!(power_toughness(&state, elves), (1, 1));
    assert!(!keyword(&state, elves, Keywords::FLYING));
    assert_eq!(power_toughness(&state, other), (3, 2));
}

#[test]
fn iron_man_armor_becomes_a_construct_hero_artifact_creature_until_end_of_turn() {
    let mut state = plains();
    let armor = put(&mut state, P0, "Iron Man Armor", Zone::Battlefield);
    assert!(!engine::object_has_type(&state, armor, CardType::Creature));
    assert!(!activatable(&mut state, armor, 0), "{{2}} needs two mana");
    pool(&mut state, P0, &[], 2);
    activate(&mut state, armor, 0, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert!(engine::object_has_type(&state, armor, CardType::Creature));
    assert!(engine::object_has_type(&state, armor, CardType::Artifact));
    for subtype in [Subtype::Construct, Subtype::Hero, Subtype::Equipment] {
        assert!(engine::has_effective_subtype(&state, armor, subtype));
    }
    assert!(keyword(&state, armor, Keywords::FLYING));
    // "+1/+1 for each artifact you control", itself included.
    assert_eq!(power_toughness(&state, armor), (1, 1));
    put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    assert_eq!(power_toughness(&state, armor), (2, 2));
    put(&mut state, P0, "Wurmcoil Engine", Zone::Battlefield);
    assert_eq!(power_toughness(&state, armor), (3, 3));
    // Not enchantments, not creatures that are not artifacts, not the
    // opponent's artifacts.
    put(&mut state, P0, "Mirri's Guile", Zone::Battlefield);
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    assert_eq!(power_toughness(&state, armor), (3, 3));
    // It lasts until end of turn.
    pass_until(&mut state, |state| state.active_player == P1);
    assert!(!engine::object_has_type(&state, armor, CardType::Creature));
    assert!(engine::object_has_type(&state, armor, CardType::Artifact));
    assert!(!keyword(&state, armor, Keywords::FLYING));
    assert!(!engine::has_effective_subtype(&state, armor, Subtype::Hero));
}

#[test]
fn iron_man_armor_can_attack_as_a_creature_when_it_has_been_there_all_turn() {
    let mut state = plains();
    let armor = put(&mut state, P0, "Iron Man Armor", Zone::Battlefield);
    put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, armor, 0, &[]);
    settled(&mut state);
    attack_with(&mut state, vec![armor], |_, _| None);
    assert_eq!(life(&state, P1), 18, "a 2/2 flier");
}

#[test]
fn iron_man_armor_comes_unattached_when_it_becomes_a_creature() {
    let mut state = plains();
    let armor = put(&mut state, P0, "Iron Man Armor", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, armor, 1, elves);
    assert_eq!(power_toughness(&state, elves), (3, 2));
    pool(&mut state, P0, &[], 2);
    activate(&mut state, armor, 0, &[]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, armor, CardType::Creature));
    assert_eq!(attached_to(&state, armor), None, "301.5c");
    assert_eq!(power_toughness(&state, elves), (1, 1));
    assert!(!keyword(&state, elves, Keywords::FLYING));
}

#[test]
fn iron_man_armor_cant_equip_while_it_is_a_creature() {
    let mut state = plains();
    let armor = put(&mut state, P0, "Iron Man Armor", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 4);
    activate(&mut state, armor, 0, &[]);
    settled(&mut state);
    // The ability may be activated (even naming the Armor itself, a creature
    // you control) but does nothing.
    activate(&mut state, armor, 1, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(attached_to(&state, armor), None);
    assert_eq!(power_toughness(&state, elves), (1, 1));
    pool(&mut state, P0, &[], 2);
    if ability_targets(&state, armor, 1).contains(&Target::Object(armor)) {
        activate(&mut state, armor, 1, &[Target::Object(armor)]);
        settled(&mut state);
    }
    assert_eq!(attached_to(&state, armor), None);
    assert!(engine::object_has_type(&state, armor, CardType::Creature));
}

#[test]
fn iron_man_armor_animation_does_nothing_while_it_is_already_a_creature() {
    // A second activation in the same turn changes nothing: the Armor is a
    // creature, so "if this Equipment isn't a creature" fails.
    let mut state = plains();
    let armor = put(&mut state, P0, "Iron Man Armor", Zone::Battlefield);
    pool(&mut state, P0, &[], 4);
    activate(&mut state, armor, 0, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::C), 2);
    let stamp = state.objects.get(armor).v4.animation_timestamp;
    assert!(stamp.is_some(), "animated by its own ability");
    assert_eq!(power_toughness(&state, armor), (1, 1));
    activate(&mut state, armor, 0, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "the cost is still paid");
    assert_eq!(state.objects.get(armor).v4.animation_timestamp, stamp);
    assert_eq!(power_toughness(&state, armor), (1, 1));
    assert!(keyword(&state, armor, Keywords::FLYING));

    // Already a creature through another effect (Karn, the Great Creator's
    // +1 makes it a 3/3 artifact creature): the ability replaces nothing, so
    // it stays a 3/3 without flying, and not a Construct Hero.
    let mut state = plains();
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    let other = put(&mut state, P0, "Iron Man Armor", Zone::Battlefield);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    activate(&mut state, karn, 0, &[Target::Object(other)]);
    settled(&mut state);
    assert!(engine::object_has_type(&state, other, CardType::Creature));
    assert_eq!(power_toughness(&state, other), (3, 3), "mana value 3");
    pool(&mut state, P0, &[], 2);
    activate(&mut state, other, 0, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::C), 0, "the cost is still paid");
    assert_eq!(state.objects.get(other).v4.animation_timestamp, None);
    assert_eq!(power_toughness(&state, other), (3, 3));
    assert!(!keyword(&state, other, Keywords::FLYING));
    assert!(!engine::has_effective_subtype(&state, other, Subtype::Hero));
}

// ---- Astrologian's Planisphere -------------------------------------------------------

/// A Planisphere equipped to a Llanowar Elves.
fn planisphere_game() -> (GameState, ObjectId, ObjectId) {
    let mut state = plains();
    let planisphere = put(
        &mut state,
        P0,
        "Astrologian's Planisphere",
        Zone::Battlefield,
    );
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(
        !activatable(&mut state, planisphere, 0),
        "equip costs {{2}}"
    );
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, planisphere, 0, elves);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    (state, planisphere, elves)
}

#[test]
fn planisphere_costs_two_and_makes_the_equipped_creature_a_wizard() {
    let mut state = plains();
    let planisphere = put(&mut state, P0, "Astrologian's Planisphere", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, planisphere, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, planisphere), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::U), 0);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let other = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, planisphere, 0, elves);
    assert!(engine::has_effective_subtype(
        &state,
        elves,
        Subtype::Wizard
    ));
    assert!(engine::has_effective_subtype(&state, elves, Subtype::Elf));
    assert!(!engine::has_effective_subtype(
        &state,
        other,
        Subtype::Wizard
    ));
    assert_eq!(power_toughness(&state, elves), (1, 1));
}

#[test]
fn planisphere_job_select_makes_a_hero_and_attaches_to_it() {
    // "Job select (When this Equipment enters, create a 1/1 colorless Hero
    // creature token, then attach this to it.)" onto an empty battlefield.
    let mut state = plains();
    let planisphere = put(&mut state, P0, "Astrologian's Planisphere", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, planisphere, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, planisphere), Zone::Battlefield);
    let heroes = named(&state, P0, "Hero Token");
    assert_eq!(heroes.len(), 1, "one Hero token");
    let hero = heroes[0];
    assert!(state.objects.get(hero).v4.is_token);
    assert_eq!(attached_to(&state, planisphere), Some(hero));
    assert_eq!(power_toughness(&state, hero), (1, 1));
    assert!(def("Hero Token").colors.is_empty(), "colorless");
    assert!(engine::has_effective_subtype(&state, hero, Subtype::Hero));
    assert!(
        engine::has_effective_subtype(&state, hero, Subtype::Wizard),
        "the equipped Hero is a Wizard"
    );

    // The granted trigger works on the Hero.
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(plus_one_counters(&state, hero), 1);
    assert_eq!(power_toughness(&state, hero), (2, 2));
}

#[test]
fn planisphere_puts_a_counter_on_its_creature_when_you_cast_a_noncreature_spell() {
    let (mut state, _planisphere, elves) = planisphere_game();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(plus_one_counters(&state, elves), 1);
    assert_eq!(power_toughness(&state, elves), (2, 2));
    // Each noncreature spell does it again.
    let second = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, second, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(plus_one_counters(&state, elves), 2);
}

#[test]
fn planisphere_ignores_creature_spells_and_the_opponents_spells() {
    let (mut state, _planisphere, elves) = planisphere_game();
    let mystic = put(&mut state, P0, "Elvish Mystic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, mystic, &[]);
    settled(&mut state);
    assert_eq!(plus_one_counters(&state, elves), 0, "a creature spell");
    opponent_bolts(&mut state, Target::Player(P0));
    settled(&mut state);
    assert_eq!(life(&state, P0), 17);
    assert_eq!(plus_one_counters(&state, elves), 0, "the opponent's spell");
}

#[test]
fn planisphere_counts_only_the_creature_it_equips() {
    let (mut state, planisphere, elves) = planisphere_game();
    let other = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    equip_to(&mut state, planisphere, 0, other);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(plus_one_counters(&state, elves), 0, "no longer equipped");
    assert_eq!(plus_one_counters(&state, other), 1);
    assert!(!engine::has_effective_subtype(
        &state,
        elves,
        Subtype::Wizard
    ));
}

#[test]
fn planisphere_does_nothing_unattached() {
    let mut state = plains();
    put(
        &mut state,
        P0,
        "Astrologian's Planisphere",
        Zone::Battlefield,
    );
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(plus_one_counters(&state, elves), 0);
}

#[test]
fn planisphere_puts_a_counter_when_you_draw_your_third_card_each_turn() {
    let (mut state, _planisphere, elves) = planisphere_game();
    let stones: Vec<ObjectId> = (0..4)
        .map(|_| put(&mut state, P0, "Mind Stone", Zone::Battlefield))
        .collect();
    for (index, &stone) in stones.iter().enumerate() {
        pool(&mut state, P0, &[], 1);
        activate(&mut state, stone, 0, &[]);
        settled(&mut state);
        assert_eq!(hand_len(&state, P0), index + 1);
        let expected = i32::from(index >= 2);
        assert_eq!(
            plus_one_counters(&state, elves),
            expected,
            "after draw {}",
            index + 1
        );
    }
}

#[test]
fn planisphere_triggers_for_both_halves_of_one_ancestral_recall() {
    // Casting it is a noncreature spell; the third of its three draws is the
    // third card of the turn.
    let (mut state, _planisphere, elves) = planisphere_game();
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, recall, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(hand_len(&state, P0), 3);
    assert_eq!(plus_one_counters(&state, elves), 2);
    assert_eq!(power_toughness(&state, elves), (3, 3));
}

// ---- Smuggler's Copter ---------------------------------------------------------------

#[test]
fn smugglers_copter_costs_two_and_crews_for_one_into_a_three_three_flier() {
    let mut state = plains();
    let copter = put(&mut state, P0, "Smuggler's Copter", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, copter, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, copter), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert!(!engine::object_has_type(&state, copter, CardType::Creature));
    assert!(!activatable(&mut state, copter, 0), "nobody to crew");
    let wall = put(&mut state, P0, "Wall of Roots", Zone::Battlefield);
    assert!(!activatable(&mut state, copter, 0), "power 0 is not enough");
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(elves).summoning_sick = true;
    assert!(
        activatable(&mut state, copter, 0),
        "a summoning-sick creature crews"
    );
    crew(&mut state, copter, 0, &[elves]);
    assert!(is_tapped(&state, elves));
    assert!(!is_tapped(&state, wall));
    assert!(engine::object_has_type(&state, copter, CardType::Creature));
    assert!(engine::object_has_type(&state, copter, CardType::Artifact));
    assert_eq!(power_toughness(&state, copter), (3, 3));
    assert!(keyword(&state, copter, Keywords::FLYING));
}

/// A Copter crewed by a Llanowar Elves, with a land in hand and cards to draw.
fn copter_game() -> (GameState, ObjectId, ObjectId) {
    let mut state = plains();
    let copter = put(&mut state, P0, "Smuggler's Copter", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P0, "Plains", Zone::Hand);
    crew(&mut state, copter, 0, &[elves]);
    (state, copter, elves)
}

#[test]
fn smugglers_copter_may_loot_when_it_attacks() {
    let (mut state, copter, _elves) = copter_game();
    attack_with(&mut state, vec![copter], |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        _ => None,
    });
    assert_eq!(life(&state, P1), 17);
    assert_eq!(library_len(&state, P0), 19, "it drew");
    assert_eq!(hand_len(&state, P0), 1, "and discarded");
    assert_eq!(state.players[0].graveyard.len(), 1);
}

#[test]
fn smugglers_copter_may_decline_to_loot() {
    let (mut state, copter, _elves) = copter_game();
    attack_with(&mut state, vec![copter], |_, _| None);
    assert_eq!(life(&state, P1), 17);
    assert_eq!(library_len(&state, P0), 20);
    assert_eq!(hand_len(&state, P0), 1);
    assert!(state.players[0].graveyard.is_empty());
}

#[test]
fn smugglers_copter_loots_when_it_blocks() {
    let mut state = plains();
    let attacker = put(&mut state, P0, "Thraben Inspector", Zone::Battlefield);
    let copter = put(&mut state, P1, "Smuggler's Copter", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    put(&mut state, P1, "Plains", Zone::Hand);
    let mut crewed = false;
    attack_with(&mut state, vec![attacker], |_, decision| match decision {
        // P1 crews in the declare attackers step, before blockers.
        Decision::CastSpellOrPass {
            player: P1,
            activatable_abilities,
            ..
        } if !crewed && activatable_abilities.contains(&(copter, 0)) => {
            crewed = true;
            Some(Action::ActivateAbility(copter, 0))
        }
        Decision::DeclareBlockers { player: P1, .. } => {
            Some(Action::DeclareBlockers(vec![(copter, attacker)]))
        }
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(1)),
        _ => None,
    });
    assert!(crewed);
    assert_eq!(zone(&state, attacker), Zone::Graveyard, "3 damage kills it");
    assert_eq!(life(&state, P0), 20);
    assert_eq!(library_len(&state, P1), 19, "the blocking Copter drew");
    assert_eq!(hand_len(&state, P1), 1, "and discarded");
    assert_eq!(state.players[1].graveyard.len(), 1);
}

// ---- Esika's Chariot -----------------------------------------------------------------

#[test]
fn esikas_chariot_makes_two_two_two_green_cats_that_can_crew_it_at_once() {
    let mut state = plains();
    let chariot = put(&mut state, P0, "Esika's Chariot", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 3);
    cast(&mut state, chariot, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, chariot), Zone::Battlefield);
    assert_eq!(mana(&state, P0, ManaColor::G), 0);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    let cats = named(&state, P0, "Green Cat Token");
    assert_eq!(cats.len(), 2);
    for &cat in &cats {
        assert_eq!(power_toughness(&state, cat), (2, 2));
        assert!(engine::has_effective_subtype(&state, cat, Subtype::Cat));
    }
    assert!(!engine::object_has_type(
        &state,
        chariot,
        CardType::Creature
    ));
    // Crew 4: one Cat is short, and both (summoning-sick) Cats do it.
    let mut one = state.clone();
    tap(&mut one, cats[1]);
    assert!(!activatable(&mut one, chariot, 0), "2 power is not 4");
    crew(&mut state, chariot, 0, &cats);
    assert!(engine::object_has_type(&state, chariot, CardType::Creature));
    assert_eq!(power_toughness(&state, chariot), (4, 4));
    assert!(cats.iter().all(|&cat| is_tapped(&state, cat)));
}

/// An uncrewed Chariot, two Cats and a Treasure of P0's, and P1's Soldier.
fn chariot_game() -> (GameState, ObjectId, Vec<ObjectId>, ObjectId, ObjectId) {
    let mut state = plains();
    let chariot = put(&mut state, P0, "Esika's Chariot", Zone::Battlefield);
    let cats = vec![
        token(&mut state, P0, "Green Cat Token"),
        token(&mut state, P0, "Green Cat Token"),
    ];
    let treasure = token(&mut state, P0, "Treasure Token");
    let soldier = token(&mut state, P1, "Soldier Token");
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    crew(&mut state, chariot, 0, &cats);
    (state, chariot, cats, treasure, soldier)
}

#[test]
fn esikas_chariot_copies_a_token_you_control_when_it_attacks() {
    let (mut state, chariot, cats, treasure, soldier) = chariot_game();
    let mut offered: Vec<Target> = Vec::new();
    attack_with(&mut state, vec![chariot], |_, decision| match decision {
        Decision::ChooseTargets {
            player: P0,
            legal_targets,
            ..
        } => {
            offered = legal_targets.clone();
            Some(Action::ChooseTarget(Target::Object(treasure)))
        }
        _ => None,
    });
    // Only P0's tokens: not the Chariot, a land, a nontoken creature or the
    // opponent's token.
    assert_eq!(offered.len(), 3, "{offered:?}");
    for token in [cats[0], cats[1], treasure] {
        assert!(offered.contains(&Target::Object(token)));
    }
    assert!(!offered.contains(&Target::Object(soldier)));
    assert_eq!(named(&state, P0, "Treasure Token").len(), 2);
    assert_eq!(named(&state, P0, "Green Cat Token").len(), 2);
    assert_eq!(life(&state, P1), 16, "the Chariot dealt 4");
}

#[test]
fn esikas_chariot_copies_a_cat_into_a_two_two_cat() {
    let (mut state, chariot, cats, _treasure, _soldier) = chariot_game();
    attack_with(&mut state, vec![chariot], |_, decision| match decision {
        Decision::ChooseTargets {
            player: P0,
            legal_targets,
            ..
        } if legal_targets.contains(&Target::Object(cats[0])) => {
            Some(Action::ChooseTarget(Target::Object(cats[0])))
        }
        _ => None,
    });
    let after = named(&state, P0, "Green Cat Token");
    assert_eq!(after.len(), 3);
    let copy = after
        .iter()
        .copied()
        .find(|cat| !cats.contains(cat))
        .expect("a new Cat");
    assert_eq!(power_toughness(&state, copy), (2, 2));
}

#[test]
fn esikas_chariot_copy_does_nothing_when_its_target_is_gone() {
    let (mut state, chariot, cats, _treasure, _soldier) = chariot_game();
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![chariot]));
    // The attack trigger copies the first Cat and waits on the stack.
    run_until(
        &mut state,
        |state| !state.stack.is_empty(),
        |_, decision| match decision {
            Decision::ChooseTargets {
                player: P0,
                legal_targets,
                ..
            } if legal_targets.contains(&Target::Object(cats[0])) => {
                Some(Action::ChooseTarget(Target::Object(cats[0])))
            }
            _ => None,
        },
    );
    assert_eq!(state.stack.len(), 1, "the copy trigger");
    // P0 passes; P1 bolts the Cat in response. The mana is added now: a mana
    // pool empties as each step ends.
    act(&mut state, Action::Pass);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(cats[0])]);
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
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "the Bolt resolved");
    let after = named(&state, P0, "Green Cat Token");
    assert_eq!(after, vec![cats[1]], "the target died, so no copy");
}

#[test]
fn esikas_chariot_attack_trigger_needs_a_token_to_target() {
    let mut state = plains();
    let chariot = put(&mut state, P0, "Esika's Chariot", Zone::Battlefield);
    let crew_member = put(&mut state, P0, "Wurmcoil Engine", Zone::Battlefield);
    crew(&mut state, chariot, 0, &[crew_member]);
    attack_with(&mut state, vec![chariot], |_, _| None);
    assert_eq!(life(&state, P1), 16);
    assert!(named(&state, P0, "Green Cat Token").is_empty());
}

// ---- Public sessions -----------------------------------------------------------------

#[test]
fn c4c_decks_finish_and_replay_through_public_sessions_both_seats() {
    let colored = deck(&[
        ("Plains", 6),
        ("Island", 5),
        ("Forest", 5),
        ("Mind Stone", 2),
        ("Sol Ring", 2),
        ("Llanowar Elves", 3),
        ("Thraben Inspector", 3),
        ("Sacred Cat", 3),
        ("Skullclamp", 2),
        ("Sword of the Meek", 2),
        ("Umezawa's Jitte", 3),
        ("Nettlecyst", 2),
        ("Kaldra Compleat", 2),
        ("Glimmer Lens", 3),
        ("Iron Man Armor", 2),
        ("Astrologian's Planisphere", 3),
        ("Smuggler's Copter", 3),
        ("Esika's Chariot", 3),
    ]);
    let colorless = deck(&[
        ("Mountain", 8),
        ("Swamp", 8),
        ("Mind Stone", 3),
        ("Sol Ring", 4),
        ("Jackal Pup", 4),
        ("Hypnotic Specter", 3),
        ("Skullclamp", 4),
        ("Sword of the Meek", 3),
        ("Umezawa's Jitte", 4),
        ("Nettlecyst", 4),
        ("Kaldra Compleat", 4),
        ("Iron Man Armor", 3),
        ("Smuggler's Copter", 4),
    ]);

    let mut casts = std::collections::BTreeMap::<u16, usize>::new();
    let mut games = Vec::new();
    for (pair, first_seed) in [
        ([colored.clone(), colorless.clone()], 0x4334_4300_u64),
        ([colorless, colored], 0x4334_4310),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [Policy::Ranked, Policy::Random(seed + 0x100)] {
                games.push((pair.clone(), seed, policy));
            }
        }
    }
    // The games run on worker threads (`in_parallel`); the totals add their
    // casts in this loop's order.
    let cast_cards = in_parallel(&games, |(pair, seed, policy)| {
        let game = play_observed(pair.clone(), None, *seed, *policy, &mut |_, _| {});
        let replay = play_observed(pair.clone(), None, *seed, *policy, &mut |_, _| {});
        assert_eq!(game.transcript_sha256, replay.transcript_sha256);
        game.cast_cards
    });
    for game_cast_cards in &cast_cards {
        for seat in game_cast_cards {
            for (&card, &count) in seat {
                *casts.entry(card).or_default() += count;
            }
        }
    }
    for name in C4C_CARDS {
        assert!(
            casts.get(&id(name)).copied().unwrap_or(0) > 0,
            "{name} was never cast"
        );
    }
}
