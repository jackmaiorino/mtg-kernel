//! Eternal wave P2b: native rules tests for the Premodern cards that
//! complete Survival and Madness Threshold (and the cards Elves shares with
//! them).
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, AltCostCondition, CardCapability, CardDef, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision};
use mtg_kernel::eternal_premodern_p2b_v1 as p2b;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::{legal_action_candidates_v1, observe_policy_v5, ActionSemanticV1};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface::{SurfaceAction, SurfaceDecision};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P2B_CARDS: [&str; 17] = [
    "Arrogant Wurm",
    "Birds of Paradise",
    "Blastoderm",
    "Caller of the Claw",
    "Chatter of the Squirrel",
    "Deranged Hermit",
    "Gaea's Cradle",
    "Glory",
    "Masticore",
    "Multani's Acolyte",
    "Nantuko Vigilante",
    "Survival of the Fittest",
    "Tireless Tribe",
    "Vintara Snapper",
    "Wirewood Symbiote",
    "Yavimaya Granger",
    "Yavimaya Hollow",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn name_of(state: &GameState, object: ObjectId) -> &'static str {
    CARD_DEFS[state.objects.get(object).card_def as usize].name
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5032_4221_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn islands() -> GameState {
    game(&["Island"; 20])
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

fn pool(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[ManaColor::C.pool_index()] += colorless;
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
}

fn controls(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| name_of(state, object) == name)
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
/// finishing a selection that may finish, declining a Boolean or kicker.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.is_empty() => Action::FinishEffectSelection,
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

/// Settles, answering effect options with `option` and choosing `object`
/// whenever a selection or target offers it; otherwise the first answer.
fn settle_picking(state: &mut GameState, option: u16, object: Option<ObjectId>) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
        Decision::ChooseTargets { legal_targets, .. }
            if object.is_some_and(|object| legal_targets.contains(&Target::Object(object))) =>
        {
            Action::ChooseTarget(Target::Object(object.unwrap()))
        }
        other => first_answer(other),
    });
}

/// Settles, taking the first candidate of every selection that has none
/// selected yet (searches find a card) and the first answer otherwise.
fn settle_taking(state: &mut GameState) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if !legal_targets.is_empty() => Action::ChooseEffectTarget(legal_targets[0]),
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

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).1.contains(&(source, index))
}

/// Casts `spell`, answering each target prompt from `targets` in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        priority(state).0.contains(&spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
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
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

/// Plays to P0's next turn's first main phase, answering upkeep choices with
/// `answer`.
fn to_next_turn(
    state: &mut GameState,
    answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    let turn = state.turn;
    run_until(
        state,
        |state| state.turn > turn && state.active_player == P0 && state.step == Step::Main1,
        answer,
    );
}

/// Moves `object` to its owner's graveyard, as if destroyed.
fn to_graveyard(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// Puts `object` onto the battlefield under its owner's control, as if an
/// effect did.
fn enter(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Battlefield));
}

/// Deals `amount` damage from `source` to `object`.
fn damage(state: &mut GameState, source: ObjectId, object: ObjectId, amount: i32) {
    event::propose_and_commit(
        state,
        ProposedEvent::damage(source, Target::Object(object), amount),
    );
}

