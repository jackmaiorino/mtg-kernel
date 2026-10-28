//! Eternal wave E8a: native rules tests for the cards that complete Legacy
//! Death and Taxes (Yorion), Hogaak Moonshadow, the Jeskai, Azorius and
//! Dimir control lists, Saga and Beseech Storm, Reanimator and 8-Cast.
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

const E8A_CARDS: [&str; 42] = [
    "Ad Nauseam",
    "Aether Vial",
    "Back to Basics",
    "Bitter Triumph",
    "Cabal Ritual",
    "Canoptek Scarab Swarm",
    "Chandra, Chill of Compliance",
    "Cloak and Dagger, Entwined",
    "Dark Matter Manipulator",
    "Dryad Arbor",
    "Emperor of Bones",
    "Erode",
    "Flagstones of Trokair",
    "Flickerwisp",
    "Harbinger of the Seas",
    "Hogaak, Arisen Necropolis",
    "Infernal Tutor",
    "Lich's Relic",
    "Lion Sash",
    "Loran of the Third Path",
    "Meteor Sword",
    "Mystic Sanctuary",
    "Once Upon a Time",
    "Overlord of the Balemurk",
    "Pre-War Formalwear",
    "Prismatic Vista",
    "Rain of Filth",
    "Recruiter of the Guard",
    "Skyclave Apparition",
    "Snow-Covered Swamp",
    "Solitude",
    "Stitcher's Supplier",
    "Stoneforge Mystic",
    "Supreme Verdict",
    "Sylvan Safekeeper",
    "Teferi, Time Raveler",
    "Thought Monitor",
    "Valgavoth's Lair",
    "White Orchid Phantom",
    "Wight of the Reliquary",
    "Wishclaw Talisman",
    "Yorion, Sky Nomad",
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
        0x4538_4121_0001,
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

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    engine::object_has_type(state, object, CardType::Creature)
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

/// Settles, answering only the order of a multi-card mill (each card in
/// turn); any other choice fails.
fn settle_ordering(state: &mut GameState) {
    let mut chosen = Vec::new();
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count,
            can_finish: false,
            ..
        } => {
            if selected_count == 0 {
                chosen.clear();
            }
            let target = *legal_targets
                .iter()
                .find(|target| !chosen.contains(*target))
                .expect("an unordered card");
            chosen.push(target);
            Action::ChooseEffectTarget(target)
        }
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        other => panic!("unexpected choice: {other:?}"),
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

/// Plays on to the active player's end step, after its beginning-of-end-step
/// triggers resolved.
fn end_step(state: &mut GameState) {
    pass_until(state, |state| {
        state.step == Step::End
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

fn known_to(state: &GameState, viewer: PlayerId, owner: PlayerId, card: ObjectId) -> bool {
    state
        .known_hand_cards(viewer, owner)
        .iter()
        .any(|entry| entry.object == card)
}

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
}

/// Moves `object` to its owner's graveyard, as if destroyed.
fn to_graveyard(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn e8a_cards_are_full_with_their_printed_characteristics() {
    for name in E8A_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let insect = def("Insect Artifact Token");
    assert!(insect.is_token && insect.capability == CardCapability::Full);
    assert!(
        insect.types.contains(&CardType::Artifact) && insect.types.contains(&CardType::Creature)
    );
    assert!(insect.keywords.has(Keywords::FLYING));
    assert_eq!((insect.power, insect.toughness), (Some(1), Some(1)));
    let illusion = def("Illusion Token");
    assert!(illusion.is_token && illusion.capability == CardCapability::Full);
    assert_eq!(illusion.subtypes, &[Subtype::Illusion]);

    assert_eq!(
        def("Chandra, Chill of Compliance").starting_loyalty,
        Some(3)
    );
    assert_eq!(def("Teferi, Time Raveler").starting_loyalty, Some(4));
    assert!(def("Supreme Verdict").spell_cannot_be_countered);
    for name in [
        "Canoptek Scarab Swarm",
        "Flickerwisp",
        "Thought Monitor",
        "Yorion, Sky Nomad",
    ] {
        assert!(def(name).keywords.has(Keywords::FLYING), "{name}");
    }
    let cloak = def("Cloak and Dagger, Entwined");
    assert!(cloak.keywords.has(Keywords::DEATHTOUCH) && cloak.keywords.has(Keywords::LIFELINK));
    assert!(def("Hogaak, Arisen Necropolis")
        .keywords
        .has(Keywords::TRAMPLE));
    for name in ["Loran of the Third Path", "Wight of the Reliquary"] {
        assert!(def(name).keywords.has(Keywords::VIGILANCE), "{name}");
    }
    let solitude = def("Solitude");
    assert!(solitude.keywords.has(Keywords::FLASH) && solitude.keywords.has(Keywords::LIFELINK));
    let phantom = def("White Orchid Phantom");
    assert!(phantom.keywords.has(Keywords::FLYING) && phantom.keywords.has(Keywords::FIRST_STRIKE));
    assert!(def("Valgavoth's Lair").keywords.has(Keywords::HEXPROOF));
    assert_eq!(
        def("Once Upon a Time").alt_cost.map(|alt| alt.condition),
        Some(AltCostCondition::FirstSpellThisGameV1)
    );
    assert_eq!(
        def("Overlord of the Balemurk")
            .alt_cost
            .map(|alt| alt.condition),
        Some(AltCostCondition::ImpendingFromHand { time_counters: 5 })
    );
    for name in ["Solitude", "Bitter Triumph", "Hogaak, Arisen Necropolis"] {
        assert_eq!(
            def(name).alt_cost.map(|alt| alt.condition),
            Some(AltCostCondition::Always),
            "{name}"
        );
    }
    assert!(def("Bitter Triumph").additional_cost.is_some());
    assert!(def("Thought Monitor").generic_cost_reduction.is_some());
    assert!(def("Mystic Sanctuary").subtypes.contains(&Subtype::Island));
    let swamp = def("Snow-Covered Swamp");
    assert!(
        swamp.supertypes.contains(&Supertype::Basic) && swamp.supertypes.contains(&Supertype::Snow)
    );
    assert!(swamp.subtypes.contains(&Subtype::Swamp));
    assert!(def("Flagstones of Trokair")
        .supertypes
        .contains(&Supertype::Legendary));
}

// ---- Lands -------------------------------------------------------------------------

#[test]
fn snow_covered_swamp_taps_for_black() {
    let mut state = islands();
    let swamp = put(&mut state, P0, "Snow-Covered Swamp", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(swamp));
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
}

#[test]
fn dryad_arbor_is_a_forest_creature_that_taps_for_green() {
    let mut state = islands();
    let arbor = put(&mut state, P0, "Dryad Arbor", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(arbor));
    assert_eq!(zone(&state, arbor), Zone::Battlefield);
    assert!(is_creature(&state, arbor));
    assert!(engine::object_has_type(&state, arbor, CardType::Land));
    assert!(engine::has_effective_subtype(
        &state,
        arbor,
        Subtype::Forest
    ));
    assert_eq!(power_toughness(&state, arbor), (1, 1));
    state.objects.get_mut(arbor).summoning_sick = false;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(arbor));
    assert_eq!(mana(&state, P0, ManaColor::G), 1);
}

#[test]
fn flagstones_of_trokair_taps_for_white_and_fetches_a_plains_when_it_dies() {
    let mut state = game(&["Island", "Plains", "Island", "Island", "Island"]);
    let flagstones = put(&mut state, P0, "Flagstones of Trokair", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(flagstones));
    assert_eq!(mana(&state, P0, ManaColor::W), 1);
    to_graveyard(&mut state, flagstones);
    settle_taking(&mut state);
    let plains = controls(&state, P0, "Plains");
    assert_eq!(plains.len(), 1);
    assert!(state.objects.get(plains[0]).tapped);
    assert_eq!(state.players[0].library.len(), 4);
}

#[test]
fn mystic_sanctuary_returns_a_spell_to_the_top_only_when_it_enters_untapped() {
    let mut state = islands();
    for _ in 0..3 {
        put(&mut state, P0, "Island", Zone::Battlefield);
    }
    let brainstorm = put(&mut state, P0, "Brainstorm", Zone::Graveyard);
    let sanctuary = put(&mut state, P0, "Mystic Sanctuary", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(sanctuary));
    assert!(!state.objects.get(sanctuary).tapped, "three other Islands");
    settle_picking(&mut state, 1, Some(brainstorm));
    assert_eq!(zone(&state, brainstorm), Zone::Library);
    assert_eq!(state.players[0].library.first(), Some(&brainstorm));

    // With fewer Islands it enters tapped and nothing triggers.
    let mut state = islands();
    put(&mut state, P0, "Island", Zone::Battlefield);
    let brainstorm = put(&mut state, P0, "Brainstorm", Zone::Graveyard);
    let sanctuary = put(&mut state, P0, "Mystic Sanctuary", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(sanctuary));
    assert!(state.objects.get(sanctuary).tapped);
    settled(&mut state);
    assert_eq!(zone(&state, brainstorm), Zone::Graveyard);
}

#[test]
fn prismatic_vista_pays_life_and_fetches_a_basic_land_untapped() {
    let mut state = game(&["Island", "Plains", "Island", "Island", "Island"]);
    let vista = put(&mut state, P0, "Prismatic Vista", Zone::Battlefield);
    activate(&mut state, vista, 0, &[]);
    settle_taking(&mut state);
    assert_eq!(zone(&state, vista), Zone::Graveyard);
    assert_eq!(state.players[0].life, 19);
    assert_eq!(state.players[0].battlefield.len(), 1);
    let land = state.players[0].battlefield[0];
    assert!(!state.objects.get(land).tapped);
}

#[test]
fn valgavoths_lair_enters_tapped_and_taps_for_the_chosen_color() {
    let mut state = islands();
    let lair = put(&mut state, P0, "Valgavoth's Lair", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(lair));
    match next(&mut state) {
        Decision::ChooseEffectOption {
            player,
            option_count,
            ..
        } => {
            assert_eq!((player, option_count), (P0, 5));
            // White, blue, black, red, green: black.
            act(&mut state, Action::ChooseEffectOption(2));
        }
        other => panic!("expected the color choice, got {other:?}"),
    }
    assert!(state.objects.get(lair).tapped);
    assert_eq!(state.objects.get(lair).v4.chosen_color, Some(ManaColor::B));
    assert!(keyword(&state, lair, Keywords::HEXPROOF));
    state.objects.get_mut(lair).tapped = false;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(lair));
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
}

#[test]
fn back_to_basics_keeps_nonbasic_lands_tapped_in_the_untap_step() {
    let mut state = islands();
    put(&mut state, P0, "Back to Basics", Zone::Battlefield);
    let volcanic = put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    state.objects.get_mut(volcanic).tapped = true;
    state.objects.get_mut(island).tapped = true;
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert!(state.objects.get(volcanic).tapped);
    assert!(!state.objects.get(island).tapped);
}

#[test]
fn harbinger_of_the_seas_makes_nonbasic_lands_islands() {
    let mut state = islands();
    let volcanic = put(&mut state, P0, "Volcanic Island", Zone::Battlefield);
    let swamp = put(&mut state, P0, "Snow-Covered Swamp", Zone::Battlefield);
    // The Harbinger's timestamp is recorded as it enters.
    let harbinger = put(&mut state, P1, "Harbinger of the Seas", Zone::Hand);
    opponents_main_phase(&mut state);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 1);
    cast(&mut state, harbinger, &[]);
    settled(&mut state);
    assert!(engine::has_effective_subtype(
        &state,
        volcanic,
        Subtype::Island
    ));
    assert!(!engine::has_effective_subtype(
        &state,
        volcanic,
        Subtype::Mountain
    ));
    assert!(
        engine::has_effective_subtype(&state, swamp, Subtype::Swamp),
        "basic"
    );
    state.active_player = P0;
    state.priority_player = P0;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(volcanic));
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
    assert_eq!(
        mana(&state, P0, ManaColor::R),
        0,
        "its own abilities are gone"
    );
}

