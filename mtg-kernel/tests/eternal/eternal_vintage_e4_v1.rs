//! Eternal wave E4: native rules tests for the cards that complete Vintage
//! Jewel Shops and Mono-White Initiative.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1, LimitedJsonlServerV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E4_CARDS: [&str; 36] = [
    "Mox Emerald",
    "Mox Jet",
    "Mox Pearl",
    "Mox Ruby",
    "Mox Sapphire",
    "Black Lotus",
    "Sol Ring",
    "Mana Crypt",
    "Snow-Covered Island",
    "Strip Mine",
    "Karakas",
    "Tolarian Academy",
    "Ancestral Recall",
    "Time Walk",
    "Mental Misstep",
    "Tinker",
    "Paradoxical Outcome",
    "Coveted Jewel",
    "Mishra's Workshop",
    "Manifold Key",
    "Vexing Bauble",
    "Void Mirror",
    "Trinisphere",
    "Archon of Emeria",
    "Clarion Conqueror",
    "Mana Vault",
    "Time Vault",
    "White Plume Adventurer",
    "Seasoned Dungeoneer",
    "Karn, the Great Creator",
    "Tezzeret, Cruel Captain",
    "Urza's Saga",
    "Phyrexian Metamorph",
    "Witch Enchanter",
    "Anointed Peacekeeper",
    "Chancellor of the Annex",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[card_id_by_name(name).unwrap_or_else(|| panic!("{name}")) as usize]
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn seeded_game(library: &[&str], seed: u64) -> GameState {
    let ids: Vec<u16> = library
        .iter()
        .map(|name| card_id_by_name(name).unwrap_or_else(|| panic!("{name}")))
        .collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        seed,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn game(library: &[&str]) -> GameState {
    seeded_game(library, 0x4534_5649_4e54)
}

fn islands() -> GameState {
    game(&["Island"; 20])
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name}"));
    let id = state.objects.push(GameObject {
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
        Zone::Hand => seat.hand.push(id),
        Zone::Battlefield => seat.battlefield.push(id),
        Zone::Graveyard => seat.graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        _ => panic!("helper zone"),
    }
    id
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

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
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

/// Passes every priority window, declaring no attackers or blockers, until
/// `stop` holds before a decision.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    for _ in 0..400 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(Vec::new())),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(Vec::new())),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::Discard { count, choices, .. } => act(
                state,
                Action::Discard(choices.into_iter().take(count as usize).collect()),
            ),
            other => panic!("unexpected decision while passing: {other:?}"),
        }
    }
    panic!("condition never held");
}

#[test]
fn e4_cards_are_full_with_their_printed_characteristics() {
    for name in E4_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
    }
    for name in [
        "Mox Emerald",
        "Mox Jet",
        "Mox Pearl",
        "Mox Ruby",
        "Mox Sapphire",
        "Black Lotus",
        "Mana Crypt",
    ] {
        let card = def(name);
        assert_eq!(card.types, &[CardType::Artifact], "{name}");
        assert_eq!(card.mana_value, 0, "{name}");
        assert!(card.colors.is_empty(), "{name}");
    }
    assert_eq!(def("Sol Ring").mana_value, 1);
    let island = def("Snow-Covered Island");
    assert!(island.is_land);
    assert_eq!(island.subtypes, &[Subtype::Island]);
    assert_eq!(island.supertypes, &[Supertype::Basic, Supertype::Snow]);
    for name in ["Karakas", "Tolarian Academy"] {
        assert!(def(name).is_land, "{name}");
        assert_eq!(def(name).supertypes, &[Supertype::Legendary], "{name}");
    }
    assert!(def("Strip Mine").is_land);
    assert_eq!(def("Ancestral Recall").types, &[CardType::Instant]);
    assert_eq!(def("Time Walk").types, &[CardType::Sorcery]);
    assert_eq!(def("Time Walk").mana_value, 2);
    for (name, subtypes) in [
        ("Archon of Emeria", &[Subtype::Archon][..]),
        ("Clarion Conqueror", &[Subtype::Dragon][..]),
        (
            "White Plume Adventurer",
            &[Subtype::Orc, Subtype::Cleric][..],
        ),
        (
            "Seasoned Dungeoneer",
            &[Subtype::Human, Subtype::Warrior][..],
        ),
        (
            "Phyrexian Metamorph",
            &[Subtype::Phyrexian, Subtype::Shapeshifter][..],
        ),
        ("Witch Enchanter", &[Subtype::Human, Subtype::Warlock][..]),
        (
            "Anointed Peacekeeper",
            &[Subtype::Human, Subtype::Cleric][..],
        ),
        (
            "Chancellor of the Annex",
            &[Subtype::Phyrexian, Subtype::Angel][..],
        ),
    ] {
        assert_eq!(def(name).subtypes, subtypes, "{name}");
        assert!(def(name).has_type(CardType::Creature), "{name}");
    }
    for name in [
        "Archon of Emeria",
        "Clarion Conqueror",
        "Chancellor of the Annex",
    ] {
        assert!(def(name).keywords.has(Keywords::FLYING), "{name}");
    }
    assert!(def("Anointed Peacekeeper")
        .keywords
        .has(Keywords::VIGILANCE));
    assert!(Subtype::Archon.is_creature_type());
    assert!(!Subtype::Karn.is_creature_type());
    assert_eq!(def("Karn, the Great Creator").subtypes, &[Subtype::Karn]);
    assert_eq!(
        def("Tezzeret, Cruel Captain").subtypes,
        &[Subtype::Tezzeret]
    );
    assert_eq!(def("Karn, the Great Creator").starting_loyalty, Some(5));
    assert_eq!(def("Tezzeret, Cruel Captain").starting_loyalty, Some(4));
    let saga = def("Urza's Saga");
    assert!(saga.is_land && saga.has_type(CardType::Enchantment));
    assert_eq!(saga.subtypes, &[Subtype::Urzas, Subtype::Saga]);
    assert_eq!(
        saga.saga.as_ref().map(|saga| saga.chapter_effects.len()),
        Some(3)
    );
    let witch = def("Witch Enchanter");
    assert_eq!(
        witch.transform_face.as_ref().map(|face| face.name),
        Some("Witch-Blessed Meadow")
    );
    assert_eq!(def("Mental Misstep").mana_value, 1);
    assert!(def("Time Vault").enters_battlefield_tapped);
    assert!(def("Tinker").additional_cost.is_some());
}

// ---- Fast mana ------------------------------------------------------------

#[test]
fn moxen_cost_nothing_and_tap_for_their_color() {
    for (name, color) in [
        ("Mox Emerald", ManaColor::G),
        ("Mox Jet", ManaColor::B),
        ("Mox Pearl", ManaColor::W),
        ("Mox Ruby", ManaColor::R),
        ("Mox Sapphire", ManaColor::U),
    ] {
        let mut state = islands();
        let mox = put(&mut state, P0, name, Zone::Hand);
        cast(&mut state, mox, &[]);
        settled(&mut state);
        assert_eq!(state.objects.get(mox).zone, Zone::Battlefield, "{name}");
        act(&mut state, Action::ActivateManaAbility(mox));
        assert_eq!(mana(&state, P0, color), 1, "{name}");
        assert!(state.objects.get(mox).tapped, "{name}");
    }
}

