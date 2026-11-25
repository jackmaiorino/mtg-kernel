//! The deck-signal experiment: seeds equal `deal.py`'s `seed` (values pinned
//! from the Python function); the deck rules' invariants over every part A
//! pair and the part B ring (40 cards, 23 spells, 2 lands, 15 basics, no
//! card in both decks of a game, colour discipline for C, nothing fenced);
//! the ring schedule; and lines that do not depend on how the index range is
//! split between workers. Plays a few games under small caps.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::cube_b_census_v1::ProcessDeath;
use mtg_kernel::cube_deck_signal_v1::{
    deal_coherent, deal_part_a_pair, deal_part_b_ring, deal_pile, part_a_deal_seed,
    part_a_game_seed, part_b_deck_seed, part_b_game_seed, part_b_matchup_decks, ring_distance,
    smoke_colour_bits, DealtDeck, DeckKind, DeckSignalRun, GameLine, Part, SmokePool, BASIC_LANDS,
    NONBASIC_LANDS, NONLAND_CARDS, PART_A_PAIRS, PART_B_DECKS, PART_B_MATCHUPS, PART_B_REACH,
};
use mtg_kernel::cube_draft_v1::{basic_land_ids, CubeDraftError, CubePool, GameCaps, DECK_CARDS};
use mtg_kernel::state::SplitMix64;
use std::collections::{BTreeMap, BTreeSet};

const CUBE_LIST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007.json"
);
const GAP_RECORD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"
);
/// A few cards of the current fence; the run passes the whole fence file.
const FENCE: &str = r#"{"cards": [
    {"name": "Time Walk", "reason": "extra turns"},
    {"name": "Brain Freeze", "reason": "stack overflow"},
    {"name": "Sage of the Skies", "reason": "InvalidEffectContinuation"},
    {"name": "Shifting Woodland", "reason": "malformed stack target metadata"}
]}"#;
const FENCED: [&str; 4] = [
    "Time Walk",
    "Brain Freeze",
    "Sage of the Skies",
    "Shifting Woodland",
];
/// Small caps, so the games stay cheap; the run itself uses `A2_CAPS`.
const TINY_CAPS: GameCaps = GameCaps {
    max_decisions: 200,
    max_physical_decisions: 16_384,
    max_policy_steps: 32_768,
};

fn pool() -> CubePool {
    let cube = std::fs::read_to_string(CUBE_LIST).expect("cube list");
    let gaps = std::fs::read_to_string(GAP_RECORD).expect("gap record");
    CubePool::from_json_texts(&cube, &gaps, Some(FENCE)).expect("draftable pool")
}

fn smoke_pool() -> SmokePool {
    SmokePool::from_pool(&pool()).expect("smoke pool")
}

/// Runs `work` on a thread with a large stack, as the workers do.
fn on_big_stack(work: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name("cube-deck-signal-v1".to_owned())
        .stack_size(64 << 20)
        .spawn(work)
        .expect("spawn")
        .join()
        .expect("test thread");
}

fn nonbasics(dealt: &DealtDeck, basics: &[u16; 5]) -> BTreeSet<u16> {
    dealt.nonbasic_ids(basics).into_iter().collect()
}

/// The shape every deck of either rule has.
fn check_shape(dealt: &DealtDeck, pool: &CubePool, smoke: &SmokePool) {
    let basics = basic_land_ids().expect("basics");
    dealt.deck.validate(pool.ids()).expect("deck validates");
    assert_eq!(dealt.deck.card_count(), DECK_CARDS);
    let ids = dealt.deck.card_ids();
    let spells: BTreeSet<u16> = smoke.spells.iter().map(|(id, _)| *id).collect();
    let lands: BTreeSet<u16> = smoke.lands.iter().copied().collect();
    assert_eq!(
        ids.iter().filter(|id| spells.contains(id)).count(),
        NONLAND_CARDS
    );
    assert_eq!(
        ids.iter().filter(|id| lands.contains(id)).count(),
        NONBASIC_LANDS
    );
    assert_eq!(
        ids.iter().filter(|id| basics.contains(id)).count(),
        BASIC_LANDS
    );
    let names: Vec<&str> = dealt
        .deck
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    for fenced in FENCED {
        assert!(!names.contains(&fenced), "{fenced} is fenced");
    }
}

