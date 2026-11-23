//! Vintage Cube wave C8b: native rules and public-session tests for the
//! Wave C8B Vintage Cube cards.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code, unused_imports)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, mana_color_mask, CardCapability, CardDef, CardType, Keywords, Subtype,
    Supertype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::EternalStateV1;
use mtg_kernel::eternal_cube_c8b_v1::C8bStateV1;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::planeswalker_v1::loyalty;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const C8B_CARDS: [&str; 6] = [
    "Phantasmal Image",
    "Superior Spider-Man",
    "Vaultborn Tyrant",
    "Saheeli, Sublime Artificer",
    "Springheart Nantuko",
    "Detective's Phoenix",
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
        0x4355_4245_0000 + 0x3862,
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

fn has_subtype(state: &GameState, object: ObjectId, subtype: Subtype) -> bool {
    engine::effective_subtype_ids(state, object).contains(&subtype.stable_id())
}

fn has_type(state: &GameState, object: ObjectId, card_type: CardType) -> bool {
    engine::object_has_type(state, object, card_type)
}

fn to_next_main(state: &mut GameState, player: PlayerId) {
    pass_until(state, |s| s.active_player != player);
    pass_until(state, |s| {
        s.active_player == player
            && s.step == Step::Main1
            && s.stack.is_empty()
            && s.engine.pending_triggers.is_empty()
    });
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn c8b_cards_are_full() {
    for name in C8B_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert!(card.is_executable(), "{name}");
    }
    let insect = def("Green Insect Token");
    assert_eq!(insect.capability, CardCapability::Full);
    assert!(insect.is_token);
    assert_eq!((insect.power, insect.toughness), (Some(1), Some(1)));
    assert_eq!(insect.colors, &[ManaColor::G]);
    assert_eq!(insect.types, &[CardType::Creature]);
    assert_eq!(insect.subtypes, &[Subtype::Insect]);

    let image = def("Phantasmal Image");
    assert_eq!((image.power, image.toughness), (Some(0), Some(0)));
    assert_eq!(image.subtypes, &[Subtype::Illusion]);

    let spidey = def("Superior Spider-Man");
    assert_eq!((spidey.power, spidey.toughness), (Some(4), Some(4)));
    assert!(spidey.supertypes.contains(&Supertype::Legendary));
    assert_eq!(
        spidey.subtypes,
        &[Subtype::Spider, Subtype::Human, Subtype::Hero]
    );

    let tyrant = def("Vaultborn Tyrant");
    assert_eq!((tyrant.power, tyrant.toughness), (Some(6), Some(6)));
    assert!(tyrant.keywords.has(Keywords::TRAMPLE));
    assert_eq!(tyrant.subtypes, &[Subtype::Dinosaur]);

    let saheeli = def("Saheeli, Sublime Artificer");
    assert_eq!(saheeli.starting_loyalty, Some(5));
    assert!(saheeli.supertypes.contains(&Supertype::Legendary));
    assert!(saheeli.types.contains(&CardType::Planeswalker));
    assert_eq!(saheeli.subtypes, &[Subtype::Saheeli]);

    let nantuko = def("Springheart Nantuko");
    assert_eq!((nantuko.power, nantuko.toughness), (Some(1), Some(1)));
    assert!(nantuko.bestow.is_some());
    assert!(nantuko.types.contains(&CardType::Enchantment));
    assert!(nantuko.types.contains(&CardType::Creature));

    let phoenix = def("Detective's Phoenix");
    assert_eq!((phoenix.power, phoenix.toughness), (Some(2), Some(2)));
    assert!(phoenix.bestow.is_some());
    assert!(phoenix.keywords.has(Keywords::FLYING));
    assert!(phoenix.keywords.has(Keywords::HASTE));
}

// ---- Phantasmal Image ----------------------------------------------------------

#[test]
fn phantasmal_image_copies_creature_adds_illusion_triggers_etb_and_sacrifices_when_targeted() {
    let mut state = plains();
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, image, &[]);
    let mut saw_choice = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption {
            source,
            option_count,
            ..
        } if source == image => {
            saw_choice = true;
            // Option 0 declines; option 1 copies Baleful Strix.
            assert_eq!(option_count, 2);
            Action::ChooseEffectOption(1)
        }
        other => first_answer(other),
    });
    assert!(saw_choice);
    assert_eq!(zone(&state, strix), Zone::Battlefield);
    assert_eq!(zone(&state, image), Zone::Battlefield);
    assert_eq!(engine::effective_name(&state, image), "Baleful Strix");
    assert_eq!(power_toughness(&state, image), (1, 1));
    assert!(keyword(&state, image, Keywords::FLYING));
    assert!(keyword(&state, image, Keywords::DEATHTOUCH));
    assert!(has_type(&state, image, CardType::Artifact));
    assert!(has_type(&state, image, CardType::Creature));
    assert!(has_subtype(&state, image, Subtype::Bird));
    assert!(has_subtype(&state, image, Subtype::Illusion));
    assert_eq!(
        state.players[0].hand.len(),
        1,
        "copied Baleful Strix ETB draw triggered"
    );

    // Becoming the target of a spell sacrifices Phantasmal Image before the
    // spell resolves, and it arrives in the graveyard as Phantasmal Image.
    let bolt = opponent_bolts(&mut state, Target::Object(image));
    assert_eq!(
        state.stack.len(),
        2,
        "sacrifice trigger sits above Lightning Bolt on the stack"
    );
    settled(&mut state);
    assert_eq!(zone(&state, image), Zone::Graveyard);
    assert_eq!(zone(&state, bolt), Zone::Graveyard);
    assert_eq!(state.objects.get(image).card_def, id("Phantasmal Image"));
    assert_eq!(state.objects.get(image).name, "Phantasmal Image");

    // Becoming the target of an ability (even your own) also sacrifices it.
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, image, &[]);
    // Candidates are P0's battlefield in order: [elves, mother] -> option 1 copies elves.
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, image), Zone::Battlefield);
    assert_eq!(engine::effective_name(&state, image), "Llanowar Elves");
    assert!(has_subtype(&state, image, Subtype::Elf));
    assert!(has_subtype(&state, image, Subtype::Illusion));
    assert_eq!(zone(&state, elves), Zone::Battlefield);

    activate(&mut state, mother, 0, &[Target::Object(image)]);
    settled(&mut state);
    assert_eq!(
        zone(&state, image),
        Zone::Graveyard,
        "targeting with an activated ability triggers the Illusion sacrifice"
    );
}