#[test]
fn black_lotus_sacrifices_for_three_mana_of_one_color() {
    let mut state = islands();
    let lotus = put(&mut state, P0, "Black Lotus", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(lotus, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 3);
    assert_eq!(state.objects.get(lotus).zone, Zone::Graveyard);
}

#[test]
fn sol_ring_and_mana_crypt_tap_for_two_colorless() {
    for name in ["Sol Ring", "Mana Crypt"] {
        let mut state = islands();
        let rock = put(&mut state, P0, name, Zone::Battlefield);
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(rock));
        assert_eq!(mana(&state, P0, ManaColor::C), 2, "{name}");
        assert!(state.objects.get(rock).tapped, "{name}");

        // The payment planner counts both mana: one rock pays a {2} spell.
        let mut state = islands();
        let rock = put(&mut state, P0, name, Zone::Battlefield);
        let wellspring = put(&mut state, P0, "Ichor Wellspring", Zone::Hand);
        cast(&mut state, wellspring, &[]);
        settled(&mut state);
        assert_eq!(
            state.objects.get(wellspring).zone,
            Zone::Battlefield,
            "{name}"
        );
        assert!(state.objects.get(rock).tapped, "{name}");
        assert_eq!(state.players[0].mana_pool, [0; 6], "{name}");
    }
}

#[test]
fn mana_crypt_flips_a_coin_each_upkeep_and_deals_three_on_a_loss() {
    let mut outcomes = Vec::new();
    for seed in 1..=24_u64 {
        let mut state = seeded_game(&["Island"; 20], seed);
        put(&mut state, P0, "Mana Crypt", Zone::Battlefield);
        pass_until(&mut state, |state| {
            state.active_player == P0 && state.step == Step::Main1 && state.turn >= 2
        });
        let life = state.players[0].life;
        assert!(life == 20 || life == 17, "seed {seed}: life {life}");
        assert_eq!(state.players[1].life, 20, "seed {seed}");
        outcomes.push(life);
    }
    assert!(outcomes.contains(&20), "some flips are won");
    assert!(outcomes.contains(&17), "some flips are lost");
}

#[test]
fn tolarian_academy_adds_blue_for_each_artifact() {
    let mut state = islands();
    let academy = put(&mut state, P0, "Tolarian Academy", Zone::Battlefield);
    for _ in 0..3 {
        put(&mut state, P0, "Mishra's Bauble", Zone::Battlefield);
    }
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(academy));
    assert_eq!(mana(&state, P0, ManaColor::U), 3);

    // No artifact: the planner offers nothing from it.
    let mut state = islands();
    put(&mut state, P0, "Tolarian Academy", Zone::Battlefield);
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    assert!(!castable(&mut state, recall));
    // One artifact pays {U}.
    let mut state = islands();
    let academy = put(&mut state, P0, "Tolarian Academy", Zone::Battlefield);
    put(&mut state, P0, "Mishra's Bauble", Zone::Battlefield);
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    cast(&mut state, recall, &[Target::Player(P0)]);
    settled(&mut state);
    assert!(state.objects.get(academy).tapped);
    assert_eq!(state.players[0].hand.len(), 3);
}

// ---- Lands ----------------------------------------------------------------

#[test]
fn snow_covered_island_is_a_basic_snow_island() {
    let mut state = islands();
    let island = put(&mut state, P0, "Snow-Covered Island", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(island));
    assert!(engine::has_effective_subtype(
        &state,
        island,
        Subtype::Island
    ));
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(island));
    assert_eq!(mana(&state, P0, ManaColor::U), 1);
}

#[test]
fn strip_mine_taps_for_colorless_and_destroys_any_land() {
    let mut state = islands();
    let mine = put(&mut state, P0, "Strip Mine", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(mine));
    assert_eq!(mana(&state, P0, ManaColor::C), 1);

    let mut state = islands();
    let mine = put(&mut state, P0, "Strip Mine", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    activate(&mut state, mine, 0, &[Target::Object(island)]);
    settled(&mut state);
    assert_eq!(state.objects.get(mine).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(island).zone, Zone::Graveyard);
}

#[test]
fn karakas_taps_for_white_and_returns_a_legendary_creature() {
    let mut state = islands();
    let karakas = put(&mut state, P0, "Karakas", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(karakas));
    assert_eq!(mana(&state, P0, ManaColor::W), 1);

    let mut state = islands();
    let karakas = put(&mut state, P0, "Karakas", Zone::Battlefield);
    let thalia = put(
        &mut state,
        P1,
        "Thalia, Guardian of Thraben",
        Zone::Battlefield,
    );
    let bears = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    assert!(activatable(&mut state, karakas, 0));
    act(&mut state, Action::ActivateAbility(karakas, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(thalia)));
            assert!(!legal_targets.contains(&Target::Object(bears)));
            act(&mut state, Action::ChooseTarget(Target::Object(thalia)));
        }
        other => panic!("expected the target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(thalia).zone, Zone::Hand);
    assert!(state.players[1].hand.contains(&thalia));
    assert!(state.objects.get(karakas).tapped);
}

// ---- Power ----------------------------------------------------------------

#[test]
fn ancestral_recall_has_target_player_draw_three() {
    let mut state = islands();
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, recall, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].hand.len(), 3);
    assert_eq!(state.players[0].hand.len(), 0);
    assert_eq!(state.objects.get(recall).zone, Zone::Graveyard);
}

#[test]
fn time_walk_takes_an_extra_turn() {
    let mut state = islands();
    let walk = put(&mut state, P0, "Time Walk", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, walk, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(walk).zone, Zone::Graveyard);
    assert!(state.extra_turn_v1.is_some());
    // P0's extra turn: its draw step draws the first card of P0's hand.
    pass_until(&mut state, |state| {
        state.step == Step::Main1 && state.players[0].hand.len() == 1
    });
    assert_eq!(state.active_player, P0, "the extra turn is P0's");
    assert_eq!(state.turn, 1, "an extra turn keeps the round");
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert_eq!(state.players[1].hand.len(), 1, "then P1's regular turn");
}

// ---- Batch two: helpers -----------------------------------------------------

