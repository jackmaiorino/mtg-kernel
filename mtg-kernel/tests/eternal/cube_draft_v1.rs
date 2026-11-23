//! The cube draft environment v1: one seed reproduces the draft record, the
//! decks and the first pod game; another seed changes the draft; pools are
//! 45 disjoint cards; pickers see only their own pack and picks; the
//! colour-count picker keeps to its colours; each step of the stated deck
//! rule; deck validation, one check at a time; caps, session errors and
//! panics are recorded; the pod pairs every two seats once; a pick outside
//! the pack is an error. Plays two games (the first pod game, twice); the
//! other pod tests stop each game at its first decision.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardDef, CARD_DEFS};
use mtg_kernel::cube_draft_v1::{
    basic_land_ids, build_deck, card_colour_bits, catch_game_panic, colour_count_pickers,
    land_colour_bits, land_fetch_bits, land_mana_bits, picker_seed, play_pod_game,
    play_pod_round_robin_with_caps, pod_pairings, pod_seed, receiving_seat, run_draft,
    uniform_draft_and_build, uniform_pickers, ColourCountPicker, CubeDraftError, CubePool, Deck,
    DraftPicker, DraftRecord, GameCaps, GameEnd, PickRecord, PickView, StatedRuleDeckBuilder,
    UniformPicker, DRAFTED_CARDS, PACK_SIZE, POD_GAMES, POOL_CARDS, ROUNDS, SEATS,
};
use mtg_kernel::mana::{ManaColor, Pip};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

const SEED: u64 = 20_261_122;

const CUBE_LIST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007.json"
);
const GAP_RECORD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"
);

fn texts() -> (String, String) {
    (
        std::fs::read_to_string(CUBE_LIST).expect("cube list"),
        std::fs::read_to_string(GAP_RECORD).expect("gap record"),
    )
}

fn pool() -> CubePool {
    let (cube, gaps) = texts();
    CubePool::from_json_texts(&cube, &gaps).expect("draftable pool")
}

/// Runs `work` on a thread with a large stack, as the engine's binaries do.
fn on_big_stack(work: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name("cube-draft-v1".to_owned())
        .stack_size(64 << 20)
        .spawn(work)
        .expect("spawn")
        .join()
        .expect("test thread");
}

fn pick_at(record: &DraftRecord, seat: usize, round: usize, pick: usize) -> &PickRecord {
    let record = &record.picks[(round * PACK_SIZE + pick) * SEATS + seat];
    assert_eq!(
        (record.seat, record.round, record.pick),
        (seat, round, pick)
    );
    record
}