// ---- Spells ------------------------------------------------------------------------

#[test]
fn ad_nauseam_reveals_until_its_controller_stops() {
    let mut state = game(&["Counterspell", "Lightning Bolt", "Island", "Island"]);
    let library = state.players[0].library.clone();
    let nauseam = put(&mut state, P0, "Ad Nauseam", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 3);
    cast(&mut state, nauseam, &[]);
    let mut offers = 0;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption {
            option_count: 2, ..
        } => {
            offers += 1;
            // Repeat once, then stop.
            Action::ChooseEffectOption(u16::from(offers == 1))
        }
        other => first_answer(other),
    });
    assert_eq!(offers, 2);
    assert!(hand_contains(&state, P0, library[0]) && hand_contains(&state, P0, library[1]));
    assert_eq!(state.players[0].hand.len(), 2);
    assert_eq!(state.players[0].life, 20 - 2 - 1);
    assert!(known_to(&state, P1, P0, library[0]), "revealed");
}

#[test]
fn bitter_triumph_costs_a_discard_or_three_life() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let triumph = put(&mut state, P0, "Bitter Triumph", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, triumph, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Normal),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(angler)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, angler), Zone::Graveyard);
    assert_eq!(zone(&state, bolt), Zone::Graveyard, "discarded");
    assert_eq!(state.players[0].life, 20);

    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let triumph = put(&mut state, P0, "Bitter Triumph", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, triumph, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(angler)),
        Decision::Discard { .. } => panic!("the life payment replaces the discard"),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, angler), Zone::Graveyard);
    assert!(hand_contains(&state, P0, bolt));
    assert_eq!(state.players[0].life, 17);
}