/// Colour discipline of a coherent deck: every spell's colour data inside
/// the pair, and 8 and 7 basics of the pair's first and second colour.
fn check_coherent(dealt: &DealtDeck) {
    let basics = basic_land_ids().expect("basics");
    assert_eq!(dealt.kind, DeckKind::Coherent);
    let [first, second] = dealt.colours.expect("a coherent deck has colours");
    assert_ne!(first, second);
    let pair: u8 = (1 << first) | (1 << second);
    for entry in &dealt.deck.entries {
        if basics.contains(&entry.id) {
            let colour = basics.iter().position(|&id| id == entry.id).unwrap();
            let expected = if colour == first { 8 } else { 7 };
            assert!(colour == first || colour == second, "{}", entry.name);
            assert_eq!(entry.count, expected, "{}", entry.name);
        } else {
            assert_eq!(
                smoke_colour_bits(entry.id) & !pair,
                0,
                "{} is outside the pair",
                entry.name
            );
        }
    }
}

#[test]
fn seeds_match_deal_py() {
    // Each value is `deal.seed(parts)` computed by tools/cube_v1/deal.py.
    assert_eq!(part_a_deal_seed(0), 1_932_665_896_930_426_396);
    assert_eq!(part_a_game_seed(0), 835_776_810_172_748_148);
    assert_eq!(part_a_deal_seed(399), 369_192_819_592_456_989);
    assert_eq!(part_a_game_seed(399), 755_421_500_339_045_017);
    assert_eq!(part_b_deck_seed(0), 8_673_614_713_713_869_574);
    assert_eq!(part_b_deck_seed(31), 8_047_439_871_059_555_769);
    assert_eq!(part_b_game_seed(0, 0), 7_157_814_299_830_319_202);
    assert_eq!(part_b_game_seed(127, 3), 5_877_801_799_429_616_286);
}

#[test]
fn ring_schedule_meets_the_eight_nearest_decks_once() {
    let mut pairs = BTreeSet::new();
    let mut met = vec![BTreeSet::new(); PART_B_DECKS];
    for matchup in 0..PART_B_MATCHUPS {
        let [anchor, other] = part_b_matchup_decks(matchup);
        assert!((1..=PART_B_REACH).contains(&ring_distance(anchor, other)));
        assert!(pairs.insert((anchor.min(other), anchor.max(other))));
        met[anchor].insert(other);
        met[other].insert(anchor);
    }
    assert_eq!(pairs.len(), 128);
    for (deck, others) in met.iter().enumerate() {
        let nearest: BTreeSet<usize> = (0..PART_B_DECKS)
            .filter(|&other| (1..=PART_B_REACH).contains(&ring_distance(deck, other)))
            .collect();
        assert_eq!(others, &nearest, "deck {deck}");
        assert_eq!(others.len(), 8);
    }
}

#[test]
fn part_a_pairs_keep_the_deck_rules() {
    let pool = pool();
    let smoke = SmokePool::from_pool(&pool).expect("smoke pool");
    let basics = basic_land_ids().expect("basics");
    for pair in 0..PART_A_PAIRS {
        let [coherent, pile] = deal_part_a_pair(&smoke, pair).expect("deal");
        check_shape(&coherent, &pool, &smoke);
        check_shape(&pile, &pool, &smoke);
        check_coherent(&coherent);
        assert_eq!(pile.kind, DeckKind::Pile);
        assert!(
            nonbasics(&coherent, &basics).is_disjoint(&nonbasics(&pile, &basics)),
            "pair {pair} shares a card"
        );
        // Dealing is a function of the pair alone.
        assert_eq!(deal_part_a_pair(&smoke, pair).unwrap(), [coherent, pile]);
    }
}