/// Settles, answering every library or effect selection by finishing with
/// whatever is already selected, every option with the first, every Boolean
/// with `false`, and every target prompt with the first legal one.
fn settle_declining_selections(state: &mut GameState) {
    while let Some(decision) = settle(state) {
        match decision {
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => act(state, Action::FinishEffectSelection),
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                act(state, Action::ChooseEffectTarget(legal_targets[0]))
            }
            Decision::ChooseEffectOption { .. } => act(state, Action::ChooseEffectOption(0)),
            Decision::ChooseEffectBoolean { .. } => act(state, Action::ChooseEffectBoolean(false)),
            Decision::ChooseTargets { legal_targets, .. } if !legal_targets.is_empty() => {
                act(state, Action::ChooseTarget(legal_targets[0]))
            }
            other => panic!("unexpected choice: {other:?}"),
        }
    }
}

/// P1 becomes the active player in its first main phase, holding priority.
fn opponents_main_phase(state: &mut GameState) {
    state.active_player = P1;
    state.priority_player = P1;
    state.step = Step::Main1;
}

fn plays(state: &GameState, player: PlayerId, name: &str) -> bool {
    state.players[player.index()]
        .battlefield
        .iter()
        .any(|&id| CARD_DEFS[state.objects.get(id).card_def as usize].name == name)
}

/// Passes priority and declares nothing until `answer` handles a decision
/// (returning true), panicking on any other decision.
fn pass_until_answered(
    state: &mut GameState,
    mut answer: impl FnMut(&mut GameState, &Decision) -> bool,
) {
    for _ in 0..400 {
        let decision = next(state);
        if answer(state, &decision) {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(Vec::new())),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(Vec::new())),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    panic!("never answered");
}

// ---- Batch two: spells ------------------------------------------------------

