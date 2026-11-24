//! Eternal wave E7b: native rules tests for the cards that complete Vintage
//! Dredge, Counter-Vine, Blue Tempo, Lurrus DRS, Doomsday and RUG Control,
//! and the artifact cards of Stiflenought, Sphere Shops and Sultai Psychic
//! Frog.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, AltCostCondition, CardCapability, CardDef, CardType, Keywords, Subtype,
    Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E7B_CARDS: [&str; 45] = [
    "Abrupt Decay",
    "Assassin's Trophy",
    "Avatar of Burgeoning Echoes",
    "Basking Rootwalla",
    "Bayou",
    "Bazaar of Baghdad",
    "Blazing Rootwalla",
    "Carnivorous Cultivator",
    "Chain of Vapor",
    "Collector Ouphe",
    "Commandeer",
    "Creeping Chill",
    "Crucible of Worlds",
    "Dack Fayden",
    "Deathrite Shaman",
    "Demonic Consultation",
    "Golgari Grave-Troll",
    "Golgari Thug",
    "Golos, Tireless Pilgrim",
    "Grief",
    "Hollow One",
    "Ichorid",
    "Karn, Argent Defender",
    "Kishla Skimmer",
    "Lodestone Golem",
    "Master of Death",
    "Mindbreak Trap",
    "Minsc & Boo, Timeless Heroes",
    "Narcomoeba",
    "Noxious Revival",
    "Phyrexian Revoker",
    "Prized Amalgam",
    "Ragavan, Nimble Pilferer",
    "Serum Powder",
    "Shambling Shell",
    "Silversmote Ghoul",
    "Squee, Goblin Nabob",
    "Stinkweed Imp",
    "Stonecoil Serpent",
    "Subtlety",
    "Thorn of Amethyst",
    "Torpor Orb",
    "Uro, Titan of Nature's Wrath",
    "Vengevine",
    "Wrenn and Six",
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
        0x4537_4221_0001,
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

fn loyalty(state: &GameState, planeswalker: ObjectId) -> Option<u32> {
    mtg_kernel::planeswalker_v1::loyalty(state, planeswalker)
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
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if object.is_some_and(|object| legal_targets.contains(&Target::Object(object))) => {
            Action::ChooseEffectTarget(Target::Object(object.unwrap()))
        }
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

/// Plays on to the active player's end step, after its beginning-of-end-step
/// triggers resolved.
fn end_step(state: &mut GameState) {
    pass_until(state, |state| {
        state.step == Step::End
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
    });
}

/// Plays on with first answers until `stop` holds.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    run_until(state, stop, |_, _| None);
}

/// P0 attacks with `attackers` in this turn's combat, then the game plays
/// on to P0's second main phase with an empty stack.
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

/// The game plays on to `player`'s next first main phase.
fn to_main(state: &mut GameState, player: PlayerId) {
    pass_until(state, |state| state.active_player != player);
    pass_until(state, |state| {
        state.active_player == player
            && state.step == Step::Main1
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
    });
}

/// P1 becomes the active player in its first main phase, holding priority.
fn opponents_main_phase(state: &mut GameState) {
    state.active_player = P1;
    state.priority_player = P1;
    state.step = Step::Main1;
}

fn hand_contains(state: &GameState, player: PlayerId, object: ObjectId) -> bool {
    state.players[player.index()].hand.contains(&object)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e7b_cards_are_full_with_their_printed_characteristics() {
    for name in E7B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let boo = def("Boo Token");
    assert!(boo.is_token && boo.capability == CardCapability::Full);
    assert_eq!(boo.object_name, "Boo");
    assert_eq!(boo.subtypes, &[Subtype::Hamster]);
    assert!(boo.supertypes.contains(&Supertype::Legendary));
    assert!(boo.keywords.has(Keywords::TRAMPLE) && boo.keywords.has(Keywords::HASTE));
    let jace = def("Jace Token");
    assert!(jace.is_token && jace.capability == CardCapability::Full);
    assert_eq!(jace.object_name, "Jace");
    assert_eq!(jace.types, &[CardType::Planeswalker]);
    assert_eq!(jace.starting_loyalty, Some(0));
    assert_eq!(jace.activated_abilities.len(), 2);
    // Carnivorous Cultivator's prepare spell: a card in no deck.
    let enroot = def("Enroot");
    assert!(!enroot.is_token && enroot.capability == CardCapability::Full);
    assert_eq!(enroot.types, &[CardType::Sorcery]);

    for (name, loyalty) in [
        ("Dack Fayden", 3),
        ("Wrenn and Six", 3),
        ("Minsc & Boo, Timeless Heroes", 3),
    ] {
        assert_eq!(def(name).starting_loyalty, Some(loyalty), "{name}");
    }
    assert!(Subtype::Incarnation.is_creature_type());
    assert!(Subtype::Snake.is_creature_type());
    assert!(Subtype::Hamster.is_creature_type());
    assert!(!Subtype::Trap.is_creature_type());
    assert!(!Subtype::Dack.is_creature_type());
    assert_eq!(def("Bayou").subtypes, &[Subtype::Swamp, Subtype::Forest]);
    assert!(def("Abrupt Decay").spell_cannot_be_countered);
    assert!(def("Grief").keywords.has(Keywords::MENACE));
    for name in ["Ichorid", "Vengevine"] {
        assert!(def(name).keywords.has(Keywords::HASTE), "{name}");
    }
    for name in ["Kishla Skimmer", "Narcomoeba", "Stinkweed Imp", "Subtlety"] {
        assert!(def(name).keywords.has(Keywords::FLYING), "{name}");
    }
    assert!(def("Subtlety").keywords.has(Keywords::FLASH));
    let serpent = def("Stonecoil Serpent");
    assert!(serpent.keywords.has(Keywords::REACH) && serpent.keywords.has(Keywords::TRAMPLE));
    for name in ["Basking Rootwalla", "Blazing Rootwalla"] {
        let cost = def(name).madness_cost.expect("madness");
        assert_eq!((cost.generic, cost.pips.len()), (0, 0), "{name}");
    }
    assert!(def("Uro, Titan of Nature's Wrath").escape.is_some());
    assert_eq!(
        def("Mindbreak Trap").alt_cost.map(|alt| alt.condition),
        Some(AltCostCondition::OpponentCastThreeSpellsThisTurnV1)
    );
    assert_eq!(
        def("Ragavan, Nimble Pilferer")
            .alt_cost
            .map(|alt| alt.condition),
        Some(AltCostCondition::DashV1)
    );
    for name in ["Grief", "Subtlety", "Commandeer"] {
        assert_eq!(
            def(name).alt_cost.map(|alt| alt.condition),
            Some(AltCostCondition::Always),
            "{name}"
        );
    }
    assert!(def("Hollow One").generic_cost_reduction.is_some());
}

// ---- Lands and mana ---------------------------------------------------------------

#[test]
fn bayou_taps_for_black_or_green() {
    let mut state = islands();
    let bayou = put(&mut state, P0, "Bayou", Zone::Battlefield);
    let choices = def("Bayou").mana_ability_choices;
    assert!(choices.contains(&ManaColor::B) && choices.contains(&ManaColor::G));
    assert!(engine::has_effective_subtype(&state, bayou, Subtype::Swamp));
    assert!(engine::has_effective_subtype(
        &state,
        bayou,
        Subtype::Forest
    ));
}

#[test]
fn serum_powder_taps_for_colorless() {
    let mut state = islands();
    let powder = put(&mut state, P0, "Serum Powder", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(powder));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);
}

#[test]
fn serum_powder_exiles_the_hand_and_draws_that_many_before_the_mulligan_choice() {
    let powders = vec![id("Serum Powder"); 40];
    let islands = vec![id("Island"); 40];
    let mut state =
        mtg_kernel::rl::build_deck_pair_state_with_starting_player_v1(7, &powders, &islands, P0)
            .unwrap();
    mtg_kernel::london_mulligan_v1::enable_london_mulligans_v1(&mut state).unwrap();
    let first_hand = state.players[0].hand.clone();
    match engine::advance_until_decision(&mut state) {
        Decision::ChooseEffectOption {
            player,
            source,
            option_count,
        } => {
            assert_eq!((player, option_count), (P0, 2));
            assert!(first_hand.contains(&source));
        }
        other => panic!("{other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(1));
    assert_eq!(state.exile.len(), 7);
    assert!(first_hand
        .iter()
        .all(|card| zone(&state, *card) == Zone::Exile));
    assert_eq!(state.players[0].hand.len(), 7);
    assert_eq!(state.players[0].library.len(), 26);
    // The new hand is offered again; declining leaves the mulligan choice.
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseEffectOption { player: P0, .. }
    ));
    act(&mut state, Action::ChooseEffectOption(0));
    assert_eq!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseLondonMulligan {
            player: P0,
            mulligan_count: 0,
        }
    );
    act(&mut state, Action::ChooseLondonMulligan { mulligan: false });
    assert_eq!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseLondonMulligan {
            player: P1,
            mulligan_count: 0,
        }
    );
    act(&mut state, Action::ChooseLondonMulligan { mulligan: false });
    engine::advance_until_decision(&mut state);
    assert!(state
        .london_mulligans_v1
        .as_ref()
        .is_some_and(|pregame| pregame.is_complete()));
    assert_eq!(state.players[0].hand.len(), 7);
}

// ---- Dredge -----------------------------------------------------------------------

#[test]
fn bazaar_of_baghdad_draws_two_then_discards_three() {
    let mut state = islands();
    let bazaar = put(&mut state, P0, "Bazaar of Baghdad", Zone::Battlefield);
    for _ in 0..3 {
        put(&mut state, P0, "Mountain", Zone::Hand);
    }
    activate(&mut state, bazaar, 0, &[]);
    settle_first(&mut state);
    assert!(state.objects.get(bazaar).tapped);
    assert_eq!(state.players[0].hand.len(), 2, "3 + 2 - 3");
    assert_eq!(state.players[0].graveyard.len(), 3);
}

#[test]
fn dredge_replaces_a_draw_with_a_mill_and_returns_the_card() {
    let mut state = islands();
    let bazaar = put(&mut state, P0, "Bazaar of Baghdad", Zone::Battlefield);
    let thug = put(&mut state, P0, "Golgari Thug", Zone::Graveyard);
    let library = state.players[0].library.len();
    activate(&mut state, bazaar, 0, &[]);
    let mut dredged = 0;
    settle_with(&mut state, |_, decision| match decision {
        // Dredge the first draw, draw the second.
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.contains(&Target::Object(thug)) && dredged == 0 => {
            dredged += 1;
            Action::ChooseEffectTarget(Target::Object(thug))
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if legal_targets.contains(&Target::Object(thug)) => Action::FinishEffectSelection,
        Decision::ChooseEffectTargets {
            selected_count: 1, ..
        } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert_eq!(dredged, 1);
    // Four milled, one drawn; the Thug, the drawn card and the milled cards
    // then fed the discard of three.
    assert_eq!(state.players[0].library.len(), library - 4 - 1);
    assert_eq!(
        state.players[0].hand.len() + state.players[0].graveyard.len(),
        4 + 1 + 1
    );
    assert_eq!(state.players[0].hand.len(), 0, "two cards, discard three");
}

#[test]
fn dredge_needs_as_many_library_cards_as_its_number() {
    let mut state = game(&["Island"; 3]);
    let bazaar = put(&mut state, P0, "Bazaar of Baghdad", Zone::Battlefield);
    put(&mut state, P0, "Golgari Thug", Zone::Graveyard);
    activate(&mut state, bazaar, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { .. } => panic!("dredge 4 needs four cards"),
        other => first_answer(other),
    });
    assert_eq!(state.players[0].library.len(), 1);
}

#[test]
fn the_draw_step_draw_may_be_dredged() {
    let mut state = islands();
    let imp = put(&mut state, P0, "Stinkweed Imp", Zone::Graveyard);
    let mut offered = false;
    run_until(
        &mut state,
        |state| state.active_player == P0 && state.step == Step::Main1 && state.turn > 1,
        |state, decision| match decision {
            Decision::ChooseEffectOption {
                player: P0,
                option_count: 2,
                ..
            } if state.step == Step::Draw => {
                offered = true;
                Some(Action::ChooseEffectOption(1))
            }
            _ => None,
        },
    );
    assert!(offered);
    assert!(hand_contains(&state, P0, imp));
    assert_eq!(state.players[0].graveyard.len(), 5, "dredge 5");
}

#[test]
fn riddler_draw_step_holds_the_first_dredge_trigger_across_the_second_choice() {
    let mut state = islands();
    for _ in 0..2 {
        put(&mut state, P0, "Quantum Riddler", Zone::Battlefield);
    }
    let imp = put(&mut state, P0, "Stinkweed Imp", Zone::Graveyard);
    let thug = put(&mut state, P0, "Golgari Thug", Zone::Graveyard);
    let narcomoeba = put(&mut state, P0, "Narcomoeba", Zone::Library);

    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Draw
    });
    assert!(state.players[P0.index()].hand.is_empty());
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectOption { player: P0, source, option_count: 3 }
            if source == imp
    ));
    act(&mut state, Action::ChooseEffectOption(1));
    assert!(hand_contains(&state, P0, imp));
    assert_eq!(zone(&state, narcomoeba), Zone::Graveyard);

    let second_choice = next(&mut state);
    assert!(matches!(
        second_choice,
        Decision::ChooseEffectOption { player: P0, source, option_count: 2 }
            if source == thug
    ));
    assert!(state.engine.pending_triggers.is_empty());
    assert!(state.stack.is_empty());

    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(next(&mut restored), second_choice);
    assert_eq!(restored.state_hash(), state.state_hash());
    for branch in [&mut state, &mut restored] {
        act(branch, Action::ChooseEffectOption(1));
        assert!(hand_contains(branch, P0, thug));
        assert_eq!(branch.players[P0.index()].hand.len(), 3);
        assert!(matches!(next(branch), Decision::CastSpellOrPass { .. }));
        assert!(matches!(
            branch.stack.as_slice(),
            [trigger] if trigger.kind == StackItemKind::TriggeredAbility
                && trigger.source == narcomoeba && trigger.controller == P0
        ));
        assert!(branch.engine.pending_triggers.is_empty());
        settle_picking(branch, 1, None);
        assert_eq!(zone(branch, narcomoeba), Zone::Battlefield);
    }
    assert_eq!(restored.state_hash(), state.state_hash());
}

