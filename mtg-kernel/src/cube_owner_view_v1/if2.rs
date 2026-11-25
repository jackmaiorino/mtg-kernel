//! Frozen `m4-interfaces/v2` seat traits and decision rows used by D16.
//!
//! Stand-in mirror of `mtg-kernel/src/cube_seat_v1.rs` (sections 1 to 3 of
//! `m4-interfaces/v2`). Kept inside `cube_owner_view_v1` so D16 compiles and
//! tests cleanly on `main` without colliding with D3 or D5 before their PRs
//! land `crate::cube_seat_v1`. When `crate::cube_seat_v1` lands, replace
//! `use crate::cube_owner_view_v1::if2::*` with `use crate::cube_seat_v1::*`.

use crate::cube_draft_v1::{
    basic_land_ids, generate_packs, picker_seed, receiving_seat, CubeDraftError, CubePool, Deck,
    DraftRecord, PickRecord, DECK_CARDS, DRAFTED_CARDS, DRAFT_RECORD_SCHEMA, PACK_SIZE, POOL_CARDS,
    ROUNDS, SEATS,
};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// Frozen interface contract version implemented by this module.
pub const M4_INTERFACES_VERSION: &str = "m4-interfaces/v2";
/// Learned sampler identifier (D2a exact integer apportionment).
pub const LEARNED_SAMPLER_ID: &str = "softmax-cdf-splitmix64-v2";
/// Uniform fixture sampler identifier.
pub const UNIFORM_SAMPLER_ID: &str = "uniform-index-splitmix64-v1";

/// Errors returned by seat pickers, seat builders and [`run_seat_draft`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeatError {
    /// An underlying draft or deck validation error.
    Draft(CubeDraftError),
    /// A seat-level picker or builder error.
    Seat(String),
}

impl fmt::Display for SeatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeatError::Draft(error) => write!(f, "{error}"),
            SeatError::Seat(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for SeatError {}

impl From<CubeDraftError> for SeatError {
    fn from(error: CubeDraftError) -> Self {
        SeatError::Draft(error)
    }
}

/// Which decision a [`DecisionRow`] records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Pick,
    Count,
    Pointer,
    TypeSet,
    Split,
}

/// One logged draft or build decision, in the frozen field order of
/// `m4-interfaces/v2` section 3.
///
/// For learned rows, `p_chosen_bits` carries the exact IEEE-754 `f64` bits of
/// the chosen option's integer count over `2^53`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRow {
    pub kind: DecisionKind,
    pub sampler: String,
    pub options: Vec<u16>,
    pub chosen: u16,
    pub stream_values: Vec<u64>,
    pub logits_bits: Option<Vec<u32>>,
    pub p_chosen_bits: Option<u64>,
    pub value_bits: Option<u32>,
}

/// Pack pass direction: `Left` in rounds 0 and 2, `Right` in round 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassDirection {
    Left,
    Right,
}

/// Exactly the five things a seat knows at a pick (`m4-interfaces/v2` section 1).
/// No seat number and no other seat's data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatPickView<'a> {
    pub round: u8,
    pub pick: u8,
    pub direction: PassDirection,
    pub pack: &'a [u16],
    pub pool: &'a [u16],
    pub seen_not_taken: &'a [u16],
}

/// Output of one [`SeatPicker::pick`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickOutcome {
    pub index: usize,
    pub row: Option<DecisionRow>,
}

/// Draft picker on the frozen `m4-interfaces/v2` contract.
pub trait SeatPicker {
    fn pick(
        &mut self,
        view: &SeatPickView<'_>,
        stream: &mut SplitMix64,
    ) -> Result<PickOutcome, SeatError>;
}

/// Result of [`run_seat_draft`]: the verbatim `cube_draft_record/v1` record
/// together with each chair's non-null [`DecisionRow`]s in draft order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatDraft {
    pub record: DraftRecord,
    pub pick_rows: [Vec<DecisionRow>; SEATS],
}

/// Unbiased index draw in `0..bound` (`bound > 0`), matching
/// `cube_draft_v1::uniform_index` word for word.
///
/// Also returns every stream word drawn so uniform rows record exact
/// `stream_values`.
pub(crate) fn uniform_index_with_words(rng: &mut SplitMix64, bound: usize) -> (usize, Vec<u64>) {
    let bound_u64 = bound as u64;
    let threshold = bound_u64.wrapping_neg() % bound_u64;
    let mut words = Vec::with_capacity(1);
    loop {
        let value = rng.next_u64();
        words.push(value);
        if value >= threshold {
            return ((value % bound_u64) as usize, words);
        }
    }
}