#[test]
fn mental_misstep_counters_a_mana_value_one_spell_for_two_life() {
    let mut state = islands();
    let misstep = put(&mut state, P0, "Mental Misstep", Zone::Hand);
    state.priority_player = P1;
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    // No blue mana: the Phyrexian symbol costs 2 life.
    cast(&mut state, misstep, &[Target::Object(bolt)]);
    settled(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 18);

    // A mana value 2 spell is not a legal target.
    let mut state = islands();
    let misstep = put(&mut state, P0, "Mental Misstep", Zone::Hand);
    opponents_main_phase(&mut state);
    let walk = put(&mut state, P1, "Time Walk", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 1);
    cast(&mut state, walk, &[]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(!castable(&mut state, misstep));
}

#[test]
fn tinker_sacrifices_an_artifact_and_puts_one_from_the_library_onto_the_battlefield() {
    let mut state = game(&["Island", "Sol Ring", "Island"]);
    let sol_ring = state.players[0].library[1];
    let mox = put(&mut state, P0, "Mox Pearl", Zone::Battlefield);
    let tinker = put(&mut state, P0, "Tinker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, tinker, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(sol_ring)]);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(sol_ring)),
            );
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(mox).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(sol_ring).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(tinker).zone, Zone::Graveyard);

    // Without an artifact to sacrifice, Tinker can't be cast.
    let mut state = islands();
    let tinker = put(&mut state, P0, "Tinker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    assert!(!castable(&mut state, tinker));
}

#[test]
fn paradoxical_outcome_returns_nontoken_nonland_permanents_and_draws_for_each() {
    let mut state = islands();
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let mox = put(&mut state, P0, "Mox Jet", Zone::Battlefield);
    let land = put(&mut state, P0, "Island", Zone::Battlefield);
    let outcome = put(&mut state, P0, "Paradoxical Outcome", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    assert!(castable(&mut state, outcome));
    act(&mut state, Action::CastSpell(outcome));
    match next(&mut state) {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(can_finish, "any number includes none");
            assert!(legal_targets.contains(&Target::Object(ring)));
            assert!(legal_targets.contains(&Target::Object(mox)));
            assert!(!legal_targets.contains(&Target::Object(land)));
            act(&mut state, Action::ChooseTarget(Target::Object(ring)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(ring)));
            act(&mut state, Action::ChooseTarget(Target::Object(mox)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    if let Decision::ChooseTargets { .. } = next(&mut state) {
        act(&mut state, Action::FinishEffectSelection);
    }
    settled(&mut state);
    assert_eq!(state.objects.get(ring).zone, Zone::Hand);
    assert_eq!(state.objects.get(mox).zone, Zone::Hand);
    assert_eq!(state.objects.get(land).zone, Zone::Battlefield);
    // Two returned cards and two draws.
    assert_eq!(state.players[0].hand.len(), 4);
}

// ---- Batch two: artifacts ---------------------------------------------------

#[test]
fn coveted_jewel_draws_three_taps_for_three_and_changes_hands_when_unblocked() {
    let mut state = islands();
    let jewel = put(&mut state, P0, "Coveted Jewel", Zone::Hand);
    pool(&mut state, P0, &[], 6);
    cast(&mut state, jewel, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(jewel).zone, Zone::Battlefield);
    assert_eq!(state.players[0].hand.len(), 3);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(jewel, ManaColor::G),
    );
    assert_eq!(mana(&state, P0, ManaColor::G), 3);

    // P0 attacks P1, who controls a tapped Jewel, and isn't blocked.
    let mut state = islands();
    let jewel = put(&mut state, P1, "Coveted Jewel", Zone::Battlefield);
    state.objects.get_mut(jewel).tapped = true;
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    for _ in 0..60 {
        match next(&mut state) {
            Decision::DeclareAttackers { .. } => {
                act(&mut state, Action::DeclareAttackers(vec![elves]))
            }
            Decision::DeclareBlockers { .. } => act(&mut state, Action::DeclareBlockers(vec![])),
            Decision::CastSpellOrPass { .. } if state.step == Step::Main2 => break,
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(state.objects.get(jewel).controller, P0);
    assert!(state.players[0].battlefield.contains(&jewel));
    assert!(!state.objects.get(jewel).tapped, "it untaps");
    assert_eq!(state.players[0].hand.len(), 3, "the attacker draws three");
    assert_eq!(state.players[1].hand.len(), 0);
}

#[test]
fn mishras_workshop_mana_casts_only_artifact_spells() {
    let mut state = islands();
    let workshop = put(&mut state, P0, "Mishra's Workshop", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    let outcome = put(&mut state, P0, "Paradoxical Outcome", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(workshop));
    assert!(state.objects.get(workshop).tapped);
    assert_eq!(state.players[0].restricted_mana_pool.0.len(), 3);
    assert_eq!(mana(&state, P0, ManaColor::C), 0);
    assert!(
        !castable(&mut state, outcome),
        "Workshop mana can't pay for a nonartifact spell"
    );
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(ring).zone, Zone::Battlefield);
}

#[test]
fn manifold_key_untaps_another_artifact_or_makes_a_creature_unblockable() {
    let mut state = islands();
    let key = put(&mut state, P0, "Manifold Key", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    state.objects.get_mut(ring).tapped = true;
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, key, 0));
    act(&mut state, Action::ActivateAbility(key, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(key)), "another");
            act(&mut state, Action::ChooseTarget(Target::Object(ring)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    settled(&mut state);
    assert!(!state.objects.get(ring).tapped);
    assert!(state.objects.get(key).tapped);

    let mut state = islands();
    let key = put(&mut state, P0, "Manifold Key", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    activate(&mut state, key, 1, &[Target::Object(elves)]);
    settled(&mut state);
    assert!(engine::has_effective_keyword(
        &state,
        elves,
        Keywords::CANT_BE_BLOCKED
    ));
}

#[test]
fn vexing_bauble_counters_spells_cast_without_mana_and_cycles() {
    let mut state = islands();
    put(&mut state, P0, "Vexing Bauble", Zone::Battlefield);
    opponents_main_phase(&mut state);
    let mox = put(&mut state, P1, "Mox Ruby", Zone::Hand);
    cast(&mut state, mox, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(mox).zone, Zone::Graveyard, "countered");
    let ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    pool(&mut state, P1, &[], 1);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(ring).zone,
        Zone::Battlefield,
        "mana spent"
    );

    let mut state = islands();
    let bauble = put(&mut state, P0, "Vexing Bauble", Zone::Battlefield);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, bauble, 0, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(bauble).zone, Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn void_mirror_counters_spells_cast_without_colored_mana() {
    let mut state = islands();
    put(&mut state, P0, "Void Mirror", Zone::Battlefield);
    opponents_main_phase(&mut state);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    pool(&mut state, P1, &[], 1);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(ring).zone, Zone::Graveyard, "colorless");
    let recall = put(&mut state, P1, "Ancestral Recall", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 0);
    cast(&mut state, recall, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].hand.len(), 3, "blue mana was spent");
    // Generic paid with colored mana counts as colored mana spent.
    let ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U], 0);
    cast(&mut state, ring, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(ring).zone, Zone::Battlefield);
}

#[test]
fn trinisphere_makes_cheap_spells_cost_three_while_untapped() {
    let mut state = islands();
    let sphere = put(&mut state, P0, "Trinisphere", Zone::Battlefield);
    let mox = put(&mut state, P0, "Mox Emerald", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    assert!(!castable(&mut state, mox));
    pool(&mut state, P0, &[], 1);
    cast(&mut state, mox, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(mox).zone, Zone::Battlefield);
    assert_eq!(state.players[0].mana_pool, [0; 6]);

    // A colored cost keeps its color and adds generic up to three.
    state.objects.get_mut(mox).tapped = true;
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(!castable(&mut state, recall));
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, recall));

    // Tapped, it does nothing.
    state.objects.get_mut(sphere).tapped = true;
    let ruby = put(&mut state, P0, "Mox Ruby", Zone::Hand);
    state.players[0].mana_pool = [0; 6];
    assert!(castable(&mut state, ruby));
}

#[test]
fn mana_vault_stays_tapped_and_pays_four_to_untap_or_burns_its_controller() {
    for pay in [true, false] {
        let mut state = islands();
        let vault = put(&mut state, P0, "Mana Vault", Zone::Battlefield);
        for _ in 0..4 {
            put(&mut state, P0, "Island", Zone::Battlefield);
        }
        next(&mut state);
        act(&mut state, Action::ActivateManaAbility(vault));
        assert_eq!(mana(&state, P0, ManaColor::C), 3);
        assert!(state.objects.get(vault).tapped);
        // Through P1's turn to P0's next upkeep payment choice.
        pass_until_answered(&mut state, |state, decision| {
            let Decision::ChooseEffectBoolean { player, .. } = decision else {
                return false;
            };
            assert_eq!(*player, P0);
            assert!(state.objects.get(vault).tapped, "it did not untap");
            act(state, Action::ChooseEffectBoolean(pay));
            true
        });
        pass_until(&mut state, |state| {
            state.active_player == P0 && state.step == Step::Main1
        });
        assert_eq!(state.objects.get(vault).tapped, !pay, "pay {pay}");
        let life = if pay { 20 } else { 19 };
        assert_eq!(state.players[0].life, life, "pay {pay}");
    }
}

#[test]
fn time_vault_enters_tapped_skips_a_turn_to_untap_and_takes_an_extra_turn() {
    let mut state = islands();
    let vault = put(&mut state, P0, "Time Vault", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, vault, &[]);
    settled(&mut state);
    assert!(state.objects.get(vault).tapped, "it enters tapped");
    // P1's end step: P0 may skip their next turn.
    pass_until_answered(&mut state, |state, decision| {
        let Decision::ChooseEffectOption {
            player,
            option_count,
            ..
        } = decision
        else {
            return false;
        };
        assert_eq!(*player, P0);
        assert_eq!(state.active_player, P1);
        assert_eq!(*option_count, 2);
        act(state, Action::ChooseEffectOption(0));
        true
    });
    assert!(state.objects.get(vault).tapped, "untapped only by the skip");
    pass_until(&mut state, |state| state.step == Step::Main1);
    assert_eq!(state.active_player, P1, "P0's turn was skipped");
    assert!(!state.objects.get(vault).tapped, "the skip untapped it");
    // At instant speed during P1's turn: P0 takes an extra turn after it.
    state.priority_player = P0;
    activate(&mut state, vault, 0, &[]);
    settled(&mut state);
    assert!(state.extra_turn_v1.is_some());
}

// ---- Batch two: creatures and static abilities ------------------------------

#[test]
fn archon_of_emeria_limits_spells_and_taps_opponents_nonbasic_lands() {
    let mut state = islands();
    put(&mut state, P0, "Archon of Emeria", Zone::Battlefield);
    let first = put(&mut state, P0, "Mox Jet", Zone::Hand);
    let second = put(&mut state, P0, "Mox Ruby", Zone::Hand);
    cast(&mut state, first, &[]);
    settled(&mut state);
    assert!(!castable(&mut state, second), "one spell each turn");

    let mut state = islands();
    put(&mut state, P0, "Archon of Emeria", Zone::Battlefield);
    let own = put(&mut state, P0, "Strip Mine", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(own));
    assert!(!state.objects.get(own).tapped, "only opponents' lands");
    opponents_main_phase(&mut state);
    let mine = put(&mut state, P1, "Strip Mine", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(mine));
    assert!(state.objects.get(mine).tapped, "nonbasic enters tapped");

    let mut state = islands();
    put(&mut state, P0, "Archon of Emeria", Zone::Battlefield);
    opponents_main_phase(&mut state);
    let island = put(&mut state, P1, "Snow-Covered Island", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(island));
    assert!(
        !state.objects.get(island).tapped,
        "basic lands are unaffected"
    );
}

#[test]
fn clarion_conqueror_stops_artifact_creature_and_planeswalker_abilities() {
    let mut state = islands();
    put(&mut state, P1, "Clarion Conqueror", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let key = put(&mut state, P0, "Manifold Key", Zone::Battlefield);
    let mine = put(&mut state, P0, "Strip Mine", Zone::Battlefield);
    let land = put(&mut state, P1, "Island", Zone::Battlefield);
    pool(&mut state, P0, &[], 3);
    next(&mut state);
    assert!(engine::step(&mut state, Action::ActivateManaAbility(ring)).is_err());
    assert!(!activatable(&mut state, key, 0));
    assert!(!activatable(&mut state, key, 1));
    activate(&mut state, mine, 0, &[Target::Object(land)]);
    settled(&mut state);
    assert_eq!(
        state.objects.get(land).zone,
        Zone::Graveyard,
        "lands still work"
    );
}

#[test]
fn white_plume_adventurer_untaps_a_creature_each_opponent_upkeep() {
    let mut state = islands();
    let plume = put(&mut state, P0, "White Plume Adventurer", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(plume).tapped = true;
    state.objects.get_mut(elves).tapped = true;
    pass_until_answered(&mut state, |state, decision| {
        let Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } = decision
        else {
            return false;
        };
        assert_eq!(*player, P0);
        assert_eq!(state.active_player, P1);
        assert!(legal_targets.contains(&Target::Object(elves)));
        act(state, Action::ChooseEffectTarget(Target::Object(elves)));
        true
    });
    settled(&mut state);
    assert!(!state.objects.get(elves).tapped);
    assert!(state.objects.get(plume).tapped);

    // Having completed a dungeon, all of them untap.
    let mut state = islands();
    let plume = put(&mut state, P0, "White Plume Adventurer", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(plume).tapped = true;
    state.objects.get_mut(elves).tapped = true;
    state.players[0].dungeon.completed_dungeons = vec![1];
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert!(!state.objects.get(elves).tapped);
    assert!(!state.objects.get(plume).tapped);
}

#[test]
fn white_plume_adventurer_and_seasoned_dungeoneer_take_the_initiative() {
    for name in ["White Plume Adventurer", "Seasoned Dungeoneer"] {
        let mut state = islands();
        let creature = put(&mut state, P0, name, Zone::Hand);
        pool(&mut state, P0, &[ManaColor::W], 3);
        cast(&mut state, creature, &[]);
        settle_declining_selections(&mut state);
        assert_eq!(state.initiative, Some(P0), "{name}");
        assert!(plays(&state, P0, name), "{name}");
    }
}

#[test]
fn seasoned_dungeoneer_protects_an_attacking_warrior_from_creatures_and_explores() {
    let mut state = islands();
    let top = state.players[0].library[0];
    let dungeoneer = put(&mut state, P0, "Seasoned Dungeoneer", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let mut blockable = None;
    for _ in 0..60 {
        match next(&mut state) {
            Decision::DeclareAttackers { .. } => {
                act(&mut state, Action::DeclareAttackers(vec![dungeoneer]))
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                assert_eq!(legal_targets, vec![Target::Object(dungeoneer)]);
                act(&mut state, Action::ChooseTarget(legal_targets[0]));
            }
            Decision::DeclareBlockers { legal_blockers, .. } => {
                blockable = Some(
                    legal_blockers
                        .iter()
                        .any(|(_, blockers)| blockers.contains(&blocker)),
                );
                act(&mut state, Action::DeclareBlockers(vec![]));
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::ChooseEffectBoolean { .. } => {
                act(&mut state, Action::ChooseEffectBoolean(false))
            }
            Decision::ChooseEffectOption { .. } => act(&mut state, Action::ChooseEffectOption(0)),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(blockable, Some(false), "protection from creatures");
    assert_eq!(state.objects.get(top).zone, Zone::Hand, "the land explored");
}

// ---- Batch three: planeswalkers -------------------------------------------

fn loyalty(state: &GameState, planeswalker: ObjectId) -> Option<u32> {
    mtg_kernel::planeswalker_v1::loyalty(state, planeswalker)
}

#[test]
fn karn_shuts_off_opponents_artifacts_and_animates_or_wishes() {
    let mut state = islands();
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    let own_ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let their_ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let their_island = put(&mut state, P1, "Island", Zone::Battlefield);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, karn), Some(5));
    // Static: P1's artifacts' abilities can't be activated; P0's can.
    opponents_main_phase(&mut state);
    next(&mut state);
    assert!(engine::step(&mut state, Action::ActivateManaAbility(their_ring)).is_err());
    act(&mut state, Action::ActivateManaAbility(their_island));
    state.active_player = P0;
    state.priority_player = P0;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(own_ring));
    assert_eq!(mana(&state, P0, ManaColor::C), 2);

    // +1: a noncreature artifact becomes a creature with power and
    // toughness equal to its mana value, until Karn's controller's next turn.
    let mut state = islands();
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    activate(&mut state, karn, 0, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(loyalty(&state, karn), Some(6));
    assert!(engine::object_has_type(&state, ring, CardType::Creature));
    assert!(engine::object_has_type(&state, ring, CardType::Artifact));
    assert_eq!(
        (
            engine::effective_power(&state, ring),
            engine::effective_toughness(&state, ring)
        ),
        (1, 1)
    );
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1
    });
    assert!(engine::object_has_type(&state, ring, CardType::Creature));
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    assert!(!engine::object_has_type(&state, ring, CardType::Creature));

    // -2: a face-up artifact card P0 owns in exile goes to P0's hand.
    let mut state = islands();
    let karn = put(&mut state, P0, "Karn, the Great Creator", Zone::Hand);
    let exiled = put(&mut state, P0, "Mana Vault", Zone::Exile);
    let theirs = put(&mut state, P1, "Sol Ring", Zone::Exile);
    pool(&mut state, P0, &[], 4);
    cast(&mut state, karn, &[]);
    settled(&mut state);
    activate(&mut state, karn, 1, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(exiled)]);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(exiled)),
            );
        }
        other => panic!("expected the exile choice, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(exiled).zone, Zone::Hand);
    assert_eq!(state.objects.get(theirs).zone, Zone::Exile);
    assert_eq!(loyalty(&state, karn), Some(3));
}

#[test]
fn tezzeret_grows_with_artifacts_untaps_tutors_and_makes_an_emblem() {
    let mut state = islands();
    let tezzeret = put(&mut state, P0, "Tezzeret, Cruel Captain", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    cast(&mut state, tezzeret, &[]);
    settled(&mut state);
    assert_eq!(loyalty(&state, tezzeret), Some(4));
    let moxen =
        ["Mox Pearl", "Mox Jet", "Mox Ruby"].map(|name| put(&mut state, P0, name, Zone::Hand));
    for mox in moxen {
        cast(&mut state, mox, &[]);
        settled(&mut state);
    }
    assert_eq!(loyalty(&state, tezzeret), Some(7), "one per artifact");

    // 0: untap an artifact creature and grow it.
    let strix = put(&mut state, P0, "Baleful Strix", Zone::Battlefield);
    state.objects.get_mut(strix).tapped = true;
    activate(&mut state, tezzeret, 0, &[Target::Object(strix)]);
    settled(&mut state);
    assert!(!state.objects.get(strix).tapped);
    assert_eq!(state.objects.get(strix).counters.plus1_plus1, 1);
    assert_eq!(loyalty(&state, tezzeret), Some(7));

    // -3 next turn: an artifact with mana value 1 or less, revealed.
    let mut state = game(&["Island", "Coveted Jewel", "Sol Ring", "Island"]);
    let library = state.players[0].library.clone();
    let tezzeret = put(&mut state, P0, "Tezzeret, Cruel Captain", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    cast(&mut state, tezzeret, &[]);
    settled(&mut state);
    activate(&mut state, tezzeret, 1, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(library[2])]);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(library[2])),
            );
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(library[2]).zone, Zone::Hand);
    assert_eq!(loyalty(&state, tezzeret), Some(1));

    // -7: the emblem puts three counters on an artifact at combat.
    let mut state = islands();
    let tezzeret = put(&mut state, P0, "Tezzeret, Cruel Captain", Zone::Hand);
    pool(&mut state, P0, &[], 3);
    cast(&mut state, tezzeret, &[]);
    settled(&mut state);
    let moxen =
        ["Mox Pearl", "Mox Jet", "Mox Ruby"].map(|name| put(&mut state, P0, name, Zone::Hand));
    for mox in moxen {
        cast(&mut state, mox, &[]);
        settled(&mut state);
    }
    activate(&mut state, tezzeret, 2, &[]);
    settled(&mut state);
    let target = moxen[0];
    pass_until_answered(&mut state, |state, decision| {
        let Decision::ChooseTargets { legal_targets, .. } = decision else {
            return false;
        };
        assert_eq!(state.step, Step::BeginCombat);
        assert!(legal_targets.contains(&Target::Object(target)));
        act(state, Action::ChooseTarget(Target::Object(target)));
        true
    });
    settled(&mut state);
    assert_eq!(state.objects.get(target).counters.plus1_plus1, 3);
    assert!(engine::object_has_type(&state, target, CardType::Creature));
    assert!(engine::has_effective_subtype(
        &state,
        target,
        Subtype::Robot
    ));
    assert_eq!(
        (
            engine::effective_power(&state, target),
            engine::effective_toughness(&state, target)
        ),
        (3, 3)
    );
}