#[test]
fn pool_is_the_cube_list_minus_the_gap_record() {
    let (cube, gaps) = texts();
    let cube_json: Value = serde_json::from_str(&cube).unwrap();
    let gaps_json: Value = serde_json::from_str(&gaps).unwrap();
    let gap_names: BTreeSet<&str> = gaps_json
        .as_object()
        .unwrap()
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .map(|name| name.as_str().unwrap())
        .collect();
    let expected: Vec<&str> = cube_json["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|card| card["name"].as_str().unwrap())
        .filter(|name| !gap_names.contains(name))
        .collect();
    let pool = CubePool::from_json_texts(&cube, &gaps).unwrap();
    let names: Vec<&str> = pool.names().iter().map(String::as_str).collect();
    assert_eq!(names, expected);
    assert_eq!(pool.ids().len(), pool.card_count());
    assert!(pool.card_count() >= DRAFTED_CARDS, "{}", pool.card_count());
    assert!(CubePool::from_json_texts("{", &gaps).is_err());
}

#[test]
fn same_seed_reproduces_draft_decks_and_first_game() {
    on_big_stack(|| {
        let pool = pool();
        let (record_a, decks_a) = uniform_draft_and_build(&pool, SEED).unwrap();
        let (record_b, decks_b) = uniform_draft_and_build(&pool, SEED).unwrap();
        assert_eq!(record_a, record_b);
        assert_eq!(record_a.to_json(), record_b.to_json());
        assert_eq!(decks_a, decks_b);
        let caps = GameCaps::default();
        let game_a = play_pod_game(&decks_a, pod_seed(SEED), 0, &caps).unwrap();
        let game_b = play_pod_game(&decks_b, pod_seed(SEED), 0, &caps).unwrap();
        assert_eq!(game_a, game_b);
        let mut seats = game_a.seats;
        seats.sort_unstable();
        assert_eq!(seats, [0, 1]);
        assert!(
            !matches!(game_a.end, GameEnd::SessionError { .. }),
            "the session refused the game: {game_a:?}"
        );
        assert!(play_pod_game(&decks_a, pod_seed(SEED), 28, &caps).is_err());
    });
}

#[test]
fn a_different_seed_gives_a_different_draft() {
    let pool = pool();
    let (first, _) = uniform_draft_and_build(&pool, SEED).unwrap();
    let (second, _) = uniform_draft_and_build(&pool, SEED + 1).unwrap();
    assert_ne!(first.packs, second.packs);
    assert_ne!(first, second);
}

#[test]
fn pools_are_disjoint_45_card_pools_and_decks_validate() {
    let pool = pool();
    let cube: BTreeSet<u16> = pool.ids().iter().copied().collect();
    for colour_count in [false, true] {
        let mut pickers = if colour_count {
            colour_count_pickers(SEED)
        } else {
            uniform_pickers(SEED)
        };
        let record = run_draft(&pool, SEED, &mut pickers).unwrap();
        // Packs as opened: 24 packs of 15, 360 distinct pool cards.
        assert_eq!(record.packs.len(), ROUNDS);
        for round_packs in &record.packs {
            assert_eq!(round_packs.len(), SEATS);
            assert!(round_packs.iter().all(|pack| pack.len() == PACK_SIZE));
        }
        let opened: BTreeSet<u16> = record.packs.iter().flatten().flatten().copied().collect();
        assert_eq!(opened.len(), DRAFTED_CARDS);
        assert!(opened.is_subset(&cube));
        assert_eq!(record.picks.len(), DRAFTED_CARDS);
        // Pools: 45 cards each, disjoint, exactly the opened cards.
        let mut union = BTreeSet::new();
        for (seat, seat_pool) in record.pools.iter().enumerate() {
            assert_eq!(seat_pool.len(), POOL_CARDS);
            for &card in seat_pool {
                assert!(union.insert(card), "card {card} in two pools");
            }
            let picked: Vec<u16> = record
                .picks
                .iter()
                .filter(|pick| pick.seat == seat)
                .map(|pick| pick.chosen)
                .collect();
            assert_eq!(&picked, seat_pool);
        }
        assert_eq!(union, opened);
        // Passing, at every pick step: left in rounds 0 and 2, right in
        // round 1.
        for round in 0..ROUNDS {
            for pick in 0..PACK_SIZE - 1 {
                for seat in 0..SEATS {
                    let before = pick_at(&record, seat, round, pick);
                    let mut rest = before.pack.clone();
                    let at = rest.iter().position(|&card| card == before.chosen).unwrap();
                    rest.remove(at);
                    let next = receiving_seat(seat, round);
                    assert_eq!(pick_at(&record, next, round, pick + 1).pack, rest);
                }
            }
        }
        assert_eq!(receiving_seat(0, 0), 1);
        assert_eq!(receiving_seat(0, 1), 7);
        assert_eq!(receiving_seat(7, 2), 0);
        // Decks: the stated rule validates at 40 cards from the seat's pool.
        for seat_pool in &record.pools {
            let deck = build_deck(&mut StatedRuleDeckBuilder, seat_pool).unwrap();
            assert_eq!(deck.card_count(), 40);
            assert_eq!(deck.card_ids().len(), 40);
            deck.validate(seat_pool).unwrap();
            let json = deck.to_session_json();
            let rows = json["cards"].as_array().unwrap();
            assert_eq!(json.as_object().unwrap().len(), 1);
            let total: u64 = rows
                .iter()
                .map(|row| {
                    assert_eq!(row.as_object().unwrap().len(), 2);
                    assert!(row["name"].is_string());
                    row["count"].as_u64().unwrap()
                })
                .sum();
            assert_eq!(total, 40);
        }
    }
}

/// The reason `validate` gives for refusing `deck`.
fn refusal(deck: &Deck, pool: &[u16]) -> String {
    match deck.validate(pool) {
        Err(CubeDraftError::Deck(message)) => message,
        other => panic!("expected a deck refusal, got {other:?}"),
    }
}

/// A 40-card deck that only the check named by `reason` refuses.
fn refused_at_40(deck: &Deck, pool: &[u16], reason: &str) {
    assert_eq!(deck.checked_card_count(), Some(40), "{deck:?}");
    let message = refusal(deck, pool);
    assert!(message.contains(reason), "{message:?} is not {reason:?}");
}

#[test]
fn invalid_decks_are_refused_one_check_at_a_time() {
    let pool = pool();
    let (record, decks) = uniform_draft_and_build(&pool, SEED).unwrap();
    let seat_pool = &record.pools[0];
    let basics = basic_land_ids().unwrap();
    let valid = &decks[0];
    valid.validate(seat_pool).unwrap();
    let is_basic = |id: u16| basics.contains(&id);
    let rows = &valid.entries;
    let spell = rows.iter().position(|e| !is_basic(e.id)).unwrap();
    let basic = rows.iter().position(|e| is_basic(e.id) && e.count > 1);
    let basic = basic.unwrap();
    // A non-basic card in a second row, and twice in one row.
    let mut twice = valid.clone();
    twice.entries[basic].count -= 1;
    twice.push(valid.entries[spell].id, 1);
    refused_at_40(&twice, seat_pool, "more than once");
    let mut doubled = valid.clone();
    doubled.entries[basic].count -= 1;
    doubled.entries[spell].count = 2;
    refused_at_40(&doubled, seat_pool, "more than once");
    // A card from another seat's pool in place of one of this pool's.
    let mut foreign = valid.clone();
    let other = record.pools[1][0];
    foreign.entries[spell].id = other;
    foreign.entries[spell].name = CARD_DEFS[usize::from(other)].name.to_owned();
    refused_at_40(&foreign, seat_pool, "not in the pool");
    // An empty row.
    let mut empty_row = valid.clone();
    empty_row.push(basics[2], 0);
    refused_at_40(&empty_row, seat_pool, "zero copies");
    // A row whose name is not its card's, and an id outside the registry.
    let mut renamed = valid.clone();
    renamed.entries[spell].name = "Not A Card Name".to_owned();
    refused_at_40(&renamed, seat_pool, "does not name");
    assert!(CARD_DEFS.len() <= usize::from(u16::MAX));
    let mut unknown = valid.clone();
    unknown.entries[spell].id = u16::MAX;
    refused_at_40(&unknown, seat_pool, "does not name");
    // 39 and 41 cards.
    let mut short = valid.clone();
    short.entries[basic].count -= 1;
    assert!(refusal(&short, seat_pool).contains("39 cards"));
    let mut long = valid.clone();
    long.entries[basic].count += 1;
    assert!(refusal(&long, seat_pool).contains("41 cards"));
    // Counts that would wrap around to 40, or to one copy, are refused.
    let mut wrapping = Deck::default();
    wrapping.push(valid.entries[spell].id, 1);
    wrapping.push(valid.entries[spell].id, u32::MAX);
    wrapping.push(basics[0], 40);
    assert_eq!(wrapping.checked_card_count(), None);
    assert_eq!(wrapping.card_count(), u32::MAX);
    assert!(refusal(&wrapping, seat_pool).contains("overflows"));
    let mut basics_only = Deck::default();
    basics_only.push(basics[0], u32::MAX);
    basics_only.push(basics[1], 41);
    assert_eq!(basics_only.checked_card_count(), None);
    assert_eq!(basics_only.card_count(), u32::MAX);
    assert!(refusal(&basics_only, seat_pool).contains("overflows"));
}

struct OutOfRange;

impl DraftPicker for OutOfRange {
    fn pick(&mut self, view: &PickView<'_>) -> usize {
        view.pack.len()
    }
}

#[test]
fn an_out_of_range_pick_is_an_error() {
    let pool = pool();
    let mut pickers = uniform_pickers(SEED);
    pickers[3] = Box::new(OutOfRange);
    let error = run_draft(&pool, SEED, &mut pickers).unwrap_err();
    assert_eq!(
        error,
        CubeDraftError::PickOutOfRange {
            seat: 3,
            round: 0,
            pick: 0,
            index: PACK_SIZE,
            pack_len: PACK_SIZE,
        }
    );
}

// ---------------------------------------------------------------------------
// Small pools and decks built directly from the registry
// ---------------------------------------------------------------------------

const WHITE: u8 = 1;
const BLUE: u8 = 2;
const BLACK: u8 = 4;
const RED: u8 = 8;
const GREEN: u8 = 16;
const ALL_COLOURS: u8 = 31;

fn card_def(id: u16) -> &'static CardDef {
    &CARD_DEFS[usize::from(id)]
}