#[test]
fn golgari_grave_troll_enters_with_counters_and_regenerates() {
    let mut state = islands();
    for name in ["Golgari Thug", "Narcomoeba", "Island"] {
        put(&mut state, P0, name, Zone::Graveyard);
    }
    let troll = put(&mut state, P0, "Golgari Grave-Troll", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 5);
    cast(&mut state, troll, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(troll).counters.plus1_plus1, 2);
    assert_eq!(power_toughness(&state, troll), (2, 2));

    // "{1}, Remove a +1/+1 counter: Regenerate this creature." A lethal
    // Lightning Bolt then taps it instead.
    pool(&mut state, P0, &[], 1);
    activate(&mut state, troll, 0, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(troll).counters.plus1_plus1, 1);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(troll)]);
    settled(&mut state);
    assert_eq!(zone(&state, troll), Zone::Battlefield, "regenerated");
    assert!(state.objects.get(troll).tapped);
    assert_eq!(state.objects.get(troll).damage, 0);
    // The shield was used up.
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(troll)]);
    settled(&mut state);
    assert_eq!(zone(&state, troll), Zone::Graveyard);
}

#[test]
fn golgari_thug_puts_a_creature_card_on_top_when_it_dies() {
    let mut state = islands();
    let thug = put(&mut state, P0, "Golgari Thug", Zone::Battlefield);
    let narcomoeba = put(&mut state, P0, "Narcomoeba", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(thug)]);
    settle_picking(&mut state, 0, Some(narcomoeba));
    assert_eq!(state.players[0].library.first(), Some(&narcomoeba));
}

#[test]
fn shambling_shell_sacrifices_for_a_counter() {
    let mut state = islands();
    let shell = put(&mut state, P0, "Shambling Shell", Zone::Battlefield);
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Battlefield);
    activate(&mut state, shell, 0, &[Target::Object(delver)]);
    settled(&mut state);
    assert_eq!(zone(&state, shell), Zone::Graveyard);
    assert_eq!(state.objects.get(delver).counters.plus1_plus1, 1);
}

#[test]
fn stinkweed_imp_destroys_a_creature_it_deals_combat_damage_to() {
    let mut state = islands();
    let imp = put(&mut state, P0, "Stinkweed Imp", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Murktide Regent", Zone::Battlefield);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![imp]));
    run_until(
        &mut state,
        |state| state.step == Step::Main2 && state.stack.is_empty(),
        |_, decision| match decision {
            Decision::DeclareBlockers { player: P1, .. } => {
                Some(Action::DeclareBlockers(vec![(blocker, imp)]))
            }
            _ => None,
        },
    );
    assert_eq!(zone(&state, blocker), Zone::Graveyard, "destroyed");
    assert_eq!(zone(&state, imp), Zone::Graveyard, "the 3/3 killed it");
}

#[test]
fn narcomoeba_may_enter_when_milled() {
    let mut state = game(&["Narcomoeba", "Creeping Chill", "Island", "Island", "Island"]);
    let narcomoeba = state.players[0].library[0];
    let chill = state.players[0].library[1];
    let bazaar = put(&mut state, P0, "Bazaar of Baghdad", Zone::Battlefield);
    put(&mut state, P0, "Golgari Thug", Zone::Graveyard);
    activate(&mut state, bazaar, 0, &[]);
    let mut dredged = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            can_finish: true,
            ..
        } if !dredged => {
            dredged = true;
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectTargets { .. } => Action::FinishEffectSelection,
        // Both milled triggers: yes.
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, narcomoeba), Zone::Battlefield);
    assert_eq!(zone(&state, chill), Zone::Exile);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].life, 23);
}

#[test]
fn creeping_chill_drains_three_when_cast() {
    let mut state = islands();
    let chill = put(&mut state, P0, "Creeping Chill", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 3);
    cast(&mut state, chill, &[]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].life, 23);
}

#[test]
fn ichorid_returns_by_exiling_a_black_creature_and_is_sacrificed_at_the_end_step() {
    let mut state = islands();
    let ichorid = put(&mut state, P0, "Ichorid", Zone::Graveyard);
    let fodder = put(&mut state, P0, "Golgari Thug", Zone::Graveyard);
    to_main(&mut state, P0);
    // The first answer exiled nothing: it stayed.
    assert_eq!(zone(&state, ichorid), Zone::Graveyard);
    pass_until(&mut state, |state| state.active_player == P1);
    run_until(
        &mut state,
        |state| state.active_player == P0 && state.step == Step::Main1 && state.stack.is_empty(),
        |_, decision| match decision {
            Decision::ChooseEffectTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(fodder)) =>
            {
                Some(Action::ChooseEffectTarget(Target::Object(fodder)))
            }
            _ => None,
        },
    );
    assert_eq!(zone(&state, fodder), Zone::Exile);
    assert_eq!(zone(&state, ichorid), Zone::Battlefield);
    assert!(keyword(&state, ichorid, Keywords::HASTE));
    end_step(&mut state);
    assert_eq!(zone(&state, ichorid), Zone::Graveyard, "sacrificed");
}

#[test]
fn squee_returns_to_hand_at_upkeep() {
    let mut state = islands();
    let squee = put(&mut state, P0, "Squee, Goblin Nabob", Zone::Graveyard);
    pass_until(&mut state, |state| state.active_player == P1);
    run_until(
        &mut state,
        |state| state.active_player == P0 && state.step == Step::Main1 && state.stack.is_empty(),
        |_, decision| match decision {
            Decision::ChooseEffectOption { player: P0, .. } => Some(Action::ChooseEffectOption(1)),
            _ => None,
        },
    );
    assert!(hand_contains(&state, P0, squee));
}

#[test]
fn master_of_death_surveils_and_returns_for_one_life() {
    let mut state = game(&["Island", "Swamp", "Mountain"]);
    let master = put(&mut state, P0, "Master of Death", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 1);
    cast(&mut state, master, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, master), Zone::Battlefield);
    // Into the graveyard, then back at the next upkeep for 1 life.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(master, Zone::Graveyard),
    );
    pass_until(&mut state, |state| state.active_player == P1);
    run_until(
        &mut state,
        |state| state.active_player == P0 && state.step == Step::Main1 && state.stack.is_empty(),
        |_, decision| match decision {
            Decision::ChooseEffectOption { player: P0, .. } => Some(Action::ChooseEffectOption(1)),
            _ => None,
        },
    );
    assert!(hand_contains(&state, P0, master));
    assert_eq!(state.players[0].life, 19);
}

#[test]
fn silversmote_ghoul_returns_at_the_end_step_after_three_life_and_draws_when_sacrificed() {
    let mut state = islands();
    let ghoul = put(&mut state, P0, "Silversmote Ghoul", Zone::Graveyard);
    event::propose_and_commit(&mut state, ProposedEvent::life_gain(P0, 3));
    end_step(&mut state);
    assert_eq!(zone(&state, ghoul), Zone::Battlefield);
    assert!(state.objects.get(ghoul).tapped);

    let mut state = islands();
    let ghoul = put(&mut state, P0, "Silversmote Ghoul", Zone::Graveyard);
    event::propose_and_commit(&mut state, ProposedEvent::life_gain(P0, 2));
    end_step(&mut state);
    assert_eq!(zone(&state, ghoul), Zone::Graveyard, "only two life");

    let mut state = islands();
    let ghoul = put(&mut state, P0, "Silversmote Ghoul", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 1);
    activate(&mut state, ghoul, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, ghoul), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn prized_amalgam_returns_at_the_next_end_step_when_a_creature_enters_from_a_graveyard() {
    let mut state = game(&["Narcomoeba", "Island", "Island", "Island", "Island"]);
    let narcomoeba = state.players[0].library[0];
    let amalgam = put(&mut state, P0, "Prized Amalgam", Zone::Graveyard);
    let bazaar = put(&mut state, P0, "Bazaar of Baghdad", Zone::Battlefield);
    put(&mut state, P0, "Golgari Thug", Zone::Graveyard);
    activate(&mut state, bazaar, 0, &[]);
    let mut dredged = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            can_finish: true,
            ..
        } if !dredged => {
            dredged = true;
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectTargets { .. } => Action::FinishEffectSelection,
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, narcomoeba), Zone::Battlefield);
    assert_eq!(zone(&state, amalgam), Zone::Graveyard, "not yet");
    end_step(&mut state);
    assert_eq!(zone(&state, amalgam), Zone::Battlefield);
    assert!(state.objects.get(amalgam).tapped);
}

#[test]
fn hollow_one_costs_two_less_per_card_discarded_and_cycles() {
    let mut state = islands();
    let hollow = put(&mut state, P0, "Hollow One", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    assert!(!castable(&mut state, hollow), "five with nothing discarded");
    let other = put(&mut state, P0, "Hollow One", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, other, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, other), Zone::Graveyard, "cycled");
    assert_eq!(state.players[0].hand.len(), 2);
    // Cycling discarded it: {2} less. Three mana remain.
    assert!(castable(&mut state, hollow));
    cast(&mut state, hollow, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, hollow), Zone::Battlefield);
}

#[test]
fn basking_and_blazing_rootwalla_pump_once_each_turn() {
    let mut state = islands();
    let basking = put(&mut state, P0, "Basking Rootwalla", Zone::Battlefield);
    let blazing = put(&mut state, P0, "Blazing Rootwalla", Zone::Battlefield);
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::R, ManaColor::R],
        2,
    );
    activate(&mut state, basking, 0, &[]);
    settled(&mut state);
    activate(&mut state, blazing, 0, &[]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, basking), (3, 3));
    assert_eq!(power_toughness(&state, blazing), (3, 1));
    assert!(!activatable(&mut state, blazing, 0), "once each turn");
}

#[test]
fn rootwallas_are_cast_free_for_madness() {
    let mut state = islands();
    let rootwalla = put(&mut state, P0, "Blazing Rootwalla", Zone::Hand);
    let looting = put(&mut state, P0, "Faithless Looting", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, looting, &[]);
    let mut madness = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::Discard { choices, count, .. } => {
            assert!(choices.contains(&rootwalla));
            let mut discard = vec![rootwalla];
            discard.extend(
                choices
                    .into_iter()
                    .filter(|&card| card != rootwalla)
                    .take(count as usize - 1),
            );
            Action::Discard(discard)
        }
        Decision::ChooseMadnessCast { .. } => {
            madness = true;
            Action::ChooseMadnessCast(true)
        }
        other => first_answer(other),
    });
    assert!(madness);
    assert_eq!(zone(&state, rootwalla), Zone::Battlefield);
}

#[test]
fn vengevine_returns_on_the_second_creature_spell_of_a_turn() {
    let mut state = islands();
    let vengevine = put(&mut state, P0, "Vengevine", Zone::Graveyard);
    let first = put(&mut state, P0, "Delver of Secrets", Zone::Hand);
    let second = put(&mut state, P0, "Delver of Secrets", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, first, &[]);
    settle_picking(&mut state, 1, None);
    assert_eq!(zone(&state, vengevine), Zone::Graveyard, "only one so far");
    cast(&mut state, second, &[]);
    settle_picking(&mut state, 1, None);
    assert_eq!(zone(&state, vengevine), Zone::Battlefield);
    assert!(keyword(&state, vengevine, Keywords::HASTE));
}

#[test]
fn kishla_skimmer_draws_once_each_turn_when_cards_leave_your_graveyard() {
    let mut state = islands();
    put(&mut state, P0, "Kishla Skimmer", Zone::Battlefield);
    let first = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let second = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(first, Zone::Exile));
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 1);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(second, Zone::Exile));
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 1, "only once each turn");
}

// ---- Evoke, free spells and alternative casts -------------------------------------

#[test]
fn grief_makes_the_opponent_discard_and_is_sacrificed_when_evoked() {
    let mut state = islands();
    let grief = put(&mut state, P0, "Grief", Zone::Hand);
    let ritual = put(&mut state, P0, "Dark Ritual", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    put(&mut state, P1, "Island", Zone::Hand);
    cast(&mut state, grief, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Alternative));
            Action::ChooseCastMode(CastMode::Alternative)
        }
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![ritual]);
            Action::ChooseCostTarget(ritual)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, ritual), Zone::Exile, "evoked");
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "the only nonland card");
    assert_eq!(zone(&state, grief), Zone::Graveyard, "evoked");
}

#[test]
fn subtlety_puts_a_creature_spell_on_top_or_bottom_of_its_owners_library() {
    let mut state = islands();
    let subtlety = put(&mut state, P0, "Subtlety", Zone::Hand);
    let regent = put(&mut state, P1, "Delver of Secrets", Zone::Hand);
    opponents_main_phase(&mut state);
    pool(&mut state, P1, &[ManaColor::U], 0);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 2);
    cast(&mut state, regent, &[]);
    // P1 passes; P0 flashes Subtlety in response.
    next(&mut state);
    act(&mut state, Action::Pass);
    assert!(castable(&mut state, subtlety));
    act(&mut state, Action::CastSpell(subtlety));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Normal),
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(regent)));
            Action::ChooseTarget(Target::Object(regent))
        }
        // The owner (P1) chooses the bottom.
        Decision::ChooseEffectOption { player, .. } => {
            assert_eq!(player, P1);
            Action::ChooseEffectOption(1)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, regent), Zone::Library);
    assert_eq!(state.players[1].library.last(), Some(&regent));
    assert_eq!(zone(&state, subtlety), Zone::Battlefield);
}