// ---- Batch three: Urza's Saga, Metamorph, Witch Enchanter -----------------

#[test]
fn urzas_saga_gains_its_abilities_by_chapter_and_fetches_a_cheap_artifact() {
    // P0 draws the first two cards on turns two and three.
    let mut state = game(&[
        "Island",
        "Island",
        "Island",
        "Coveted Jewel",
        "Sol Ring",
        "Mox Jet",
        "Island",
        "Island",
    ]);
    let library = state.players[0].library.clone();
    let saga = put(&mut state, P0, "Urza's Saga", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(saga));
    settled(&mut state);
    assert_eq!(state.objects.get(saga).counters.lore, 1);
    act(&mut state, Action::ActivateManaAbility(saga));
    assert_eq!(mana(&state, P0, ManaColor::C), 1, "chapter I");
    assert!(!activatable(&mut state, saga, 0), "chapter II not yet");

    // Chapter II on P0's next turn.
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1 && state.turn == 2
    });
    settled(&mut state);
    assert_eq!(state.objects.get(saga).counters.lore, 2);
    pool(&mut state, P0, &[], 2);
    activate(&mut state, saga, 0, &[]);
    settled(&mut state);
    let constructs = state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| {
            CARD_DEFS[state.objects.get(id).card_def as usize].name == "Karn Construct Token"
        })
        .collect::<Vec<_>>();
    assert_eq!(constructs.len(), 1);
    assert_eq!(engine::effective_power(&state, constructs[0]), 1);

    // Chapter III: an artifact with mana cost {0} or {1}, then the Saga is
    // sacrificed.
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1 && state.turn == 3
    });
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert!(
                legal_targets.contains(&Target::Object(library[4])),
                "Sol Ring"
            );
            assert!(
                legal_targets.contains(&Target::Object(library[5])),
                "Mox Jet"
            );
            assert!(
                !legal_targets.contains(&Target::Object(library[3])),
                "Jewel"
            );
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(library[4])),
            );
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(library[4]).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(saga).zone, Zone::Graveyard);
}

