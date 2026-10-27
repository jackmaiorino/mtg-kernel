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
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E7B_CARDS: [&str; 44] = [
    "Abrupt Decay",
    "Assassin's Trophy",
    "Avatar of Burgeoning Echoes",
    "Basking Rootwalla",
    "Bayou",
    "Bazaar of Baghdad",
    "Blazing Rootwalla",
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
    for name in ["Grief"] {
        assert!(def(name).keywords.has(Keywords::MENACE), "{name}");
    }
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
    pass_until(&mut state, |state| state.step == Step::Cleanup);
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
    pass_until(&mut state, |state| state.step == Step::Cleanup);
    assert_eq!(zone(&state, ghoul), Zone::Battlefield);
    assert!(state.objects.get(ghoul).tapped);

    let mut state = islands();
    let ghoul = put(&mut state, P0, "Silversmote Ghoul", Zone::Graveyard);
    event::propose_and_commit(&mut state, ProposedEvent::life_gain(P0, 2));
    pass_until(&mut state, |state| state.step == Step::Cleanup);
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
    pass_until(&mut state, |state| state.step == Step::Cleanup);
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
    pass_until(&mut state, |state| state.step == Step::Cleanup);
    assert!(
        hand_contains(&state, P0, ragavan),
        "returned at the end step"
    );
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
    settle_picking(&mut state, 0, Some(bayou));
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