#[test]
fn mindbreak_trap_is_free_after_three_opposing_spells_and_exiles_spells() {
    let mut state = islands();
    let trap = put(&mut state, P0, "Mindbreak Trap", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    opponents_main_phase(&mut state);
    state.players[1].spells_cast_this_turn = 2;
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    next(&mut state);
    assert_eq!(state.players[1].spells_cast_this_turn, 3);
    act(&mut state, Action::Pass);
    assert!(castable(&mut state, trap), "free with no mana");
    act(&mut state, Action::CastSpell(trap));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(bolt)) =>
        {
            Action::ChooseTarget(Target::Object(bolt))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(state.players[0].life, 20);

    // Without three opposing spells it costs {2}{U}{U}.
    let mut state = islands();
    let trap = put(&mut state, P0, "Mindbreak Trap", Zone::Hand);
    assert!(!castable(&mut state, trap));
}

#[test]
fn commandeer_exiles_two_blue_cards_takes_a_spell_and_may_retarget_it() {
    let mut state = islands();
    let commandeer = put(&mut state, P0, "Commandeer", Zone::Hand);
    let blue_a = put(&mut state, P0, "Brainstorm", Zone::Hand);
    let blue_b = put(&mut state, P0, "Ponder", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    opponents_main_phase(&mut state);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    next(&mut state);
    act(&mut state, Action::Pass);
    assert!(castable(&mut state, commandeer));
    act(&mut state, Action::CastSpell(commandeer));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseCostTargets { candidates, .. } => Action::ChooseCostTarget(candidates[0]),
        Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(bolt)) =>
        {
            Action::ChooseTarget(Target::Object(bolt))
        }
        // The new target: P1.
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(P1)));
            Action::ChooseEffectTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, blue_a), Zone::Exile);
    assert_eq!(zone(&state, blue_b), Zone::Exile);
    assert_eq!(state.players[0].life, 20);
    assert_eq!(state.players[1].life, 17, "the stolen Bolt hit its caster");
}

fn commandeered_pursue_the_past_dredges(caster: PlayerId) {
    let mut state = islands();
    if caster == P1 {
        opponents_main_phase(&mut state);
    }
    let drawer = caster.opponent();
    let pursue = put(&mut state, caster, "Pursue the Past", Zone::Hand);
    let commandeer = put(&mut state, drawer, "Commandeer", Zone::Hand);
    let fodder = put(&mut state, drawer, "Lightning Bolt", Zone::Hand);
    let thug = put(&mut state, drawer, "Golgari Thug", Zone::Graveyard);
    let library = state.players[drawer.index()].library.clone();

    pool(&mut state, caster, &[ManaColor::R, ManaColor::W], 0);
    pool(&mut state, drawer, &[ManaColor::U, ManaColor::U], 5);
    cast(&mut state, pursue, &[]);
    next(&mut state);
    act(&mut state, Action::Pass);
    cast(&mut state, commandeer, &[Target::Object(pursue)]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            selected_count: 0,
            ..
        } if legal_targets.contains(&Target::Object(fodder)) => {
            assert_eq!(player, drawer);
            Action::ChooseEffectTarget(Target::Object(fodder))
        }
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            selected_count: 0,
            ..
        } if legal_targets.contains(&Target::Object(thug)) => {
            assert_eq!(player, drawer);
            Action::ChooseEffectTarget(Target::Object(thug))
        }
        other => first_answer(other),
    });

    assert_eq!(state.players[drawer.index()].life, 22);
    assert!(state.players[drawer.index()].graveyard.contains(&fodder));
    assert!(library[..4]
        .iter()
        .all(|card| state.players[drawer.index()].graveyard.contains(card)));
    assert!(hand_contains(&state, drawer, thug));
    assert!(hand_contains(&state, drawer, library[4]), "second draw");
    assert_eq!(state.players[drawer.index()].library, library[5..]);
    assert!(state.players[caster.index()].graveyard.contains(&pursue));
}

#[test]
fn commandeered_pursue_dredges_after_discard_with_p0_as_caster() {
    commandeered_pursue_the_past_dredges(P0);
}

#[test]
fn commandeered_pursue_dredges_after_discard_with_p1_as_caster() {
    commandeered_pursue_the_past_dredges(P1);
}

#[test]
fn commandeered_artifact_spells_enter_under_the_new_controller() {
    let mut state = islands();
    let commandeer = put(&mut state, P0, "Commandeer", Zone::Hand);
    put(&mut state, P0, "Brainstorm", Zone::Hand);
    put(&mut state, P0, "Ponder", Zone::Hand);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    opponents_main_phase(&mut state);
    pool(&mut state, P1, &[], 1);
    cast(&mut state, ring, &[]);
    next(&mut state);
    act(&mut state, Action::Pass);
    act(&mut state, Action::CastSpell(commandeer));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(ring)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, ring), Zone::Battlefield);
    assert_eq!(state.objects.get(ring).controller, P0);
}

#[test]
fn ragavan_dashes_with_haste_returns_at_end_and_pilfers_on_combat_damage() {
    let mut state = game(&["Lightning Bolt", "Island", "Island", "Island"]);
    let ragavan = put(&mut state, P0, "Ragavan, Nimble Pilferer", Zone::Hand);
    let top = state.players[1].library[0];
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, ragavan, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, ragavan), Zone::Battlefield);
    state.objects.get_mut(ragavan).summoning_sick = true;
    assert!(keyword(&state, ragavan, Keywords::HASTE), "dashed");
    attack_with(&mut state, vec![ragavan], |_, _| None);
    assert_eq!(state.players[1].life, 18);
    assert_eq!(controls(&state, P0, "Treasure Token").len(), 1);
    assert_eq!(zone(&state, top), Zone::Exile);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, top), "may cast the exiled card");
    end_step(&mut state);
    assert!(
        hand_contains(&state, P0, ragavan),
        "returned at the end step"
    );
}

#[test]
fn carnivorous_cultivator_enters_prepared_and_its_enroot_copy_is_cast_from_exile() {
    let copies = |state: &GameState| {
        state
            .exile
            .iter()
            .copied()
            .filter(|&id| name_of(state, id) == "Enroot")
            .collect::<Vec<_>>()
    };
    let mut state = islands();
    let cultivator = put(&mut state, P0, "Carnivorous Cultivator", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, cultivator, &[]);
    settled(&mut state);
    assert!(keyword(&state, cultivator, Keywords::DEATHTOUCH));
    let enroot = copies(&state);
    assert_eq!(enroot.len(), 1, "it enters prepared");
    let enroot = enroot[0];
    assert!(!castable(&mut state, enroot), "Enroot costs {{G}}");
    pool(&mut state, P0, &[ManaColor::G], 0);
    let library = state.players[0].library.len();
    cast(&mut state, enroot, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => Action::ChooseEffectTarget(legal_targets[0]),
        other => first_answer(other),
    });
    assert_eq!(state.players[0].library.len(), library - 1);
    let graveyard: Vec<&str> = state.players[0]
        .graveyard
        .iter()
        .map(|&id| name_of(&state, id))
        .collect();
    assert_eq!(graveyard, vec!["Island"], "the copy ceased to exist");
    assert!(copies(&state).is_empty(), "no longer prepared");
    assert!(!state.engine.event_history.iter().any(|event| matches!(
        event,
        CommittedEvent::ZoneChange { object, from: Zone::Stack, .. } if *object == enroot
    )));

    // Combat damage to a player returns a land card from the graveyard.
    let island = state.players[0].graveyard[0];
    state.objects.get_mut(cultivator).summoning_sick = false;
    attack_with(&mut state, vec![cultivator], |_, _| None);
    assert_eq!(state.players[1].life, 18);
    assert!(hand_contains(&state, P0, island));

    // A prepared creature that leaves takes its copy with it.
    let mut state = islands();
    let cultivator = put(&mut state, P0, "Carnivorous Cultivator", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, cultivator, &[]);
    settled(&mut state);
    let enroot = copies(&state)[0];
    let decay = put(&mut state, P1, "Abrupt Decay", Zone::Hand);
    opponents_main_phase(&mut state);
    pool(&mut state, P1, &[ManaColor::B, ManaColor::G], 0);
    cast(&mut state, decay, &[Target::Object(cultivator)]);
    settled(&mut state);
    assert_eq!(zone(&state, cultivator), Zone::Graveyard);
    assert!(!state.exile.contains(&enroot), "the copy ceases to exist");
}

#[test]
fn sailors_bane_ignores_prepared_spell_copies_in_exile() {
    let mut state = islands();
    let cultivator = put(&mut state, P0, "Carnivorous Cultivator", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, cultivator, &[]);
    settled(&mut state);
    assert!(state
        .exile
        .iter()
        .any(|&id| name_of(&state, id) == "Enroot"));

    let bane = put(&mut state, P0, "Sailors' Bane", Zone::Hand);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 5);
    assert!(
        !castable(&mut state, bane),
        "the exiled copy does not reduce the cost"
    );

    put(&mut state, P0, "Grapeshot", Zone::Exile);
    assert!(
        castable(&mut state, bane),
        "an exiled sorcery card reduces the cost"
    );
    cast(&mut state, bane, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bane), Zone::Battlefield);
}

#[test]
fn prepared_spell_copies_cease_when_removed_from_the_stack() {
    for (response, colors, generic) in [
        ("Counterspell", [ManaColor::U, ManaColor::U], 0),
        ("No More Lies", [ManaColor::W, ManaColor::U], 0),
        ("Mindbreak Trap", [ManaColor::U, ManaColor::U], 2),
    ] {
        let mut state = islands();
        let cultivator = put(&mut state, P0, "Carnivorous Cultivator", Zone::Hand);
        let response = put(&mut state, P1, response, Zone::Hand);
        pool(&mut state, P0, &[ManaColor::G], 1);
        cast(&mut state, cultivator, &[]);
        settled(&mut state);
        let enroot = *state
            .exile
            .iter()
            .find(|&&id| name_of(&state, id) == "Enroot")
            .expect("prepared Enroot copy");

        pool(&mut state, P0, &[ManaColor::G], 0);
        let library = state.players[0].library.len();
        cast(&mut state, enroot, &[]);
        next(&mut state);
        assert_eq!(state.players[0].spells_cast_this_turn, 2);
        act(&mut state, Action::Pass);
        pool(&mut state, P1, &colors, generic);
        cast(&mut state, response, &[Target::Object(enroot)]);
        settle_first(&mut state);

        assert_eq!(state.players[0].library.len(), library);
        assert!(!state.players[0].graveyard.contains(&enroot));
        assert!(!state.exile.contains(&enroot));
        assert!(state.stack.is_empty());
        assert!(state.engine.event_history.iter().any(|event| matches!(
            event,
            CommittedEvent::SpellCast { spell, controller: P0 } if *spell == enroot
        )));
        assert!(!state.engine.event_history.iter().any(|event| matches!(
            event,
            CommittedEvent::ZoneChange { object, from: Zone::Stack, .. } if *object == enroot
        )));
        let prepare = &state.eternal_v1.as_ref().unwrap().e7b;
        assert!(prepare.prepared.is_empty());
        assert!(prepare.prepare_copies.is_empty());
    }
}

// ---- Removal and graveyard hate ---------------------------------------------------

#[test]
fn abrupt_decay_destroys_a_nonland_permanent_with_mana_value_three_or_less() {
    let mut state = islands();
    let decay = put(&mut state, P0, "Abrupt Decay", Zone::Hand);
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    let regent = put(&mut state, P1, "Murktide Regent", Zone::Battlefield);
    let land = put(&mut state, P1, "Island", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 0);
    act(&mut state, Action::CastSpell(decay));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(strix)));
            assert!(!legal_targets.contains(&Target::Object(regent)));
            assert!(!legal_targets.contains(&Target::Object(land)));
            act(&mut state, Action::ChooseTarget(Target::Object(strix)));
        }
        other => panic!("{other:?}"),
    }
    settled(&mut state);
    assert_eq!(zone(&state, strix), Zone::Graveyard);
}

#[test]
fn assassins_trophy_destroys_and_its_controller_may_search_a_basic_land() {
    let mut state = game(&["Forest", "Island", "Island"]);
    let forest = state.players[1].library[0];
    let trophy = put(&mut state, P0, "Assassin's Trophy", Zone::Hand);
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 0);
    cast(&mut state, trophy, &[Target::Object(strix)]);
    let mut searched = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { player, .. } => {
            assert_eq!(player, P1, "the permanent's controller chooses to search");
            Action::ChooseEffectOption(1)
        }
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert_eq!(player, P1);
            assert!(legal_targets.contains(&Target::Object(forest)));
            searched = true;
            Action::ChooseEffectTarget(Target::Object(forest))
        }
        Decision::ChooseEffectTargets { .. } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert!(searched);
    assert_eq!(zone(&state, strix), Zone::Graveyard);
    assert_eq!(zone(&state, forest), Zone::Battlefield);
    assert!(!state.objects.get(forest).tapped);
    assert_eq!(state.objects.get(forest).controller, P1);

    // Declining searches nothing and does not shuffle.
    let mut state = game(&["Forest", "Island", "Island"]);
    let before = state.players[1].library.clone();
    let trophy = put(&mut state, P0, "Assassin's Trophy", Zone::Hand);
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 0);
    cast(&mut state, trophy, &[Target::Object(strix)]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption {
            player,
            option_count: 2,
            ..
        }) => assert_eq!(player, P1),
        other => panic!("expected the choice to search, got {other:?}"),
    }
    act(&mut state, Action::ChooseEffectOption(0));
    settled(&mut state);
    assert_eq!(zone(&state, strix), Zone::Graveyard);
    assert_eq!(state.players[1].library, before);
}

#[test]
fn chain_of_vapor_bounces_and_its_controller_may_sacrifice_a_land_to_copy_it() {
    let mut state = islands();
    let chain = put(&mut state, P0, "Chain of Vapor", Zone::Hand);
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    let regent = put(&mut state, P1, "Murktide Regent", Zone::Battlefield);
    let mine = put(&mut state, P0, "Delver of Secrets", Zone::Battlefield);
    let land = put(&mut state, P1, "Island", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, chain, &[Target::Object(strix)]);
    settle_with(&mut state, |_, decision| match decision {
        // P1 sacrifices its Island and copies the spell onto P0's Delver.
        Decision::ChooseEffectTargets {
            player: P1,
            legal_targets,
            selected_count: 0,
            ..
        } if legal_targets.contains(&Target::Object(land)) => {
            Action::ChooseEffectTarget(Target::Object(land))
        }
        Decision::ChooseEffectTargets {
            player: P1,
            legal_targets,
            selected_count: 0,
            ..
        } if legal_targets.contains(&Target::Object(mine)) => {
            Action::ChooseEffectTarget(Target::Object(mine))
        }
        Decision::ChooseEffectTargets { .. } => Action::FinishEffectSelection,
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P1, strix));
    assert_eq!(zone(&state, land), Zone::Graveyard);
    assert!(hand_contains(&state, P0, mine), "the copy returned it");
    assert_eq!(zone(&state, regent), Zone::Battlefield);
}