#[test]
fn cabal_ritual_adds_three_black_or_five_with_threshold() {
    let mut state = islands();
    let ritual = put(&mut state, P0, "Cabal Ritual", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, ritual, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::B), 3);

    let mut state = islands();
    for _ in 0..7 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    let ritual = put(&mut state, P0, "Cabal Ritual", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, ritual, &[]);
    settled(&mut state);
    assert_eq!(mana(&state, P0, ManaColor::B), 5);
}

#[test]
fn erode_destroys_and_its_controller_may_fetch_a_tapped_basic() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let erode = put(&mut state, P0, "Erode", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 0);
    cast(&mut state, erode, &[Target::Object(angler)]);
    let mut searcher = None;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            selected_count: 0,
            ..
        } => {
            searcher = Some(player);
            Action::ChooseEffectTarget(legal_targets[0])
        }
        other => first_answer(other),
    });
    assert_eq!(searcher, Some(P1));
    assert_eq!(zone(&state, angler), Zone::Graveyard);
    let island = controls(&state, P1, "Island");
    assert_eq!(island.len(), 1);
    assert!(state.objects.get(island[0]).tapped);
}

#[test]
fn infernal_tutor_finds_a_copy_of_a_revealed_card_or_any_card_when_hellbent() {
    let mut state = game(&["Island", "Lightning Bolt", "Island", "Island"]);
    let library_bolt = state.players[0].library[1];
    let tutor = put(&mut state, P0, "Infernal Tutor", Zone::Hand);
    let hand_bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, tutor, &[]);
    let mut reveal_offered = false;
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } if legal_targets.contains(&Target::Object(hand_bolt)) => {
            reveal_offered = true;
            Action::ChooseEffectTarget(Target::Object(hand_bolt))
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert_eq!(legal_targets, vec![Target::Object(library_bolt)]);
            Action::ChooseEffectTarget(Target::Object(library_bolt))
        }
        other => first_answer(other),
    });
    assert!(reveal_offered);
    assert!(hand_contains(&state, P0, library_bolt));
    assert!(known_to(&state, P1, P0, hand_bolt), "the revealed card");
    assert!(known_to(&state, P1, P0, library_bolt), "the found card");

    // Hellbent: any card, not revealed.
    let mut state = game(&["Island", "Lightning Bolt", "Island", "Island"]);
    let library_bolt = state.players[0].library[1];
    let tutor = put(&mut state, P0, "Infernal Tutor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, tutor, &[]);
    settle_picking(&mut state, 0, Some(library_bolt));
    assert!(hand_contains(&state, P0, library_bolt));
    assert!(!known_to(&state, P1, P0, library_bolt));
}