/// The targets Lightning Bolt could choose for P0 (the cast is left
/// announcing its target).
fn bolt_targets(state: &mut GameState) -> Vec<Target> {
    let bolt = put(state, P0, "Lightning Bolt", Zone::Hand);
    pool(state, P0, &[ManaColor::R], 0);
    assert!(priority(state).0.contains(&bolt));
    act(state, Action::CastSpell(bolt));
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p2b_cards_are_full_with_their_printed_characteristics() {
    for name in P2B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    for token in ["Bear Token", "Squirrel Token"] {
        assert!(
            def(token).is_token && def(token).capability == CardCapability::Full,
            "{token}"
        );
    }
    let bear = def("Bear Token");
    assert_eq!((bear.power, bear.toughness), (Some(2), Some(2)));
    assert_eq!(bear.subtypes, &[Subtype::Bear]);
    assert!(def("Birds of Paradise").keywords.has(Keywords::FLYING));
    assert!(def("Glory").keywords.has(Keywords::FLYING));
    assert!(def("Caller of the Claw").keywords.has(Keywords::FLASH));
    assert!(def("Arrogant Wurm").keywords.has(Keywords::TRAMPLE));
    assert!(def("Blastoderm").keywords.has(Keywords::SHROUD));
    assert!(!def("Vintara Snapper").keywords.has(Keywords::SHROUD));
    let wurm = def("Arrogant Wurm").madness_cost.expect("madness");
    assert_eq!((wurm.generic, wurm.pips.len()), (2, 1));
    assert!(def("Chatter of the Squirrel").flashback.is_some());
    assert_eq!(
        def("Nantuko Vigilante").alt_cost.map(|alt| alt.condition),
        Some(AltCostCondition::DisguiseFromHand)
    );
    assert_eq!(def("Masticore").subtypes, &[Subtype::Masticore]);
    assert!(Subtype::Masticore.is_creature_type());
    assert_eq!(Subtype::Masticore.stable_id(), 260);
    assert_eq!(
        def("Nantuko Vigilante").subtypes,
        &[Subtype::Insect, Subtype::Druid, Subtype::Mutant]
    );
    assert!(def("Gaea's Cradle")
        .supertypes
        .contains(&mtg_kernel::card_def::Supertype::Legendary));
    assert!(def("Yavimaya Hollow")
        .supertypes
        .contains(&mtg_kernel::card_def::Supertype::Legendary));
}

// ---- Lands and mana ---------------------------------------------------------------

#[test]
fn birds_of_paradise_taps_for_any_color() {
    let mut state = islands();
    let birds = put(&mut state, P0, "Birds of Paradise", Zone::Battlefield);
    assert!(keyword(&state, birds, Keywords::FLYING));
    assert_eq!(power_toughness(&state, birds), (0, 1));
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(birds, ManaColor::R),
    );
    assert_eq!(mana(&state, P0, ManaColor::R), 1);
    assert!(state.objects.get(birds).tapped);
}

#[test]
fn gaeas_cradle_adds_green_for_each_creature_you_control() {
    let mut state = islands();
    let cradle = put(&mut state, P0, "Gaea's Cradle", Zone::Battlefield);
    put(&mut state, P0, "Birds of Paradise", Zone::Battlefield);
    put(&mut state, P0, "Wirewood Symbiote", Zone::Battlefield);
    put(&mut state, P0, "Squirrel Token", Zone::Battlefield);
    put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(cradle));
    assert_eq!(mana(&state, P0, ManaColor::G), 3, "only your creatures");
    assert!(state.objects.get(cradle).tapped);
}

#[test]
fn yavimaya_hollow_adds_colorless_and_regenerates_a_creature() {
    let mut state = islands();
    let hollow = put(&mut state, P0, "Yavimaya Hollow", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(hollow));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);

    let mut state = islands();
    let hollow = put(&mut state, P0, "Yavimaya Hollow", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let source = put(&mut state, P1, "Masticore", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 0);
    activate(&mut state, hollow, 0, &[Target::Object(elves)]);
    settled(&mut state);
    assert!(state.objects.get(hollow).tapped);
    damage(&mut state, source, elves, 4);
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield, "regenerated");
    assert!(state.objects.get(elves).tapped);
    assert_eq!(state.objects.get(elves).damage, 0);
}

// ---- Abilities ---------------------------------------------------------------------

#[test]
fn survival_of_the_fittest_discards_a_creature_card_to_tutor_a_creature() {
    let mut state = game(&["Blastoderm", "Island", "Island"]);
    let survival = put(&mut state, P0, "Survival of the Fittest", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    assert!(
        !activatable(&mut state, survival, 0),
        "no creature card to discard"
    );
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    let birds = put(&mut state, P0, "Birds of Paradise", Zone::Hand);
    activate(&mut state, survival, 0, &[]);
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::Discard { choices, .. } => {
            offered = choices.clone();
            Action::Discard(vec![birds])
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if !legal_targets.is_empty() => Action::ChooseEffectTarget(legal_targets[0]),
        other => first_answer(other),
    });
    assert_eq!(offered, vec![elves, birds], "creature cards only");
    assert_eq!(zone(&state, birds), Zone::Graveyard);
    assert_eq!(zone(&state, island), Zone::Hand);
    assert_eq!(
        state.players[0]
            .hand
            .iter()
            .filter(|&&card| name_of(&state, card) == "Blastoderm")
            .count(),
        1,
        "found the only creature card in the library"
    );
}