#[test]
fn noxious_revival_puts_a_graveyard_card_on_top_of_its_owners_library() {
    let mut state = islands();
    let revival = put(&mut state, P0, "Noxious Revival", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    // {G/P}: two life instead of green mana.
    cast(&mut state, revival, &[Target::Object(bolt)]);
    settle_first(&mut state);
    assert_eq!(state.players[1].library.first(), Some(&bolt));
    assert_eq!(state.players[0].life, 18);
}

#[test]
fn deathrite_shaman_exiles_graveyard_cards_for_mana_drain_and_life() {
    let mut state = islands();
    let shaman = put(&mut state, P0, "Deathrite Shaman", Zone::Battlefield);
    let land = put(&mut state, P1, "Island", Zone::Graveyard);
    activate(&mut state, shaman, 0, &[Target::Object(land)]);
    settle_picking(&mut state, 4, None);
    assert_eq!(zone(&state, land), Zone::Exile);
    assert_eq!(mana(&state, P0, ManaColor::G), 1);

    let mut state = islands();
    let shaman = put(&mut state, P0, "Deathrite Shaman", Zone::Battlefield);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::B], 0);
    activate(&mut state, shaman, 1, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(state.players[1].life, 18);

    let mut state = islands();
    let shaman = put(&mut state, P0, "Deathrite Shaman", Zone::Battlefield);
    let delver = put(&mut state, P1, "Delver of Secrets", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::G], 0);
    activate(&mut state, shaman, 2, &[Target::Object(delver)]);
    settled(&mut state);
    assert_eq!(zone(&state, delver), Zone::Exile);
    assert_eq!(state.players[0].life, 22);
}

#[test]
fn demonic_consultation_exiles_six_then_finds_the_named_card() {
    let mut library = vec!["Island"; 8];
    library.push("Lightning Bolt");
    library.extend(["Island"; 3]);
    let mut state = game(&library);
    let bolt = state.players[0].library[8];
    let consultation = put(&mut state, P0, "Demonic Consultation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, consultation, &[]);
    // Name "Lightning Bolt": the letter group, then the name within it.
    let bolt_id = id("Lightning Bolt");
    let groups = mtg_kernel::eternal_vintage_v1::nameable_definitions();
    let group = groups
        .iter()
        .position(|ids| ids.contains(&bolt_id))
        .unwrap();
    let within = groups[group]
        .iter()
        .position(|&named| named == bolt_id)
        .unwrap();
    let mut answers = vec![group, within].into_iter();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => {
            Action::ChooseEffectOption(u16::try_from(answers.next().unwrap()).unwrap())
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, bolt));
    assert_eq!(state.exile.len(), 8, "six, then two more revealed");
    assert_eq!(state.players[0].library.len(), 3);
}

#[test]
fn collector_ouphe_stops_artifact_abilities_including_mana_abilities() {
    let mut state = islands();
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    put(&mut state, P1, "Collector Ouphe", Zone::Battlefield);
    next(&mut state);
    assert!(engine::step(&mut state, Action::ActivateManaAbility(ring)).is_err());
}

#[test]
fn phyrexian_revoker_names_a_nonland_card_whose_abilities_cant_be_activated() {
    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let revoker = put(&mut state, P0, "Phyrexian Revoker", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, revoker, &[]);
    let ring_id = id("Sol Ring");
    settle_with(&mut state, |state, decision| match decision {
        Decision::ChooseEffectOption { .. } => {
            let options = mtg_kernel::eternal_lurrus_v1::needle_name_options(state, P0)
                .into_iter()
                .filter(|&named| !CARD_DEFS[named as usize].is_land)
                .collect::<Vec<_>>();
            let index = options.iter().position(|&named| named == ring_id).unwrap();
            Action::ChooseEffectOption(u16::try_from(index).unwrap())
        }
        other => first_answer(other),
    });
    assert_eq!(
        mtg_kernel::eternal_vintage_e7b_v1::statics::revoker_name(&state, revoker),
        Some(ring_id)
    );
    opponents_main_phase(&mut state);
    next(&mut state);
    assert!(engine::step(&mut state, Action::ActivateManaAbility(ring)).is_err());
}

// ---- Statics ----------------------------------------------------------------------

#[test]
fn thorn_of_amethyst_and_lodestone_golem_tax_spells_by_type() {
    // Thorn of Amethyst: noncreature spells cost {1} more.
    let mut state = islands();
    put(&mut state, P1, "Thorn of Amethyst", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, bolt), "red plus one");
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, bolt));
    let mut state = islands();
    put(&mut state, P1, "Thorn of Amethyst", Zone::Battlefield);
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(castable(&mut state, delver), "creatures are untaxed");

    // Lodestone Golem: nonartifact spells cost {1} more.
    let mut state = islands();
    put(&mut state, P1, "Lodestone Golem", Zone::Battlefield);
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Hand);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(!castable(&mut state, delver), "blue plus one");
    assert!(castable(&mut state, ring), "artifacts are untaxed");
}

#[test]
fn torpor_orb_stops_creature_entry_triggers() {
    let mut state = islands();
    put(&mut state, P1, "Torpor Orb", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, inspector), Zone::Battlefield);
    assert!(controls(&state, P0, "Clue Token").is_empty(), "no trigger");
    // Noncreature permanents still trigger.
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Hand);
    cast(&mut state, bauble, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bauble), Zone::Battlefield);
}

#[test]
fn karn_argent_defender_stops_artifact_and_creature_entry_triggers() {
    let mut state = islands();
    put(&mut state, P1, "Karn, Argent Defender", Zone::Battlefield);
    let inspector = put(&mut state, P0, "Thraben Inspector", Zone::Hand);
    let strix = put(&mut state, P0, "Baleful Strix", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::U, ManaColor::B],
        0,
    );
    cast(&mut state, inspector, &[]);
    settled(&mut state);
    cast(&mut state, strix, &[]);
    settled(&mut state);
    assert!(controls(&state, P0, "Clue Token").is_empty());
    assert!(state.players[0].hand.is_empty(), "Strix drew nothing");
    assert_eq!(
        power_toughness(&state, controls(&state, P1, "Karn, Argent Defender")[0]),
        (1, 3)
    );
}

#[test]
fn crucible_of_worlds_plays_lands_from_the_graveyard() {
    let mut state = islands();
    put(&mut state, P0, "Crucible of Worlds", Zone::Battlefield);
    let land = put(&mut state, P0, "Bayou", Zone::Graveyard);
    next(&mut state);
    act(&mut state, Action::PlayLand(land));
    assert_eq!(zone(&state, land), Zone::Battlefield);

    let mut state = islands();
    let land = put(&mut state, P0, "Bayou", Zone::Graveyard);
    next(&mut state);
    assert!(engine::step(&mut state, Action::PlayLand(land)).is_err());
}

#[test]
fn stonecoil_serpent_enters_with_x_counters_and_has_protection_from_multicolored() {
    let mut state = islands();
    let serpent = put(&mut state, P0, "Stonecoil Serpent", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    cast(&mut state, serpent, &[]);
    settle_with(&mut state, |_, decision| match decision {
        // X: the largest payable value.
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(option_count - 1)
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, serpent), Zone::Battlefield);
    assert_eq!(power_toughness(&state, serpent), (3, 3));
    // Multicolored sources can't target it; a monocolored one can.
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let decay = put(&mut state, P1, "Abrupt Decay", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    opponents_main_phase(&mut state);
    pool(
        &mut state,
        P1,
        &[ManaColor::B, ManaColor::G, ManaColor::R],
        0,
    );
    let mut probe = state.clone();
    act(&mut probe, Action::CastSpell(decay));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(ring)));
            assert!(!legal_targets.contains(&Target::Object(serpent)));
        }
        other => panic!("{other:?}"),
    }
    act(&mut state, Action::CastSpell(bolt));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(serpent)));
        }
        other => panic!("{other:?}"),
    }
}

// ---- Planeswalkers ----------------------------------------------------------------

#[test]
fn dack_fayden_loots_steals_artifacts_and_its_emblem_steals_targets() {
    let mut state = islands();
    let dack = put(&mut state, P0, "Dack Fayden", Zone::Hand);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    cast(&mut state, dack, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, dack), Some(3));
    activate(&mut state, dack, 1, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(state.objects.get(ring).controller, P0);
    assert_eq!(loyalty(&state, dack), Some(1));

    to_main(&mut state, P0);
    let before = state.players[0].hand.len();
    activate(&mut state, dack, 0, &[Target::Player(P0)]);
    settle_first(&mut state);
    assert_eq!(state.players[0].hand.len(), before, "draw two, discard two");
    assert_eq!(loyalty(&state, dack), Some(2));

    // The -6 emblem.
    let mut state = islands();
    let dack = put(&mut state, P0, "Dack Fayden", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::R], 1);
    cast(&mut state, dack, &[]);
    settled(&mut state);
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, dack, 3);
    activate(&mut state, dack, 2, &[]);
    settled(&mut state);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Object(angler)]);
    // The emblem's trigger resolves first; the 5/5 survives the Bolt.
    settled(&mut state);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).controller, P0);
}

#[test]
fn wrenn_and_six_returns_lands_pings_and_makes_an_emblem() {
    let mut state = islands();
    let wrenn = put(&mut state, P0, "Wrenn and Six", Zone::Hand);
    let land = put(&mut state, P0, "Bayou", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
    cast(&mut state, wrenn, &[]);
    settled(&mut state);
    activate(&mut state, wrenn, 0, &[Target::Object(land)]);
    settled(&mut state);
    assert!(hand_contains(&state, P0, land));
    assert_eq!(loyalty(&state, wrenn), Some(4));

    to_main(&mut state, P0);
    let delver = put(&mut state, P1, "Delver of Secrets", Zone::Battlefield);
    activate(&mut state, wrenn, 1, &[Target::Object(delver)]);
    settled(&mut state);
    assert_eq!(zone(&state, delver), Zone::Graveyard);
    assert_eq!(loyalty(&state, wrenn), Some(3));
}

#[test]
fn wrenn_and_six_emblem_gives_graveyard_instants_and_sorceries_retrace() {
    let mut state = islands();
    let wrenn = put(&mut state, P0, "Wrenn and Six", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
    cast(&mut state, wrenn, &[]);
    settled(&mut state);
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, wrenn, 4);
    activate(&mut state, wrenn, 2, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, wrenn), Zone::Graveyard, "no loyalty left");

    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(!castable(&mut state, bolt), "no land card to discard");
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Hand);
    assert!(!castable(&mut state, bolt), "a nonland card can't pay");
    let island = put(&mut state, P0, "Island", Zone::Hand);
    let bayou = put(&mut state, P0, "Bayou", Zone::Hand);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    match next(&mut state) {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert!(candidates.contains(&island) && candidates.contains(&bayou));
            assert!(!candidates.contains(&delver));
        }
        other => panic!("{other:?}"),
    }
    act(&mut state, Action::ChooseCostTarget(bayou));
    settled(&mut state);
    assert_eq!(zone(&state, bayou), Zone::Graveyard, "discarded");
    assert!(hand_contains(&state, P0, island) && hand_contains(&state, P0, delver));
    assert_eq!(state.players[1].life, 17);
    assert_eq!(
        zone(&state, bolt),
        Zone::Graveyard,
        "a retraced card returns"
    );
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, bolt), "and may be cast again");
}

#[test]
fn minsc_and_boo_make_boo_grow_it_and_fling_it() {
    let mut state = game(&["Island"; 10]);
    let minsc = put(&mut state, P0, "Minsc & Boo, Timeless Heroes", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 2);
    cast(&mut state, minsc, &[]);
    settle_picking(&mut state, 1, None);
    let boo = controls(&state, P0, "Boo Token");
    assert_eq!(boo.len(), 1);
    let boo = boo[0];
    assert!(keyword(&state, boo, Keywords::TRAMPLE) && keyword(&state, boo, Keywords::HASTE));
    activate(&mut state, minsc, 0, &[Target::Object(boo)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, boo), (4, 4));
    assert_eq!(loyalty(&state, minsc), Some(4));

    // -2: sacrifice Boo (a Hamster): 4 damage to P1 and draw four.
    to_main(&mut state, P0);
    let hand = state.players[0].hand.len();
    activate(&mut state, minsc, 1, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 16);
    assert_eq!(state.players[0].hand.len(), hand + 4);
}

