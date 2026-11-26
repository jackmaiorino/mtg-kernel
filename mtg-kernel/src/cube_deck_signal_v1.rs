//! The two-part deck-signal experiment of the cube drafting design note
//! (section 10, "The first cheap experiment, in two parts"), under the
//! uniform-legal pilot, as a labelled shakedown. Measurement only: no
//! training, no model.
//!
//! - Part A ([`Part::A`]): 400 pairs. Pair `p` deals one coherent deck (C)
//!   and one pile (P) from `seed(["deck-signal", "A", "deal", p])` and plays
//!   them twice from the one game seed `seed(["deck-signal", "A", "game",
//!   p])`, C as p0 then P as p0 (p0 always starts). Logical index `2 p + g`.
//! - Part B ([`Part::B`]): 32 C decks on a fixed ring, deck `d` dealt from
//!   `seed(["deck-signal", "B", "deck", d])`. Matchup `m` is anchor deck
//!   `m / 4` against deck `(m / 4 + m % 4 + 1) mod 32`, so every deck meets
//!   the 8 decks nearest to it in the ring (128 matchups). Each matchup is 4
//!   deals; deal `r` is played from `seed(["deck-signal", "B", "game", m,
//!   r])` twice, the anchor deck as p0 then as p1. Logical index
//!   `8 m + 2 r + s`.
//!
//! Deck rules (section 4 and section 10 of the note):
//!
//! - C, coherent: a faithful port of the smoke driver's rule
//!   (`cube_smoke_v1.rs`, `deal_decks` at 40 cards). Two distinct colours,
//!   the first uniform over W, U, B, R, G and the second uniform over the
//!   other four; 23 nonland cards drawn uniformly without replacement from
//!   the pool's nonland cards whose colour data (`CardDef::colors`) lies
//!   inside the pair, colourless included; 2 nonbasic lands drawn uniformly
//!   from all of the pool's lands, whatever they produce; 15 basics of the
//!   two colours, 8 of the first and 7 of the second. Candidates are kept in
//!   pool order and drawn by unbiased index and `swap_remove`, as the smoke
//!   driver does. One departure: where the smoke driver would deal a short
//!   deck padded with basics when fewer than 23 spells (or 2 lands) were
//!   left, this port draws the colour pair again from the same stream (at
//!   most [`MAX_PAIR_DRAWS`] times), so every deck keeps 23, 2 and 15.
//! - P, pile: 23 nonland cards drawn uniformly from the pool's nonland
//!   cards, whatever their colour; 2 lands as for C; 15 basics, each of a
//!   type drawn uniformly from the five.
//! - Both: no nonbasic card in both decks of a game. In part A the pile is
//!   dealt after the coherent deck, from the same stream, without its cards.
//!   In part B deck `d` is dealt without the cards of every already dealt
//!   deck within ring distance 4, so any two decks that ever meet are
//!   disjoint.
//!
//! The pool is [`CubePool`]: the cube list minus the gap record minus the
//! fence. Every game is [`play_seeded_game_tallied`] under the caps the run
//! is given (the census runner passes [`crate::cube_b_census_v1::A2_CAPS`]).
//!
//! The uniform pilot and these deck rules are measurement fixtures, never an
//! opponent or teacher in training.

use crate::card_def::{CardDef, CARD_DEFS};
use crate::cube_b_census_v1::{seed, stop_kind, ProcessDeath, SeedPart};
use crate::cube_draft_v1::{
    basic_land_ids, derive_seed, play_seeded_game_tallied, CubeDraftError, CubePool, Deck,
    GameCaps, GameEnd, GameRecord, GameTally, SeedStream, COLOUR_ORDER,
};
use crate::mana::ManaColor;
use crate::state::SplitMix64;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Schema tag of a [`GameLine`].
pub const GAME_LINE_SCHEMA: &str = "cube_deck_signal_game/v1";
/// Schema tag of the run record the example writes.
pub const RUN_RECORD_SCHEMA: &str = "cube_deck_signal_run/v1";
/// First element of every seed tuple of the experiment.
pub const SEED_NAMESPACE: &str = "deck-signal";