fn pass_packs(packs: Vec<Vec<u16>>, round: usize) -> Vec<Vec<u16>> {
    let mut passed = vec![Vec::new(); SEATS];
    for (seat, pack) in packs.into_iter().enumerate() {
        passed[receiving_seat(seat, round)] = pack;
    }
    passed
}

/// Runs an 8-seat draft using the `m4-interfaces/v2` [`SeatPicker`] trait and
/// per-chair RNG streams seeded by [`picker_seed`].
pub fn run_seat_draft(
    pool: &CubePool,
    draft_seed: u64,
    pickers: &mut [Box<dyn SeatPicker>; SEATS],
) -> Result<SeatDraft, SeatError> {
    let opened = generate_packs(pool, draft_seed)?;
    let mut streams: [SplitMix64; SEATS] =
        std::array::from_fn(|chair| SplitMix64::seed(picker_seed(draft_seed, chair)));
    let mut pools: Vec<Vec<u16>> = (0..SEATS).map(|_| Vec::with_capacity(POOL_CARDS)).collect();
    let mut seen_not_taken: [BTreeSet<u16>; SEATS] = std::array::from_fn(|_| BTreeSet::new());
    let mut pick_rows: [Vec<DecisionRow>; SEATS] = std::array::from_fn(|_| Vec::new());
    let mut picks = Vec::with_capacity(DRAFTED_CARDS);

    for (round, round_packs) in opened.iter().enumerate() {
        let direction = if round % 2 == 1 {
            PassDirection::Right
        } else {
            PassDirection::Left
        };
        let mut in_hand: Vec<Vec<u16>> = round_packs.clone();
        for pick in 0..PACK_SIZE {
            for (seat, picker) in pickers.iter_mut().enumerate() {
                let pack = &in_hand[seat];
                let history: Vec<u16> = seen_not_taken[seat].iter().copied().collect();
                let view = SeatPickView {
                    round: round as u8,
                    pick: pick as u8,
                    direction,
                    pack,
                    pool: &pools[seat],
                    seen_not_taken: &history,
                };
                let outcome = picker.pick(&view, &mut streams[seat])?;
                if outcome.index >= pack.len() {
                    return Err(SeatError::Draft(CubeDraftError::PickOutOfRange {
                        seat,
                        round,
                        pick,
                        index: outcome.index,
                        pack_len: pack.len(),
                    }));
                }
                let chosen = pack[outcome.index];
                if let Some(row) = outcome.row {
                    pick_rows[seat].push(row);
                }
                for &card in pack {
                    if card != chosen {
                        seen_not_taken[seat].insert(card);
                    }
                }
                seen_not_taken[seat].remove(&chosen);
                picks.push(PickRecord {
                    seat,
                    round,
                    pick,
                    pack: pack.clone(),
                    chosen,
                });
                let removed = in_hand[seat].remove(outcome.index);
                debug_assert_eq!(removed, chosen);
                pools[seat].push(chosen);
            }
            in_hand = pass_packs(in_hand, round);
        }
    }

    debug_assert_eq!(opened.len(), ROUNDS);
    Ok(SeatDraft {
        record: DraftRecord {
            schema: DRAFT_RECORD_SCHEMA,
            draft_seed,
            packs: opened,
            picks,
            pools,
        },
        pick_rows,
    })
}

/// Fixture uniform picker (`m4-interfaces/v2` section 1): draws uniformly over
/// the pack on every pick (including forced picks) and returns `row: None`.
#[derive(Debug, Default, Clone, Copy)]
pub struct UniformSeatPicker;

impl SeatPicker for UniformSeatPicker {
    fn pick(
        &mut self,
        view: &SeatPickView<'_>,
        stream: &mut SplitMix64,
    ) -> Result<PickOutcome, SeatError> {
        if view.pack.is_empty() {
            return Err(SeatError::Seat(
                "empty pack in UniformSeatPicker".to_owned(),
            ));
        }
        let (index, _words) = uniform_index_with_words(stream, view.pack.len());
        Ok(PickOutcome { index, row: None })
    }
}

/// Output of one [`SeatBuilder::build`] call (`m4-interfaces/v2` section 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutcome {
    pub deck: Deck,
    pub rows: Option<BuildRows>,
}

/// Logged decisions of a four-step build (`m4-interfaces/v2` section 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRows {
    pub count: DecisionRow,
    pub pointer: Vec<DecisionRow>,
    pub type_set: Option<DecisionRow>,
    pub split: Vec<DecisionRow>,
}

/// Deck builder on the frozen `m4-interfaces/v2` contract.
pub trait SeatBuilder {
    fn build(&mut self, pool: &[u16], stream: &mut SplitMix64) -> Result<BuildOutcome, SeatError>;
}