#[test]
fn minsc_hamster_draw_resumes_after_damage_and_fizzles_with_its_target_both_seats() {
    use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};

    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(&["Island"; 12]);
        state.active_player = actor;
        state.priority_player = actor;
        let jace = put(&mut state, actor, "Jace, Wielder of Mysteries", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U; 3], 1);
        cast(&mut state, jace, &[]);
        settled(&mut state);
        assert_eq!(zone(&state, jace), Zone::Battlefield);
        assert_eq!(mtg_kernel::planeswalker_v1::loyalty(&state, jace), Some(4));
        let minsc = put(
            &mut state,
            actor,
            "Minsc & Boo, Timeless Heroes",
            Zone::Hand,
        );
        pool(&mut state, actor, &[ManaColor::R, ManaColor::G], 2);
        cast(&mut state, minsc, &[]);
        settle_picking(&mut state, 1, None);
        let [boo] = controls(&state, actor, "Boo Token").try_into().unwrap();
        activate(&mut state, minsc, 0, &[Target::Object(boo)]);
        settled(&mut state);
        pass_until(&mut state, |state| {
            state.active_player == opponent && state.step == Step::Main1
        });
        let painter = put(&mut state, opponent, "Painter's Servant", Zone::Hand);
        pool(&mut state, opponent, &[], 2);
        cast(&mut state, painter, &[]);
        settle_picking(&mut state, 1, None);
        assert_eq!(
            state.objects.get(painter).v4.chosen_color,
            Some(ManaColor::U)
        );
        to_main(&mut state, actor);
        assert_eq!(power_toughness(&state, boo), (4, 4));
        let other_target = put(&mut state, opponent, "Wall of Blossoms", Zone::Battlefield);
        let phantom = put(&mut state, opponent, "Phantom Nishoba", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(phantom, Zone::Battlefield),
        );
        assert_eq!(state.objects.get(phantom).counters.plus1_plus1, 7);
        let bounce = put(&mut state, actor, "Echoing Truth", Zone::Hand);
        activate(&mut state, minsc, 1, &[]);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets {
            player, legal_targets, .. }) if player == actor
                && legal_targets.as_slice() == [Target::Object(boo)])
        );
        act(&mut state, Action::ChooseEffectTarget(Target::Object(boo)));
        assert!(matches!(next(&mut state), Decision::ChooseTargets {
            player, legal_targets, .. } if player == actor
                && legal_targets.contains(&Target::Object(other_target))
                && legal_targets.contains(&Target::Player(opponent))));
        assert_eq!(loyalty(&state, minsc), Some(2));

        // Removing the only announced target makes the entire trigger fizzle.
        let mut fizzled: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        let library_before = fizzled.players[actor.index()].library.clone();
        act(
            &mut fizzled,
            Action::ChooseTarget(Target::Object(other_target)),
        );
        pool(&mut fizzled, actor, &[ManaColor::U], 1);
        cast(&mut fizzled, bounce, &[Target::Object(other_target)]);
        settled(&mut fizzled);
        assert_eq!(zone(&fizzled, other_target), Zone::Hand);
        assert_eq!(fizzled.players[actor.index()].library, library_before);
        assert!(!fizzled.engine.event_history.iter().any(
            |event| matches!(event, CommittedEvent::Damage { source, .. } if *source == minsc)
        ));

        // Legal target, actual Phantom prevention: draw still uses Hamster power.
        let mut prevented = state.clone();
        let hand = prevented.players[actor.index()].hand.len();
        act(
            &mut prevented,
            Action::ChooseTarget(Target::Object(phantom)),
        );
        settled(&mut prevented);
        assert_eq!(prevented.objects.get(phantom).counters.plus1_plus1, 6);
        assert_eq!(prevented.objects.get(phantom).damage, 0);
        assert_eq!(prevented.players[actor.index()].hand.len(), hand + 4);
        assert!(!prevented.engine.event_history.iter().any(
            |event| matches!(event, CommittedEvent::Damage { source, .. } if *source == minsc)
        ));

        act(&mut state, Action::ChooseTarget(Target::Player(opponent)));
        pool(&mut state, actor, &[ManaColor::U], 1);
        cast(&mut state, bounce, &[Target::Object(minsc)]);
        pass_until(&mut state, |state| zone(state, minsc) == Zone::Hand);
        assert_eq!(state.players[opponent.index()].life, 20);
        assert_eq!(state.stack.last().unwrap().source, minsc);

        // Painter made the departed Minsc blue. Removing Painter before the
        // opponent's Veil resolves cannot turn the old source red/green again.
        let mut veiled = state.clone();
        let painter_bounce = put(&mut veiled, actor, "Echoing Truth", Zone::Hand);
        let veil = put(&mut veiled, opponent, "Veil of Summer", Zone::Hand);
        pool(&mut veiled, actor, &[ManaColor::U], 1);
        cast(&mut veiled, painter_bounce, &[Target::Object(painter)]);
        pass_until(&mut veiled, |state| zone(state, painter) == Zone::Hand);
        assert_eq!(
            engine::object_color_mask(&veiled, minsc)
                & mtg_kernel::card_def::mana_color_mask(ManaColor::U),
            0
        );
        pool(&mut veiled, opponent, &[ManaColor::G], 0);
        act(&mut veiled, Action::Pass);
        cast(&mut veiled, veil, &[]);
        let own_library = veiled.players[actor.index()].library.clone();
        settled(&mut veiled);
        assert_eq!(veiled.players[opponent.index()].life, 20);
        assert_eq!(veiled.players[actor.index()].library, own_library);
        assert!(!veiled.engine.event_history.iter().any(
            |event| matches!(event, CommittedEvent::Damage { source, .. } if *source == minsc)
        ));

        let damage_count = |state: &GameState| {
            state
                .engine
                .event_history
                .iter()
                .filter(|event| {
                    matches!(event,
                CommittedEvent::Damage { source, target: Target::Player(player), amount: 4 }
                    if *source == minsc && *player == opponent)
                })
                .count()
        };
        put(&mut state, opponent, "Hullbreacher", Zone::Battlefield);
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        let library = state.players[actor.index()].library.clone();
        let before_draws = serde_json::to_vec(&state).unwrap();
        let prompt = settle(&mut state).expect("Minsc & Boo's first replacement choice");
        assert!(matches!(&prompt, Decision::ChooseEffectTargets {
            player, source, legal_targets, can_finish: true, .. }
            if *player == actor && *source == minsc && legal_targets.as_slice() == [Target::Object(thug)]));
        assert_eq!(state.players[opponent.index()].life, 16);
        assert_eq!(damage_count(&state), 1);
        let snapshot = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&snapshot).unwrap();
        assert_eq!(next(&mut restored), prompt);
        act(
            &mut restored,
            Action::ChooseEffectTarget(Target::Object(thug)),
        );
        settled(&mut restored);
        assert_eq!(zone(&restored, thug), Zone::Hand);
        assert_eq!(restored.players[actor.index()].library, library[4..]);
        assert_eq!(controls(&restored, opponent, "Treasure Token").len(), 3);
        assert_eq!(damage_count(&restored), 1);

        let mut declined: GameState = serde_json::from_slice(&snapshot).unwrap();
        settle_with(&mut declined, |_, decision| {
            assert!(matches!(decision, Decision::ChooseEffectTargets {
                player, source, can_finish: true, .. } if player == actor && source == minsc));
            Action::FinishEffectSelection
        });
        assert_eq!(declined.players[actor.index()].library, library);
        assert_eq!(controls(&declined, opponent, "Treasure Token").len(), 4);
        assert_eq!(damage_count(&declined), 1);

        let mut forged = serde_json::to_value(&state).unwrap();
        let at = forged["engine"]["event_history"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find_map(|event| {
                event
                    .get_mut("EternalOwnedDrawV1")?
                    .get_mut("journal")?
                    .get_mut("MinscDamageCompleted")?
                    .get_mut("published_at")
            })
            .unwrap();
        *at = serde_json::json!(0);
        let forged: GameState = serde_json::from_value(forged).unwrap();
        assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err());

        // Remove the draw start, but keep the independent, genuine damage
        // completion. The printed root cannot resume by dealing damage again.
        let mut rewind = serde_json::to_value(&state).unwrap();
        rewind
            .as_object_mut()
            .unwrap()
            .remove("eternal_owned_draw_progress_v1")
            .unwrap();
        let history = rewind["engine"]["event_history"].as_array_mut().unwrap();
        let removed = history
            .iter()
            .position(|event| {
                event["EternalOwnedDrawV1"]["journal"]
                    .get("Started")
                    .is_some()
            })
            .unwrap();
        history.remove(removed);
        let pending = &mut rewind["engine"]["pending_effect"];
        let root = pending["resolving_item"]["inline_effect"].clone();
        pending["frames"] = serde_json::json!([{"Program":{"op":root,"path":[]}}]);
        pending["choice"] = serde_json::Value::Null;
        pending["answered_choice_guard"] = serde_json::Value::Null;
        let rewind: GameState = serde_json::from_value(rewind).unwrap();
        assert!(mtg_kernel::effect::validate_pending_effect_choice(&rewind).is_err());
        assert_eq!(damage_count(&rewind), 1);

        let mut lethal: GameState = serde_json::from_slice(&before_draws).unwrap();
        lethal.players[opponent.index()].life = 4;
        assert!(
            matches!(settle(&mut lethal), Some(Decision::ChooseEffectTargets {
            player, source, legal_targets, .. }) if player == actor
                && source == minsc && legal_targets.as_slice() == [Target::Object(thug)])
        );
        assert_eq!(lethal.players[opponent.index()].life, 0);
        assert!(!lethal.players[opponent.index()].has_lost);
        act(
            &mut lethal,
            Action::ChooseEffectTarget(Target::Object(thug)),
        );
        assert!(
            matches!(next(&mut lethal), Decision::GameOver { winner: Some(player) } if player == actor)
        );
        assert_eq!(zone(&lethal, thug), Zone::Hand);
        assert_eq!(controls(&lethal, opponent, "Treasure Token").len(), 3);
        assert_eq!(damage_count(&lethal), 1);

        let mut terminal: GameState = serde_json::from_slice(&before_draws).unwrap();
        event::propose_and_commit_batch(
            &mut terminal,
            library
                .iter()
                .copied()
                .map(|card| ProposedEvent::zone_change(card, Zone::Exile))
                .collect(),
        );
        assert_eq!(zone(&terminal, jace), Zone::Battlefield);
        assert_eq!(
            mtg_kernel::planeswalker_v1::loyalty(&terminal, jace),
            Some(4)
        );
        assert!(
            matches!(settle(&mut terminal), Some(Decision::ChooseEffectTargets {
            player, source, legal_targets, can_finish: true, .. })
            if player == actor && source == minsc && legal_targets.as_slice() == [Target::Object(jace)])
        );
        act(
            &mut terminal,
            Action::ChooseEffectTarget(Target::Object(jace)),
        );
        assert!(
            matches!(next(&mut terminal), Decision::GameOver { winner: Some(player) } if player == actor)
        );
        assert_eq!(damage_count(&terminal), 1);
        assert!(controls(&terminal, opponent, "Treasure Token").is_empty());
    }
}

#[test]
fn avatar_of_burgeoning_echoes_empowers_jace_and_grants_its_ability() {
    let mut state = islands();
    let avatar = put(
        &mut state,
        P0,
        "Avatar of Burgeoning Echoes",
        Zone::Battlefield,
    );
    let land = put(&mut state, P0, "Island", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(land));
    settled(&mut state);
    let jace = controls(&state, P0, "Jace Token");
    assert_eq!(jace.len(), 1);
    let jace = jace[0];
    assert_eq!(loyalty(&state, jace), Some(2));
    // A second landfall puts the counters on the same token.
    let land = put(&mut state, P0, "Island", Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(land, Zone::Hand));
    state.players[0].lands_played_this_turn = 0;
    next(&mut state);
    act(&mut state, Action::PlayLand(land));
    settled(&mut state);
    assert_eq!(controls(&state, P0, "Jace Token").len(), 1);
    assert_eq!(loyalty(&state, jace), Some(4));
    // [-3]: Draw a card.
    let hand = state.players[0].hand.len();
    activate(&mut state, jace, 1, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), hand + 1);
    assert_eq!(loyalty(&state, jace), Some(1));

    // The granted -10, one index past the token's two abilities.
    to_main(&mut state, P0);
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, jace, 9);
    activate(&mut state, jace, 2, &[Target::Object(avatar)]);
    settled(&mut state);
    let lands = state.players[0]
        .battlefield
        .iter()
        .filter(|&&object| engine::object_has_type(&state, object, CardType::Land))
        .count() as i32;
    assert_eq!(state.objects.get(avatar).counters.plus1_plus1, lands);
    assert_eq!(loyalty(&state, jace), None, "no loyalty left");
}

// ---- Uro and Golos ----------------------------------------------------------------

#[test]
fn uro_is_sacrificed_unless_it_escaped_and_ramps_on_entry() {
    let mut state = islands();
    let uro = put(&mut state, P0, "Uro, Titan of Nature's Wrath", Zone::Hand);
    let land = put(&mut state, P0, "Forest", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, uro, &[]);
    settle_picking(&mut state, 0, Some(land));
    assert_eq!(zone(&state, uro), Zone::Graveyard, "sacrificed");
    assert_eq!(state.players[0].life, 23);
    assert_eq!(zone(&state, land), Zone::Battlefield);

    for _ in 0..5 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    pool(
        &mut state,
        P0,
        &[ManaColor::G, ManaColor::G, ManaColor::U, ManaColor::U],
        0,
    );
    cast(&mut state, uro, &[]);
    settle_first(&mut state);
    assert_eq!(zone(&state, uro), Zone::Battlefield, "escaped");
    assert_eq!(state.players[0].life, 26);
}

#[test]
fn golos_fetches_a_land_and_exiles_three_to_play_free() {
    let mut state = game(&["Bayou", "Lightning Bolt", "Island", "Island", "Island"]);
    let bayou = state.players[0].library[0];
    let golos = put(&mut state, P0, "Golos, Tireless Pilgrim", Zone::Hand);
    pool(&mut state, P0, &[], 5);
    cast(&mut state, golos, &[]);
    // Option 1: search.
    settle_picking(&mut state, 1, Some(bayou));
    assert_eq!(zone(&state, bayou), Zone::Battlefield);
    assert!(state.objects.get(bayou).tapped);

    pool(
        &mut state,
        P0,
        &[
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ],
        2,
    );
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    activate(&mut state, golos, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert!(castable(&mut state, bolt), "free this turn");
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
}

#[test]
fn faithless_looting_dredging_snacker_on_second_draw_does_not_trigger_snacker() {
    for actor in [P0, P1] {
        let mut state = game(&[
            "Mountain",
            "Sneaky Snacker",
            "Island",
            "Island",
            "Island",
            "Island",
        ]);
        state.active_player = actor;
        state.priority_player = actor;
        state.players[actor.index()].draws_this_turn = 2;
        let snacker = state.players[actor.index()].library[1];
        assert_eq!(name_of(&state, snacker), "Sneaky Snacker");
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        put(&mut state, actor, "Swamp", Zone::Hand);
        let looting = put(&mut state, actor, "Faithless Looting", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        cast(&mut state, looting, &[]);

        let mut draw_prompts = 0usize;
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets {
                legal_targets,
                can_finish: true,
                selected_count: 0,
                ..
            } if legal_targets.contains(&Target::Object(thug)) => {
                draw_prompts += 1;
                if draw_prompts == 1 {
                    // Draw 1 (3rd card drawn this turn): decline dredge while Snacker is in library.
                    Action::FinishEffectSelection
                } else {
                    // Draw 2: dredge Golgari Thug, milling Snacker into the graveyard.
                    Action::ChooseEffectTarget(Target::Object(thug))
                }
            }
            Decision::ChooseEffectTargets {
                selected_count: 1, ..
            } => Action::FinishEffectSelection,
            other => first_answer(other),
        });
        assert_eq!(draw_prompts, 2);
        assert_eq!(
            zone(&state, snacker),
            Zone::Graveyard,
            "Snacker was still in library when the 3rd card was drawn (CR 603.2, 113.6)"
        );
        assert!(controls(&state, actor, "Sneaky Snacker").is_empty());
    }
}