/// Nonland cards in a deck.
pub const NONLAND_CARDS: usize = 23;
/// Nonbasic cube lands in a deck.
pub const NONBASIC_LANDS: usize = 2;
/// Basic lands in a deck.
pub const BASIC_LANDS: usize = 15;
/// Times the coherent rule may draw its colour pair before refusing.
pub const MAX_PAIR_DRAWS: u32 = 64;

/// Part A: coherent-against-pile pairs.
pub const PART_A_PAIRS: u64 = 400;
/// Part A: games per pair (the decks swapped between seats).
pub const PART_A_GAMES_PER_PAIR: u64 = 2;
/// Part B: coherent decks on the ring.
pub const PART_B_DECKS: usize = 32;
/// Part B: ring neighbours each side a deck meets.
pub const PART_B_REACH: usize = 4;
/// Part B: matchups (each deck against the next [`PART_B_REACH`] decks).
pub const PART_B_MATCHUPS: u64 = (PART_B_DECKS * PART_B_REACH) as u64;
/// Part B: deals per matchup.
pub const PART_B_DEALS: u64 = 4;
/// Part B: games per matchup (each deal from both seats).
pub const PART_B_GAMES_PER_MATCHUP: u64 = 2 * PART_B_DEALS;

/// One part of the experiment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    A,
    B,
}

impl Part {
    /// `"A"` or `"B"`, as the seed tuples and the lines name the part.
    pub fn label(self) -> &'static str {
        match self {
            Part::A => "A",
            Part::B => "B",
        }
    }

    /// Parses `"A"` or `"B"`.
    pub fn parse(text: &str) -> Option<Part> {
        match text {
            "A" => Some(Part::A),
            "B" => Some(Part::B),
            _ => None,
        }
    }

    /// Games in the part: 800 (A) or 1,024 (B).
    pub fn games(self) -> u64 {
        match self {
            Part::A => PART_A_PAIRS * PART_A_GAMES_PER_PAIR,
            Part::B => PART_B_MATCHUPS * PART_B_GAMES_PER_MATCHUP,
        }
    }
}

// ---------------------------------------------------------------------------
// Seeds
// ---------------------------------------------------------------------------

fn part_seed(part: Part, role: &str, indices: &[u64]) -> u64 {
    let mut parts = vec![
        SeedPart::Str(SEED_NAMESPACE),
        SeedPart::Str(part.label()),
        SeedPart::Str(role),
    ];
    parts.extend(indices.iter().map(|&index| SeedPart::Int(index)));
    seed(&parts)
}

/// Part A, pair `pair`: the deal seed, `seed(["deck-signal", "A", "deal",
/// pair])`.
pub fn part_a_deal_seed(pair: u64) -> u64 {
    part_seed(Part::A, "deal", &[pair])
}

/// Part A, pair `pair`: the game seed of both games, `seed(["deck-signal",
/// "A", "game", pair])`.
pub fn part_a_game_seed(pair: u64) -> u64 {
    part_seed(Part::A, "game", &[pair])
}

/// Part B, ring deck `deck`: the deal seed, `seed(["deck-signal", "B",
/// "deck", deck])`.
pub fn part_b_deck_seed(deck: u64) -> u64 {
    part_seed(Part::B, "deck", &[deck])
}

/// Part B, matchup `matchup`, deal `deal`: the game seed of both seat
/// orders, `seed(["deck-signal", "B", "game", matchup, deal])`.
pub fn part_b_game_seed(matchup: u64, deal: u64) -> u64 {
    part_seed(Part::B, "game", &[matchup, deal])
}