#[test]
fn wirewood_symbiote_returns_an_elf_to_untap_a_creature_once_each_turn() {
    let mut state = islands();
    let symbiote = put(&mut state, P0, "Wirewood Symbiote", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let birds = put(&mut state, P0, "Birds of Paradise", Zone::Battlefield);
    state.objects.get_mut(birds).tapped = true;
    activate(&mut state, symbiote, 0, &[Target::Object(birds)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![elves], "an Elf, not the Symbiote or Birds");
            Action::ChooseCostTarget(elves)
        }
        other => first_answer(other),
    });
    assert!(!state.objects.get(birds).tapped);
    assert_eq!(zone(&state, elves), Zone::Hand);
    let again = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(again != elves);
    assert!(!activatable(&mut state, symbiote, 0), "only once each turn");
}

#[test]
fn tireless_tribe_discards_a_card_to_get_plus_zero_plus_four() {
    let mut state = islands();
    let tribe = put(&mut state, P0, "Tireless Tribe", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Hand);
    assert_eq!(power_toughness(&state, tribe), (1, 1));
    activate(&mut state, tribe, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, island), Zone::Graveyard);
    assert_eq!(power_toughness(&state, tribe), (1, 5));
}

#[test]
fn glory_gives_protection_from_a_color_while_in_the_graveyard() {
    let mut state = islands();
    let glory = put(&mut state, P0, "Glory", Zone::Graveyard);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let live_glory = put(&mut state, P0, "Glory", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 2);
    assert!(
        !activatable(&mut state, live_glory, 0),
        "only from the graveyard"
    );
    activate(&mut state, glory, 0, &[]);
    // WUBRG order: red.
    settle_picking(&mut state, 3, None);
    let targets = bolt_targets(&mut state);
    assert!(
        !targets.contains(&Target::Object(elves)),
        "protection from red"
    );
    assert!(!targets.contains(&Target::Object(live_glory)));
    assert!(targets.contains(&Target::Player(P1)));
}

#[test]
fn masticore_pings_regenerates_and_costs_a_discard_each_upkeep() {
    let mut state = islands();
    let masticore = put(&mut state, P0, "Masticore", Zone::Battlefield);
    let birds = put(&mut state, P1, "Birds of Paradise", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, masticore, 0, &[Target::Object(birds)]);
    settled(&mut state);
    assert_eq!(zone(&state, birds), Zone::Graveyard);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, masticore, 1, &[]);
    settled(&mut state);
    let source = put(&mut state, P1, "Blastoderm", Zone::Battlefield);
    damage(&mut state, source, masticore, 5);
    settled(&mut state);
    assert_eq!(zone(&state, masticore), Zone::Battlefield, "regenerated");

    // Upkeep: discarding keeps it, declining sacrifices it.
    for (option, survives) in [(1u16, true), (0, false)] {
        let mut state = islands();
        let masticore = put(&mut state, P0, "Masticore", Zone::Battlefield);
        let card = put(&mut state, P0, "Island", Zone::Hand);
        to_next_turn(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(option)),
            _ => None,
        });
        assert_eq!(zone(&state, masticore) == Zone::Battlefield, survives);
        assert_eq!(zone(&state, card) == Zone::Graveyard, survives);
    }
    // With no card to discard it is sacrificed.
    let mut state = islands();
    let masticore = put(&mut state, P0, "Masticore", Zone::Battlefield);
    to_next_turn(&mut state, |_, _| None);
    assert_eq!(zone(&state, masticore), Zone::Graveyard);
}

// ---- Creatures ---------------------------------------------------------------------