#[test]
fn faithless_looting_with_snacker_already_in_graveyard_and_dredge_returns_snacker() {
    for actor in [P0, P1] {
        let mut state = game(&["Mountain", "Island", "Island", "Island", "Island"]);
        state.active_player = actor;
        state.priority_player = actor;
        state.players[actor.index()].draws_this_turn = 2;
        let snacker = put(&mut state, actor, "Sneaky Snacker", Zone::Graveyard);
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        put(&mut state, actor, "Swamp", Zone::Hand);
        let looting = put(&mut state, actor, "Faithless Looting", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        cast(&mut state, looting, &[]);

        let mut draw_prompts = 0usize;
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets {
                legal_targets,
                can_finish: true,
                selected_count: 0,
                ..
            } if legal_targets.contains(&Target::Object(thug)) => {
                draw_prompts += 1;
                if draw_prompts == 1 {
                    Action::FinishEffectSelection
                } else {
                    Action::ChooseEffectTarget(Target::Object(thug))
                }
            }
            Decision::ChooseEffectTargets {
                selected_count: 1, ..
            } => Action::FinishEffectSelection,
            other => first_answer(other),
        });
        assert_eq!(draw_prompts, 2);
        assert_eq!(zone(&state, snacker), Zone::Battlefield);
        assert!(state.objects.get(snacker).tapped);
    }
}

#[test]
fn faithless_looting_dredging_snacker_on_first_draw_then_drawing_third_card_triggers_snacker() {
    for actor in [P0, P1] {
        let mut state = game(&[
            "Sneaky Snacker",
            "Island",
            "Island",
            "Island",
            "Mountain",
            "Island",
        ]);
        state.active_player = actor;
        state.priority_player = actor;
        state.players[actor.index()].draws_this_turn = 2;
        let snacker = state.players[actor.index()].library[0];
        assert_eq!(name_of(&state, snacker), "Sneaky Snacker");
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        put(&mut state, actor, "Swamp", Zone::Hand);
        let looting = put(&mut state, actor, "Faithless Looting", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        cast(&mut state, looting, &[]);

        let mut draw_prompts = 0usize;
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectTargets {
                legal_targets,
                can_finish: true,
                selected_count: 0,
                ..
            } if legal_targets.contains(&Target::Object(thug)) => {
                draw_prompts += 1;
                // Draw 1: dredge Golgari Thug, milling Snacker into the graveyard; Draw 2 draws the 3rd card.
                Action::ChooseEffectTarget(Target::Object(thug))
            }
            Decision::ChooseEffectTargets {
                selected_count: 1, ..
            } => Action::FinishEffectSelection,
            other => first_answer(other),
        });
        assert_eq!(draw_prompts, 1);
        assert_eq!(zone(&state, snacker), Zone::Battlefield);
        assert!(state.objects.get(snacker).tapped);
    }
}

#[test]
fn faithless_looting_dredge_orders_mixed_controller_triggers_apnap_both_seats() {
    for actor in [P0, P1] {
        let opp = actor.opponent();
        let mut state = game(&[
            "Island",
            "Creeping Chill",
            "Island",
            "Island",
            "Island",
            "Island",
        ]);
        state.active_player = actor;
        state.priority_player = actor;
        state.players[actor.index()].life = 1;
        state.players[opp.index()].life = 3;
        put(&mut state, opp, "Scrawling Crawler", Zone::Battlefield);
        let chill = state.players[actor.index()].library[1];
        assert_eq!(name_of(&state, chill), "Creeping Chill");
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        put(&mut state, actor, "Swamp", Zone::Hand);
        let looting = put(&mut state, actor, "Faithless Looting", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::R], 0);
        cast(&mut state, looting, &[]);

        let mut draw_prompts = 0usize;
        for _ in 0..200 {
            let decision = next(&mut state);
            match decision {
                Decision::CastSpellOrPass { .. }
                    if state.stack.len() == 2
                        && state
                            .stack
                            .iter()
                            .all(|item| item.kind == StackItemKind::TriggeredAbility) =>
                {
                    break;
                }
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                Decision::OrderTriggers { pending, .. } => {
                    act(
                        &mut state,
                        Action::OrderTriggers((0..pending.len()).collect()),
                    );
                }
                Decision::ChooseEffectTargets {
                    legal_targets,
                    can_finish: true,
                    selected_count: 0,
                    ..
                } if zone(&state, thug) == Zone::Graveyard
                    && legal_targets.contains(&Target::Object(thug)) =>
                {
                    draw_prompts += 1;
                    let action = if draw_prompts == 1 {
                        // Draw 1: draw Island, triggering opponent's Scrawling Crawler.
                        Action::FinishEffectSelection
                    } else {
                        // Draw 2: dredge Golgari Thug, milling Creeping Chill.
                        Action::ChooseEffectTarget(Target::Object(thug))
                    };
                    act(&mut state, action);
                }
                Decision::ChooseEffectTargets {
                    selected_count: 1, ..
                } => act(&mut state, Action::FinishEffectSelection),
                other => act(&mut state, first_answer(other)),
            }
        }
        assert_eq!(draw_prompts, 2);
        assert_eq!(state.stack.len(), 2);
        assert_eq!(state.stack[0].controller, actor);
        assert_eq!(state.stack[0].source, chill);
        assert_eq!(state.stack[1].controller, opp);

        // Opponent's Scrawling Crawler resolves first and ends the game at 0 life
        // before active player's Creeping Chill can drain.
        assert!(matches!(
            settle(&mut state),
            Some(Decision::GameOver { winner: Some(winner) }) if winner == opp
        ));
        assert_eq!(state.players[actor.index()].life, 0);
        assert_eq!(state.players[opp.index()].life, 3);
        assert_eq!(zone(&state, chill), Zone::Graveyard);
    }
}

#[test]
fn brainstorm_by_nonactive_player_dredge_orders_mixed_controller_triggers_apnap_both_seats() {
    for actor in [P0, P1] {
        let opp = actor.opponent();
        let mut state = game(&[
            "Island",
            "Creeping Chill",
            "Island",
            "Island",
            "Island",
            "Island",
            "Island",
            "Island",
            "Island",
            "Island",
        ]);
        state.active_player = actor;
        state.priority_player = actor;
        state.players[actor.index()].life = 3;
        state.players[opp.index()].life = 1;
        put(&mut state, actor, "Scrawling Crawler", Zone::Battlefield);
        let chill = state.players[opp.index()].library[1];
        assert_eq!(name_of(&state, chill), "Creeping Chill");
        let thug = put(&mut state, opp, "Golgari Thug", Zone::Graveyard);
        let brainstorm = put(&mut state, opp, "Brainstorm", Zone::Hand);
        pool(&mut state, opp, &[ManaColor::U], 0);
        assert!(matches!(
            next(&mut state),
            Decision::CastSpellOrPass { player, .. } if player == actor
        ));
        act(&mut state, Action::Pass);
        assert!(castable(&mut state, brainstorm));
        act(&mut state, Action::CastSpell(brainstorm));

        let mut draw_prompts = 0usize;
        for _ in 0..200 {
            let decision = next(&mut state);
            match decision {
                Decision::CastSpellOrPass { .. }
                    if draw_prompts == 2
                        && state.stack.len() == 3
                        && state
                            .stack
                            .iter()
                            .all(|item| item.kind == StackItemKind::TriggeredAbility) =>
                {
                    break;
                }
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                Decision::OrderTriggers { pending, .. } => {
                    act(
                        &mut state,
                        Action::OrderTriggers((0..pending.len()).collect()),
                    );
                }
                Decision::ChooseEffectTargets {
                    legal_targets,
                    can_finish: true,
                    selected_count: 0,
                    ..
                } if zone(&state, thug) == Zone::Graveyard
                    && legal_targets.contains(&Target::Object(thug)) =>
                {
                    draw_prompts += 1;
                    let action = if draw_prompts == 2 {
                        // Draw 2: dredge Golgari Thug, milling non-active player's Creeping Chill;
                        // Thug is then in hand so Draw 3 draws automatically.
                        Action::ChooseEffectTarget(Target::Object(thug))
                    } else {
                        // Draw 1: draw Island, triggering active player's Scrawling Crawler.
                        Action::FinishEffectSelection
                    };
                    act(&mut state, action);
                }
                Decision::ChooseEffectTargets {
                    can_finish: true,
                    selected_count: 1,
                    ..
                } if zone(&state, thug) == Zone::Graveyard => {
                    act(&mut state, Action::FinishEffectSelection);
                }
                other => act(&mut state, first_answer(other)),
            }
        }
        assert_eq!(draw_prompts, 2);
        assert_eq!(state.stack.len(), 3);
        assert_eq!(state.stack[0].controller, actor);
        assert_eq!(state.stack[1].controller, actor);
        assert_eq!(state.stack[2].controller, opp);
        assert_eq!(state.stack[2].source, chill);

        // Non-active player's Creeping Chill resolves first, drains active player to 0,
        // and ends the game before either Scrawling Crawler trigger resolves.
        let mut terminal = None;
        for _ in 0..20 {
            match settle(&mut state) {
                Some(Decision::ChooseEffectOption { .. }) => {
                    act(&mut state, Action::ChooseEffectOption(1));
                }
                Some(other @ Decision::GameOver { .. }) => {
                    terminal = Some(other);
                    break;
                }
                other => panic!("unexpected decision settling triggers: {other:?}"),
            }
        }
        assert!(matches!(
            terminal,
            Some(Decision::GameOver { winner: Some(winner) }) if winner == opp
        ));
        assert_eq!(state.players[actor.index()].life, 0);
        assert_eq!(state.players[opp.index()].life, 4);
        assert_eq!(zone(&state, chill), Zone::Exile);
    }
}

fn approach_mad_mages_last_room(state: &mut GameState, player: PlayerId) -> ObjectId {
    const ACERERAK: &str = "Acererak the Archlich";
    state.active_player = player;
    state.priority_player = player;
    let acererak = put(state, player, ACERERAK, Zone::Hand);
    for option in [Some(1), None, Some(0), None, Some(1), None] {
        pool(state, player, &[ManaColor::B], 2);
        cast(state, acererak, &[]);
        let mut room = option;
        while let Some(decision) = settle(state) {
            match decision {
                Decision::ChooseEffectOption { .. } => {
                    act(state, Action::ChooseEffectOption(room.take().unwrap()));
                }
                Decision::ChooseEffectTargets { legal_targets, .. } => {
                    act(state, Action::ChooseEffectTarget(legal_targets[0]));
                }
                other => panic!("unexpected approach to Mad Wizard's Lair: {other:?}"),
            }
        }
        assert!(room.is_none());
    }
    assert_eq!(state.players[player.index()].dungeon.dungeon_id, Some(3));
    assert_eq!(state.players[player.index()].dungeon.room_id, Some(8));
    acererak
}