/// The seed tuples of a part, as the run record states them.
pub fn seed_tuples(part: Part) -> Value {
    match part {
        Part::A => json!({
            "deal": ["deck-signal", "A", "deal", "<pair>"],
            "game": ["deck-signal", "A", "game", "<pair>"],
            "pair": "0..400",
            "function": "cube_b_census_v1::seed (tools/cube_v1/deal.py seed)",
        }),
        Part::B => json!({
            "deck": ["deck-signal", "B", "deck", "<deck>"],
            "game": ["deck-signal", "B", "game", "<matchup>", "<deal>"],
            "deck_range": "0..32",
            "matchup": "0..128",
            "deal": "0..4",
            "function": "cube_b_census_v1::seed (tools/cube_v1/deal.py seed)",
        }),
    }
}

/// The matchup's two ring decks: `[anchor, other]`.
pub fn part_b_matchup_decks(matchup: u64) -> [usize; 2] {
    let anchor = (matchup / PART_B_REACH as u64) as usize;
    let step = (matchup % PART_B_REACH as u64) as usize + 1;
    [anchor, (anchor + step) % PART_B_DECKS]
}

/// Distance between two ring positions.
pub fn ring_distance(first: usize, second: usize) -> usize {
    let gap = first.abs_diff(second) % PART_B_DECKS;
    gap.min(PART_B_DECKS - gap)
}

// ---------------------------------------------------------------------------
// Pool and deck rules
// ---------------------------------------------------------------------------

fn colour_bit(color: ManaColor) -> u8 {
    match color {
        ManaColor::W => 1,
        ManaColor::U => 2,
        ManaColor::B => 4,
        ManaColor::R => 8,
        ManaColor::G => 16,
        ManaColor::C => 0,
    }
}

fn def(id: u16) -> &'static CardDef {
    &CARD_DEFS[usize::from(id)]
}

/// The colour mask the smoke rule reads: the card's colour data
/// (`CardDef::colors`), bit `i` for [`COLOUR_ORDER`]`[i]`.
pub fn smoke_colour_bits(id: u16) -> u8 {
    def(id)
        .colors
        .iter()
        .fold(0, |bits, color| bits | colour_bit(*color))
}

/// The pool as the smoke rule splits it, in pool (cube-list) order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmokePool {
    /// Nonland cards and their colour masks ([`smoke_colour_bits`]).
    pub spells: Vec<(u16, u8)>,
    /// Lands that are not basic lands.
    pub lands: Vec<u16>,
    /// Basic land ids in [`COLOUR_ORDER`].
    pub basics: [u16; 5],
}

impl SmokePool {
    /// Splits `pool` as the smoke driver's `load_pool` does: a basic land on
    /// the list is skipped, a land is a land, everything else is a spell.
    /// The smoke driver skipped tokens and cards without Full support; here
    /// such a card in the pool is refused, since the pool is meant to hold
    /// only playable cards.
    pub fn from_pool(pool: &CubePool) -> Result<Self, CubeDraftError> {
        let basics = basic_land_ids()?;
        let mut out = SmokePool {
            spells: Vec::new(),
            lands: Vec::new(),
            basics,
        };
        let mut not_full = Vec::new();
        for &id in pool.ids() {
            let card = def(id);
            if card.is_token || !card.has_full_support() {
                not_full.push(card.name);
                continue;
            }
            if basics.contains(&id) {
                continue;
            }
            if card.is_land {
                out.lands.push(id);
            } else {
                out.spells.push((id, smoke_colour_bits(id)));
            }
        }
        if !not_full.is_empty() {
            return Err(CubeDraftError::Deck(format!(
                "pool cards without Full support: {}",
                not_full.join(", ")
            )));
        }
        Ok(out)
    }
}

/// The smoke driver's `draw`: an unbiased index in `0..bound` (`bound > 0`).
fn draw(rng: &mut SplitMix64, bound: usize) -> usize {
    let bound = bound as u64;
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return (value % bound) as usize;
        }
    }
}