#[test]
fn part_b_ring_keeps_the_deck_rules() {
    let pool = pool();
    let smoke = SmokePool::from_pool(&pool).expect("smoke pool");
    let basics = basic_land_ids().expect("basics");
    let ring = deal_part_b_ring(&smoke).expect("ring");
    assert_eq!(ring.len(), PART_B_DECKS);
    for dealt in &ring {
        check_shape(dealt, &pool, &smoke);
        check_coherent(dealt);
    }
    for first in 0..PART_B_DECKS {
        for second in first + 1..PART_B_DECKS {
            if ring_distance(first, second) <= PART_B_REACH {
                assert!(
                    nonbasics(&ring[first], &basics)
                        .is_disjoint(&nonbasics(&ring[second], &basics)),
                    "ring decks {first} and {second} meet and share a card"
                );
            }
        }
    }
    assert_eq!(deal_part_b_ring(&smoke).unwrap(), ring);
}

#[test]
fn coherent_rule_redraws_a_pair_that_cannot_fill_and_then_refuses() {
    let smoke = smoke_pool();
    // Everything but the colourless spells and two lands is excluded, so
    // no pair has 23 spells unless at least 23 colourless spells remain.
    let colourless: Vec<u16> = smoke
        .spells
        .iter()
        .filter(|(_, bits)| *bits == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut excluded: BTreeSet<u16> = smoke
        .spells
        .iter()
        .filter(|(_, bits)| *bits != 0)
        .map(|(id, _)| *id)
        .collect();
    excluded.extend(colourless.iter().skip(NONLAND_CARDS - 1));
    let mut rng = SplitMix64::seed(7);
    assert!(matches!(
        deal_coherent(&smoke, &mut rng, &excluded),
        Err(CubeDraftError::Deck(_))
    ));
    // A pile from the same leftovers refuses too.
    let mut rng = SplitMix64::seed(7);
    assert!(matches!(
        deal_pile(&smoke, &mut rng, &excluded),
        Err(CubeDraftError::Deck(_))
    ));
}

#[test]
fn coherent_rule_redraws_until_a_pair_fits() {
    let smoke = smoke_pool();
    // Keep the two-colour cards of one pair and 22 colourless spells, and
    // exclude every other spell: only that pair can fill 23 spells.
    let mut by_pair: BTreeMap<u8, Vec<u16>> = BTreeMap::new();
    for &(id, bits) in &smoke.spells {
        if bits.count_ones() == 2 {
            by_pair.entry(bits).or_default().push(id);
        }
    }
    let (&pair, gold) = by_pair
        .iter()
        .max_by_key(|(bits, ids)| (ids.len(), std::cmp::Reverse(**bits)))
        .expect("a two-colour card");
    let colourless: Vec<u16> = smoke
        .spells
        .iter()
        .filter(|(_, bits)| *bits == 0)
        .map(|(id, _)| *id)
        .take(NONLAND_CARDS - 1)
        .collect();
    assert_eq!(colourless.len(), NONLAND_CARDS - 1);
    let kept: BTreeSet<u16> = gold.iter().chain(colourless.iter()).copied().collect();
    let excluded: BTreeSet<u16> = smoke
        .spells
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !kept.contains(id))
        .collect();
    let mut redrawn = 0;
    for seed in 0..16 {
        let mut rng = SplitMix64::seed(seed);
        // A seed whose 64 pair draws all miss the one pair refuses; any
        // deck that is dealt has that pair.
        let Ok(dealt) = deal_coherent(&smoke, &mut rng, &excluded) else {
            continue;
        };
        let [first, second] = dealt.colours.expect("colours");
        assert_eq!((1u8 << first) | (1u8 << second), pair);
        assert_eq!(dealt.deck.card_count(), DECK_CARDS);
        check_coherent(&dealt);
        let ids = dealt.deck.card_ids();
        assert!(ids.iter().all(|id| !excluded.contains(id)));
        if dealt.pair_draws > 1 {
            redrawn += 1;
        }
    }
    assert!(redrawn > 0, "some seed drew another pair first");
}