fn colour_mask(colour: ManaColor) -> u8 {
    match colour {
        ManaColor::W => WHITE,
        ManaColor::U => BLUE,
        ManaColor::B => BLACK,
        ManaColor::R => RED,
        ManaColor::G => GREEN,
        ManaColor::C => 0,
    }
}

/// The first `count` fully supported, non-token, non-basic registry cards,
/// in id order, that satisfy `keep`.
fn registry_cards(count: usize, keep: impl Fn(u16) -> bool) -> Vec<u16> {
    let basics = basic_land_ids().unwrap();
    let found: Vec<u16> = (0..CARD_DEFS.len())
        .map(|index| u16::try_from(index).unwrap())
        .filter(|id| !basics.contains(id))
        .filter(|&id| card_def(id).has_full_support() && !card_def(id).is_token)
        .filter(|&id| keep(id))
        .take(count)
        .collect();
    assert_eq!(found.len(), count, "too few such registry cards");
    found
}

/// A nonland card without a second half whose only mana symbol is one
/// `colour` symbol.
fn one_pip(colour: ManaColor) -> impl Fn(u16) -> bool {
    move |id| {
        let def = card_def(id);
        !def.is_land
            && def.adventure.is_none()
            && card_colour_bits(id) == colour_mask(colour)
            && matches!(def.cost.pips, [Pip::Colored(pip)] if *pip == colour)
    }
}