/// Which rule dealt a deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DeckKind {
    /// Coherent (the smoke rule).
    #[serde(rename = "C")]
    Coherent,
    /// Pile.
    #[serde(rename = "P")]
    Pile,
}

/// A dealt deck and how it was dealt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DealtDeck {
    pub kind: DeckKind,
    /// The colour pair as [`COLOUR_ORDER`] indices, first colour first
    /// (coherent decks only).
    pub colours: Option<[usize; 2]>,
    /// Colour pairs drawn, the last one kept (coherent decks; 1 when the
    /// first pair fitted).
    pub pair_draws: u32,
    pub deck: Deck,
}

impl DealtDeck {
    /// The deck's nonbasic card ids.
    pub fn nonbasic_ids(&self, basics: &[u16; 5]) -> Vec<u16> {
        self.deck
            .card_ids()
            .into_iter()
            .filter(|id| !basics.contains(id))
            .collect()
    }

    /// The colour pair as letters, such as `"WU"` (coherent decks only).
    pub fn colour_letters(&self) -> Option<String> {
        self.colours
            .map(|pair| pair.iter().map(|&index| colour_letter(index)).collect())
    }
}

fn colour_letter(index: usize) -> char {
    match COLOUR_ORDER[index] {
        ManaColor::W => 'W',
        ManaColor::U => 'U',
        ManaColor::B => 'B',
        ManaColor::R => 'R',
        ManaColor::G => 'G',
        ManaColor::C => 'C',
    }
}

/// Draws `count` entries of `candidates` without replacement, as the smoke
/// driver does (unbiased index, `swap_remove`).
fn draw_from(rng: &mut SplitMix64, mut candidates: Vec<u16>, count: usize) -> Vec<u16> {
    let mut drawn = Vec::with_capacity(count);
    for _ in 0..count {
        let at = draw(rng, candidates.len());
        drawn.push(candidates.swap_remove(at));
    }
    drawn
}

fn land_candidates(pool: &SmokePool, excluded: &BTreeSet<u16>) -> Vec<u16> {
    pool.lands
        .iter()
        .copied()
        .filter(|id| !excluded.contains(id))
        .collect()
}

/// The coherent rule (C): see the module documentation. `excluded` holds
/// the nonbasic cards the deck may not use.
pub fn deal_coherent(
    pool: &SmokePool,
    rng: &mut SplitMix64,
    excluded: &BTreeSet<u16>,
) -> Result<DealtDeck, CubeDraftError> {
    for attempt in 1..=MAX_PAIR_DRAWS {
        let first = draw(rng, 5);
        let mut second = draw(rng, 4);
        if second >= first {
            second += 1;
        }
        let pair = colour_bit(COLOUR_ORDER[first]) | colour_bit(COLOUR_ORDER[second]);
        let spells: Vec<u16> = pool
            .spells
            .iter()
            .filter(|(id, bits)| !excluded.contains(id) && bits & !pair == 0)
            .map(|(id, _)| *id)
            .collect();
        let lands = land_candidates(pool, excluded);
        if spells.len() < NONLAND_CARDS || lands.len() < NONBASIC_LANDS {
            continue;
        }
        let spells = draw_from(rng, spells, NONLAND_CARDS);
        let lands = draw_from(rng, lands, NONBASIC_LANDS);
        let mut deck = Deck::default();
        for &id in spells.iter().chain(lands.iter()) {
            deck.push(id, 1);
        }
        let first_count = BASIC_LANDS.div_ceil(2);
        deck.push(pool.basics[first], first_count as u32);
        deck.push(pool.basics[second], (BASIC_LANDS - first_count) as u32);
        return Ok(DealtDeck {
            kind: DeckKind::Coherent,
            colours: Some([first, second]),
            pair_draws: attempt,
            deck,
        });
    }
    Err(CubeDraftError::Deck(format!(
        "no colour pair in {MAX_PAIR_DRAWS} draws left {NONLAND_CARDS} spells and {NONBASIC_LANDS} lands"
    )))
}