#[test]
fn phantasmal_image_decline_empty_board_reanimate_and_dress_down() {
    // Empty battlefield: no copy choice is offered; 0/0 dies immediately to SBA.
    let mut state = plains();
    let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, image, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, image), Zone::Graveyard);

    // Declining the copy choice (option 0) enters as 0/0 and dies to SBA.
    let mut state = plains();
    let _elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, image, &[]);
    settle_option(&mut state, 0);
    assert_eq!(zone(&state, image), Zone::Graveyard);

    // Non-cast entry via Reanimate still offers the as-enters copy choice and
    // fires the copied creature's ETB trigger.
    let mut state = plains();
    let _strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    let image = put(&mut state, P0, "Phantasmal Image", Zone::Graveyard);
    let reanimate = put(&mut state, P0, "Reanimate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, reanimate, &[Target::Object(image)]);
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, image), Zone::Battlefield);
    assert_eq!(engine::effective_name(&state, image), "Baleful Strix");
    assert!(has_subtype(&state, image, Subtype::Illusion));
    assert_eq!(state.players[0].hand.len(), 1);

    // Dress Down removes the granted Illusion sacrifice trigger while active.
    let mut state = plains();
    let _wall = put(&mut state, P1, "Wall of Blossoms", Zone::Battlefield);
    let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, image, &[]);
    settle_option(&mut state, 1);
    assert_eq!(power_toughness(&state, image), (0, 4));
    let dress = put(&mut state, P0, "Dress Down", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, dress, &[]);
    settled(&mut state);
    opponent_bolts(&mut state, Target::Object(image));
    settled(&mut state);
    assert_eq!(
        zone(&state, image),
        Zone::Battlefield,
        "Dress Down suppresses Phantasmal Image's sacrifice trigger"
    );
    assert_eq!(state.objects.get(image).damage, 3);

    // Dress Down already on the battlefield suppresses the as-enters copy
    // replacement (CR 614.12), so Phantasmal Image enters as 0/0 and dies.
    let image2 = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, image2, &[]);
    settled(&mut state);
    assert_eq!(
        zone(&state, image2),
        Zone::Graveyard,
        "Dress Down suppresses as-enters copy replacement"
    );
}

// ---- Superior Spider-Man -------------------------------------------------------