/// A colourless nonland card without mana symbols or a second half.
fn colourless(id: u16) -> bool {
    let def = card_def(id);
    let plain = !def.is_land && def.adventure.is_none();
    plain && def.cost.pips.is_empty() && card_colour_bits(id) == 0
}

/// A nonland card of exactly these colours.
fn gold(colours: u8) -> impl Fn(u16) -> bool {
    move |id| !card_def(id).is_land && card_colour_bits(id) == colours
}

/// A land without colours of its own that fetches nothing.
fn plain_land(id: u16) -> bool {
    card_def(id).is_land && card_colour_bits(id) == 0 && land_fetch_bits(id) == 0
}

/// A plain land that makes some colour, and only colours of `pair`.
fn land_within(pair: u8) -> impl Fn(u16) -> bool {
    move |id| plain_land(id) && land_colour_bits(id) != 0 && land_colour_bits(id) & !pair == 0
}

/// A plain land that makes some colour, but none of `pair`.
fn land_outside(pair: u8) -> impl Fn(u16) -> bool {
    move |id| plain_land(id) && land_colour_bits(id) != 0 && land_colour_bits(id) & pair == 0
}

/// A plain land that makes no coloured mana.
fn utility_land(id: u16) -> bool {
    plain_land(id) && land_colour_bits(id) == 0
}

/// The cards of `pool` that are in one of `groups`, in pool order.
fn kept(pool: &[u16], groups: &[&[u16]]) -> Vec<u16> {
    let keep: BTreeSet<u16> = groups
        .iter()
        .flat_map(|group| group.iter().copied())
        .collect();
    let mut kept = pool.to_vec();
    kept.retain(|id| keep.contains(id));
    kept
}

/// The deck the stated rule should build: one copy of each of `cards`, in
/// order, then `(colour, count)` basics.
fn deck_of(cards: &[u16], basics: &[(usize, u32)]) -> Deck {
    let ids = basic_land_ids().unwrap();
    let mut deck = Deck::default();
    for &id in cards {
        deck.push(id, 1);
    }
    for &(colour, count) in basics {
        deck.push(ids[colour], count);
    }
    deck
}

// ---------------------------------------------------------------------------
// The stated deck rule, step by step
// ---------------------------------------------------------------------------

#[test]
fn the_deck_rule_pair_counts_every_colour_of_a_card_and_breaks_ties_wubrg() {
    let w = registry_cards(1, one_pip(ManaColor::W));
    let u = registry_cards(2, one_pip(ManaColor::U));
    let b = registry_cards(1, one_pip(ManaColor::B));
    let r = registry_cards(1, one_pip(ManaColor::R));
    let g = registry_cards(2, one_pip(ManaColor::G));
    let black_red = registry_cards(1, gold(BLACK | RED))[0];
    let pair = StatedRuleDeckBuilder::colour_pair;
    assert_eq!(pair(&[]), [0, 1]);
    assert_eq!(pair(&[g[0], r[0]]), [3, 4]);
    assert_eq!(pair(&[g[0], g[1], b[0], w[0]]), [4, 0]);
    // The gold card counts once for black and once for red.
    assert_eq!(pair(&[u[0], black_red, u[1], r[0]]), [1, 3]);
    assert_eq!(pair(&[black_red, b[0], r[0], u[0]]), [2, 3]);
}