fn lines(run: &DeckSignalRun, ranges: &[(u64, u64)]) -> Vec<String> {
    let mut out = Vec::new();
    for &(start, end) in ranges {
        run.play_range(
            start,
            end,
            |line: &GameLine| -> Result<(), CubeDraftError> {
                out.push(line.to_json_line());
                Ok(())
            },
        )
        .expect("range plays");
    }
    out
}

#[test]
fn lines_do_not_depend_on_the_worker_split() {
    on_big_stack(|| {
        let pool = pool();
        for part in [Part::A, Part::B] {
            let run = DeckSignalRun::new(part, &pool, TINY_CAPS).expect("run");
            let whole = lines(&run, &[(0, 4)]);
            let split = lines(&run, &[(0, 1), (1, 3), (3, 4)]);
            assert_eq!(whole, split, "part {}", part.label());
            let again = DeckSignalRun::new(part, &pool, TINY_CAPS).expect("run");
            assert_eq!(lines(&again, &[(0, 4)]), whole);
            for (index, text) in whole.iter().enumerate() {
                let single = run.play(index as u64).expect("game").to_json_line();
                assert_eq!(&single, text);
            }
        }
    });
}

#[test]
fn both_seat_orders_share_the_deal_and_the_environment() {
    on_big_stack(|| {
        let pool = pool();
        let run = DeckSignalRun::new(Part::A, &pool, TINY_CAPS).expect("run");
        let first = run.play(0).expect("game");
        let second = run.play(1).expect("game");
        assert_eq!(first.pair, Some(0));
        assert_eq!(second.pair, Some(0));
        assert_eq!(first.kinds, [DeckKind::Coherent, DeckKind::Pile]);
        assert_eq!(second.kinds, [DeckKind::Pile, DeckKind::Coherent]);
        assert_eq!(first.decks[0], second.decks[1]);
        assert_eq!(first.decks[1], second.decks[0]);
        assert_eq!(first.game_seed, second.game_seed);
        assert_eq!(first.env_seed, second.env_seed);
        assert_eq!(first.episode, second.episode);

        let run = DeckSignalRun::new(Part::B, &pool, TINY_CAPS).expect("run");
        let first = run.play(8 * 5 + 2 * 3).expect("game");
        let second = run.play(8 * 5 + 2 * 3 + 1).expect("game");
        assert_eq!(
            (first.matchup, first.deal, first.seat),
            (Some(5), Some(3), Some(0))
        );
        assert_eq!(
            (second.matchup, second.deal, second.seat),
            (Some(5), Some(3), Some(1))
        );
        assert_eq!(first.matchup_decks, Some(part_b_matchup_decks(5)));
        assert_eq!(first.seat_decks, Some([1, 3]));
        assert_eq!(second.seat_decks, Some([3, 1]));
        assert_eq!(first.decks[0], second.decks[1]);
        assert_eq!(first.env_seed, second.env_seed);
        assert_eq!(first.game_seed, part_b_game_seed(5, 3));
    });
}

#[test]
fn a_process_death_line_names_the_game() {
    let pool = pool();
    let run = DeckSignalRun::new(Part::A, &pool, TINY_CAPS).expect("run");
    let line = run
        .death(3, ProcessDeath::Exit(0xC000_00FD))
        .expect("death line");
    assert_eq!(line.index, 3);
    assert_eq!((line.pair, line.pair_game), (Some(1), Some(1)));
    assert_eq!(line.kinds, [DeckKind::Pile, DeckKind::Coherent]);
    assert!(!line.completed);
    assert_eq!(line.score, None);
    assert_eq!(
        line.stop_kind.as_deref(),
        Some("process dies: stack overflow (0xC00000FD)")
    );
    assert_eq!(line.end["kind"], "process_died");
    assert!(run.death(800, ProcessDeath::Exit(1)).is_err());
    assert!(run.play(800).is_err());
}