#[test]
fn superior_spider_man_mind_swap_overrides_name_cda_pt_subtypes_exiles_and_layer7b() {
    // Copying Nethergoyf in opponent's graveyard overrides its */*+1 CDA to 4/4
    // (CR 707.9d), sets name to Superior Spider-Man, adds Spider/Human/Hero to
    // Lhurgoyf, drops Legendary (Nethergoyf is nonlegendary), keeps Black color,
    // and exiles Nethergoyf via the reflexive trigger.
    let mut state = plains();
    let goyf = put(&mut state, P1, "Nethergoyf", Zone::Graveyard);
    // Give P0 3 distinct card types in graveyard so a real Nethergoyf would be 3/4.
    put(&mut state, P0, "Plains", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Sol Ring", Zone::Graveyard);
    let spidey = put(&mut state, P0, "Superior Spider-Man", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    cast(&mut state, spidey, &[]);
    settle_option(&mut state, 1);

    assert_eq!(zone(&state, spidey), Zone::Battlefield);
    assert_eq!(
        zone(&state, goyf),
        Zone::Exile,
        "reflexive trigger exiled copied graveyard card"
    );
    assert_eq!(
        engine::effective_name(&state, spidey),
        "Superior Spider-Man"
    );
    assert_eq!(
        power_toughness(&state, spidey),
        (4, 4),
        "4/4 copy override replaces Nethergoyf's */*+1 CDA"
    );
    assert!(has_subtype(&state, spidey, Subtype::Lhurgoyf));
    assert!(has_subtype(&state, spidey, Subtype::Spider));
    assert!(has_subtype(&state, spidey, Subtype::Human));
    assert!(has_subtype(&state, spidey, Subtype::Hero));
    assert!(
        !engine::effective_supertypes(&state, spidey).contains(&Supertype::Legendary),
        "not Legendary when copying a nonlegendary creature card"
    );
    assert_eq!(
        engine::object_color_mask(&state, spidey),
        mana_color_mask(ManaColor::B),
        "retains copied card's colors"
    );

    // Layer 7b base P/T setters (Oko's +1 Elk) still override Superior Spider-Man's
    // Layer 1 copy-modified 4/4.
    let oko = put(&mut state, P0, "Oko, Thief of Crowns", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, oko, &[]);
    settled(&mut state);
    activate(&mut state, oko, 1, &[Target::Object(spidey)]);
    settled(&mut state);
    assert_eq!(
        power_toughness(&state, spidey),
        (3, 3),
        "Oko Layer 7b 3/3 wins over Layer 1 4/4 copy override"
    );

    // Copying a creature with an ETB trigger (Baleful Strix in own graveyard)
    // fires the copied ETB AND exiles the graveyard card.
    let mut state = plains();
    let strix = put(&mut state, P0, "Baleful Strix", Zone::Graveyard);
    let spidey = put(&mut state, P0, "Superior Spider-Man", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    cast(&mut state, spidey, &[]);
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, spidey), Zone::Battlefield);
    assert_eq!(zone(&state, strix), Zone::Exile);
    assert_eq!(power_toughness(&state, spidey), (4, 4));
    assert!(keyword(&state, spidey, Keywords::FLYING));
    assert!(keyword(&state, spidey, Keywords::DEATHTOUCH));
    assert!(has_type(&state, spidey, CardType::Artifact));
    assert!(has_subtype(&state, spidey, Subtype::Bird));
    assert!(has_subtype(&state, spidey, Subtype::Spider));
    assert_eq!(
        state.players[0].hand.len(),
        1,
        "copied Baleful Strix ETB drew a card"
    );
}

#[test]
fn superior_spider_man_legend_rule_decline_reanimate_and_exile_fizzle() {
    // Copying a nonlegendary creature yields a nonlegendary "Superior Spider-Man",
    // which coexists with an uncopied Legendary "Superior Spider-Man".
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let spidey1 = put(&mut state, P0, "Superior Spider-Man", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    cast(&mut state, spidey1, &[]);
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, elves), Zone::Exile);
    assert!(!engine::effective_supertypes(&state, spidey1).contains(&Supertype::Legendary));

    let spidey2 = put(&mut state, P0, "Superior Spider-Man", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    cast(&mut state, spidey2, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, spidey1), Zone::Battlefield);
    assert_eq!(zone(&state, spidey2), Zone::Battlefield);

    // Copying a Legendary creature (Ragavan) produces a Legendary "Superior
    // Spider-Man" that triggers the Legend Rule with the Legendary spidey2.
    let ragavan = put(&mut state, P1, "Ragavan, Nimble Pilferer", Zone::Graveyard);
    let spidey3 = put(&mut state, P0, "Superior Spider-Man", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    cast(&mut state, spidey3, &[]);
    let mut saw_legend_rule = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { source, .. } if source == spidey3 => {
            Action::ChooseEffectOption(1)
        }
        Decision::ChooseLegendPermanent { candidates, .. } => {
            saw_legend_rule = true;
            assert!(candidates.contains(&spidey2));
            assert!(candidates.contains(&spidey3));
            assert!(
                !candidates.contains(&spidey1),
                "nonlegendary Superior Spider-Man is exempt from the legend rule"
            );
            Action::ChooseLegendPermanent(spidey3)
        }
        other => first_answer(other),
    });
    assert!(saw_legend_rule);
    assert_eq!(zone(&state, spidey1), Zone::Battlefield);
    assert_eq!(zone(&state, spidey2), Zone::Graveyard);
    assert_eq!(zone(&state, spidey3), Zone::Battlefield);
    assert_eq!(zone(&state, ragavan), Zone::Exile);

    // Declining Mind Swap keeps Superior Spider-Man as 4/4 Legendary and leaves
    // the graveyard card in the graveyard.
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Graveyard);
    let spidey = put(&mut state, P0, "Superior Spider-Man", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    cast(&mut state, spidey, &[]);
    settle_option(&mut state, 0);
    assert_eq!(zone(&state, spidey), Zone::Battlefield);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert!(engine::effective_supertypes(&state, spidey).contains(&Supertype::Legendary));

    // Non-cast entry via Reanimate offers Mind Swap cleanly, and if the copied
    // card leaves the graveyard before the reflexive exile trigger resolves, it
    // does nothing cleanly.
    let mut state = plains();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Graveyard);
    let spidey = put(&mut state, P0, "Superior Spider-Man", Zone::Graveyard);
    let reanimate = put(&mut state, P0, "Reanimate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, reanimate, &[Target::Object(spidey)]);
    let mut chose = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseEffectOption { source, .. } if source == spidey => {
                act(&mut state, Action::ChooseEffectOption(1));
                chose = true;
            }
            Decision::CastSpellOrPass { .. } if chose && !state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(chose);
    // Remove Llanowar Elves from P1's graveyard before the reflexive trigger resolves.
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(elves, Zone::Hand));
    settled(&mut state);
    assert_eq!(zone(&state, spidey), Zone::Battlefield);
    assert_eq!(zone(&state, elves), Zone::Hand);
}

// ---- Vaultborn Tyrant ----------------------------------------------------------

#[test]
fn vaultborn_tyrant_trample_etb_threshold_controller_and_dredge() {
    let mut state = plains();
    let tyrant = put(&mut state, P0, "Vaultborn Tyrant", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 5);
    cast(&mut state, tyrant, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, tyrant), Zone::Battlefield);
    assert!(keyword(&state, tyrant, Keywords::TRAMPLE));
    assert_eq!(life(&state, P0), 23, "own entry gained 3 life");
    assert_eq!(state.players[0].hand.len(), 1, "own entry drew 1 card");

    // Creature with power < 4 does not trigger Vaultborn Tyrant.
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 23);
    assert_eq!(state.players[0].hand.len(), 1);

    // Another controlled creature with power 4 triggers Vaultborn Tyrant.
    let spidey = put(&mut state, P0, "Superior Spider-Man", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 2);
    cast(&mut state, spidey, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 26);
    assert_eq!(state.players[0].hand.len(), 2);

    // Opponent's power >= 4 creature does NOT trigger P0's Vaultborn Tyrant.
    to_next_main(&mut state, P1);
    let opp_tyrant = put(&mut state, P1, "Vaultborn Tyrant", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::G, ManaColor::G], 5);
    cast(&mut state, opp_tyrant, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 26);
    assert_eq!(life(&state, P1), 23);

    // Draw uses the resumable draw path, so Dredge (Stinkweed Imp) replaces it.
    let mut state = plains();
    let imp = put(&mut state, P0, "Stinkweed Imp", Zone::Graveyard);
    let tyrant = put(&mut state, P0, "Vaultborn Tyrant", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::G], 5);
    cast(&mut state, tyrant, &[]);
    let mut dredged = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.contains(&Target::Object(imp)) => {
            dredged = true;
            Action::ChooseEffectTarget(Target::Object(imp))
        }
        other => first_answer(other),
    });
    assert!(dredged, "Vaultborn Tyrant's draw is replaceable by Dredge");
    assert_eq!(life(&state, P0), 23);
    assert!(hand_contains(&state, P0, imp));
    assert_eq!(state.players[0].graveyard.len(), 5);
}