#[test]
fn mad_wizards_lair_dredge_then_resolution_cast_orders_mixed_controller_triggers_apnap_both_seats()
{
    for active_seat in [P0, P1] {
        for venturer in [P0, P1] {
            let mut state = game(&["Island"; 24]);
            let acererak = approach_mad_mages_last_room(&mut state, venturer);
            let opp = venturer.opponent();
            state.active_player = active_seat;
            state.priority_player = active_seat;
            state.step = Step::Main1;
            state.players[venturer.index()].life = 1;
            state.players[opp.index()].life = 3;
            put(&mut state, active_seat, "Aluren", Zone::Battlefield);
            put(&mut state, opp, "Scrawling Crawler", Zone::Battlefield);
            let thug = put(&mut state, venturer, "Golgari Thug", Zone::Graveyard);
            let dark_ritual = put(&mut state, venturer, "Dark Ritual", Zone::Library);
            for _ in 0..3 {
                put(&mut state, venturer, "Island", Zone::Library);
            }
            let chill = put(&mut state, venturer, "Creeping Chill", Zone::Library);
            let first_island = put(&mut state, venturer, "Island", Zone::Library);
            assert_eq!(state.players[venturer.index()].library[0], first_island);
            assert_eq!(state.players[venturer.index()].library[1], chill);
            assert_eq!(state.players[venturer.index()].library[5], dark_ritual);

            if active_seat != venturer {
                assert!(matches!(
                    next(&mut state),
                    Decision::CastSpellOrPass { player, .. } if player == active_seat
                ));
                act(&mut state, Action::Pass);
            }
            assert!(castable(&mut state, acererak));
            act(&mut state, Action::CastSpell(acererak));

            let mut draw_prompts = 0usize;
            let mut dark_ritual_cast = false;
            for _ in 0..250 {
                let decision = next(&mut state);
                match decision {
                    Decision::CastSpellOrPass { .. }
                        if draw_prompts == 2
                            && dark_ritual_cast
                            && state.stack.len() == 4
                            && state.stack[0].source == dark_ritual
                            && state.stack[1..]
                                .iter()
                                .all(|item| item.kind == StackItemKind::TriggeredAbility) =>
                    {
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                    Decision::OrderTriggers { pending, .. } => {
                        act(
                            &mut state,
                            Action::OrderTriggers((0..pending.len()).collect()),
                        );
                    }
                    Decision::ChooseEffectTargets {
                        legal_targets,
                        can_finish: true,
                        selected_count: 0,
                        ..
                    } if zone(&state, thug) == Zone::Graveyard
                        && legal_targets.contains(&Target::Object(thug)) =>
                    {
                        draw_prompts += 1;
                        let action = if draw_prompts == 1 {
                            Action::FinishEffectSelection
                        } else {
                            Action::ChooseEffectTarget(Target::Object(thug))
                        };
                        act(&mut state, action);
                    }
                    Decision::ChooseEffectTargets {
                        legal_targets,
                        selected_count: 0,
                        ..
                    } if !dark_ritual_cast
                        && legal_targets.contains(&Target::Object(dark_ritual)) =>
                    {
                        dark_ritual_cast = true;
                        act(
                            &mut state,
                            Action::ChooseEffectTarget(Target::Object(dark_ritual)),
                        );
                    }
                    Decision::ChooseEffectTargets {
                        can_finish: true,
                        selected_count: 1,
                        ..
                    } => act(&mut state, Action::FinishEffectSelection),
                    other => act(&mut state, first_answer(other)),
                }
            }
            assert_eq!(draw_prompts, 2);
            assert!(dark_ritual_cast);
            assert_eq!(zone(&state, dark_ritual), Zone::Stack);
            assert_eq!(state.stack.len(), 4);
            assert_eq!(state.stack[0].kind, StackItemKind::Spell);
            assert_eq!(state.stack[0].source, dark_ritual);
            if venturer == active_seat {
                assert_eq!(state.stack[1].controller, active_seat);
                assert_eq!(state.stack[1].source, chill);
                assert_eq!(state.stack[2].controller, opp);
                assert_eq!(state.stack[3].controller, opp);
            } else {
                assert_eq!(state.stack[1].controller, active_seat);
                assert_eq!(state.stack[2].controller, active_seat);
                assert_eq!(state.stack[3].controller, venturer);
                assert_eq!(state.stack[3].source, chill);
            }

            let mut terminal = None;
            for _ in 0..20 {
                match settle(&mut state) {
                    Some(Decision::ChooseEffectOption { .. }) => {
                        act(&mut state, Action::ChooseEffectOption(1));
                    }
                    Some(other @ Decision::GameOver { .. }) => {
                        terminal = Some(other);
                        break;
                    }
                    other => panic!("unexpected decision settling triggers: {other:?}"),
                }
            }
            if venturer == active_seat {
                assert!(matches!(
                    terminal,
                    Some(Decision::GameOver { winner: Some(winner) }) if winner == opp
                ));
                assert_eq!(state.players[venturer.index()].life, 0);
            } else {
                assert!(matches!(
                    terminal,
                    Some(Decision::GameOver { winner: Some(winner) }) if winner == venturer
                ));
                assert_eq!(state.players[active_seat.index()].life, 0);
                assert_eq!(state.players[venturer.index()].life, 4);
                assert_eq!(zone(&state, chill), Zone::Exile);
            }
        }
    }
}

#[test]
fn discard_before_dredge_absorbs_pending_triggers_and_orders_apnap_both_seats() {
    for active_seat in [P0, P1] {
        let opp = active_seat.opponent();

        // Case 1: opponent's Tourach trigger captured during Pursue the Past's
        // discard before dredging Creeping Chill must follow active player's Chill.
        {
            let mut state = game(&["Island"; 16]);
            state.active_player = active_seat;
            state.priority_player = active_seat;
            state.step = Step::Main1;
            let tourach = put(&mut state, opp, "Tourach, Dread Cantor", Zone::Battlefield);
            let imp = put(&mut state, active_seat, "Stinkweed Imp", Zone::Graveyard);
            for _ in 0..3 {
                put(&mut state, active_seat, "Island", Zone::Library);
            }
            let chill = put(&mut state, active_seat, "Creeping Chill", Zone::Library);
            put(&mut state, active_seat, "Island", Zone::Library);
            let fodder = put(&mut state, active_seat, "Lightning Bolt", Zone::Hand);
            let pursue = put(&mut state, active_seat, "Pursue the Past", Zone::Hand);
            pool(&mut state, active_seat, &[ManaColor::R, ManaColor::W], 0);
            cast(&mut state, pursue, &[]);

            match settle(&mut state) {
                Some(Decision::ChooseEffectTargets { legal_targets, .. })
                    if legal_targets.contains(&Target::Object(fodder)) =>
                {
                    act(
                        &mut state,
                        Action::ChooseEffectTarget(Target::Object(fodder)),
                    );
                }
                other => panic!("expected optional discard choice, got {other:?}"),
            }
            match settle(&mut state) {
                Some(Decision::ChooseEffectTargets { legal_targets, .. })
                    if legal_targets.contains(&Target::Object(imp)) =>
                {
                    act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
                }
                other => panic!("expected dredge choice, got {other:?}"),
            }
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(state.stack.len(), 2);
            assert_eq!(state.stack[0].controller, active_seat);
            assert_eq!(state.stack[0].source, chill);
            assert_eq!(state.stack[1].controller, opp);
            assert_eq!(state.stack[1].source, tourach);
        }

        // Case 2: active Madness offer + active Ivora trigger before dredge +
        // active Creeping Chill during dredge + opponent Tourach order all four under APNAP.
        {
            let mut state = game(&["Island"; 16]);
            state.active_player = active_seat;
            state.priority_player = active_seat;
            state.step = Step::Main1;
            let ivora = put(
                &mut state,
                active_seat,
                "Ivora, Insatiable Heir",
                Zone::Battlefield,
            );
            let tourach = put(&mut state, opp, "Tourach, Dread Cantor", Zone::Battlefield);
            let imp = put(&mut state, active_seat, "Stinkweed Imp", Zone::Graveyard);
            for _ in 0..3 {
                put(&mut state, active_seat, "Island", Zone::Library);
            }
            let chill = put(&mut state, active_seat, "Creeping Chill", Zone::Library);
            put(&mut state, active_seat, "Island", Zone::Library);
            let temper = put(&mut state, active_seat, "Fiery Temper", Zone::Hand);
            let pursue = put(&mut state, active_seat, "Pursue the Past", Zone::Hand);
            pool(&mut state, active_seat, &[ManaColor::R, ManaColor::W], 0);
            cast(&mut state, pursue, &[]);

            match settle(&mut state) {
                Some(Decision::ChooseEffectTargets { legal_targets, .. })
                    if legal_targets.contains(&Target::Object(temper)) =>
                {
                    act(
                        &mut state,
                        Action::ChooseEffectTarget(Target::Object(temper)),
                    );
                }
                other => panic!("expected Madness discard choice, got {other:?}"),
            }
            match settle(&mut state) {
                Some(Decision::ChooseEffectTargets { legal_targets, .. })
                    if legal_targets.contains(&Target::Object(imp)) =>
                {
                    act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
                }
                other => panic!("expected dredge choice, got {other:?}"),
            }
            match next(&mut state) {
                Decision::OrderTriggers { player, pending } => {
                    assert_eq!(player, active_seat);
                    assert_eq!(pending.len(), 3);
                    let chill_idx = pending
                        .iter()
                        .position(|trigger| trigger.source == chill)
                        .unwrap();
                    let temper_idx = pending
                        .iter()
                        .position(|trigger| trigger.source == temper && trigger.is_madness_offer)
                        .unwrap();
                    let ivora_idx = pending
                        .iter()
                        .position(|trigger| trigger.source == ivora)
                        .unwrap();
                    act(
                        &mut state,
                        Action::OrderTriggers(vec![chill_idx, temper_idx, ivora_idx]),
                    );
                }
                other => panic!("expected active-player OrderTriggers, got {other:?}"),
            }
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(state.stack.len(), 4);
            assert_eq!(state.stack[0].controller, active_seat);
            assert_eq!(state.stack[0].source, chill);
            assert_eq!(state.stack[1].controller, active_seat);
            assert_eq!(state.stack[1].source, temper);
            assert_eq!(state.stack[1].kind, StackItemKind::MadnessOffer);
            assert_eq!(state.stack[2].controller, active_seat);
            assert_eq!(state.stack[2].source, ivora);
            assert_eq!(state.stack[3].controller, opp);
            assert_eq!(state.stack[3].source, tourach);
        }

        // Case 3: non-active player resolves Seasoned Pyromancer via Aluren,
        // auto-discards Fiery Temper (Madness in pending_triggers) while active_seat
        // controls Tourach, then dredges Creeping Chill; Madness must still join APNAP
        // with Chill ahead of Tourach instead of remaining ahead in pending_triggers.
        {
            let mut state = game(&["Island"; 16]);
            state.active_player = active_seat;
            state.priority_player = active_seat;
            state.step = Step::Main1;
            put(&mut state, active_seat, "Aluren", Zone::Battlefield);
            let tourach = put(
                &mut state,
                active_seat,
                "Tourach, Dread Cantor",
                Zone::Battlefield,
            );
            let imp = put(&mut state, opp, "Stinkweed Imp", Zone::Graveyard);
            for _ in 0..3 {
                put(&mut state, opp, "Island", Zone::Library);
            }
            let chill = put(&mut state, opp, "Creeping Chill", Zone::Library);
            put(&mut state, opp, "Island", Zone::Library);
            let temper = put(&mut state, opp, "Fiery Temper", Zone::Hand);
            let pyromancer = put(&mut state, opp, "Seasoned Pyromancer", Zone::Hand);

            assert!(matches!(
                next(&mut state),
                Decision::CastSpellOrPass { player, .. } if player == active_seat
            ));
            act(&mut state, Action::Pass);
            assert!(castable(&mut state, pyromancer));
            act(&mut state, Action::CastSpell(pyromancer));

            match settle(&mut state) {
                Some(Decision::ChooseEffectTargets { legal_targets, .. })
                    if legal_targets.contains(&Target::Object(imp)) =>
                {
                    act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
                }
                other => {
                    panic!("expected opp dredge choice after Pyromancer discard, got {other:?}")
                }
            }
            match next(&mut state) {
                Decision::OrderTriggers { player, pending } => {
                    assert_eq!(player, opp);
                    assert_eq!(pending.len(), 2);
                    let chill_idx = pending
                        .iter()
                        .position(|trigger| trigger.source == chill)
                        .unwrap();
                    let temper_idx = pending
                        .iter()
                        .position(|trigger| trigger.source == temper && trigger.is_madness_offer)
                        .unwrap();
                    act(
                        &mut state,
                        Action::OrderTriggers(vec![chill_idx, temper_idx]),
                    );
                }
                other => panic!("expected opp OrderTriggers, got {other:?}"),
            }
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(state.stack.len(), 3);
            assert_eq!(state.stack[0].controller, active_seat);
            assert_eq!(state.stack[0].source, tourach);
            assert_eq!(state.stack[1].controller, opp);
            assert_eq!(state.stack[1].source, chill);
            assert_eq!(state.stack[2].controller, opp);
            assert_eq!(state.stack[2].source, temper);
            assert_eq!(state.stack[2].kind, StackItemKind::MadnessOffer);
        }
    }
}

#[test]
fn miracle_reveal_mid_resolution_with_dredge_and_resolution_cast_preserves_deferred_checkpoint_and_apnap_both_seats(
) {
    // Part A: multi-draw resolution drawing Terminus first and dredging second
    // keeps SBA deferred across both accept and decline Miracle responses and
    // orders Miracle, Creeping Chill, and Scrawling Crawler under APNAP.
    for active_seat in [P0, P1] {
        for caster in [P0, P1] {
            let opp = caster.opponent();
            let mut state = game(&["Island"; 16]);
            state.active_player = active_seat;
            state.priority_player = active_seat;
            state.step = Step::Main1;
            let crawler = put(&mut state, opp, "Scrawling Crawler", Zone::Battlefield);
            let sba_canary = put(&mut state, opp, "Llanowar Elves", Zone::Battlefield);
            let imp = put(&mut state, caster, "Stinkweed Imp", Zone::Graveyard);
            for _ in 0..3 {
                put(&mut state, caster, "Island", Zone::Library);
            }
            let chill = put(&mut state, caster, "Creeping Chill", Zone::Library);
            put(&mut state, caster, "Island", Zone::Library);
            let terminus = put(&mut state, caster, "Terminus", Zone::Library);
            let brainstorm = put(&mut state, caster, "Brainstorm", Zone::Hand);
            pool(&mut state, caster, &[ManaColor::U], 0);
            if active_seat != caster {
                assert!(matches!(
                    next(&mut state),
                    Decision::CastSpellOrPass { player, .. } if player == active_seat
                ));
                act(&mut state, Action::Pass);
            }
            cast(&mut state, brainstorm, &[]);

            // Draw 1: decline dredge to draw Terminus as first card of turn,
            // marking lethal damage on sba_canary mid-resolution.
            match settle(&mut state) {
                Some(Decision::ChooseEffectTargets {
                    player,
                    legal_targets,
                    can_finish: true,
                    ..
                }) if player == caster && legal_targets.contains(&Target::Object(imp)) => {
                    state.objects.get_mut(sba_canary).damage = 1;
                    act(&mut state, Action::FinishEffectSelection);
                }
                other => panic!("expected first Brainstorm dredge prompt, got {other:?}"),
            }
            assert_eq!(zone(&state, sba_canary), Zone::Battlefield);

            // Draw 2: dredge Stinkweed Imp, milling Creeping Chill while recording Terminus's miracle offer.
            match settle(&mut state) {
                Some(Decision::ChooseEffectTargets {
                    player,
                    legal_targets,
                    ..
                }) if player == caster && legal_targets.contains(&Target::Object(imp)) => {
                    act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
                }
                other => panic!("expected second Brainstorm dredge prompt, got {other:?}"),
            }
            assert_eq!(zone(&state, sba_canary), Zone::Battlefield);

            // While Brainstorm is still resolving, Miracle reveal is offered.
            let offer_decision = next(&mut state);
            assert!(matches!(
                offer_decision,
                Decision::ChooseEffectOption {
                    player,
                    source,
                    option_count: 2,
                } if player == caster && source == terminus
            ));

            // Branch 1: decline reveal; SBA and dredge deferral remain preserved until Brainstorm finishes.
            {
                let mut declined = state.clone();
                act(&mut declined, Action::ChooseEffectOption(0));
                assert_eq!(zone(&declined, sba_canary), Zone::Battlefield);
                for _ in 0..8 {
                    match next(&mut declined) {
                        Decision::ChooseEffectTargets {
                            legal_targets,
                            can_finish: false,
                            ..
                        } => {
                            let pick = legal_targets
                                .into_iter()
                                .find(|target| *target != Target::Object(terminus))
                                .unwrap();
                            act(&mut declined, Action::ChooseEffectTarget(pick));
                        }
                        Decision::OrderTriggers { pending, .. } => {
                            act(
                                &mut declined,
                                Action::OrderTriggers((0..pending.len()).collect()),
                            );
                        }
                        Decision::CastSpellOrPass { .. } => break,
                        other => panic!("unexpected decline branch decision: {other:?}"),
                    }
                    if declined.engine.pending_effect.is_some() {
                        assert_eq!(zone(&declined, sba_canary), Zone::Battlefield);
                    }
                }
                assert!(matches!(
                    next(&mut declined),
                    Decision::CastSpellOrPass { .. }
                ));
                assert_eq!(zone(&declined, sba_canary), Zone::Graveyard);
                assert_eq!(declined.stack.len(), 3);
                if caster == active_seat {
                    assert_eq!(declined.stack[0].controller, active_seat);
                    assert_eq!(declined.stack[0].source, chill);
                    assert_eq!(declined.stack[1].controller, opp);
                    assert_eq!(declined.stack[1].source, crawler);
                    assert_eq!(declined.stack[2].controller, opp);
                    assert_eq!(declined.stack[2].source, crawler);
                } else {
                    assert_eq!(declined.stack[0].controller, active_seat);
                    assert_eq!(declined.stack[0].source, crawler);
                    assert_eq!(declined.stack[1].controller, active_seat);
                    assert_eq!(declined.stack[1].source, crawler);
                    assert_eq!(declined.stack[2].controller, caster);
                    assert_eq!(declined.stack[2].source, chill);
                }
            }

            // Branch 2: accept reveal; SBA must not run early and Miracle joins APNAP with Chill & Crawler.
            act(&mut state, Action::ChooseEffectOption(1));
            assert_eq!(zone(&state, sba_canary), Zone::Battlefield);
            assert_eq!(state.stack.len(), 1);
            assert_eq!(state.stack[0].source, brainstorm);
            assert!(state.engine.pending_effect.is_some());

            let mut caster_ordered_both = false;
            for _ in 0..16 {
                match next(&mut state) {
                    Decision::ChooseEffectTargets {
                        legal_targets,
                        can_finish: false,
                        ..
                    } => {
                        assert_eq!(zone(&state, sba_canary), Zone::Battlefield);
                        let pick = legal_targets
                            .into_iter()
                            .find(|target| *target != Target::Object(terminus))
                            .unwrap();
                        act(&mut state, Action::ChooseEffectTarget(pick));
                    }
                    Decision::OrderTriggers { player, pending } => {
                        if player == caster {
                            caster_ordered_both = true;
                            assert_eq!(pending.len(), 2);
                            assert!(pending.iter().any(|trigger| trigger.source == terminus));
                            assert!(pending.iter().any(|trigger| trigger.source == chill));
                        }
                        act(
                            &mut state,
                            Action::OrderTriggers((0..pending.len()).collect()),
                        );
                    }
                    Decision::CastSpellOrPass { .. } => break,
                    other => panic!("unexpected accept branch decision: {other:?}"),
                }
            }
            assert!(caster_ordered_both);
            assert_eq!(zone(&state, sba_canary), Zone::Graveyard);
            assert_eq!(state.stack.len(), 4);
            if caster == active_seat {
                assert!(state.stack[0..2]
                    .iter()
                    .all(|item| item.controller == active_seat));
                assert!(state.stack[2..4]
                    .iter()
                    .all(|item| item.controller == opp && item.source == crawler));
            } else {
                assert!(state.stack[0..2]
                    .iter()
                    .all(|item| item.controller == active_seat && item.source == crawler));
                assert!(state.stack[2..4]
                    .iter()
                    .all(|item| item.controller == caster));
            }
        }
    }

    // Part B: Mad Wizard's Lair drawing Terminus first, dredging second, and
    // casting Dark Ritual from hand preserves Miracle inside the deferred
    // checkpoint alongside Creeping Chill and Scrawling Crawler.
    for active_seat in [P0, P1] {
        let mut state = game(&["Island"; 24]);
        let venturer = active_seat.opponent();
        let acererak = approach_mad_mages_last_room(&mut state, venturer);
        state.active_player = active_seat;
        state.priority_player = active_seat;
        state.step = Step::Main1;
        state.players[venturer.index()].draws_this_turn = 0;

        put(&mut state, active_seat, "Aluren", Zone::Battlefield);
        let crawler = put(
            &mut state,
            active_seat,
            "Scrawling Crawler",
            Zone::Battlefield,
        );
        let sba_canary = put(&mut state, active_seat, "Llanowar Elves", Zone::Battlefield);
        let imp = put(&mut state, venturer, "Stinkweed Imp", Zone::Graveyard);
        let dark_ritual = put(&mut state, venturer, "Dark Ritual", Zone::Library);
        for _ in 0..3 {
            put(&mut state, venturer, "Island", Zone::Library);
        }
        let chill = put(&mut state, venturer, "Creeping Chill", Zone::Library);
        put(&mut state, venturer, "Island", Zone::Library);
        let terminus = put(&mut state, venturer, "Terminus", Zone::Library);

        assert!(matches!(
            next(&mut state),
            Decision::CastSpellOrPass { player, .. } if player == active_seat
        ));
        act(&mut state, Action::Pass);
        assert!(castable(&mut state, acererak));
        act(&mut state, Action::CastSpell(acererak));

        let mut draw_prompts = 0usize;
        let mut miracle_accepted = false;
        let mut dark_ritual_cast = false;
        for _ in 0..250 {
            let decision = next(&mut state);
            match decision {
                Decision::CastSpellOrPass { .. }
                    if draw_prompts == 2
                        && miracle_accepted
                        && dark_ritual_cast
                        && state.stack.len() == 5
                        && state.stack[0].source == dark_ritual =>
                {
                    break;
                }
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                Decision::OrderTriggers { pending, .. } => {
                    act(
                        &mut state,
                        Action::OrderTriggers((0..pending.len()).collect()),
                    );
                }
                Decision::ChooseEffectOption {
                    player,
                    source,
                    option_count: 2,
                } if player == venturer && source == terminus && !miracle_accepted => {
                    assert_eq!(zone(&state, sba_canary), Zone::Battlefield);
                    miracle_accepted = true;
                    act(&mut state, Action::ChooseEffectOption(1));
                    assert_eq!(zone(&state, sba_canary), Zone::Battlefield);
                }
                Decision::ChooseEffectTargets {
                    legal_targets,
                    can_finish: true,
                    selected_count: 0,
                    ..
                } if zone(&state, imp) == Zone::Graveyard
                    && legal_targets.contains(&Target::Object(imp)) =>
                {
                    draw_prompts += 1;
                    if draw_prompts == 1 {
                        state.objects.get_mut(sba_canary).damage = 1;
                    }
                    let action = if draw_prompts == 1 {
                        Action::FinishEffectSelection
                    } else {
                        Action::ChooseEffectTarget(Target::Object(imp))
                    };
                    act(&mut state, action);
                }
                Decision::ChooseEffectTargets {
                    legal_targets,
                    selected_count: 0,
                    ..
                } if !dark_ritual_cast && legal_targets.contains(&Target::Object(dark_ritual)) => {
                    dark_ritual_cast = true;
                    act(
                        &mut state,
                        Action::ChooseEffectTarget(Target::Object(dark_ritual)),
                    );
                }
                Decision::ChooseEffectTargets {
                    can_finish: true,
                    selected_count: 1,
                    ..
                } => act(&mut state, Action::FinishEffectSelection),
                other => act(&mut state, first_answer(other)),
            }
        }
        assert_eq!(draw_prompts, 2);
        assert!(miracle_accepted);
        assert!(dark_ritual_cast);
        assert_eq!(zone(&state, sba_canary), Zone::Graveyard);
        assert_eq!(state.stack.len(), 5);
        assert_eq!(state.stack[0].kind, StackItemKind::Spell);
        assert_eq!(state.stack[0].source, dark_ritual);
        assert_eq!(state.stack[1].controller, active_seat);
        assert_eq!(state.stack[1].source, crawler);
        assert_eq!(state.stack[2].controller, active_seat);
        assert_eq!(state.stack[2].source, crawler);
        assert!(state.stack[3..5]
            .iter()
            .all(|item| item.controller == venturer));
        assert!(state.stack[3..5].iter().any(|item| item.source == chill));
        assert!(state.stack[3..5].iter().any(|item| item.source == terminus));
    }
}

#[test]
fn pursue_the_past_discard_against_ivora_crosses_second_dredge_decision_and_public_capture_both_seats(
) {
    use mtg_kernel::{rl::observe_v2, surface_v2::HarnessSurfaceV2};

    for active_seat in [PlayerId::P0, PlayerId::P1] {
        let opp = active_seat.opponent();
        let mut state = game(&["Island"; 16]);
        state.active_player = active_seat;
        state.priority_player = active_seat;
        state.step = Step::Main1;
        let ivora = put(
            &mut state,
            active_seat,
            "Ivora, Insatiable Heir",
            Zone::Battlefield,
        );
        let imp = put(&mut state, active_seat, "Stinkweed Imp", Zone::Graveyard);
        let thug = put(&mut state, active_seat, "Golgari Thug", Zone::Graveyard);
        let chill = put(&mut state, active_seat, "Creeping Chill", Zone::Library);
        put(&mut state, active_seat, "Island", Zone::Library);
        let fodder = put(&mut state, active_seat, "Island", Zone::Hand);
        let pursue = put(&mut state, active_seat, "Pursue the Past", Zone::Hand);
        pool(&mut state, active_seat, &[ManaColor::R, ManaColor::W], 0);
        cast(&mut state, pursue, &[]);

        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets { legal_targets, .. })
                if legal_targets.contains(&Target::Object(fodder)) =>
            {
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(fodder)),
                );
            }
            other => panic!("expected discard choice, got {other:?}"),
        }
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets { legal_targets, .. })
                if legal_targets.contains(&Target::Object(imp)) =>
            {
                act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
            }
            other => panic!("expected first dredge choice, got {other:?}"),
        }
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets {
                legal_targets,
                can_finish: true,
                ..
            }) if legal_targets.contains(&Target::Object(thug)) => {
                assert!(state.engine.pending_triggers.is_empty());
                let surface = HarnessSurfaceV2::default();
                for viewer in [active_seat, opp] {
                    let obs = observe_v2(&state, &surface, viewer, 0).unwrap();
                    let pending = &obs.projection.engine_context.pending_triggers;
                    assert_eq!(pending.len(), 1);
                    assert!(pending[0]
                        .source
                        .as_ref()
                        .is_some_and(|s| s.arena_id == ivora.0));
                }
                act(&mut state, Action::FinishEffectSelection);
            }
            other => panic!("expected second dredge choice, got {other:?}"),
        }
        match next(&mut state) {
            Decision::OrderTriggers { player, pending } => {
                assert_eq!(player, active_seat);
                assert_eq!(pending.len(), 2);
                let chill_idx = pending
                    .iter()
                    .position(|trigger| trigger.source == chill)
                    .unwrap();
                let ivora_idx = pending
                    .iter()
                    .position(|trigger| trigger.source == ivora)
                    .unwrap();
                act(
                    &mut state,
                    Action::OrderTriggers(vec![chill_idx, ivora_idx]),
                );
            }
            other => panic!("expected active-player OrderTriggers, got {other:?}"),
        }
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.stack.len(), 2);
        assert_eq!(state.stack[0].controller, active_seat);
        assert_eq!(state.stack[0].source, chill);
        assert_eq!(state.stack[1].controller, active_seat);
        assert_eq!(state.stack[1].source, ivora);
    }
}