#[test]
fn once_upon_a_time_is_free_as_the_first_spell_and_finds_a_creature_or_land() {
    let mut state = game(&[
        "Lightning Bolt",
        "Delver of Secrets",
        "Counterspell",
        "Brainstorm",
        "Ponder",
        "Island",
    ]);
    let library = state.players[0].library.clone();
    let once = put(&mut state, P0, "Once Upon a Time", Zone::Hand);
    let second = put(&mut state, P0, "Once Upon a Time", Zone::Hand);
    cast(&mut state, once, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert!(legal_targets.contains(&Target::Object(library[1])));
            Action::ChooseEffectTarget(Target::Object(library[1]))
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, library[1]));
    assert_eq!(state.players[0].library.first(), Some(&library[5]));
    assert_eq!(state.players[0].library.len(), library.len() - 1);
    // No longer the first spell: the second costs {1}{G}.
    assert!(!castable(&mut state, second));
}

#[test]
fn rain_of_filth_lets_lands_be_sacrificed_for_black_without_the_stack() {
    let mut state = islands();
    let first = put(&mut state, P0, "Island", Zone::Battlefield);
    let second = put(&mut state, P0, "Island", Zone::Battlefield);
    let opposing = put(&mut state, P1, "Island", Zone::Battlefield);
    let rain = put(&mut state, P0, "Rain of Filth", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, rain, &[]);
    settled(&mut state);
    let index = u8::try_from(def("Island").activated_abilities.len()).unwrap();
    let (_, abilities) = priority(&mut state);
    assert!(abilities.contains(&(first, index)) && abilities.contains(&(second, index)));
    assert!(!abilities.contains(&(opposing, index)));
    act(&mut state, Action::ActivateAbility(first, index));
    assert_eq!(zone(&state, first), Zone::Graveyard);
    assert_eq!(mana(&state, P0, ManaColor::B), 1);
    assert!(state.stack.is_empty());
    // Only until end of turn.
    pass_until(&mut state, |state| state.active_player == P1);
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1 && state.stack.is_empty()
    });
    assert!(!activatable(&mut state, second, index));
}

#[test]
fn supreme_verdict_destroys_every_creature() {
    let mut state = islands();
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let verdict = put(&mut state, P0, "Supreme Verdict", Zone::Hand);
    pool(
        &mut state,
        P0,
        &[ManaColor::W, ManaColor::W, ManaColor::U],
        1,
    );
    cast(&mut state, verdict, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, delver), Zone::Graveyard);
    assert_eq!(zone(&state, angler), Zone::Graveyard);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
}

// ---- Creatures ---------------------------------------------------------------------

#[test]
fn canoptek_scarab_swarm_exiles_a_graveyard_for_insects() {
    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Graveyard);
    let island = put(&mut state, P1, "Island", Zone::Graveyard);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let swarm = put(&mut state, P0, "Canoptek Scarab Swarm", Zone::Hand);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, swarm, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Player(P1)));
            Action::ChooseTarget(Target::Player(P1))
        }
        other => first_answer(other),
    });
    for card in [ring, island, bolt] {
        assert_eq!(zone(&state, card), Zone::Exile);
    }
    let insects = controls(&state, P0, "Insect Artifact Token");
    assert_eq!(insects.len(), 2, "an artifact and a land");
    assert!(keyword(&state, insects[0], Keywords::FLYING));
    assert!(engine::object_has_type(
        &state,
        insects[0],
        CardType::Artifact
    ));
}

#[test]
fn cloak_and_dagger_exiles_a_hand_card_or_the_chosen_creature_until_it_leaves() {
    for take_creature in [false, true] {
        let mut state = islands();
        let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
        let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
        let land = put(&mut state, P1, "Island", Zone::Hand);
        let cloak = put(&mut state, P0, "Cloak and Dagger, Entwined", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::W, ManaColor::B], 1);
        cast(&mut state, cloak, &[]);
        let wanted = if take_creature { angler } else { bolt };
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Player(P1)) =>
            {
                Action::ChooseTarget(Target::Player(P1))
            }
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(angler)) =>
            {
                Action::ChooseTarget(Target::Object(angler))
            }
            Decision::ChooseEffectTargets {
                legal_targets,
                selected_count: 0,
                ..
            } => {
                assert!(legal_targets.contains(&Target::Object(bolt)));
                assert!(legal_targets.contains(&Target::Object(angler)));
                assert!(!legal_targets.contains(&Target::Object(land)), "nonland");
                Action::ChooseEffectTarget(Target::Object(wanted))
            }
            other => first_answer(other),
        });
        assert!(known_to(&state, P0, P1, land), "they reveal their hand");
        assert_eq!(zone(&state, wanted), Zone::Exile);
        to_graveyard(&mut state, cloak);
        settled(&mut state);
        if take_creature {
            assert_eq!(zone(&state, angler), Zone::Battlefield);
            assert_eq!(state.objects.get(angler).controller, P1);
        } else {
            assert!(hand_contains(&state, P1, bolt));
        }
    }
}

#[test]
fn dark_matter_manipulator_mills_three_and_grows_for_every_seven_cards() {
    let mut state = islands();
    let manipulator = put(&mut state, P0, "Dark Matter Manipulator", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, manipulator, &[]);
    settle_ordering(&mut state);
    assert_eq!(state.players[0].graveyard.len(), 3);
    assert_eq!(power_toughness(&state, manipulator), (1, 2));
    for _ in 0..4 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    assert_eq!(power_toughness(&state, manipulator), (3, 2));
}