#[test]
fn phyrexian_metamorph_enters_as_an_artifact_copy_and_leaves_as_itself() {
    let mut state = islands();
    let jewel = put(&mut state, P1, "Coveted Jewel", Zone::Battlefield);
    let metamorph = put(&mut state, P0, "Phyrexian Metamorph", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, metamorph, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert!(legal_targets.contains(&Target::Object(jewel)));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(jewel)),
            );
        }
        other => panic!("expected the copy choice, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(metamorph).zone, Zone::Battlefield);
    assert_eq!(engine::effective_name(&state, metamorph), "Coveted Jewel");
    assert!(engine::object_has_type(
        &state,
        metamorph,
        CardType::Artifact
    ));
    assert_eq!(state.players[0].hand.len(), 3, "the copy's ETB draws three");
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(metamorph, ManaColor::U),
    );
    assert_eq!(mana(&state, P0, ManaColor::U), 3);
    // Leaving the battlefield, it is Phyrexian Metamorph again.
    event_move(&mut state, metamorph, Zone::Graveyard);
    assert_eq!(state.objects.get(metamorph).zone, Zone::Graveyard);
    assert_eq!(
        CARD_DEFS[state.objects.get(metamorph).card_def as usize].name,
        "Phyrexian Metamorph"
    );

    // A creature copy is also an artifact.
    let mut state = islands();
    let archon = put(&mut state, P1, "Archon of Emeria", Zone::Battlefield);
    let metamorph = put(&mut state, P0, "Phyrexian Metamorph", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, metamorph, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { .. }) => {
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(archon)),
            );
        }
        other => panic!("expected the copy choice, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert!(engine::object_has_type(
        &state,
        metamorph,
        CardType::Creature
    ));
    assert!(engine::object_has_type(
        &state,
        metamorph,
        CardType::Artifact
    ));
    assert_eq!(
        (
            engine::effective_power(&state, metamorph),
            engine::effective_toughness(&state, metamorph)
        ),
        (2, 3)
    );

    // Copying nothing, it is a 0/0 and dies.
    let mut state = islands();
    let metamorph = put(&mut state, P0, "Phyrexian Metamorph", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 3);
    cast(&mut state, metamorph, &[]);
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(metamorph).zone, Zone::Graveyard);
}

fn event_move(state: &mut GameState, object: ObjectId, zone: Zone) {
    mtg_kernel::event::propose_and_commit(
        state,
        mtg_kernel::event::ProposedEvent::zone_change(object, zone),
    );
}

