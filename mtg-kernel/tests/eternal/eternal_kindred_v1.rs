//! The Kindred card type (#716): Tarfire, Kozilek's Command and Faerie
//! Trickery are Kindred instants. Kindred counts as a card type wherever card
//! types are counted and changes nothing else: the cards are cast, countered
//! and put into a graveyard as instants, and their subtypes are creature
//! types. Policy observation v5's fixed type bits leave Kindred out.
#![cfg(feature = "eternal-fixtures")]
#![allow(dead_code)]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::{json, Value};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const KINDRED_CARDS: [(&str, Subtype); 3] = [
    ("Faerie Trickery", Subtype::Faerie),
    ("Kozilek's Command", Subtype::Eldrazi),
    ("Tarfire", Subtype::Goblin),
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
        0x4b49_4e44_0716,
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

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

fn power_toughness(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(
        !matches!(decision, Decision::Halted { .. }),
        "{decision:?}\nstack: {:#?}",
        state.stack
    );
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty; any other choice fails the test.
fn settled(state: &mut GameState) {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    panic!("did not settle");
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => castable_spells.contains(&spell),
        other => panic!("expected priority, got {other:?}"),
    }
}

/// Casts `spell`, answering each target prompt from `targets` in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    for &target in targets {
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

/// Casts the X spell `spell` with `x` and no targets.
fn cast_with_x(state: &mut GameState, spell: ObjectId, x: u16) {
    assert!(castable(state, spell));
    act(state, Action::CastSpell(spell));
    loop {
        match next(state) {
            Decision::ChooseEffectOption { option_count, .. } => {
                assert!(x < option_count, "X = {x} of {option_count} options");
                act(state, Action::ChooseEffectOption(x));
            }
            Decision::ChooseFullRangeCastXValueV59 { .. } => {
                act(state, Action::ChooseEffectOption(x));
            }
            _ => break,
        }
    }
}

/// P1 casts `name` with `mana` at `targets` during P0's turn and passes,
/// leaving it on the stack with P0 holding priority.
fn opponent_casts(
    state: &mut GameState,
    name: &str,
    colored: &[ManaColor],
    targets: &[Target],
) -> ObjectId {
    state.priority_player = P1;
    let spell = put(state, P1, name, Zone::Hand);
    pool(state, P1, colored, 0);
    cast(state, spell, targets);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(state, Action::Pass);
    spell
}

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

// ---- Characteristics ---------------------------------------------------------------

#[test]
fn the_printed_kindred_cards_are_kindred_instants_with_a_creature_subtype() {
    for (name, subtype) in KINDRED_CARDS {
        let card = def(name);
        assert_eq!(card.capability, CardCapability::Full, "{name}");
        assert_eq!(card.types, &[CardType::Kindred, CardType::Instant], "{name}");
        assert!(card.has_type(CardType::Kindred) && card.has_type(CardType::Instant));
        assert!(!card.has_type(CardType::Creature), "{name}");
        assert!(!card.is_land && !card.is_token, "{name}");
        assert_eq!(card.subtypes, &[subtype], "{name}");
        assert!(subtype.is_creature_type(), "{name}");

        let mut state = islands();
        let card = put(&mut state, P0, name, Zone::Hand);
        for card_type in CardType::ALL {
            assert_eq!(
                engine::object_has_type(&state, card, card_type),
                matches!(card_type, CardType::Kindred | CardType::Instant),
                "{name} {card_type:?}"
            );
        }
        assert!(engine::has_effective_subtype(&state, card, subtype), "{name}");
    }
}

// ---- Counting card types ----------------------------------------------------------