#[test]
fn blastoderm_has_shroud_and_fades_after_three_upkeeps() {
    let mut state = islands();
    let blastoderm = put(&mut state, P0, "Blastoderm", Zone::Hand);
    enter(&mut state, blastoderm);
    assert_eq!(p2b::statics::fade_counters(&state, blastoderm), 3);
    assert_eq!(power_toughness(&state, blastoderm), (5, 5));
    assert!(keyword(&state, blastoderm, Keywords::SHROUD));
    let targets = bolt_targets(&mut state);
    assert!(!targets.contains(&Target::Object(blastoderm)));
    let mut state = islands();
    let blastoderm = put(&mut state, P0, "Blastoderm", Zone::Hand);
    enter(&mut state, blastoderm);
    for remaining in [2, 1, 0] {
        to_next_turn(&mut state, |_, _| None);
        assert_eq!(zone(&state, blastoderm), Zone::Battlefield);
        assert_eq!(p2b::statics::fade_counters(&state, blastoderm), remaining);
    }
    to_next_turn(&mut state, |_, _| None);
    assert_eq!(zone(&state, blastoderm), Zone::Graveyard, "cannot fade");
}

#[test]
fn vintara_snapper_has_shroud_while_you_control_no_untapped_lands() {
    let mut state = islands();
    let snapper = put(&mut state, P0, "Vintara Snapper", Zone::Battlefield);
    assert!(
        keyword(&state, snapper, Keywords::SHROUD),
        "no lands at all"
    );
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    assert!(!keyword(&state, snapper, Keywords::SHROUD));
    put(&mut state, P1, "Forest", Zone::Battlefield);
    state.objects.get_mut(forest).tapped = true;
    assert!(
        keyword(&state, snapper, Keywords::SHROUD),
        "an opponent's untapped land does not count"
    );
    let targets = bolt_targets(&mut state);
    assert!(!targets.contains(&Target::Object(snapper)));
    let mut state = islands();
    let snapper = put(&mut state, P0, "Vintara Snapper", Zone::Battlefield);
    put(&mut state, P0, "Forest", Zone::Battlefield);
    let targets = bolt_targets(&mut state);
    assert!(targets.contains(&Target::Object(snapper)));
}

#[test]
fn arrogant_wurm_is_cast_for_its_madness_cost_when_discarded() {
    let mut state = islands();
    let wurm = put(&mut state, P0, "Arrogant Wurm", Zone::Hand);
    let tribe = put(&mut state, P0, "Tireless Tribe", Zone::Battlefield);
    for _ in 0..3 {
        put(&mut state, P0, "Forest", Zone::Battlefield);
    }
    activate(&mut state, tribe, 0, &[]);
    let mut offered = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseMadnessCast { card, .. } => {
            assert_eq!(card, wurm);
            offered = true;
            Action::ChooseMadnessCast(true)
        }
        other => first_answer(other),
    });
    assert!(offered);
    assert_eq!(zone(&state, wurm), Zone::Battlefield);
    assert_eq!(power_toughness(&state, wurm), (4, 4));
    assert!(keyword(&state, wurm, Keywords::TRAMPLE));
}

#[test]
fn chatter_of_the_squirrel_makes_a_squirrel_and_flashes_back() {
    let mut state = islands();
    let chatter = put(&mut state, P0, "Chatter of the Squirrel", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, chatter, &[]);
    settled(&mut state);
    let squirrels = controls(&state, P0, "Squirrel Token");
    assert_eq!(squirrels.len(), 1);
    assert_eq!(power_toughness(&state, squirrels[0]), (1, 1));
    assert_eq!(zone(&state, chatter), Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, chatter, &[]);
    settled(&mut state);
    assert_eq!(controls(&state, P0, "Squirrel Token").len(), 2);
    assert_eq!(zone(&state, chatter), Zone::Exile, "flashback exiles it");
}

#[test]
fn deranged_hermit_makes_pumped_squirrels_and_asks_for_echo() {
    for (option, stays) in [(0u16, true), (1, false)] {
        let mut state = islands();
        let hermit = put(&mut state, P0, "Deranged Hermit", Zone::Hand);
        for _ in 0..5 {
            put(&mut state, P0, "Forest", Zone::Battlefield);
        }
        enter(&mut state, hermit);
        settled(&mut state);
        let squirrels = controls(&state, P0, "Squirrel Token");
        assert_eq!(squirrels.len(), 4);
        assert_eq!(power_toughness(&state, squirrels[0]), (2, 2), "lord");
        let foreign = put(&mut state, P1, "Squirrel Token", Zone::Battlefield);
        assert_eq!(power_toughness(&state, foreign), (2, 2), "every Squirrel");
        assert_eq!(power_toughness(&state, hermit), (1, 1));
        to_next_turn(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(option)),
            _ => None,
        });
        assert_eq!(zone(&state, hermit) == Zone::Battlefield, stays);
        if stays {
            let tapped = controls(&state, P0, "Forest")
                .into_iter()
                .filter(|&land| state.objects.get(land).tapped)
                .count();
            assert_eq!(tapped, 5, "paid {{3}}{{G}}{{G}}");
            // Echo is paid once: the next upkeep asks nothing.
            to_next_turn(&mut state, |_, decision| match decision {
                Decision::ChooseEffectOption { .. } => panic!("echo is paid only once"),
                _ => None,
            });
            assert_eq!(zone(&state, hermit), Zone::Battlefield);
        } else {
            assert_eq!(power_toughness(&state, squirrels[0]), (1, 1), "lord gone");
        }
    }
}