#[test]
fn emperor_of_bones_exiles_at_combat_and_reanimates_when_it_adapts() {
    let mut state = islands();
    let emperor = put(&mut state, P0, "Emperor of Bones", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Graveyard);
    pass_until(&mut state, |state| state.step == Step::BeginCombat);
    settle_picking(&mut state, 0, Some(angler));
    assert_eq!(zone(&state, angler), Zone::Exile);
    pool(&mut state, P0, &[ManaColor::B], 1);
    activate(&mut state, emperor, 0, &[]);
    settle_picking(&mut state, 0, Some(angler));
    assert_eq!(state.objects.get(emperor).counters.plus1_plus1, 2);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).controller, P0);
    assert!(keyword(&state, angler, Keywords::HASTE));
    // Adapt does nothing while it has counters.
    pool(&mut state, P0, &[ManaColor::B], 1);
    activate(&mut state, emperor, 0, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(emperor).counters.plus1_plus1, 2);
    // Sacrificed at the next end step; the finality counter exiles it.
    end_step(&mut state);
    assert_eq!(zone(&state, angler), Zone::Exile);
}

#[test]
fn flickerwisp_returns_its_target_at_the_next_end_step() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let flickerwisp = put(&mut state, P0, "Flickerwisp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 1);
    cast(&mut state, flickerwisp, &[]);
    settle_picking(&mut state, 0, Some(angler));
    assert_eq!(zone(&state, angler), Zone::Exile);
    end_step(&mut state);
    assert_eq!(zone(&state, angler), Zone::Battlefield);
    assert_eq!(state.objects.get(angler).controller, P1);
}

#[test]
fn hogaak_is_paid_only_by_convoke_and_delve_and_casts_from_the_graveyard() {
    for from_graveyard in [false, true] {
        let mut state = islands();
        let elves: Vec<ObjectId> = (0..3)
            .map(|_| put(&mut state, P0, "Llanowar Elves", Zone::Battlefield))
            .collect();
        let delved: Vec<ObjectId> = (0..4)
            .map(|_| put(&mut state, P0, "Island", Zone::Graveyard))
            .collect();
        let hogaak = put(
            &mut state,
            P0,
            "Hogaak, Arisen Necropolis",
            if from_graveyard {
                Zone::Graveyard
            } else {
                Zone::Hand
            },
        );
        // Mana in the pool never pays for it.
        pool(&mut state, P0, &[ManaColor::B, ManaColor::G], 5);
        cast(&mut state, hogaak, &[]);
        let mut chosen = Vec::new();
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseCastMode { options, .. } => {
                assert_eq!(options, vec![CastMode::Alternative]);
                Action::ChooseCastMode(CastMode::Alternative)
            }
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                match legal_targets
                    .iter()
                    .find(|target| !chosen.contains(*target))
                {
                    Some(&target) => {
                        chosen.push(target);
                        Action::ChooseEffectTarget(target)
                    }
                    None => Action::FinishEffectSelection,
                }
            }
            other => first_answer(other),
        });
        assert_eq!(chosen.len(), 3, "the three Elves");
        assert_eq!(zone(&state, hogaak), Zone::Battlefield);
        assert!(elves.iter().all(|&elf| state.objects.get(elf).tapped));
        assert!(delved.iter().all(|&card| zone(&state, card) == Zone::Exile));
        assert_eq!(mana(&state, P0, ManaColor::C), 5, "no mana spent");
        assert_eq!(power_toughness(&state, hogaak), (8, 8));
    }
}

#[test]
fn loran_destroys_an_artifact_and_draws_for_both_players() {
    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let loran = put(&mut state, P0, "Loran of the Third Path", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, loran, &[]);
    settle_picking(&mut state, 0, Some(ring));
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    state.objects.get_mut(loran).summoning_sick = false;
    activate(&mut state, loran, 0, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.players[1].hand.len(), 1);
}

#[test]
fn overlord_of_the_balemurk_impends_mills_and_returns_a_creature() {
    let mut state = game(&["Island", "Gurmag Angler", "Island", "Island", "Island"]);
    let angler = state.players[0].library[1];
    let overlord = put(&mut state, P0, "Overlord of the Balemurk", Zone::Hand);
    // Only the impending cost is affordable.
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, overlord, &[]);
    settle_picking(&mut state, 0, Some(angler));
    assert_eq!(zone(&state, overlord), Zone::Battlefield);
    assert_eq!(state.objects.get(overlord).v4.time_counters_v1, 5);
    assert!(!is_creature(&state, overlord));
    assert!(hand_contains(&state, P0, angler));
    assert_eq!(state.players[0].graveyard.len(), 3);
}

#[test]
fn recruiter_of_the_guard_finds_a_creature_with_toughness_two_or_less() {
    let mut state = game(&["Island", "Gurmag Angler", "Delver of Secrets", "Island"]);
    let angler = state.players[0].library[1];
    let delver = state.players[0].library[2];
    let recruiter = put(&mut state, P0, "Recruiter of the Guard", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, recruiter, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert!(!legal_targets.contains(&Target::Object(angler)));
            Action::ChooseEffectTarget(Target::Object(delver))
        }
        other => first_answer(other),
    });
    assert!(hand_contains(&state, P0, delver));
    assert!(known_to(&state, P1, P0, delver), "revealed");
}