/// Canonical deck representation (`m4-interfaces/v2` section 2): 40 rows of one
/// card each, in ascending registry card id order so copies of a basic are
/// consecutive.
pub fn canonical_deck(cards: &[u16]) -> Deck {
    let mut sorted = cards.to_vec();
    sorted.sort_unstable();
    let mut deck = Deck::default();
    deck.entries.reserve(sorted.len());
    for id in sorted {
        deck.push(id, 1);
    }
    deck
}

/// Uniform four-step builder fixture (`m4-interfaces/v2` section 2).
#[derive(Debug, Default, Clone, Copy)]
pub struct UniformFourStepBuilder;

impl SeatBuilder for UniformFourStepBuilder {
    fn build(&mut self, pool: &[u16], stream: &mut SplitMix64) -> Result<BuildOutcome, SeatError> {
        if pool.len() != POOL_CARDS {
            return Err(SeatError::Seat(format!(
                "expected {POOL_CARDS} pool cards, got {}",
                pool.len()
            )));
        }
        let mut remaining = pool.to_vec();
        remaining.sort_unstable();

        let count_options: Vec<u16> = (0..=DECK_CARDS as u16).collect();
        let (count_idx, count_words) = uniform_index_with_words(stream, count_options.len());
        let basics = count_options[count_idx];
        let count_row = DecisionRow {
            kind: DecisionKind::Count,
            sampler: UNIFORM_SAMPLER_ID.to_owned(),
            options: count_options,
            chosen: basics,
            stream_values: count_words,
            logits_bits: None,
            p_chosen_bits: None,
            value_bits: None,
        };

        let nonbasics_needed = (DECK_CARDS as usize) - usize::from(basics);
        let mut chosen_cards = Vec::with_capacity(DECK_CARDS as usize);
        let mut pointer_rows = Vec::with_capacity(nonbasics_needed);
        for _ in 0..nonbasics_needed {
            let (idx, words) = uniform_index_with_words(stream, remaining.len());
            let card = remaining[idx];
            pointer_rows.push(DecisionRow {
                kind: DecisionKind::Pointer,
                sampler: UNIFORM_SAMPLER_ID.to_owned(),
                options: remaining.clone(),
                chosen: card,
                stream_values: words,
                logits_bits: None,
                p_chosen_bits: None,
                value_bits: None,
            });
            remaining.remove(idx);
            chosen_cards.push(card);
        }

        let mut type_set_row = None;
        let mut split_rows = Vec::new();
        if basics > 0 {
            let masks: Vec<u16> = (1u16..32)
                .filter(|mask| mask.count_ones() <= u32::from(basics))
                .collect();
            let (mask_idx, mask_words) = uniform_index_with_words(stream, masks.len());
            let mask = masks[mask_idx];
            type_set_row = Some(DecisionRow {
                kind: DecisionKind::TypeSet,
                sampler: UNIFORM_SAMPLER_ID.to_owned(),
                options: masks,
                chosen: mask,
                stream_values: mask_words,
                logits_bits: None,
                p_chosen_bits: None,
                value_bits: None,
            });

            let bits: Vec<u16> = [1u16, 2, 4, 8, 16]
                .into_iter()
                .filter(|&bit| mask & bit != 0)
                .collect();
            let mut copies = vec![1usize; bits.len()];
            let extra = usize::from(basics) - bits.len();
            if bits.len() == 1 {
                copies[0] += extra;
            } else if extra > 0 {
                for _ in 0..extra {
                    let (bit_idx, words) = uniform_index_with_words(stream, bits.len());
                    copies[bit_idx] += 1;
                    split_rows.push(DecisionRow {
                        kind: DecisionKind::Split,
                        sampler: UNIFORM_SAMPLER_ID.to_owned(),
                        options: bits.clone(),
                        chosen: bits[bit_idx],
                        stream_values: words,
                        logits_bits: None,
                        p_chosen_bits: None,
                        value_bits: None,
                    });
                }
            }
            let basic_ids = basic_land_ids()?;
            for (bit, count) in bits.into_iter().zip(copies) {
                let id = basic_ids[bit.trailing_zeros() as usize];
                chosen_cards.extend(std::iter::repeat_n(id, count));
            }
        }

        let deck = canonical_deck(&chosen_cards);
        deck.validate(pool)?;
        Ok(BuildOutcome {
            deck,
            rows: Some(BuildRows {
                count: count_row,
                pointer: pointer_rows,
                type_set: type_set_row,
                split: split_rows,
            }),
        })
    }
}