#[test]
fn tarfire_in_a_graveyard_adds_two_card_types_to_nethergoyf_and_barrowgoyf() {
    let mut state = islands();
    let nethergoyf = put(&mut state, P0, "Nethergoyf", Zone::Battlefield);
    let barrowgoyf = put(&mut state, P0, "Barrowgoyf", Zone::Battlefield);
    assert_eq!(power_toughness(&state, nethergoyf), (0, 1));
    assert_eq!(power_toughness(&state, barrowgoyf), (0, 1));
    put(&mut state, P0, "Tarfire", Zone::Graveyard);
    assert_eq!(power_toughness(&state, nethergoyf), (2, 3), "Kindred and Instant");
    assert_eq!(power_toughness(&state, barrowgoyf), (2, 3), "Kindred and Instant");
    // A second instant adds no type.
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    assert_eq!(power_toughness(&state, nethergoyf), (2, 3));

    // Barrowgoyf counts every graveyard, Nethergoyf only its owner's.
    let mut state = islands();
    let nethergoyf = put(&mut state, P0, "Nethergoyf", Zone::Battlefield);
    let barrowgoyf = put(&mut state, P0, "Barrowgoyf", Zone::Battlefield);
    put(&mut state, P1, "Kozilek's Command", Zone::Graveyard);
    assert_eq!(power_toughness(&state, nethergoyf), (0, 1));
    assert_eq!(power_toughness(&state, barrowgoyf), (2, 3));
}

/// Delirium needs four card types: a land, an artifact and Tarfire make four,
/// a land, an artifact and Lightning Bolt three.
#[test]
fn tarfire_counts_two_card_types_for_delirium() {
    let graveyard = |instant: &str| {
        let mut state = islands();
        for name in ["Island", "Mishra's Bauble", instant] {
            put(&mut state, P0, name, Zone::Graveyard);
        }
        state
    };
    for (instant, delirium) in [("Tarfire", true), ("Lightning Bolt", false)] {
        let mut state = graveyard(instant);
        let channeler = put(&mut state, P0, "Dragon's Rage Channeler", Zone::Battlefield);
        let expected = if delirium { (3, 3) } else { (1, 1) };
        assert_eq!(power_toughness(&state, channeler), expected, "{instant}");

        // Without the Channeler, whose cast trigger would surveil.
        let mut state = graveyard(instant);
        let heat = put(&mut state, P0, "Unholy Heat", Zone::Hand);
        let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
        pool(&mut state, P0, &[ManaColor::R], 0);
        cast(&mut state, heat, &[Target::Object(angler)]);
        settled(&mut state);
        if delirium {
            assert_eq!(zone(&state, angler), Zone::Graveyard, "six damage");
        } else {
            assert_eq!(zone(&state, angler), Zone::Battlefield, "{instant}");
            assert_eq!(state.objects.get(angler).damage, 2, "{instant}");
        }
    }
}

fn spirits(state: &GameState, player: PlayerId) -> usize {
    let spirit = id("White Flying Spirit Token");
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|&&object| state.objects.get(object).card_def == spirit)
        .count()
}

/// Occult Epiphany makes a Spirit for each card type among the discarded
/// cards: two for Tarfire, one for Lightning Bolt.
#[test]
fn occult_epiphany_discarding_tarfire_makes_two_spirits() {
    for (drawn, spirit_count) in [("Tarfire", 2), ("Lightning Bolt", 1)] {
        // X = 1 draws the card into an otherwise empty hand and discards it.
        let mut state = game(&[drawn; 20]);
        let epiphany = put(&mut state, P0, "Occult Epiphany", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::U], 1);
        cast_with_x(&mut state, epiphany, 1);
        settled(&mut state);
        assert!(state.players[P0.index()].hand.is_empty(), "{drawn}");
        assert_eq!(state.players[P0.index()].graveyard.len(), 2, "{drawn}");
        assert_eq!(spirits(&state, P0), spirit_count, "{drawn}");
    }
}

// ---- Still an instant ---------------------------------------------------------------

/// The opponent casts Tarfire during P0's turn (instant timing), Counterspell
/// counters it, and it goes to its owner's graveyard.
#[test]
fn tarfire_is_cast_at_instant_speed_countered_and_put_into_a_graveyard() {
    let mut state = islands();
    let tarfire = opponent_casts(&mut state, "Tarfire", &[ManaColor::R], &[Target::Player(P0)]);
    assert_eq!(zone(&state, tarfire), Zone::Stack);
    let counterspell = put(&mut state, P0, "Counterspell", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, counterspell, &[Target::Object(tarfire)]);
    settled(&mut state);
    assert_eq!(zone(&state, tarfire), Zone::Graveyard);
    assert!(state.players[P1.index()].graveyard.contains(&tarfire));
    assert_eq!(zone(&state, counterspell), Zone::Graveyard);
    assert_eq!(life(&state, P0), 20);

    // Uncountered, it resolves and goes to the graveyard, never the battlefield.
    let mut state = islands();
    state.step = Step::Upkeep;
    let tarfire = put(&mut state, P0, "Tarfire", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, tarfire, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 18);
    assert_eq!(zone(&state, tarfire), Zone::Graveyard);
    assert!(state.players[P0.index()].battlefield.is_empty());
}