#[test]
fn skyclave_apparition_exiles_and_leaves_an_illusion_of_that_mana_value() {
    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let skyclave = put(&mut state, P0, "Skyclave Apparition", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 1);
    cast(&mut state, skyclave, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                !legal_targets.contains(&Target::Object(angler)),
                "mana value 7"
            );
            Action::ChooseTarget(Target::Object(ring))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, ring), Zone::Exile);
    to_graveyard(&mut state, skyclave);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Exile, "the card stays exiled");
    let illusions = controls(&state, P1, "Illusion Token");
    assert_eq!(illusions.len(), 1);
    assert_eq!(power_toughness(&state, illusions[0]), (1, 1));
}

#[test]
fn solitude_is_evoked_by_exiling_a_white_card_and_exiles_a_creature() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let solitude = put(&mut state, P0, "Solitude", Zone::Hand);
    let swords = put(&mut state, P0, "Swords to Plowshares", Zone::Hand);
    cast(&mut state, solitude, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCastMode { .. } => Action::ChooseCastMode(CastMode::Alternative),
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![swords]);
            Action::ChooseCostTarget(swords)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(angler)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, swords), Zone::Exile);
    assert_eq!(zone(&state, angler), Zone::Exile);
    assert_eq!(state.players[1].life, 25);
    assert_eq!(zone(&state, solitude), Zone::Graveyard, "evoked");
}

#[test]
fn stitchers_supplier_mills_three_on_entering_and_dying() {
    let mut state = islands();
    let supplier = put(&mut state, P0, "Stitcher's Supplier", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, supplier, &[]);
    settle_ordering(&mut state);
    assert_eq!(state.players[0].graveyard.len(), 3);
    to_graveyard(&mut state, supplier);
    settle_ordering(&mut state);
    assert_eq!(state.players[0].graveyard.len(), 7);
}

#[test]
fn stoneforge_mystic_finds_and_puts_in_equipment() {
    let mut state = game(&["Island", "Shadowspear", "Island", "Island"]);
    let spear = state.players[0].library[1];
    let mystic = put(&mut state, P0, "Stoneforge Mystic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, mystic, &[]);
    settle_picking(&mut state, 0, Some(spear));
    assert!(hand_contains(&state, P0, spear));
    assert!(known_to(&state, P1, P0, spear));
    state.objects.get_mut(mystic).summoning_sick = false;
    pool(&mut state, P0, &[ManaColor::W], 1);
    activate(&mut state, mystic, 0, &[]);
    settle_picking(&mut state, 0, Some(spear));
    assert_eq!(zone(&state, spear), Zone::Battlefield);
    assert!(state.objects.get(mystic).tapped);
}

#[test]
fn sylvan_safekeeper_sacrifices_a_land_to_give_shroud() {
    let mut state = islands();
    let safekeeper = put(&mut state, P0, "Sylvan Safekeeper", Zone::Battlefield);
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    assert!(activatable(&mut state, safekeeper, 0));
    act(&mut state, Action::ActivateAbility(safekeeper, 0));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![island]);
            Action::ChooseCostTarget(island)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(delver)),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, island), Zone::Graveyard);
    assert!(keyword(&state, delver, Keywords::SHROUD));
}

#[test]
fn thought_monitor_has_affinity_and_draws_two() {
    let mut state = islands();
    for _ in 0..4 {
        put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    }
    let monitor = put(&mut state, P0, "Thought Monitor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, monitor, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, monitor), Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 2);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn white_orchid_phantom_destroys_a_nonbasic_land_and_its_controller_may_fetch() {
    let mut state = islands();
    let volcanic = put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    let basic = put(&mut state, P1, "Island", Zone::Battlefield);
    let phantom = put(&mut state, P0, "White Orchid Phantom", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::W], 0);
    cast(&mut state, phantom, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(basic)));
            Action::ChooseTarget(Target::Object(volcanic))
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => Action::ChooseEffectTarget(legal_targets[0]),
        other => first_answer(other),
    });
    assert_eq!(zone(&state, volcanic), Zone::Graveyard);
    assert_eq!(controls(&state, P1, "Island").len(), 2);
}

#[test]
fn wight_of_the_reliquary_grows_and_sacrifices_a_creature_for_a_land() {
    let mut state = game(&["Island", "Volcanic Island", "Island", "Island"]);
    let volcanic = state.players[0].library[1];
    let wight = put(&mut state, P0, "Wight of the Reliquary", Zone::Battlefield);
    put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    assert_eq!(power_toughness(&state, wight), (3, 3));
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Battlefield);
    assert!(activatable(&mut state, wight, 0));
    act(&mut state, Action::ActivateAbility(wight, 0));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseCostTargets { candidates, .. } => {
            assert_eq!(candidates, vec![delver]);
            Action::ChooseCostTarget(delver)
        }
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert!(legal_targets.contains(&Target::Object(volcanic)));
            Action::ChooseEffectTarget(Target::Object(volcanic))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, delver), Zone::Graveyard);
    assert_eq!(zone(&state, volcanic), Zone::Battlefield);
    assert!(state.objects.get(volcanic).tapped);
    assert_eq!(power_toughness(&state, wight), (4, 4));
}