/// The pile rule (P): see the module documentation. `excluded` holds the
/// nonbasic cards the deck may not use.
pub fn deal_pile(
    pool: &SmokePool,
    rng: &mut SplitMix64,
    excluded: &BTreeSet<u16>,
) -> Result<DealtDeck, CubeDraftError> {
    let spells: Vec<u16> = pool
        .spells
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !excluded.contains(id))
        .collect();
    let lands = land_candidates(pool, excluded);
    if spells.len() < NONLAND_CARDS || lands.len() < NONBASIC_LANDS {
        return Err(CubeDraftError::Deck(format!(
            "a pile needs {NONLAND_CARDS} spells and {NONBASIC_LANDS} lands; {} and {} are left",
            spells.len(),
            lands.len()
        )));
    }
    let spells = draw_from(rng, spells, NONLAND_CARDS);
    let lands = draw_from(rng, lands, NONBASIC_LANDS);
    let mut counts = [0u32; 5];
    for _ in 0..BASIC_LANDS {
        counts[draw(rng, 5)] += 1;
    }
    let mut deck = Deck::default();
    for &id in spells.iter().chain(lands.iter()) {
        deck.push(id, 1);
    }
    for (colour, &count) in counts.iter().enumerate() {
        if count > 0 {
            deck.push(pool.basics[colour], count);
        }
    }
    Ok(DealtDeck {
        kind: DeckKind::Pile,
        colours: None,
        pair_draws: 0,
        deck,
    })
}

/// Part A, pair `pair`: `[C, P]`, both from the pair's deal seed, the pile
/// without the coherent deck's nonbasic cards.
pub fn deal_part_a_pair(pool: &SmokePool, pair: u64) -> Result<[DealtDeck; 2], CubeDraftError> {
    let mut rng = SplitMix64::seed(part_a_deal_seed(pair));
    let coherent = deal_coherent(pool, &mut rng, &BTreeSet::new())?;
    let excluded: BTreeSet<u16> = coherent.nonbasic_ids(&pool.basics).into_iter().collect();
    let pile = deal_pile(pool, &mut rng, &excluded)?;
    Ok([coherent, pile])
}

/// Part B: the 32 ring decks, in ring order. Deck `d` is dealt from its own
/// seed without the nonbasic cards of decks already dealt within ring
/// distance [`PART_B_REACH`].
pub fn deal_part_b_ring(pool: &SmokePool) -> Result<Vec<DealtDeck>, CubeDraftError> {
    let mut decks: Vec<DealtDeck> = Vec::with_capacity(PART_B_DECKS);
    for deck in 0..PART_B_DECKS {
        let excluded: BTreeSet<u16> = decks
            .iter()
            .enumerate()
            .filter(|(other, _)| ring_distance(deck, *other) <= PART_B_REACH)
            .flat_map(|(_, dealt)| dealt.nonbasic_ids(&pool.basics))
            .collect();
        let mut rng = SplitMix64::seed(part_b_deck_seed(deck as u64));
        decks.push(deal_coherent(pool, &mut rng, &excluded)?);
    }
    Ok(decks)
}

// ---------------------------------------------------------------------------
// Game lines
// ---------------------------------------------------------------------------