/// Faerie Trickery counters a non-Faerie spell. Tarfire is a Goblin spell,
/// so it is countered and exiled; another Faerie Trickery is a Faerie spell
/// (its subtype is a creature type), so it is not a legal target.
#[test]
fn faerie_trickery_counters_tarfire_but_not_another_faerie_trickery() {
    let mut state = islands();
    let tarfire = opponent_casts(&mut state, "Tarfire", &[ManaColor::R], &[Target::Player(P0)]);
    let trickery = put(&mut state, P0, "Faerie Trickery", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    cast(&mut state, trickery, &[Target::Object(tarfire)]);
    settled(&mut state);
    assert_eq!(zone(&state, tarfire), Zone::Exile);
    assert_eq!(life(&state, P0), 20);

    // P0's Lightning Bolt, then P1's Faerie Trickery at it.
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P0, .. }
    ));
    act(&mut state, Action::Pass);
    let theirs = put(&mut state, P1, "Faerie Trickery", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::U, ManaColor::U], 1);
    cast(&mut state, theirs, &[Target::Object(bolt)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    let mine = put(&mut state, P0, "Faerie Trickery", Zone::Hand);
    let counterspell = put(&mut state, P0, "Counterspell", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    let trickery_targets = cast_targets(&state, mine);
    assert!(trickery_targets.contains(&Target::Object(bolt)));
    assert!(!trickery_targets.contains(&Target::Object(theirs)));
    assert!(cast_targets(&state, counterspell).contains(&Target::Object(theirs)));
}

// ---- Policy observation v5 ------------------------------------------------------------

const ONLY_INSTANT: [(&str, bool); 6] = [
    ("land", false),
    ("creature", false),
    ("instant", true),
    ("sorcery", false),
    ("artifact", false),
    ("enchantment", false),
];

/// Policy observation v5's six fixed type bits have no Kindred (or
/// Planeswalker) bit: a Kindred instant sets `instant` alone, as it did
/// before the registry had the type, and the observation layout is
/// unchanged. Public sessions with Tarfire in both decks finish and replay.
#[test]
fn policy_observation_v5_reports_a_kindred_instant_with_its_instant_bit() {
    let tarfire = u64::from(id("Tarfire"));
    let bolt = u64::from(id("Lightning Bolt"));
    let expected: Value = ONLY_INSTANT
        .iter()
        .map(|&(name, set)| (name.to_owned(), json!(set)))
        .collect::<serde_json::Map<_, _>>()
        .into();
    let burn = deck(&[
        ("Mountain", 22),
        ("Tarfire", 16),
        ("Lightning Bolt", 6),
        ("Jackal Pup", 16),
    ]);
    let mut seen = [0_usize; 2];
    for (seed, policy) in [
        (0x4b49_0716_u64, Policy::Ranked),
        (0x4b49_0717, Policy::Random(0x4b49_0817)),
    ] {
        let game = play_observed(
            [burn.clone(), burn.clone()],
            None,
            seed,
            policy,
            &mut |decision, _| {
                let graveyards = &decision["observation"]["projection"]["graveyards"];
                for card in graveyards
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_array)
                    .flatten()
                {
                    let card_id = card["stable"]["card_db_id"].as_u64();
                    let slot = match card_id {
                        Some(id) if id == tarfire => 0,
                        Some(id) if id == bolt => 1,
                        _ => continue,
                    };
                    assert_eq!(card["characteristics"]["type_flags"], expected, "{card}");
                    seen[slot] += 1;
                }
            },
        );
        let replay = play_observed(
            [burn.clone(), burn.clone()],
            None,
            seed,
            policy,
            &mut |_, _| {},
        );
        assert_eq!(game.transcript_sha256, replay.transcript_sha256);
    }
    assert!(seen[0] > 0, "no Tarfire reached a public graveyard");
    assert!(seen[1] > 0, "no Lightning Bolt reached a public graveyard");
}