#[test]
fn decree_of_pain_with_drop_of_honey_and_dredge_queues_single_state_trigger_both_seats() {
    for seat in [PlayerId::P0, PlayerId::P1] {
        let mut state = game(&["Island"; 16]);
        state.active_player = seat;
        state.priority_player = seat;
        state.step = Step::Main1;
        let honey = put(&mut state, seat, "Drop of Honey", Zone::Battlefield);
        let imp = put(&mut state, seat, "Stinkweed Imp", Zone::Battlefield);
        let decree = put(&mut state, seat, "Decree of Pain", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::B, ManaColor::B], 6);
        cast(&mut state, decree, &[]);

        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets { legal_targets, .. })
                if legal_targets.contains(&Target::Object(imp)) =>
            {
                act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
            }
            other => panic!("expected dredge choice, got {other:?}"),
        }
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.stack.len(), 1);
        assert_eq!(state.stack[0].source, honey);
        settled(&mut state);
        assert_eq!(zone(&state, honey), Zone::Graveyard);
    }
}

#[test]
fn paradoxical_outcome_returning_animate_dead_across_dredge_preserves_public_link_and_sacrifices_creature_both_seats(
) {
    for seat in [PlayerId::P0, PlayerId::P1] {
        let mut state = game(&["Island"; 16]);
        state.active_player = seat;
        state.priority_player = seat;
        state.step = Step::Main1;
        let guide = put(&mut state, seat, "Elvish Spirit Guide", Zone::Graveyard);
        let ring = put(&mut state, seat, "Sol Ring", Zone::Battlefield);
        let aura = put(&mut state, seat, "Animate Dead", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::B], 1);
        cast(&mut state, aura, &[Target::Object(guide)]);
        settled(&mut state);
        assert_eq!(zone(&state, aura), Zone::Battlefield);
        assert_eq!(zone(&state, guide), Zone::Battlefield);
        let aura_bf_generation = state.objects.get(aura).zone_change_count;

        let imp = put(&mut state, seat, "Stinkweed Imp", Zone::Graveyard);
        let thug = put(&mut state, seat, "Golgari Thug", Zone::Graveyard);
        let outcome = put(&mut state, seat, "Paradoxical Outcome", Zone::Hand);
        pool(&mut state, seat, &[ManaColor::U], 3);
        cast(
            &mut state,
            outcome,
            &[Target::Object(aura), Target::Object(ring)],
        );
        if matches!(
            next(&mut state),
            Decision::ChooseTargets {
                can_finish: true,
                ..
            }
        ) {
            act(&mut state, Action::FinishEffectSelection);
        }

        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets { legal_targets, .. })
                if legal_targets.contains(&Target::Object(imp)) =>
            {
                act(&mut state, Action::ChooseEffectTarget(Target::Object(imp)));
            }
            other => panic!("expected first dredge choice, got {other:?}"),
        }
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets {
                legal_targets,
                can_finish: true,
                ..
            }) if legal_targets.contains(&Target::Object(thug)) => {
                assert_eq!(zone(&state, aura), Zone::Hand);
                assert_eq!(zone(&state, guide), Zone::Battlefield);
                let dynamic = mtg_kernel::eternal_actor_public_v1::capture_v1(&state).unwrap();
                assert_eq!(dynamic.animate_dead_attached.len(), 1);
                assert_eq!(
                    dynamic.animate_dead_attached[0].source.zone_change_count,
                    aura_bf_generation
                );
                act(&mut state, Action::FinishEffectSelection);
            }
            other => panic!("expected second dredge choice, got {other:?}"),
        }
        settled(&mut state);
        assert_eq!(zone(&state, guide), Zone::Graveyard);
    }
}