/// One game, as one JSONL line. Fields that do not apply to the part, or
/// that the game never reached, are `null`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameLine {
    pub schema: &'static str,
    /// `"A"` or `"B"`.
    pub part: &'static str,
    /// Logical index: `2 pair + game` (A) or `8 matchup + 2 deal + seat`
    /// (B).
    pub index: u64,
    /// Part A: the pair, and the game in it (0: C is p0; 1: P is p0).
    pub pair: Option<u64>,
    pub pair_game: Option<u64>,
    /// Part B: the matchup, its two ring decks (`[anchor, other]`), the
    /// deal, and the anchor deck's seat (0: p0, on the play; 1: p1).
    pub matchup: Option<u64>,
    pub matchup_decks: Option<[usize; 2]>,
    pub deal: Option<u64>,
    pub seat: Option<u64>,
    /// Part B: the ring deck of each seat, p0 first.
    pub seat_decks: Option<[usize; 2]>,
    /// The deck kind of each seat, p0 first (`"C"` or `"P"`).
    pub kinds: [DeckKind; 2],
    /// The coherent decks' colour pairs, p0 first (`null` for a pile).
    pub colours: [Option<String>; 2],
    /// Part A: the pair's deal seed.
    pub deal_seed: Option<u64>,
    /// The game seed (shared by the two seat orders of a pair or deal).
    pub game_seed: u64,
    /// The session `env_seed` and episode id (both shared by the two seat
    /// orders of a pair or deal).
    pub env_seed: u64,
    pub episode: u64,
    /// Each seat's deck as sorted registry card ids, one per copy, p0 first.
    pub decks: [Vec<u16>; 2],
    /// The raw end: [`GameEnd`] (tagged `kind`) or a process death.
    pub end: Value,
    /// Whether the game completed (`natural_game_over`).
    pub completed: bool,
    /// The normalised cause of a halted game; `null` for a completed game.
    pub stop_kind: Option<String>,
    /// Decisions the driver made; `null` after a process death.
    pub driver_decisions: Option<u64>,
    /// The terminal reply's counts.
    pub physical_decisions: Option<u64>,
    pub policy_steps: Option<u64>,
    /// Source card of the last chosen action, and of the latest of the last
    /// ten chosen actions that had one.
    pub last_action_source: Option<u16>,
    pub recent_action_source: Option<u16>,
    /// The winner as a session player (0 for p0, 1 for p1).
    pub winner_player: Option<usize>,
    /// A completed game without a winner.
    pub draw: bool,
    /// Each seat's result for a completed game (win 1, draw 0.5, loss 0),
    /// p0 first; `null` for a halted game.
    pub score: Option<[f64; 2]>,
}

impl GameLine {
    /// The line as compact JSON, without a line break.
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).expect("a game line is plain data")
    }
}

fn sorted_ids(deck: &Deck) -> Vec<u16> {
    let mut ids = deck.card_ids();
    ids.sort_unstable();
    ids
}

/// Where a game sits and what it plays, apart from how it ended.
struct Placement<'a> {
    part: Part,
    index: u64,
    pair: Option<u64>,
    pair_game: Option<u64>,
    matchup: Option<u64>,
    matchup_decks: Option<[usize; 2]>,
    deal: Option<u64>,
    seat: Option<u64>,
    seat_decks: Option<[usize; 2]>,
    deal_seed: Option<u64>,
    game_seed: u64,
    episode: u64,
    decks: [&'a DealtDeck; 2],
}