#[test]
fn the_deck_rule_takes_up_to_23_spells_in_the_pair_in_pick_order() {
    let w = registry_cards(14, one_pip(ManaColor::W));
    let u = registry_cards(8, one_pip(ManaColor::U));
    let r = registry_cards(3, one_pip(ManaColor::R));
    let c = registry_cards(2, colourless);
    let white_blue = registry_cards(1, gold(WHITE | BLUE));
    let white_red = registry_cards(1, gold(WHITE | RED));
    let mut pool = vec![r[0]];
    pool.extend(&w[..7]);
    pool.push(c[0]);
    pool.extend(&u[..4]);
    pool.extend([white_red[0], white_blue[0]]);
    pool.extend(&w[7..]);
    pool.push(r[1]);
    pool.extend(&u[4..]);
    pool.extend([c[1], r[2]]);
    assert_eq!(StatedRuleDeckBuilder::colour_pair(&pool), [0, 1]);
    // In the pair: white, blue, colourless and the white-blue card, but
    // not the red or white-red cards. 25 of them; the first 23 are taken.
    let in_pair = kept(&pool, &[&w[..], &u[..], &c[..], &white_blue[..]]);
    assert_eq!(in_pair.len(), 25);
    let deck = StatedRuleDeckBuilder.try_build(&pool).unwrap();
    let basics = basic_land_ids().unwrap();
    let ids: Vec<u16> = deck.entries.iter().map(|e| e.id).collect();
    assert_eq!(ids[..23], in_pair[..23]);
    assert!(deck.entries[..23].iter().all(|e| e.count == 1));
    let pair_basics = [basics[0], basics[1]];
    assert!(ids[23..].iter().all(|id| pair_basics.contains(id)));
    assert_eq!(deck.card_count(), 40);
    deck.validate(&pool).unwrap();
}

#[test]
fn the_deck_rule_fills_to_22_spells_with_off_colour_cards_in_pick_order() {
    let w = registry_cards(12, one_pip(ManaColor::W));
    let u = registry_cards(6, one_pip(ManaColor::U));
    let r = registry_cards(2, one_pip(ManaColor::R));
    let b = registry_cards(2, one_pip(ManaColor::B));
    let g = registry_cards(2, one_pip(ManaColor::G));
    let mut pool = vec![r[0]];
    pool.extend(&w[..6]);
    pool.push(b[0]);
    pool.extend(&u[..3]);
    pool.push(g[0]);
    pool.extend(&w[6..]);
    pool.push(r[1]);
    pool.extend(&u[3..]);
    pool.extend([b[1], g[1]]);
    let mut spells = kept(&pool, &[&w[..], &u[..]]);
    assert_eq!(spells.len(), 18);
    // The first four off-colour cards in pick order, and no more.
    spells.extend([r[0], b[0], g[0], r[1]]);
    // 18 basics by the pair's 12 white and 6 blue symbols.
    let expected = deck_of(&spells, &[(0, 12), (1, 6)]);
    assert_eq!(StatedRuleDeckBuilder.try_build(&pool).unwrap(), expected);
}

#[test]
fn the_deck_rule_adds_no_off_colour_card_to_22_spells() {
    let w = registry_cards(14, one_pip(ManaColor::W));
    let u = registry_cards(8, one_pip(ManaColor::U));
    let r = registry_cards(3, one_pip(ManaColor::R));
    let mut pool = w[..7].to_vec();
    pool.push(r[0]);
    pool.extend(&u[..4]);
    pool.push(r[1]);
    pool.extend(&w[7..]);
    pool.extend(&u[4..]);
    pool.push(r[2]);
    let spells = kept(&pool, &[&w[..], &u[..]]);
    assert_eq!(spells.len(), 22);
    // 18 basics by 14 white and 8 blue symbols: 11.45 and 6.55, so the
    // largest remainder gives the odd basic to blue.
    let expected = deck_of(&spells, &[(0, 11), (1, 7)]);
    assert_eq!(StatedRuleDeckBuilder.try_build(&pool).unwrap(), expected);
}