#[test]
fn vaultborn_tyrant_nontoken_dies_creates_artifact_token_copy_and_inherits_copy_mods() {
    let mut state = plains();
    let tyrant = put(&mut state, P0, "Vaultborn Tyrant", Zone::Battlefield);
    // Kill Vaultborn Tyrant with Wrath of God.
    let wrath = put(&mut state, P0, "Wrath of God", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, wrath, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, tyrant), Zone::Graveyard);
    let copies = named(&state, P0, "Vaultborn Tyrant");
    assert_eq!(copies.len(), 1, "nontoken dies trigger created one copy");
    let copy = copies[0];
    assert!(state.objects.get(copy).v4.is_token);
    assert!(has_type(&state, copy, CardType::Artifact));
    assert!(has_type(&state, copy, CardType::Creature));
    assert!(has_subtype(&state, copy, Subtype::Dinosaur));
    assert!(keyword(&state, copy, Keywords::TRAMPLE));
    assert_eq!(power_toughness(&state, copy), (6, 6));
    assert_eq!(
        life(&state, P0),
        23,
        "artifact token copy triggered its own ETB for +3 life"
    );
    assert_eq!(
        state.players[0].hand.len(),
        1,
        "artifact token copy triggered its own ETB to draw 1"
    );

    // When the token copy dies, "if it's not a token" prevents another copy.
    let wrath2 = put(&mut state, P0, "Wrath of God", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, wrath2, &[]);
    settled(&mut state);
    assert!(
        named(&state, P0, "Vaultborn Tyrant").is_empty(),
        "token copy must not create a second token copy on death"
    );

    // Nontoken Phantasmal Image copying Vaultborn Tyrant dies to Wrath of God:
    // the created token copy inherits Artifact, Illusion, and the Illusion
    // sacrifice trigger, while Phantasmal Image arrives in graveyard as itself.
    let mut state = plains();
    let original = put(&mut state, P1, "Vaultborn Tyrant", Zone::Battlefield);
    let image = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, image, &[]);
    settle_option(&mut state, 1);
    assert_eq!(zone(&state, original), Zone::Battlefield);
    assert_eq!(life(&state, P0), 23);

    let wrath = put(&mut state, P0, "Wrath of God", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 2);
    cast(&mut state, wrath, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, image), Zone::Graveyard);
    assert_eq!(state.objects.get(image).card_def, id("Phantasmal Image"));
    let p0_copies = named(&state, P0, "Vaultborn Tyrant");
    assert_eq!(p0_copies.len(), 1);
    let token_copy = p0_copies[0];
    assert!(state.objects.get(token_copy).v4.is_token);
    assert!(has_type(&state, token_copy, CardType::Artifact));
    assert!(has_subtype(&state, token_copy, Subtype::Illusion));
    assert!(has_subtype(&state, token_copy, Subtype::Dinosaur));
    // Targeting the Illusion token copy sacrifices it!
    opponent_bolts(&mut state, Target::Object(token_copy));
    settled(&mut state);
    assert!(named(&state, P0, "Vaultborn Tyrant").is_empty());
}

// ---- Saheeli, Sublime Artificer ------------------------------------------------