#[test]
fn witch_enchanter_destroys_an_artifact_or_plays_as_its_land_face() {
    let mut state = islands();
    let witch = put(&mut state, P0, "Witch Enchanter", Zone::Hand);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 3);
    cast(&mut state, witch, &[]);
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(ring).zone, Zone::Graveyard);
    assert!(engine::object_has_type(&state, witch, CardType::Creature));

    for pay in [true, false] {
        let mut state = islands();
        let witch = put(&mut state, P0, "Witch Enchanter", Zone::Hand);
        next(&mut state);
        act(&mut state, Action::PlayLand(witch));
        match next(&mut state) {
            Decision::ChooseEffectOption {
                player,
                option_count,
                ..
            } => {
                assert_eq!(player, P0);
                assert_eq!(option_count, 2);
                act(&mut state, Action::ChooseEffectOption(u16::from(!pay)));
            }
            other => panic!("expected the life payment choice, got {other:?}"),
        }
        assert_eq!(state.objects.get(witch).zone, Zone::Battlefield);
        assert!(engine::object_has_type(&state, witch, CardType::Land));
        assert!(!engine::object_has_type(&state, witch, CardType::Creature));
        assert_eq!(state.objects.get(witch).tapped, !pay, "pay {pay}");
        assert_eq!(state.players[0].life, if pay { 17 } else { 20 });
        if pay {
            act(&mut state, Action::ActivateManaAbility(witch));
            assert_eq!(mana(&state, P0, ManaColor::W), 1);
        }
    }
}

// ---- Batch three: Peacekeeper and Chancellor ------------------------------

/// The (letter group, name) option indices of `name` in Anointed
/// Peacekeeper's two-step name choice.
fn name_choice(name: &str) -> (u16, u16) {
    let mut seen: Vec<&str> = Vec::new();
    let mut groups: Vec<(char, Vec<&str>)> = Vec::new();
    for def in CARD_DEFS.iter() {
        if def.is_token || !def.is_executable() || seen.contains(&def.object_name) {
            continue;
        }
        seen.push(def.object_name);
        let letter = def
            .object_name
            .chars()
            .next()
            .map_or(' ', |letter| letter.to_ascii_uppercase());
        match groups.iter_mut().find(|(group, _)| *group == letter) {
            Some((_, names)) => names.push(def.object_name),
            None => groups.push((letter, vec![def.object_name])),
        }
    }
    groups.sort_by_key(|(letter, _)| *letter);
    for (group, (_, names)) in groups.iter().enumerate() {
        if let Some(index) = names.iter().position(|candidate| *candidate == name) {
            return (group as u16, index as u16);
        }
    }
    panic!("{name} is not nameable");
}

/// Casts Anointed Peacekeeper for P0 and names `name`.
fn peacekeeper_naming(state: &mut GameState, name: &str) -> ObjectId {
    let keeper = put(state, P0, "Anointed Peacekeeper", Zone::Hand);
    pool(state, P0, &[ManaColor::W], 2);
    cast(state, keeper, &[]);
    let (group, index) = name_choice(name);
    match settle(state) {
        Some(Decision::ChooseEffectOption { player, .. }) => {
            assert_eq!(player, P0);
            act(state, Action::ChooseEffectOption(group));
        }
        other => panic!("expected the letter, got {other:?}"),
    }
    if let Some(Decision::ChooseEffectOption { .. }) = settle(state) {
        act(state, Action::ChooseEffectOption(index));
    }
    settled(state);
    assert_eq!(state.objects.get(keeper).zone, Zone::Battlefield);
    keeper
}

#[test]
fn anointed_peacekeeper_sees_the_hand_and_taxes_the_named_card() {
    let mut state = islands();
    let secret = put(&mut state, P1, "Sol Ring", Zone::Hand);
    peacekeeper_naming(&mut state, "Sol Ring");
    assert!(state
        .known_hand_cards(P0, P1)
        .iter()
        .any(|entry| entry.object == secret));
    // P1's Sol Ring now costs {3}.
    opponents_main_phase(&mut state);
    pool(&mut state, P1, &[], 2);
    assert!(!castable(&mut state, secret));
    pool(&mut state, P1, &[], 1);
    assert!(castable(&mut state, secret));

    // Naming Strip Mine taxes its activated ability, not its mana ability.
    let mut state = islands();
    peacekeeper_naming(&mut state, "Strip Mine");
    opponents_main_phase(&mut state);
    let mine = put(&mut state, P1, "Strip Mine", Zone::Battlefield);
    put(&mut state, P1, "Island", Zone::Battlefield);
    assert!(!activatable(&mut state, mine, 0));
    pool(&mut state, P1, &[], 2);
    assert!(activatable(&mut state, mine, 0));
    act(&mut state, Action::ActivateManaAbility(mine));
    assert_eq!(mana(&state, P1, ManaColor::C), 3);
}

/// A game at its very beginning: turn 1's untap step, P0 active.
fn fresh_game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library
        .iter()
        .map(|name| card_id_by_name(name).unwrap_or_else(|| panic!("{name}")))
        .collect();
    GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4348_414e,
    )
}

#[test]
fn chancellor_of_the_annex_taxes_the_first_spell_from_the_opening_hand() {
    for pay in [true, false] {
        let mut state = fresh_game(&["Island"; 20]);
        let chancellor = put(&mut state, P1, "Chancellor of the Annex", Zone::Hand);
        pass_until_answered(&mut state, |state, decision| {
            let Decision::ChooseEffectOption { player, .. } = decision else {
                return false;
            };
            assert_eq!(*player, P1);
            assert_eq!(state.step, Step::Upkeep);
            act(state, Action::ChooseEffectOption(0));
            true
        });
        pass_until(&mut state, |state| {
            state.step == Step::Main1 && state.stack.is_empty()
        });
        assert!(state
            .known_hand_cards(P0, P1)
            .iter()
            .any(|entry| entry.object == chancellor));
        let mox = put(&mut state, P0, "Mox Pearl", Zone::Hand);
        pool(&mut state, P0, &[], 1);
        cast(&mut state, mox, &[]);
        match settle(&mut state) {
            Some(Decision::ChooseEffectOption { player, .. }) => {
                assert_eq!(player, P0);
                act(&mut state, Action::ChooseEffectOption(u16::from(!pay)));
            }
            other => panic!("expected the payment choice, got {other:?}"),
        }
        settled(&mut state);
        let zone = if pay {
            Zone::Battlefield
        } else {
            Zone::Graveyard
        };
        assert_eq!(state.objects.get(mox).zone, zone, "pay {pay}");
        // The second spell is not taxed.
        let jet = put(&mut state, P0, "Mox Jet", Zone::Hand);
        cast(&mut state, jet, &[]);
        settled(&mut state);
        assert_eq!(state.objects.get(jet).zone, Zone::Battlefield);
    }
}