#[test]
fn the_deck_rule_takes_three_fitting_nonbasic_lands_and_reads_fetchlands() {
    let id = |name: &str| card_id_by_name(name).unwrap_or_else(|| panic!("{name}"));
    // The colours a fetchland can fetch come from its search filter.
    let fetches = [
        ("Arid Mesa", RED | WHITE),
        ("Bloodstained Mire", BLACK | RED),
        ("Flooded Strand", WHITE | BLUE),
        ("Marsh Flats", WHITE | BLACK),
        ("Misty Rainforest", GREEN | BLUE),
        ("Polluted Delta", BLUE | BLACK),
        ("Scalding Tarn", BLUE | RED),
        ("Verdant Catacombs", BLACK | GREEN),
        ("Windswept Heath", GREEN | WHITE),
        ("Wooded Foothills", RED | GREEN),
        ("Fabled Passage", ALL_COLOURS),
        ("Prismatic Vista", ALL_COLOURS),
    ];
    for (name, colours) in fetches {
        let land = id(name);
        assert_eq!(card_colour_bits(land), 0, "{name}");
        assert_eq!(land_mana_bits(land), 0, "{name}");
        assert_eq!(land_fetch_bits(land), colours, "{name}");
        assert_eq!(land_colour_bits(land), colours, "{name}");
    }
    let pair = WHITE | BLUE;
    let w = registry_cards(12, one_pip(ManaColor::W));
    let u = registry_cards(10, one_pip(ManaColor::U));
    let within = registry_cards(2, land_within(pair));
    let outside = registry_cards(1, land_outside(pair))[0];
    let utility = registry_cards(1, utility_land)[0];
    let plains = basic_land_ids().unwrap()[0];
    let mire = id("Bloodstained Mire");
    let strand = id("Flooded Strand");
    let passage = id("Fabled Passage");
    let mut pool = w[..6].to_vec();
    pool.extend([plains, outside]);
    pool.extend(&u[..5]);
    pool.extend([within[0], mire]);
    pool.extend(&w[6..]);
    pool.extend([utility, strand]);
    pool.extend(&u[5..]);
    pool.extend([within[1], passage]);
    let mut cards = kept(&pool, &[&w[..], &u[..]]);
    // In pick order, leaving out the basic, the off-colour land and the
    // black-red fetchland; the cap of 3 leaves out the last two that fit.
    cards.extend([within[0], utility, strand]);
    // 15 basics by 12 white and 10 blue symbols: 8.18 and 6.82.
    let expected = deck_of(&cards, &[(0, 8), (1, 7)]);
    assert_eq!(StatedRuleDeckBuilder.try_build(&pool).unwrap(), expected);
}

#[test]
fn the_deck_rule_splits_basics_by_largest_remainder_with_ties_to_the_first_colour() {
    let w = registry_cards(11, one_pip(ManaColor::W));
    let u = registry_cards(12, one_pip(ManaColor::U));
    let utility = registry_cards(1, utility_land)[0];
    // A tie: 11 white and 11 blue symbols share 17 basics; the odd basic
    // goes to the first colour, white (first in W, U, B, R, G order).
    let mut pool = [&w[..11], &u[..11]].concat();
    pool.push(utility);
    assert_eq!(StatedRuleDeckBuilder::colour_pair(&pool), [0, 1]);
    let expected = deck_of(&pool, &[(0, 9), (1, 8)]);
    assert_eq!(StatedRuleDeckBuilder.try_build(&pool).unwrap(), expected);
    // Blue first: 12 blue and 10 white symbols share 17 basics, 9.27 and
    // 7.73, so white's larger remainder takes the odd basic.
    let mut pool = [&u[..12], &w[..10]].concat();
    pool.push(utility);
    assert_eq!(StatedRuleDeckBuilder::colour_pair(&pool), [1, 0]);
    let expected = deck_of(&pool, &[(1, 9), (0, 8)]);
    assert_eq!(StatedRuleDeckBuilder.try_build(&pool).unwrap(), expected);
    // No coloured symbols: half each, the odd basic to the first colour of
    // the pair, white.
    let mut pool = registry_cards(22, colourless);
    pool.push(utility);
    assert_eq!(StatedRuleDeckBuilder::colour_pair(&pool), [0, 1]);
    let expected = deck_of(&pool, &[(0, 9), (1, 8)]);
    assert_eq!(StatedRuleDeckBuilder.try_build(&pool).unwrap(), expected);
}

