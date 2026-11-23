//! The cube draft environment v1: one seed reproduces the draft record, the
//! decks and the first pod game; another seed changes the draft; pools are
//! 45 disjoint cards; built decks validate; a pick outside the pack is an
//! error. Plays two games (the first pod game, twice).
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::cube_draft_v1::{
    build_deck, colour_count_pickers, play_pod_game, pod_seed, receiving_seat, run_draft,
    uniform_draft_and_build, uniform_pickers, CubeDraftError, CubePool, Deck, DraftPicker,
    DraftRecord, GameCaps, GameEnd, PickRecord, PickView, StatedRuleDeckBuilder, DRAFTED_CARDS,
    PACK_SIZE, POOL_CARDS, ROUNDS, SEATS,
};
use serde_json::Value;
use std::collections::BTreeSet;

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
    assert_eq!((record.seat, record.round, record.pick), (seat, round, pick));
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
        // Passing: left in rounds 0 and 2, right in round 1.
        for round in 0..ROUNDS {
            for seat in 0..SEATS {
                let before = pick_at(&record, seat, round, 0);
                let mut rest = before.pack.clone();
                let at = rest.iter().position(|&card| card == before.chosen).unwrap();
                rest.remove(at);
                let next = receiving_seat(seat, round);
                assert_eq!(pick_at(&record, next, round, 1).pack, rest);
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

#[test]
fn invalid_decks_are_refused() {
    let pool = pool();
    let (record, decks) = uniform_draft_and_build(&pool, SEED).unwrap();
    let seat_pool = &record.pools[0];
    // A non-basic card twice.
    let mut twice = decks[0].clone();
    let first = twice.entries[0].id;
    twice.entries.pop();
    twice.push(first, 1);
    assert!(matches!(twice.validate(seat_pool), Err(CubeDraftError::Deck(_))));
    // A card from another seat's pool.
    let mut foreign = Deck::default();
    foreign.push(record.pools[1][0], 1);
    assert!(foreign.validate(seat_pool).is_err());
    // The wrong size.
    let mut short = decks[0].clone();
    short.entries.pop();
    assert!(short.validate(seat_pool).is_err());
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