#[test]
fn chancellor_of_the_annex_taxes_every_opponent_spell_while_in_play() {
    let mut state = islands();
    put(&mut state, P0, "Chancellor of the Annex", Zone::Battlefield);
    opponents_main_phase(&mut state);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Hand);
    pool(&mut state, P1, &[], 2);
    cast(&mut state, ring, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { player, .. }) => {
            assert_eq!(player, P1);
            act(&mut state, Action::ChooseEffectOption(0));
        }
        other => panic!("expected the payment choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(ring).zone, Zone::Battlefield);
    assert_eq!(state.players[1].mana_pool, [0; 6]);
    // Unable to pay, the spell is countered.
    let mox = put(&mut state, P1, "Mox Jet", Zone::Hand);
    cast(&mut state, mox, &[]);
    if let Some(Decision::ChooseEffectOption { .. }) = settle(&mut state) {
        act(&mut state, Action::ChooseEffectOption(0));
    }
    settled(&mut state);
    assert_eq!(state.objects.get(mox).zone, Zone::Graveyard);
}

// ---- Public session games -------------------------------------------------

/// The two E4 decks from the snapshot, with the six cards wave E3 supplies
/// replaced by basic lands until E3 lands: they play every E4 card through
/// public sessions before the decks join `PLAYABLE`.
fn proxy_decks() -> [CustomDeckV1; 2] {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("data/eternal_v1/meta_snapshot_mtggoldfish_20261026.json");
    let snapshot: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let e3 = [
        "Ancient Tomb",
        "Cavern of Souls",
        "Chrome Mox",
        "Misdirection",
        "Sink into Stupor",
        "The One Ring",
    ];
    ["vintage-jewel-shops", "vintage-mono-white-initiative"].map(|archetype| {
        let deck = snapshot["formats"]["vintage"]
            .as_array()
            .unwrap()
            .iter()
            .find(|deck| deck["archetype"] == archetype)
            .unwrap();
        let basic = if archetype == "vintage-jewel-shops" {
            "Island"
        } else {
            "Plains"
        };
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for row in deck["mainboard"].as_array().unwrap() {
            let name = row[1].as_str().unwrap();
            let count = row[0].as_u64().unwrap();
            let name = if e3.contains(&name) { basic } else { name };
            *counts.entry(name.to_owned()).or_default() += count;
        }
        CustomDeckV1 {
            cards: counts
                .into_iter()
                .map(|(name, count)| CustomCardCountV1 {
                    name,
                    count: count as _,
                })
                .collect(),
        }
    })
}

fn rank(action: &Value) -> u8 {
    let s = &action["semantic"];
    match s["action_kind"].as_str().unwrap() {
        "play_land" => 0,
        "cast_spell" => 1,
        "choose_attacker_inclusion" | "choose_blocker_inclusion" if s["include"] == true => 2,
        "choose_combat_damage_range" if s["upper_half"] == false => 3,
        "pass" => 10,
        "activate_mana_ability" | "activate_ability" => 11,
        _ => 5,
    }
}

#[derive(Clone, Copy)]
enum Policy {
    Ranked,
    Random(u64),
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Plays one game to a natural terminal and returns its transcript hash
/// and the kinds of actions chosen.
fn play_session(
    decks: [CustomDeckV1; 2],
    seed: u64,
    policy: Policy,
) -> (String, BTreeMap<String, usize>) {
    const MAX_STEPS: usize = 131_072;
    let mut server = LimitedJsonlServerV1::new_with_london_mulligans_v1();
    let mut request = json!({"schema_version":4,"request_type":"reset","request_id":"reset",
        "decks":decks,"episode_id":seed,"env_seed":seed,
        "max_physical_decisions":65536,"max_policy_steps":MAX_STEPS})
    .to_string();
    let mut transcript = Sha256::new();
    let mut counts = BTreeMap::new();
    let mut recent: Vec<String> = Vec::new();
    let mut random_state = match policy {
        Policy::Random(state) => state,
        Policy::Ranked => 0,
    };
    for index in 0..MAX_STEPS {
        let raw = server.handle_line(&request);
        transcript.update(request.as_bytes());
        transcript.update(b"\n");
        transcript.update(raw.as_bytes());
        transcript.update(b"\n");
        let reply: Value = serde_json::from_str(&raw).unwrap();
        assert_ne!(
            reply["response_type"],
            "error",
            "seed={seed} step={index}: {reply}\nlast actions:\n{}",
            recent.join("\n")
        );
        if reply["response_type"] == "terminal" {
            assert_eq!(
                reply["terminal"]["terminal_code"],
                "natural_game_over",
                "seed={seed}: {reply}\nlast actions:\n{}",
                recent.join("\n")
            );
            return (format!("{:x}", transcript.finalize()), counts);
        }
        let decision = &reply["decision"];
        let actions: Vec<&Value> = decision["legal_actions"]
            .as_array()
            .unwrap()
            .iter()
            .collect();
        let action = match policy {
            Policy::Ranked => *actions.iter().min_by_key(|a| rank(a)).unwrap(),
            Policy::Random(_) => {
                actions[(splitmix64(&mut random_state) % actions.len() as u64) as usize]
            }
        };
        recent.push(action["semantic"].to_string());
        if recent.len() > 8 {
            recent.remove(0);
        }
        *counts
            .entry(
                action["semantic"]["action_kind"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            )
            .or_default() += 1;
        request =
            json!({"schema_version":4,"request_type":"step","request_id":format!("step-{index}"),
            "episode_id":decision["episode_id"],"expected_step":decision["step"],
            "selected_index":action["selected_index"],"selected_action_id":action["stable_id"]})
            .to_string();
    }
    panic!("seed {seed} did not reach a natural terminal");
}

#[test]
fn e4_proxy_decks_finish_and_replay_through_public_sessions() {
    let [shops, initiative] = proxy_decks();
    for deck in [&shops, &initiative] {
        let ids = deck.resolve().unwrap();
        mtg_kernel::card_def::preflight_fully_supported_deck(&ids).unwrap();
    }
    let mut activations = 0;
    for (index, pair) in [
        [shops.clone(), initiative.clone()],
        [initiative.clone(), shops.clone()],
    ]
    .into_iter()
    .enumerate()
    {
        for round in 0..3_u64 {
            let seed = 0x4534_5052_0000 + index as u64 * 16 + round;
            for policy in [Policy::Ranked, Policy::Random(seed)] {
                let (transcript, counts) = play_session(pair.clone(), seed, policy);
                let (replay, _) = play_session(pair.clone(), seed, policy);
                assert_eq!(transcript, replay, "seed {seed}");
                if let Policy::Random(_) = policy {
                    activations += counts.get("activate_ability").unwrap_or(&0);
                }
                println!("seed={seed} actions={counts:?}");
            }
        }
    }
    assert!(activations > 0, "random games activated no abilities");
}

/// Temporary while wave E4 is in progress: reports the catalog hash.
#[test]
fn e4_temp_reports_the_eternal_catalog_hash() {
    panic!(
        "eternal catalog hash {:#018x}",
        mtg_kernel::card_def::KERNEL_CARDDB_HASH
    );
}