#[test]
fn echo_is_sacrificed_when_it_cannot_be_paid() {
    let mut state = islands();
    let acolyte = put(&mut state, P0, "Multani's Acolyte", Zone::Hand);
    let card = state.players[0].library.len();
    enter(&mut state, acolyte);
    settled(&mut state);
    assert_eq!(state.players[0].library.len(), card - 1, "drew a card");
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => panic!("nothing to pay with"),
        _ => None,
    });
    assert_eq!(zone(&state, acolyte), Zone::Graveyard);
}

#[test]
fn multanis_acolyte_draws_and_may_pay_echo() {
    let mut state = islands();
    let acolyte = put(&mut state, P0, "Multani's Acolyte", Zone::Hand);
    put(&mut state, P0, "Forest", Zone::Battlefield);
    put(&mut state, P0, "Forest", Zone::Battlefield);
    enter(&mut state, acolyte);
    settled(&mut state);
    assert_eq!(power_toughness(&state, acolyte), (2, 1));
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Some(Action::ChooseEffectOption(0)),
        _ => None,
    });
    assert_eq!(zone(&state, acolyte), Zone::Battlefield, "paid {{G}}{{G}}");
}

#[test]
fn yavimaya_granger_fetches_a_basic_land_tapped_and_asks_for_echo() {
    let mut state = game(&["Forest", "Island", "Island"]);
    let granger = put(&mut state, P0, "Yavimaya Granger", Zone::Hand);
    enter(&mut state, granger);
    settle_taking(&mut state);
    let lands = state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| def(name_of(&state, object)).is_land)
        .collect::<Vec<_>>();
    assert_eq!(lands.len(), 1);
    assert!(state.objects.get(lands[0]).tapped, "enters tapped");
    assert_eq!(power_toughness(&state, granger), (2, 2));
    let mut asked = false;
    to_next_turn(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => {
            asked = true;
            Some(Action::ChooseEffectOption(1))
        }
        _ => None,
    });
    assert!(!asked, "one land cannot pay {{2}}{{G}}");
    assert_eq!(zone(&state, granger), Zone::Graveyard);
}

#[test]
fn nantuko_vigilante_is_turned_face_up_to_destroy_an_artifact_or_enchantment() {
    let mut state = islands();
    let vigilante = put(&mut state, P0, "Nantuko Vigilante", Zone::Hand);
    let bauble = put(&mut state, P1, "Mishra's Bauble", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    act(&mut state, Action::CastSpell(vigilante));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, vigilante), Zone::Battlefield);
    assert!(state.objects.get(vigilante).v4.face_down_v1.is_some());
    assert_eq!(power_toughness(&state, vigilante), (2, 2));
    assert_eq!(zone(&state, bauble), Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::G], 1);
    assert!(activatable(&mut state, vigilante, 255));
    act(&mut state, Action::ActivateAbility(vigilante, 255));
    assert!(state.objects.get(vigilante).v4.face_down_v1.is_none());
    assert_eq!(power_toughness(&state, vigilante), (3, 2));
    settle_picking(&mut state, 0, Some(bauble));
    assert_eq!(zone(&state, bauble), Zone::Graveyard);
}