#[test]
fn saheeli_hybrid_cast_loyalty_and_noncreature_servo_trigger() {
    // Castable with {1}{U}{R}, {1}{U}{U}, or {1}{R}{R}; enters with 5 loyalty.
    for colors in [
        [ManaColor::U, ManaColor::R],
        [ManaColor::U, ManaColor::U],
        [ManaColor::R, ManaColor::R],
    ] {
        let mut state = plains();
        let saheeli = put(&mut state, P0, "Saheeli, Sublime Artificer", Zone::Hand);
        pool(&mut state, P0, &colors, 1);
        cast(&mut state, saheeli, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, saheeli), Zone::Battlefield);
        assert_eq!(loyalty(&state, saheeli), Some(5));
    }

    let mut state = plains();
    let saheeli = put(&mut state, P0, "Saheeli, Sublime Artificer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    cast(&mut state, saheeli, &[]);
    settled(&mut state);

    // Noncreature spell (Sol Ring) creates a 1/1 colorless Servo artifact creature token.
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    let servos = named(&state, P0, "Servo Token");
    assert_eq!(servos.len(), 1);
    assert_eq!(power_toughness(&state, servos[0]), (1, 1));
    assert!(has_type(&state, servos[0], CardType::Artifact));
    assert!(has_type(&state, servos[0], CardType::Creature));
    assert!(has_subtype(&state, servos[0], Subtype::Servo));
    assert_eq!(engine::object_color_mask(&state, servos[0]), 0);

    // Creature spell (Llanowar Elves) does NOT trigger Saheeli.
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, elves, &[]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Servo Token").len(), 1);

    // Bestow spell (Springheart Nantuko mode 1) is a noncreature Enchantment
    // spell on the stack, so it triggers Saheeli!
    let nantuko = put(&mut state, P0, "Springheart Nantuko", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast_mode(&mut state, nantuko, 1, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(named(&state, P0, "Servo Token").len(), 2);
}

#[test]
fn saheeli_minus_two_copies_until_end_of_turn_rechecks_targets_and_reverts_at_cleanup() {
    let mut state = plains();
    let saheeli = put(&mut state, P0, "Saheeli, Sublime Artificer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    cast(&mut state, saheeli, &[]);
    settled(&mut state);

    // With only one artifact and no other artifact/creature, -2 is not activatable.
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let _opp_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    assert!(
        !activatable(&mut state, saheeli, 0),
        "needs another controlled artifact or creature as second target"
    );

    // Add a controlled creature (Vaultborn Tyrant): now -2 is legal, with slot 0
    // restricted to controlled artifacts and slot 1 excluding slot 0.
    let tyrant = put(&mut state, P0, "Vaultborn Tyrant", Zone::Battlefield);
    assert!(activatable(&mut state, saheeli, 0));
    act(&mut state, Action::ActivateAbility(saheeli, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(ring)]);
            act(&mut state, Action::ChooseTarget(Target::Object(ring)));
        }
        other => panic!("expected first target prompt, got {other:?}"),
    }
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Object(tyrant)]);
            act(&mut state, Action::ChooseTarget(Target::Object(tyrant)));
        }
        other => panic!("expected second target prompt, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(loyalty(&state, saheeli), Some(3));
    assert_eq!(engine::effective_name(&state, ring), "Vaultborn Tyrant");
    assert_eq!(power_toughness(&state, ring), (6, 6));
    assert!(keyword(&state, ring, Keywords::TRAMPLE));
    assert!(has_type(&state, ring, CardType::Artifact));
    assert!(has_type(&state, ring, CardType::Creature));
    assert!(has_subtype(&state, ring, Subtype::Dinosaur));

    // At cleanup (advancing to P1's main phase), Sol Ring reverts cleanly.
    to_next_main(&mut state, P1);
    assert_eq!(engine::effective_name(&state, ring), "Sol Ring");
    assert!(has_type(&state, ring, CardType::Artifact));
    assert!(!has_type(&state, ring, CardType::Creature));

    // If either target becomes illegal before resolution, no copy happens.
    to_next_main(&mut state, P0);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    activate(
        &mut state,
        saheeli,
        0,
        &[Target::Object(ring), Target::Object(elves)],
    );
    next(&mut state);
    opponent_bolts(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(
        engine::effective_name(&state, ring),
        "Sol Ring",
        "illegal second target prevents copying"
    );
    assert!(!has_type(&state, ring, CardType::Creature));
}

// ---- Springheart Nantuko -------------------------------------------------------

#[test]
fn springheart_nantuko_normal_cast_and_unattached_landfall_creates_insect_without_prompt() {
    let mut state = plains();
    let nantuko = put(&mut state, P0, "Springheart Nantuko", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast_mode(&mut state, nantuko, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, nantuko), Zone::Battlefield);
    assert_eq!(power_toughness(&state, nantuko), (1, 1));
    assert!(has_type(&state, nantuko, CardType::Enchantment));
    assert!(has_type(&state, nantuko, CardType::Creature));
    assert!(has_subtype(&state, nantuko, Subtype::Insect));
    assert!(has_subtype(&state, nantuko, Subtype::Monk));

    // Even with {1}{G} floating, unattached Landfall must NOT prompt and must
    // directly create a 1/1 Green Insect Token.
    pool(&mut state, P0, &[ManaColor::G], 1);
    let forest = put(&mut state, P0, "Forest", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(forest));
    settled(&mut state);
    let insects = named(&state, P0, "Green Insect Token");
    assert_eq!(insects.len(), 1);
    assert_eq!(power_toughness(&state, insects[0]), (1, 1));
    assert_eq!(
        engine::object_color_mask(&state, insects[0]),
        mana_color_mask(ManaColor::G)
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 1, "mana untouched");
    assert_eq!(mana(&state, P0, ManaColor::C), 1, "mana untouched");
}

#[test]
fn springheart_nantuko_bestow_landfall_copy_decline_opponent_host_detach_and_fizzle() {
    let mut state = plains();
    let strix = put(&mut state, P0, "Baleful Strix", Zone::Battlefield);
    let nantuko = put(&mut state, P0, "Springheart Nantuko", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast_mode(&mut state, nantuko, 1, &[Target::Object(strix)]);
    settled(&mut state);

    assert_eq!(zone(&state, nantuko), Zone::Battlefield);
    assert!(has_type(&state, nantuko, CardType::Enchantment));
    assert!(!has_type(&state, nantuko, CardType::Creature));
    assert!(has_subtype(&state, nantuko, Subtype::Aura));
    assert_eq!(
        power_toughness(&state, strix),
        (2, 2),
        "enchanted creature gets +1/+1"
    );

    // Landfall with {1}{G}: choosing option 0 pays {1}{G} and creates a token
    // copy of Baleful Strix (triggering its ETB draw) and NO Insect token.
    pool(&mut state, P0, &[ManaColor::G], 1);
    let forest1 = put(&mut state, P0, "Forest", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(forest1));
    let mut saw_pay_prompt = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            saw_pay_prompt = true;
            assert_eq!(option_count, 2);
            Action::ChooseEffectOption(0)
        }
        other => first_answer(other),
    });
    assert!(saw_pay_prompt);
    assert_eq!(mana(&state, P0, ManaColor::G), 0);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert_eq!(named(&state, P0, "Baleful Strix").len(), 2);
    assert!(named(&state, P0, "Green Insect Token").is_empty());
    assert_eq!(state.players[0].hand.len(), 1, "token Strix drew a card");

    // Next turn: declining option 1 keeps {1}{G} and creates a Green Insect Token.
    to_next_main(&mut state, P0);
    pool(&mut state, P0, &[ManaColor::G], 1);
    let forest2 = put(&mut state, P0, "Forest", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(forest2));
    settle_option(&mut state, 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
    assert_eq!(named(&state, P0, "Baleful Strix").len(), 2);
    assert_eq!(named(&state, P0, "Green Insect Token").len(), 1);

    // When enchanted Baleful Strix dies, Springheart Nantuko unattaches and
    // becomes a 1/1 Enchantment Creature - Insect Monk (survives SBA).
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(strix)]);
    settled(&mut state);
    assert_eq!(zone(&state, strix), Zone::Graveyard);
    assert_eq!(zone(&state, nantuko), Zone::Battlefield);
    assert!(has_type(&state, nantuko, CardType::Creature));
    assert!(has_subtype(&state, nantuko, Subtype::Insect));
    assert!(!has_subtype(&state, nantuko, Subtype::Aura));

    // Bestowed onto an opponent's creature: Landfall does NOT prompt {1}{G}
    // ("attached to a creature you control") and directly makes an Insect.
    let mut state = plains();
    let opp_elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let nantuko = put(&mut state, P0, "Springheart Nantuko", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast_mode(&mut state, nantuko, 1, &[Target::Object(opp_elves)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, opp_elves), (2, 2));
    pool(&mut state, P0, &[ManaColor::G], 1);
    let forest = put(&mut state, P0, "Forest", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(forest));
    settled(&mut state);
    assert_eq!(named(&state, P0, "Green Insect Token").len(), 1);
    assert_eq!(mana(&state, P0, ManaColor::G), 1, "no payment offered");

    // CR 702.103f: if Bestow target leaves while on stack, resolves as creature.
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let nantuko = put(&mut state, P0, "Springheart Nantuko", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast_mode(&mut state, nantuko, 1, &[Target::Object(elves)]);
    next(&mut state);
    opponent_bolts(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, nantuko), Zone::Battlefield);
    assert!(has_type(&state, nantuko, CardType::Creature));
    assert_eq!(power_toughness(&state, nantuko), (1, 1));
}