#[test]
fn yorion_flickers_nonland_permanents_and_is_a_companion_of_eighty_card_decks() {
    let yorion_id = id("Yorion, Sky Nomad");
    let deck = |count: usize| vec![id("Island"); count];
    assert_eq!(
        mtg_kernel::eternal_lurrus_v1::companion_condition_met(yorion_id, &deck(80)),
        Some(true)
    );
    assert_eq!(
        mtg_kernel::eternal_lurrus_v1::companion_condition_met(yorion_id, &deck(60)),
        Some(false)
    );

    let mut state = islands();
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let yorion = put(&mut state, P0, "Yorion, Sky Nomad", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 3);
    cast(&mut state, yorion, &[]);
    let mut chosen = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(island)));
            assert!(!legal_targets.contains(&Target::Object(yorion)));
            match legal_targets
                .iter()
                .find(|target| !chosen.contains(*target))
            {
                Some(&target) => {
                    chosen.push(target);
                    Action::ChooseEffectTarget(target)
                }
                None => Action::FinishEffectSelection,
            }
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, delver), Zone::Exile);
    assert_eq!(zone(&state, ring), Zone::Exile);
    end_step(&mut state);
    assert_eq!(zone(&state, delver), Zone::Battlefield);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
    assert_eq!(zone(&state, island), Zone::Battlefield);
}

#[test]
fn yorion_waits_in_exile_as_a_companion_and_joins_the_hand_for_three() {
    let mut state = islands();
    mtg_kernel::eternal_lurrus_v1::add_companions(
        &mut state,
        [Some(id("Yorion, Sky Nomad")), None],
    );
    let yorion = *state.exile.last().unwrap();
    pool(&mut state, P0, &[], 3);
    assert!(activatable(&mut state, yorion, 0));
    act(&mut state, Action::ActivateAbility(yorion, 0));
    assert_eq!(zone(&state, yorion), Zone::Hand);
    assert!(state.stack.is_empty());
}

// ---- Equipment ---------------------------------------------------------------------

#[test]
fn lichs_relic_may_pay_two_to_destroy_and_equips_for_two_one() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let relic = put(&mut state, P0, "Lich's Relic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, relic, &[]);
    let mut seen = Vec::new();
    settle_with(&mut state, |_, decision| {
        seen.push(format!("{decision:?}"));
        match decision {
            Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
            Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(angler)) =>
            {
                Action::ChooseTarget(Target::Object(angler))
            }
            other => first_answer(other),
        }
    });
    assert_eq!(state.players[0].mana_pool, [0; 6], "{seen:?}");
    if zone(&state, angler) != Zone::Graveyard {
        let log = &state.engine.event_log;
        panic!(
            "the reflexive trigger did not destroy the target
seen: {seen:?}
queued: {:?}
pending: {:?}
stack: {:?}
events: {:#?}",
            state.eternal_v1.as_ref().map(|eternal| &eternal.e8a.queued_triggers),
            state.engine.pending_triggers,
            state.stack,
            &log[log.len().saturating_sub(12)..],
        );
    }
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, relic, 0, &[Target::Object(delver)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, delver), (3, 2));
}

#[test]
fn lion_sash_exiles_graveyard_cards_and_reconfigures() {
    let mut state = islands();
    let sash = put(&mut state, P0, "Lion Sash", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Graveyard);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, sash, 0, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Exile);
    assert_eq!(state.objects.get(sash).counters.plus1_plus1, 1);
    pool(&mut state, P0, &[ManaColor::W], 0);
    activate(&mut state, sash, 0, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert_eq!(
        state.objects.get(sash).counters.plus1_plus1,
        1,
        "not a permanent card"
    );
    assert_eq!(power_toughness(&state, sash), (2, 2));
    assert!(!activatable(&mut state, sash, 2), "not attached");

    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, sash, 1, &[Target::Object(delver)]);
    settled(&mut state);
    assert!(!is_creature(&state, sash), "attached");
    assert_eq!(power_toughness(&state, delver), (2, 2));
    pool(&mut state, P0, &[], 2);
    activate(&mut state, sash, 2, &[]);
    settled(&mut state);
    assert!(is_creature(&state, sash));
    assert_eq!(power_toughness(&state, delver), (1, 1));
}

#[test]
fn meteor_sword_destroys_a_permanent_and_equips_for_three_three() {
    let mut state = islands();
    let volcanic = put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    let sword = put(&mut state, P0, "Meteor Sword", Zone::Hand);
    pool(&mut state, P0, &[], 7);
    cast(&mut state, sword, &[]);
    settle_picking(&mut state, 0, Some(volcanic));
    assert_eq!(zone(&state, volcanic), Zone::Graveyard);
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    activate(&mut state, sword, 0, &[Target::Object(delver)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, delver), (4, 4));
}

#[test]
fn pre_war_formalwear_returns_a_small_creature_and_equips_it() {
    let mut state = islands();
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Graveyard);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Graveyard);
    let formalwear = put(&mut state, P0, "Pre-War Formalwear", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    cast(&mut state, formalwear, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                !legal_targets.contains(&Target::Object(angler)),
                "mana value 7"
            );
            Action::ChooseTarget(Target::Object(delver))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, delver), Zone::Battlefield);
    assert_eq!(
        state
            .objects
            .get(formalwear)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(delver)
    );
    assert_eq!(power_toughness(&state, delver), (3, 3));
    assert!(keyword(&state, delver, Keywords::VIGILANCE));
}