#[test]
fn the_deck_rule_counts_the_symbols_of_both_halves_of_a_split_card() {
    let life_death = card_id_by_name("Life // Death").unwrap();
    assert!(card_def(life_death).adventure.is_some());
    assert_eq!(card_colour_bits(life_death), BLACK | GREEN);
    let mut pool = vec![life_death];
    pool.extend(registry_cards(21, colourless));
    assert_eq!(StatedRuleDeckBuilder::colour_pair(&pool), [2, 4]);
    // One black symbol ({1}{B}, Death) and one green ({G}, Life): 18
    // basics split 9 and 9.
    let expected = deck_of(&pool, &[(2, 9), (4, 9)]);
    assert_eq!(StatedRuleDeckBuilder.try_build(&pool).unwrap(), expected);
}

// ---------------------------------------------------------------------------
// Pickers
// ---------------------------------------------------------------------------

/// What one picker was shown at one pick.
#[derive(Debug, Clone)]
struct SeenView {
    seat: usize,
    round: usize,
    pick: usize,
    pack: Vec<u16>,
    picks: Vec<u16>,
}

type ViewLog = Rc<RefCell<Vec<SeenView>>>;

/// A uniform picker that logs every view it is shown.
struct Recording {
    log: ViewLog,
    inner: UniformPicker,
}

impl DraftPicker for Recording {
    fn pick(&mut self, view: &PickView<'_>) -> usize {
        self.log.borrow_mut().push(SeenView {
            seat: view.seat,
            round: view.round,
            pick: view.pick,
            pack: view.pack.to_vec(),
            picks: view.picks.to_vec(),
        });
        self.inner.pick(view)
    }
}

/// The seat that passes its pack to `seat` in `round`.
fn passing_seat(seat: usize, round: usize) -> usize {
    (0..SEATS)
        .find(|&other| receiving_seat(other, round) == seat)
        .unwrap()
}

#[test]
fn a_picker_sees_only_its_own_pack_and_its_own_picks() {
    let pool = pool();
    let logs: Vec<ViewLog> = (0..SEATS).map(|_| ViewLog::default()).collect();
    let mut pickers: [Box<dyn DraftPicker>; SEATS] = std::array::from_fn(|seat| {
        let log = Rc::clone(&logs[seat]);
        let inner = UniformPicker::new(picker_seed(SEED, seat));
        Box::new(Recording { log, inner }) as Box<dyn DraftPicker>
    });
    let record = run_draft(&pool, SEED, &mut pickers).unwrap();
    let uniform = run_draft(&pool, SEED, &mut uniform_pickers(SEED)).unwrap();
    assert_eq!(record, uniform);
    let seen: Vec<Vec<SeenView>> = logs.iter().map(|log| log.borrow().clone()).collect();
    for (seat, views) in seen.iter().enumerate() {
        assert_eq!(views.len(), POOL_CARDS);
        for (step, view) in views.iter().enumerate() {
            let (round, pick) = (step / PACK_SIZE, step % PACK_SIZE);
            assert_eq!((view.seat, view.round, view.pick), (seat, round, pick));
            // Every seat picks before any pack passes: no pack has lost a
            // card to another seat's pick in this step.
            assert_eq!(view.pack.len(), PACK_SIZE - pick);
            // The seat's own picks so far, in order, and nobody else's.
            assert_eq!(view.picks, record.pools[seat][..step]);
            assert_eq!(view.pack, pick_at(&record, seat, round, pick).pack);
            // The pack as opened, or as the passing seat left it.
            let expected = if pick == 0 {
                record.packs[round][seat].clone()
            } else {
                let from = passing_seat(seat, round);
                let mut rest = seen[from][step - 1].pack.clone();
                rest.retain(|&card| card != record.pools[from][step - 1]);
                rest
            };
            assert_eq!(view.pack, expected);
        }
    }
}

fn view<'a>(pack: &'a [u16], picks: &'a [u16]) -> PickView<'a> {
    PickView {
        seat: 0,
        round: picks.len() / PACK_SIZE,
        pick: picks.len() % PACK_SIZE,
        pack,
        picks,
    }
}

/// The indices a [`ColourCountPicker`] takes from `pack` over 64 seeds.
fn choices(pack: &[u16], picks: &[u16]) -> BTreeSet<usize> {
    (0..64)
        .map(|seed| ColourCountPicker::new(seed).pick(&view(pack, picks)))
        .collect()
}