impl Placement<'_> {
    fn line(self, end: Value, record: Option<(&GameRecord, &GameTally)>) -> GameLine {
        let (completed, stop, winner, decisions, tally) = match record {
            Some((record, tally)) => (
                matches!(record.end, GameEnd::Natural { .. }),
                stop_kind(&record.end),
                record.winner(),
                Some(record.decisions),
                Some(tally),
            ),
            None => (false, None, None, None, None),
        };
        let score = completed.then_some(match winner {
            Some(0) => [1.0, 0.0],
            Some(_) => [0.0, 1.0],
            None => [0.5, 0.5],
        });
        GameLine {
            schema: GAME_LINE_SCHEMA,
            part: self.part.label(),
            index: self.index,
            pair: self.pair,
            pair_game: self.pair_game,
            matchup: self.matchup,
            matchup_decks: self.matchup_decks,
            deal: self.deal,
            seat: self.seat,
            seat_decks: self.seat_decks,
            kinds: [self.decks[0].kind, self.decks[1].kind],
            colours: [
                self.decks[0].colour_letters(),
                self.decks[1].colour_letters(),
            ],
            deal_seed: self.deal_seed,
            game_seed: self.game_seed,
            env_seed: derive_seed(self.game_seed, SeedStream::GameEnv, 0),
            episode: self.episode,
            decks: [
                sorted_ids(&self.decks[0].deck),
                sorted_ids(&self.decks[1].deck),
            ],
            end,
            completed,
            stop_kind: stop,
            driver_decisions: decisions,
            physical_decisions: tally.and_then(|tally| tally.physical_decisions),
            policy_steps: tally.and_then(|tally| tally.policy_steps),
            last_action_source: tally.and_then(|tally| tally.last_action_source),
            recent_action_source: tally.and_then(GameTally::recent_action_source),
            winner_player: winner,
            draw: completed && winner.is_none(),
            score,
        }
    }

    fn play(self, caps: &GameCaps) -> GameLine {
        let decks = [&self.decks[0].deck, &self.decks[1].deck];
        let (record, tally) =
            play_seeded_game_tallied(decks, self.game_seed, self.episode as usize, caps);
        let end = serde_json::to_value(&record.end).expect("a game end is plain data");
        self.line(end, Some((&record, &tally)))
    }

    fn death(self, death: ProcessDeath) -> GameLine {
        let end = match death {
            ProcessDeath::Exit(code) => json!({"kind": "process_died", "exit_code": code}),
            ProcessDeath::Signal(signal) => json!({"kind": "process_died", "signal": signal}),
        };
        let mut line = self.line(end, None);
        line.stop_kind = Some(death.stop_kind());
        line
    }
}

// ---------------------------------------------------------------------------
// Runs
// ---------------------------------------------------------------------------

/// One part of the experiment, ready to play any of its games.
#[derive(Debug, Clone)]
pub struct DeckSignalRun {
    pub part: Part,
    pub pool: SmokePool,
    pub caps: GameCaps,
    /// Part B: the ring decks (empty for part A, whose decks are dealt per
    /// pair).
    pub ring: Vec<DealtDeck>,
}

impl DeckSignalRun {
    /// Prepares `part` over `pool` under `caps`. Part B deals its ring here.
    pub fn new(part: Part, pool: &CubePool, caps: GameCaps) -> Result<Self, CubeDraftError> {
        let pool = SmokePool::from_pool(pool)?;
        let ring = match part {
            Part::A => Vec::new(),
            Part::B => deal_part_b_ring(&pool)?,
        };
        Ok(DeckSignalRun {
            part,
            pool,
            caps,
            ring,
        })
    }

    /// Games in the run.
    pub fn games(&self) -> u64 {
        self.part.games()
    }

    /// The error of an index range outside the part's games. (Not
    /// [`CubeDraftError::GameOutOfRange`], whose message names a pod game.)
    fn out_of_range(&self, start: u64, end: u64) -> CubeDraftError {
        CubeDraftError::Deck(format!(
            "deck-signal games [{start}, {end}) are outside part {}'s 0..{}",
            self.part.label(),
            self.games()
        ))
    }

    fn check_index(&self, index: u64) -> Result<(), CubeDraftError> {
        if index >= self.games() {
            return Err(self.out_of_range(index, index.saturating_add(1)));
        }
        Ok(())
    }

    /// Plays game `index`.
    pub fn play(&self, index: u64) -> Result<GameLine, CubeDraftError> {
        self.check_index(index)?;
        match self.part {
            Part::A => {
                let pair = deal_part_a_pair(&self.pool, index / PART_A_GAMES_PER_PAIR)?;
                Ok(self.placement_a(index, &pair).play(&self.caps))
            }
            Part::B => Ok(self.placement_b(index).play(&self.caps)),
        }
    }