#[test]
fn aether_vial_gains_charge_counters_and_puts_in_a_matching_creature() {
    let mut state = islands();
    let vial = put(&mut state, P0, "Aether Vial", Zone::Battlefield);
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Hand);
    let angler = put(&mut state, P0, "Gurmag Angler", Zone::Hand);
    // The upkeep counter is optional; take it.
    pass_until(&mut state, |state| state.active_player == P1);
    run_until(
        &mut state,
        |state| state.active_player == P0 && state.step == Step::Main1,
        |_, decision| match decision {
            Decision::ChooseEffectOption {
                player: P0,
                option_count: 2,
                ..
            } => Some(Action::ChooseEffectOption(1)),
            _ => None,
        },
    );
    settled(&mut state);
    assert_eq!(state.objects.get(vial).counters.charge, 1);
    activate(&mut state, vial, 0, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert!(!legal_targets.contains(&Target::Object(angler)));
            Action::ChooseEffectTarget(Target::Object(delver))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, delver), Zone::Battlefield);
    assert!(state.objects.get(vial).tapped);
}

#[test]
fn wishclaw_talisman_tutors_then_passes_to_the_opponent() {
    let mut state = game(&["Island", "Lightning Bolt", "Island", "Island"]);
    let bolt = state.players[0].library[1];
    let talisman = put(&mut state, P0, "Wishclaw Talisman", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    cast(&mut state, talisman, &[]);
    settled(&mut state);
    let wishes = |state: &GameState| {
        state
            .eternal_v1
            .as_ref()
            .and_then(|eternal| {
                eternal
                    .e8a
                    .wish_counters
                    .iter()
                    .find(|(host, _)| host.object == talisman)
                    .map(|(_, count)| *count)
            })
            .unwrap_or(0)
    };
    assert_eq!(wishes(&state), 3);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, talisman, 0, &[]);
    settle_picking(&mut state, 0, Some(bolt));
    assert!(hand_contains(&state, P0, bolt));
    assert!(!known_to(&state, P1, P0, bolt), "not revealed");
    assert_eq!(state.objects.get(talisman).controller, P1);
    assert_eq!(wishes(&state), 2);
    // Only during its controller's turn.
    state.priority_player = P1;
    pool(&mut state, P1, &[], 1);
    assert!(!activatable(&mut state, talisman, 0));
}

// ---- Planeswalkers -------------------------------------------------------------------

fn cast_chandra(state: &mut GameState) -> ObjectId {
    let chandra = put(state, P0, "Chandra, Chill of Compliance", Zone::Hand);
    pool(state, P0, &[ManaColor::U, ManaColor::U], 1);
    cast(state, chandra, &[]);
    settled(state);
    assert_eq!(loyalty(state, chandra), Some(3));
    chandra
}

#[test]
fn chandra_surveils_and_keeps_a_noncreature_nonland_card() {
    let mut state = game(&["Lightning Bolt", "Island", "Island"]);
    let bolt = state.players[0].library[0];
    let chandra = cast_chandra(&mut state);
    activate(&mut state, chandra, 0, &[]);
    settle_picking(&mut state, 1, None);
    assert!(hand_contains(&state, P0, bolt));
    assert_eq!(loyalty(&state, chandra), Some(4));
}

#[test]
fn chandras_mana_pays_only_for_noncreature_spells() {
    let mut state = islands();
    let chandra = cast_chandra(&mut state);
    let brainstorm = put(&mut state, P0, "Brainstorm", Zone::Hand);
    let delver = put(&mut state, P0, "Delver of Secrets", Zone::Hand);
    activate(&mut state, chandra, 1, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].restricted_mana_pool.0.len(), 1);
    let (spells, _) = priority(&mut state);
    assert!(spells.contains(&brainstorm));
    assert!(!spells.contains(&delver));
}

#[test]
fn chandra_stuns_with_minus_x_and_her_emblem_draws_on_each_spell() {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let chandra = cast_chandra(&mut state);
    assert!(activatable(&mut state, chandra, 2));
    act(&mut state, Action::ActivateAbility(chandra, 2));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert_eq!(option_count, 4, "X from 0 to 3");
            Action::ChooseEffectOption(2)
        }
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Object(angler)),
        other => first_answer(other),
    });
    assert!(state.objects.get(angler).tapped);
    assert_eq!(state.objects.get(angler).counters.stun, 2);
    assert_eq!(loyalty(&state, chandra), Some(1));

    let mut state = islands();
    let chandra = cast_chandra(&mut state);
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, chandra, 3);
    activate(&mut state, chandra, 3, &[]);
    settled(&mut state);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(state.players[0].hand.len(), 1, "the emblem drew a card");
}

#[test]
fn teferi_restricts_opponents_to_sorcery_timing_and_bounces() {
    let mut state = islands();
    let teferi = put(&mut state, P0, "Teferi, Time Raveler", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 1);
    cast(&mut state, teferi, &[]);
    settled(&mut state);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let brainstorm = put(&mut state, P0, "Brainstorm", Zone::Hand);
    let ponder = put(&mut state, P0, "Ponder", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    // +1: sorceries have flash.
    activate(&mut state, teferi, 0, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, teferi), Some(5));
    cast(&mut state, brainstorm, &[]);
    assert!(
        castable(&mut state, ponder),
        "a sorcery with the stack nonempty"
    );
    act(&mut state, Action::Pass);
    // P1 may not respond.
    let (spells, _) = priority(&mut state);
    assert_eq!(state.priority_player, P1);
    assert!(!spells.contains(&bolt));

    // -3 returns a creature and draws.
    let mut state = islands();
    let teferi = put(&mut state, P0, "Teferi, Time Raveler", Zone::Hand);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W, ManaColor::U], 1);
    cast(&mut state, teferi, &[]);
    settled(&mut state);
    activate(&mut state, teferi, 1, &[Target::Object(angler)]);
    settled(&mut state);
    assert!(hand_contains(&state, P1, angler));
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(loyalty(&state, teferi), Some(1));
}