#[test]
fn birchlore_projects_each_pair_and_pays_only_with_two_distinct_untapped_elves() {
    let mut state = islands();
    let ranger = put(&mut state, P0, "Birchlore Rangers", Zone::Battlefield);
    let elf = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let other_elf = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let birds = put(&mut state, P0, "Birds of Paradise", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    for object in [ranger, elf, other_elf] {
        state.objects.get_mut(object).summoning_sick = true;
    }

    let decision = SurfaceDecision::Decision(next(&mut state));
    let rows = legal_action_candidates_v1(&decision, &state).unwrap();
    let pairs = rows
        .iter()
        .filter_map(|row| match &row.record.semantic {
            ActionSemanticV1::ActivateManaAbilityWithTwoCostTargetsV1 {
                source,
                mana_choice: ManaColor::B,
                cost_targets,
                ..
            } if source.arena_id == ranger.0 => Some(
                cost_targets
                    .clone()
                    .map(|reference| ObjectId(reference.arena_id)),
            ),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        pairs,
        [[ranger, elf], [ranger, other_elf], [elf, other_elf]]
    );

    let before = state.state_hash();
    for pair in [[elf, elf], [ranger, birds], [ranger, theirs]] {
        let action = Action::ActivateManaAbilityWithTwoCostTargetsV1(ranger, ManaColor::B, pair);
        assert!(engine::step(&mut state, action).is_err());
        assert_eq!(state.state_hash(), before);
    }

    let selected =
        Action::ActivateManaAbilityWithTwoCostTargetsV1(ranger, ManaColor::B, [ranger, elf]);
    assert!(rows
        .iter()
        .any(|row| row.surface_action == SurfaceAction::Action(selected.clone())));
    act(&mut state, selected);
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert!(state.stack.is_empty());
    assert!(state.objects.get(ranger).tapped && state.objects.get(elf).tapped);
    assert!(!state.objects.get(other_elf).tapped);
    assert!(!state.objects.get(birds).tapped && !state.objects.get(theirs).tapped);
    let Decision::CastSpellOrPass { mana_abilities, .. } = next(&mut state) else {
        panic!("expected priority");
    };
    assert!(!mana_abilities.contains(&ranger));
}

#[test]
fn birchlore_casts_face_down_and_turns_up_for_one_green() {
    let mut state = islands();
    let ranger = put(&mut state, P0, "Birchlore Rangers", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    act(&mut state, Action::CastSpell(ranger));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        other => first_answer(other),
    });
    assert!(state.objects.get(ranger).v4.face_down_v1.is_some());
    assert_eq!(power_toughness(&state, ranger), (2, 2));
    assert!(!activatable(&mut state, ranger, 255));

    pool(&mut state, P0, &[ManaColor::G], 0);
    assert!(activatable(&mut state, ranger, 255));
    act(&mut state, Action::ActivateAbility(ranger, 255));
    assert!(state.objects.get(ranger).v4.face_down_v1.is_none());
    assert_eq!(power_toughness(&state, ranger), (1, 1));
    assert_eq!(mana(&state, P0, ManaColor::G), 0);
    assert!(state.stack.is_empty());
}

#[test]
fn thornscape_pays_each_kicker_separately_and_only_triggers_what_it_paid() {
    let surface = PolicySurfaceV5::new();
    for free in [false, true] {
        for (red, white) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut state = islands();
            let battlemage = put(&mut state, P0, "Thornscape Battlemage", Zone::Hand);
            let artifact = put(&mut state, P1, "Mishra's Bauble", Zone::Battlefield);
            if free {
                put(&mut state, P0, "Aluren", Zone::Battlefield);
                pool(&mut state, P0, &[ManaColor::R, ManaColor::W], 0);
            } else {
                pool(
                    &mut state,
                    P0,
                    &[ManaColor::G, ManaColor::R, ManaColor::W],
                    2,
                );
            }
            act(&mut state, Action::CastSpell(battlemage));
            assert!(matches!(
                next(&mut state), Decision::ChooseKicker { spell, .. } if spell == battlemage
            ));
            act(&mut state, Action::ChooseKicker(red));
            assert!(matches!(
                next(&mut state),
                Decision::ChooseEffectOption { source, option_count: 2, .. } if source == battlemage
            ));
            let observation = observe_policy_v5(&state, &surface, P0, 0, 0, 0, 1).unwrap();
            let visible = observation.projection.surface.engine_context.pending_cast.unwrap();
            assert_eq!(visible.kicked, Some(red));
            assert_eq!(visible.thornscape_white_kicker_v1, None);
            act(&mut state, Action::ChooseEffectOption(u16::from(white)));
            if matches!(next(&mut state), Decision::ChooseEffectOption { source, .. } if source == battlemage)
            {
                assert!(free);
                let observation = observe_policy_v5(&state, &surface, P0, 0, 0, 0, 1).unwrap();
                let visible = observation.projection.surface.engine_context.pending_cast.unwrap();
                assert_eq!(visible.thornscape_white_kicker_v1, Some(white));
                act(&mut state, Action::ChooseEffectOption(1));
            }
            let Decision::CastSpellOrPass { player, .. } = next(&mut state) else {
                panic!("expected priority after Thornscape Battlemage was published");
            };
            let observation = observe_policy_v5(&state, &surface, player, 0, 0, 0, 1).unwrap();
            let visible = observation.projection.surface.stack.iter().find(|row| {
                row.source.arena_id == battlemage.0
            }).unwrap();
            let kickers = u8::from(red) | (u8::from(white) << 1);
            assert_eq!(visible.thornscape_kickers_v1, (kickers != 0).then_some(kickers));

            settle_with(&mut state, |_, decision| match decision {
                Decision::ChooseTargets { legal_targets, .. } => {
                    let target = if legal_targets.contains(&Target::Player(P1)) {
                        Target::Player(P1)
                    } else {
                        assert!(legal_targets.contains(&Target::Object(artifact)));
                        Target::Object(artifact)
                    };
                    Action::ChooseTarget(target)
                }
                other => first_answer(other),
            });
            assert_eq!(zone(&state, battlemage), Zone::Battlefield);
            assert_eq!(state.players[P1.index()].life, if red { 18 } else { 20 });
            assert_eq!(
                zone(&state, artifact),
                if white {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
            assert_eq!(mana(&state, P0, ManaColor::R), u8::from(!red));
            assert_eq!(mana(&state, P0, ManaColor::W), u8::from(!white));
        }
    }
}

#[test]
fn thornscape_can_enter_from_a_noncast_effect_without_either_kicker() {
    let mut state = islands();
    let artifact = put(&mut state, P1, "Mishra's Bauble", Zone::Battlefield);
    let battlemage = put(&mut state, P0, "Thornscape Battlemage", Zone::Graveyard);
    enter(&mut state, battlemage);
    settled(&mut state);
    assert_eq!(zone(&state, battlemage), Zone::Battlefield);
    assert_eq!(zone(&state, artifact), Zone::Battlefield);
    assert_eq!(state.players[P1.index()].life, 20);
}

#[test]
fn caller_of_the_claw_makes_a_bear_for_each_nontoken_creature_that_died_this_turn() {
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let birds = put(&mut state, P0, "Birds of Paradise", Zone::Battlefield);
    let token = put(&mut state, P0, "Goblin Token", Zone::Battlefield);
    let theirs = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let survivor = put(&mut state, P0, "Wirewood Symbiote", Zone::Battlefield);
    let caller = put(&mut state, P0, "Caller of the Claw", Zone::Hand);
    for creature in [elves, birds, token, theirs] {
        to_graveyard(&mut state, creature);
    }
    settled(&mut state);
    assert_eq!(zone(&state, survivor), Zone::Battlefield);
    enter(&mut state, caller);
    settled(&mut state);
    let bears = controls(&state, P0, "Bear Token");
    assert_eq!(bears.len(), 2, "your nontoken creatures only");
    assert_eq!(power_toughness(&state, bears[0]), (2, 2));
    assert!(has_subtype(&state, bears[0], Subtype::Bear));
    // Creatures that died on an earlier turn do not count.
    to_next_turn(&mut state, |_, _| None);
    let second = put(&mut state, P0, "Caller of the Claw", Zone::Hand);
    enter(&mut state, second);
    settled(&mut state);
    assert_eq!(controls(&state, P0, "Bear Token").len(), 2);
}

#[test]
fn creature_deaths_leave_no_eternal_state_without_a_caller_of_the_claw() {
    let mut state = islands();
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(state.eternal_v1.is_none());
    to_graveyard(&mut state, elves);
    settled(&mut state);
    assert!(
        state.eternal_v1.is_none(),
        "no Caller, so nothing is tracked"
    );
}

fn has_subtype(state: &GameState, object: ObjectId, subtype: Subtype) -> bool {
    engine::has_effective_subtype(state, object, subtype)
}