// ---- Detective's Phoenix -------------------------------------------------------

#[test]
fn detectives_phoenix_hand_normal_and_bestow_with_collect_evidence() {
    // From hand with empty graveyard: Bestow is NOT viable (CE 6 unpayable), so
    // {2}{R} casts it directly as a 2/2 Flying Haste Enchantment Creature.
    let mut state = plains();
    let _elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let phoenix = put(&mut state, P0, "Detective's Phoenix", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(
        !castable(&mut state, phoenix),
        "Bestow requires Collect evidence 6"
    );
    pool(&mut state, P0, &[], 2);
    cast(&mut state, phoenix, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, phoenix), Zone::Battlefield);
    assert_eq!(power_toughness(&state, phoenix), (2, 2));
    assert!(keyword(&state, phoenix, Keywords::FLYING));
    assert!(keyword(&state, phoenix, Keywords::HASTE));
    assert!(has_type(&state, phoenix, CardType::Creature));

    // From hand with graveyard MV >= 6 (Mulldrifter MV 5 + Lightning Bolt MV 1):
    // Bestow for {R} exiles both cards and grants +2/+2, Flying, and Haste.
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let drifter = put(&mut state, P0, "Mulldrifter", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let phoenix = put(&mut state, P0, "Detective's Phoenix", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, phoenix));
    cast_mode(&mut state, phoenix, 1, &[Target::Object(elves)]);
    settle_first(&mut state);
    assert_eq!(zone(&state, drifter), Zone::Exile);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(zone(&state, phoenix), Zone::Battlefield);
    assert!(has_type(&state, phoenix, CardType::Enchantment));
    assert!(!has_type(&state, phoenix, CardType::Creature));
    assert!(has_subtype(&state, phoenix, Subtype::Aura));
    assert_eq!(power_toughness(&state, elves), (3, 3));
    assert!(keyword(&state, elves, Keywords::FLYING));
    assert!(keyword(&state, elves, Keywords::HASTE));
}