#[test]
fn the_colour_count_picker_keeps_to_its_two_most_picked_colours() {
    let w = registry_cards(2, one_pip(ManaColor::W));
    let u = registry_cards(3, one_pip(ManaColor::U));
    let b = registry_cards(3, one_pip(ManaColor::B));
    let r = registry_cards(1, one_pip(ManaColor::R))[0];
    let g = registry_cards(1, one_pip(ManaColor::G))[0];
    let c = registry_cards(1, colourless)[0];
    // Blue twice, white and black once each: the pair is blue and white,
    // the tie with black going to white.
    let picks = [w[0], u[0], u[1], b[0]];
    let one_fits = [b[1], r, w[1], g, b[2]];
    assert_eq!(choices(&one_fits, &picks), BTreeSet::from([2]));
    // Uniform among the cards that fit, colourless included.
    let three_fit = [b[1], w[1], c, u[2], r];
    assert_eq!(choices(&three_fit, &picks), BTreeSet::from([1, 2, 3]));
    // Nothing fits, or nothing picked yet: uniform over the pack.
    let none_fit = [b[1], r, g];
    assert_eq!(choices(&none_fit, &picks), BTreeSet::from([0, 1, 2]));
    assert_eq!(choices(&none_fit, &[]), BTreeSet::from([0, 1, 2]));
}

// ---------------------------------------------------------------------------
// Pod play
// ---------------------------------------------------------------------------

#[test]
fn the_pod_plays_every_pair_once_and_records_caps_and_session_errors() {
    let all_pairs: BTreeSet<(usize, usize)> = (0..SEATS)
        .flat_map(|low| (low + 1..SEATS).map(move |high| (low, high)))
        .collect();
    assert_eq!(all_pairs.len(), 28);
    let pairings = pod_pairings();
    assert_eq!(pairings.len(), POD_GAMES);
    let distinct: BTreeSet<(usize, usize)> = pairings.iter().copied().collect();
    assert_eq!(distinct, all_pairs);
    on_big_stack(move || {
        let pool = pool();
        let (_, decks) = uniform_draft_and_build(&pool, SEED).unwrap();
        // A driver cap of 0 ends every game at its first decision, after
        // the session has accepted both decks.
        let no_decisions = GameCaps {
            max_decisions: 0,
            ..GameCaps::default()
        };
        let pod = play_pod_round_robin_with_caps(&decks, pod_seed(SEED), &no_decisions);
        assert_eq!(pod.pod_seed, pod_seed(SEED));
        assert_eq!(pod.games.len(), POD_GAMES);
        let mut played = BTreeSet::new();
        for (index, game) in pod.games.iter().enumerate() {
            assert_eq!(game.game, index);
            let [first, second] = game.seats;
            let pair = (first.min(second), first.max(second));
            assert_eq!(pair, pairings[index]);
            assert!(played.insert(pair));
            assert_eq!((game.decisions, &game.end), (0, &GameEnd::DecisionCap));
        }
        assert_eq!(played, all_pairs);
        assert_eq!(pod.end_counts(), BTreeMap::from([("decision_cap", 28)]));
        assert_eq!(pod.wins_by_seat(), [0; SEATS]);
        // A deck the session refuses ends its game as a recorded error.
        let mut bad = decks.clone();
        bad[1].entries.truncate(1);
        let caps = GameCaps::default();
        let refused = play_pod_game(&bad, pod_seed(SEED), 0, &caps).unwrap();
        assert_eq!(refused.decisions, 0);
        assert!(
            matches!(refused.end, GameEnd::SessionError { .. }),
            "{refused:?}"
        );
    });
}

#[test]
fn a_panic_is_recorded_with_the_decisions_made_before_it() {
    let (decisions, end) = catch_game_panic(|decisions| {
        *decisions += 7;
        panic!("engine failure at decision {}", *decisions);
    });
    let message = "engine failure at decision 7".to_owned();
    assert_eq!((decisions, end), (7, GameEnd::Panic { message }));
    let (_, end) = catch_game_panic(|_| panic!("a static message"));
    let message = "a static message".to_owned();
    assert_eq!(end, GameEnd::Panic { message });
    let (_, end) = catch_game_panic(|_| std::panic::panic_any(5u8));
    let message = "panic".to_owned();
    assert_eq!(end, GameEnd::Panic { message });
    let (decisions, end) = catch_game_panic(|decisions| {
        *decisions = 3;
        GameEnd::DecisionCap
    });
    assert_eq!((decisions, end), (3, GameEnd::DecisionCap));
}