    /// The line of game `index` when its worker process died while playing
    /// it.
    pub fn death(&self, index: u64, death: ProcessDeath) -> Result<GameLine, CubeDraftError> {
        self.check_index(index)?;
        match self.part {
            Part::A => {
                let pair = deal_part_a_pair(&self.pool, index / PART_A_GAMES_PER_PAIR)?;
                Ok(self.placement_a(index, &pair).death(death))
            }
            Part::B => Ok(self.placement_b(index).death(death)),
        }
    }

    /// Plays `[start, end)` in order, handing each line to `emit` as soon as
    /// it is played. A part A pair is dealt once.
    pub fn play_range<E>(
        &self,
        start: u64,
        end: u64,
        mut emit: impl FnMut(&GameLine) -> Result<(), E>,
    ) -> Result<(), E>
    where
        E: From<CubeDraftError>,
    {
        if end > self.games() || start > end {
            return Err(self.out_of_range(start, end).into());
        }
        let mut dealt: Option<(u64, [DealtDeck; 2])> = None;
        for index in start..end {
            let line = match self.part {
                Part::A => {
                    let pair = index / PART_A_GAMES_PER_PAIR;
                    if dealt.as_ref().is_none_or(|(at, _)| *at != pair) {
                        dealt = Some((pair, deal_part_a_pair(&self.pool, pair)?));
                    }
                    let (_, decks) = dealt.as_ref().expect("dealt above");
                    self.placement_a(index, decks).play(&self.caps)
                }
                Part::B => self.placement_b(index).play(&self.caps),
            };
            emit(&line)?;
        }
        Ok(())
    }

    fn placement_a<'a>(&self, index: u64, pair_decks: &'a [DealtDeck; 2]) -> Placement<'a> {
        let pair = index / PART_A_GAMES_PER_PAIR;
        let game = index % PART_A_GAMES_PER_PAIR;
        let [coherent, pile] = pair_decks;
        Placement {
            part: Part::A,
            index,
            pair: Some(pair),
            pair_game: Some(game),
            matchup: None,
            matchup_decks: None,
            deal: None,
            seat: None,
            seat_decks: None,
            deal_seed: Some(part_a_deal_seed(pair)),
            game_seed: part_a_game_seed(pair),
            episode: pair,
            decks: if game == 0 {
                [coherent, pile]
            } else {
                [pile, coherent]
            },
        }
    }

    fn placement_b(&self, index: u64) -> Placement<'_> {
        let matchup = index / PART_B_GAMES_PER_MATCHUP;
        let within = index % PART_B_GAMES_PER_MATCHUP;
        let (deal, seat) = (within / 2, within % 2);
        let [anchor, other] = part_b_matchup_decks(matchup);
        let seat_decks = if seat == 0 {
            [anchor, other]
        } else {
            [other, anchor]
        };
        Placement {
            part: Part::B,
            index,
            pair: None,
            pair_game: None,
            matchup: Some(matchup),
            matchup_decks: Some([anchor, other]),
            deal: Some(deal),
            seat: Some(seat),
            seat_decks: Some(seat_decks),
            deal_seed: None,
            game_seed: part_b_game_seed(matchup, deal),
            episode: matchup * PART_B_DEALS + deal,
            decks: [&self.ring[seat_decks[0]], &self.ring[seat_decks[1]]],
        }
    }

    /// Part B's ring for the run record: each deck's colours, pair draws and
    /// sorted card ids.
    pub fn ring_record(&self) -> Value {
        Value::Array(
            self.ring
                .iter()
                .enumerate()
                .map(|(index, dealt)| {
                    json!({
                        "deck": index,
                        "seed": part_b_deck_seed(index as u64),
                        "colours": dealt.colour_letters(),
                        "pair_draws": dealt.pair_draws,
                        "cards": sorted_ids(&dealt.deck),
                    })
                })
                .collect(),
        )
    }
}