#[test]
fn detectives_phoenix_graveyard_bestow_excludes_self_fizzle_and_unattaches_from_noncreature() {
    // Phoenix (MV 3) + one Barrowgoyf (MV 3) in graveyard = 6 total including
    // Phoenix, but only 3 excluding Phoenix -> NOT castable from graveyard even
    // with {2}{R} available (normal cast from graveyard is also forbidden).
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let phoenix = put(&mut state, P0, "Detective's Phoenix", Zone::Graveyard);
    let goyf1 = put(&mut state, P0, "Barrowgoyf", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 2);
    assert!(
        !castable(&mut state, phoenix),
        "Phoenix cannot exile itself for Collect evidence 6 or be cast normally from graveyard"
    );

    // Add a second Barrowgoyf (other cards total 3 + 3 = 6): now castable from
    // graveyard ONLY via Bestow for {R} (no ChooseSpellMode choice offered!).
    let goyf2 = put(&mut state, P0, "Barrowgoyf", Zone::Graveyard);
    assert!(castable(&mut state, phoenix));
    act(&mut state, Action::CastSpell(phoenix));
    // Must go directly to Bestow targeting (never ChooseSpellMode).
    choose_target(&mut state, Target::Object(elves));
    settle_first(&mut state);
    assert_eq!(zone(&state, goyf1), Zone::Exile);
    assert_eq!(zone(&state, goyf2), Zone::Exile);
    assert_eq!(zone(&state, phoenix), Zone::Battlefield);
    assert_eq!(power_toughness(&state, elves), (3, 3));
    assert!(keyword(&state, elves, Keywords::FLYING));
    assert!(keyword(&state, elves, Keywords::HASTE));
    assert_eq!(
        mana(&state, P0, ManaColor::C),
        2,
        "paid Bestow {{R}}, not normal {{2}}{{R}}"
    );

    // When enchanted creature dies, Detective's Phoenix becomes a 2/2 Flying
    // Haste creature on the battlefield.
    let kill = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, kill, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, phoenix), Zone::Battlefield);
    assert!(has_type(&state, phoenix, CardType::Creature));
    assert_eq!(power_toughness(&state, phoenix), (2, 2));
    assert!(keyword(&state, phoenix, Keywords::FLYING));
    assert!(keyword(&state, phoenix, Keywords::HASTE));

    // CR 702.103f: Bestow from graveyard with target removed on stack resolves
    // as a 2/2 Flying Haste creature.
    let mut state = plains();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let phoenix = put(&mut state, P0, "Detective's Phoenix", Zone::Graveyard);
    let tyrant = put(&mut state, P0, "Vaultborn Tyrant", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, phoenix));
    act(&mut state, Action::CastSpell(phoenix));
    choose_target(&mut state, Target::Object(elves));
    // Auto-selects the single remaining candidate (Vaultborn Tyrant, MV 7 >= 6),
    // then finishes Collect Evidence so Phoenix lands on the stack.
    let mut on_stack = false;
    for _ in 0..20 {
        match next(&mut state) {
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => act(&mut state, Action::FinishEffectSelection),
            Decision::CastSpellOrPass { .. } if !state.stack.is_empty() => {
                on_stack = true;
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(on_stack);
    assert_eq!(zone(&state, tyrant), Zone::Exile);
    opponent_bolts(&mut state, Target::Object(elves));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Graveyard);
    assert_eq!(zone(&state, phoenix), Zone::Battlefield);
    assert!(has_type(&state, phoenix, CardType::Creature));
    assert_eq!(power_toughness(&state, phoenix), (2, 2));

    // CR 702.103e: Bestowed onto an animated Ghitu Encampment (2/1 -> 4/3 Flying
    // Haste); at cleanup the land stops being a creature and Phoenix detaches
    // back into a 2/2 creature instead of going to the graveyard.
    let mut state = plains();
    let camp = put(&mut state, P0, "Ghitu Encampment", Zone::Battlefield);
    let phoenix = put(&mut state, P0, "Detective's Phoenix", Zone::Graveyard);
    let _tyrant = put(&mut state, P0, "Vaultborn Tyrant", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 1);
    activate(&mut state, camp, 0, &[]);
    settled(&mut state);
    assert!(has_type(&state, camp, CardType::Creature));
    assert!(castable(&mut state, phoenix));
    act(&mut state, Action::CastSpell(phoenix));
    choose_target(&mut state, Target::Object(camp));
    settle_first(&mut state);
    assert_eq!(power_toughness(&state, camp), (4, 3));
    assert!(keyword(&state, camp, Keywords::FLYING));
    assert!(keyword(&state, camp, Keywords::HASTE));

    to_next_main(&mut state, P1);
    assert!(!has_type(&state, camp, CardType::Creature));
    assert_eq!(
        zone(&state, phoenix),
        Zone::Battlefield,
        "Bestow Aura detaches as a creature rather than dying to Aura SBA"
    );
    assert!(has_type(&state, phoenix, CardType::Creature));
    assert!(has_subtype(&state, phoenix, Subtype::Phoenix));
    assert!(!has_subtype(&state, phoenix, Subtype::Aura));
    assert_eq!(power_toughness(&state, phoenix), (2, 2));
}

#[test]
fn empty_c8b_state_hashes_zero_bytes_and_skips_serde() {
    let empty = EternalStateV1::default();
    assert_eq!(empty.c8b, C8bStateV1::default());
    let json = serde_json::to_string(&empty).expect("serialize empty EternalStateV1");
    assert!(
        !json.contains("c8b"),
        "empty C8bStateV1 must skip serde: {json}"
    );
    let mut h_empty = std::collections::hash_map::DefaultHasher::new();
    C8bStateV1::default().hash(&mut h_empty);
    let h_zero = std::collections::hash_map::DefaultHasher::new();
    assert_eq!(
        h_empty.finish(),
        h_zero.finish(),
        "empty C8bStateV1 must hash zero bytes"
    );
}

#[test]
fn oko_layer_four_overrides_vaultborn_and_saheeli_added_artifact_and_insect_token_name() {
    // 1. CR 111.4: Green Insect Token's visible object name is "Insect Token".
    assert_eq!(
        CARD_DEFS[id("Green Insect Token") as usize].object_name,
        "Insect Token"
    );

    // 2. Oko +1 (Layer 4 type-setter) overrides Vaultborn Tyrant artifact token
    //    copy's Layer 1 added Artifact type: becomes a green 3/3 Elk Creature only.
    let mut state = plains();
    let tyrant = put(&mut state, P0, "Vaultborn Tyrant", Zone::Battlefield);
    let oko = put(&mut state, P0, "Oko, Thief of Crowns", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, oko, &[]);
    settled(&mut state);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(tyrant, Zone::Graveyard),
    );
    settled(&mut state);
    let token_copy = *named(&state, P0, "Vaultborn Tyrant")
        .first()
        .expect("artifact token copy created");
    assert!(has_type(&state, token_copy, CardType::Artifact));
    assert!(has_type(&state, token_copy, CardType::Creature));
    activate(&mut state, oko, 1, &[Target::Object(token_copy)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, token_copy), (3, 3));
    assert!(has_type(&state, token_copy, CardType::Creature));
    assert!(
        !has_type(&state, token_copy, CardType::Artifact),
        "Oko Layer 4 Elk override must remove Layer 1 copy-added Artifact type"
    );

    // 3. Oko +1 overrides Saheeli -2's Layer 1 added Artifact type as well.
    let mut state = plains();
    let saheeli = put(&mut state, P0, "Saheeli, Sublime Artificer", Zone::Hand);
    let oko = put(&mut state, P0, "Oko, Thief of Crowns", Zone::Hand);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let bears = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    cast(&mut state, saheeli, &[]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, oko, &[]);
    settled(&mut state);
    activate(
        &mut state,
        saheeli,
        0,
        &[Target::Object(ring), Target::Object(bears)],
    );
    settled(&mut state);
    assert!(has_type(&state, ring, CardType::Artifact));
    assert!(has_type(&state, ring, CardType::Creature));
    activate(&mut state, oko, 1, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, ring), (3, 3));
    assert!(has_type(&state, ring, CardType::Creature));
    assert!(
        !has_type(&state, ring, CardType::Artifact),
        "Oko Layer 4 Elk override must remove Saheeli -2's added Artifact type"
    );
}

#[test]
fn stacked_phantasmal_image_and_absorbing_man_copies_propagate_and_unwind_cleanly() {
    // 1. Phantasmal Image enters copying P1's Absorbing Man; next Main 1 it
    //    copies Sol Ring (stacked C8b permanent copy + C8c absorbing copy).
    let mut state = plains();
    let _opp_ab = put(&mut state, P1, "Absorbing Man", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let img = put(&mut state, P0, "Phantasmal Image", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, img, &[]);
    settle_option(&mut state, 1);
    assert_eq!(
        CARD_DEFS[state.objects.get(img).card_def as usize].name,
        "Absorbing Man"
    );

    let turn_start = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > turn_start
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(ring)) =>
            {
                Some(Action::ChooseTarget(Target::Object(ring)))
            }
            _ => None,
        },
    );
    assert_eq!(engine::effective_name(&state, img), "Absorbing Man");
    assert_eq!(power_toughness(&state, img), (4, 4));
    assert!(has_type(&state, img, CardType::Artifact));
    assert!(has_type(&state, img, CardType::Creature));
    assert!(engine::effective_keywords(&state, img, Keywords::VIGILANCE).has(Keywords::VIGILANCE));

    // 2. A second Phantasmal Image (cast on P1's Main 1) copying P0's active
    //    Absorbing Man copy inherits CR 707.9 exceptions (name "Absorbing Man",
    //    4/4, Legendary, Creature, Artifact, Human, Villain, Illusion, Vigilance).
    let mut clone_state = plains();
    let _p0_ab = put(&mut clone_state, P0, "Absorbing Man", Zone::Battlefield);
    let p0_ring = put(&mut clone_state, P0, "Sol Ring", Zone::Battlefield);
    let ts = clone_state.turn;
    run_until(
        &mut clone_state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > ts
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(p0_ring)) =>
            {
                Some(Action::ChooseTarget(Target::Object(p0_ring)))
            }
            _ => None,
        },
    );
    to_next_main(&mut clone_state, P1);
    let p1_copy = put(&mut clone_state, P1, "Phantasmal Image", Zone::Hand);
    pool(&mut clone_state, P1, &[ManaColor::U], 1);
    cast(&mut clone_state, p1_copy, &[]);
    settle_option(&mut clone_state, 1);
    assert_eq!(zone(&clone_state, p1_copy), Zone::Battlefield);
    assert_eq!(
        engine::effective_name(&clone_state, p1_copy),
        "Absorbing Man"
    );
    assert_eq!(power_toughness(&clone_state, p1_copy), (4, 4));
    assert!(has_type(&clone_state, p1_copy, CardType::Artifact));
    assert!(has_type(&clone_state, p1_copy, CardType::Creature));
    assert!(engine::effective_supertypes(&clone_state, p1_copy).contains(&Supertype::Legendary));
    assert!(has_subtype(&clone_state, p1_copy, Subtype::Human));
    assert!(has_subtype(&clone_state, p1_copy, Subtype::Villain));
    assert!(has_subtype(&clone_state, p1_copy, Subtype::Illusion));
    assert!(
        engine::effective_keywords(&clone_state, p1_copy, Keywords::VIGILANCE)
            .has(Keywords::VIGILANCE)
    );
    assert!(keyword(&clone_state, p1_copy, Keywords::VIGILANCE));

    // 3. Departure unwinding: killing P0's stacked `img` (Phantasmal Image ->
    //    Absorbing Man -> Sol Ring) restores it to Phantasmal Image in graveyard!
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(img, Zone::Graveyard));
    assert_eq!(zone(&state, img), Zone::Graveyard);
    assert_eq!(
        CARD_DEFS[state.objects.get(img).card_def as usize].name,
        "Phantasmal Image",
        "stacked C8b+C8c copy must unwind to physical Phantasmal Image on departure"
    );

    // 4. Absorbing Man copying a Vaultborn Tyrant artifact token copy inherits
    //    its Layer 1 added Artifact type.
    let mut state = plains();
    let ab = put(&mut state, P0, "Absorbing Man", Zone::Battlefield);
    let tyrant = put(&mut state, P0, "Vaultborn Tyrant", Zone::Battlefield);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(tyrant, Zone::Graveyard),
    );
    settled(&mut state);
    let art_tok = *named(&state, P0, "Vaultborn Tyrant")
        .first()
        .expect("Vaultborn artifact token");
    assert!(has_type(&state, art_tok, CardType::Artifact));
    let ts = state.turn;
    run_until(
        &mut state,
        |s| {
            s.active_player == P0
                && s.step == Step::Main1
                && s.turn > ts
                && s.stack.is_empty()
                && s.engine.pending_triggers.is_empty()
        },
        |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(art_tok)) =>
            {
                Some(Action::ChooseTarget(Target::Object(art_tok)))
            }
            _ => None,
        },
    );
    assert!(
        has_type(&state, ab, CardType::Artifact),
        "Absorbing Man must inherit C8b added Artifact type from Vaultborn token copy"
    );
    assert!(has_type(&state, ab, CardType::Creature));
    assert_eq!(power_toughness(&state, ab), (4, 4));
    assert!(engine::effective_keywords(&state, ab, Keywords::VIGILANCE).has(Keywords::VIGILANCE));
}

// ---- Public Sessions -----------------------------------------------------------

#[derive(Default)]
struct C8bEvents {
    casts: [usize; C8B_CARDS.len()],
}

impl C8bEvents {
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
        if let Some(index) = C8B_CARDS.iter().position(|name| *name == card.name) {
            self.casts[index] += 1;
        }
    }
}

#[test]
fn c8b_decks_finish_and_replay_through_public_sessions_both_seats() {
    let sultai_clones = deck(&[
        ("Underground Sea", 10),
        ("Tropical Island", 10),
        ("Sol Ring", 6),
        ("Mox Emerald", 4),
        ("Mox Sapphire", 4),
        ("Phantasmal Image", 6),
        ("Superior Spider-Man", 6),
        ("Vaultborn Tyrant", 6),
        ("Springheart Nantuko", 6),
        ("Llanowar Elves", 2),
    ]);
    let izzet_phoenix = deck(&[
        ("Volcanic Island", 18),
        ("Sol Ring", 6),
        ("Mox Ruby", 4),
        ("Mox Sapphire", 4),
        ("Saheeli, Sublime Artificer", 8),
        ("Detective's Phoenix", 8),
        ("Lightning Bolt", 6),
        ("Faithless Looting", 6),
    ]);
    let mut events = C8bEvents::default();
    // 2 seats x 3 seeds x 2 policies x (original + replay) = 24 games total.
    for pair in [
        [sultai_clones.clone(), izzet_phoenix.clone()],
        [izzet_phoenix, sultai_clones],
    ] {
        for seed in 0x4338_6200_u64..0x4338_6203 {
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
    for (name, casts) in C8B_CARDS.into_iter().zip(events.casts) {
        assert!(casts > 0, "{name} was never cast in a public session");
    }
}
