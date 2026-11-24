//! Shared seat views and decision records for m4-interfaces/v1.

use crate::cube_draft_v1::{
    generate_packs, picker_seed, receiving_seat, CubeDraftError, CubePool, Deck, DraftRecord,
    PickRecord, DRAFTED_CARDS, DRAFT_RECORD_SCHEMA, PACK_SIZE, POOL_CARDS, SEATS,
};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// Frozen interface contract version implemented by this module.
pub const M4_INTERFACES_VERSION: &str = "m4-interfaces/v1";

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

// ---------------------------------------------------------------------------
// Decision rows (M4 interfaces section 3)
// ---------------------------------------------------------------------------

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
/// `m4-interfaces/v1` section 3.
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

/// Pack pass direction: [`PassDirection::Left`] in rounds 0 and 2,
/// [`PassDirection::Right`] in round 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassDirection {
    Left,
    Right,
}

/// Exactly what a seat knows at a draft pick. No seat number, no other seat's
/// data.
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

/// Draft picker on the frozen `m4-interfaces/v1` contract.
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

fn pass_packs(packs: Vec<Vec<u16>>, round: usize) -> Vec<Vec<u16>> {
    let mut passed = vec![Vec::new(); SEATS];
    for (seat, pack) in packs.into_iter().enumerate() {
        passed[receiving_seat(seat, round)] = pack;
    }
    passed
}

/// Runs an 8-seat draft using the `m4-interfaces/v1` [`SeatPicker`] trait and
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
                let seen_vec: Vec<u16> = seen_not_taken[seat].iter().copied().collect();
                let view = SeatPickView {
                    round: round as u8,
                    pick: pick as u8,
                    direction,
                    pack,
                    pool: &pools[seat],
                    seen_not_taken: &seen_vec,
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
                picks.push(PickRecord {
                    seat,
                    round,
                    pick,
                    pack: pack.clone(),
                    chosen,
                });
                if let Some(row) = outcome.row {
                    pick_rows[seat].push(row);
                }
                for &card in pack {
                    if card != chosen {
                        seen_not_taken[seat].insert(card);
                    }
                }
                seen_not_taken[seat].remove(&chosen);
                let removed = in_hand[seat].remove(outcome.index);
                pools[seat].push(removed);
            }
            in_hand = pass_packs(in_hand, round);
        }
    }

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

/// Output of one [`SeatBuilder::build`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutcome {
    pub deck: Deck,
    pub rows: Option<BuildRows>,
}

/// Decision rows for the four-step deck builder, in frozen field order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRows {
    pub count: DecisionRow,
    pub pointer: Vec<DecisionRow>,
    pub type_set: Option<DecisionRow>,
    pub split: Vec<DecisionRow>,
}

/// Deck builder on the frozen `m4-interfaces/v1` contract.
pub trait SeatBuilder {
    fn build(&mut self, pool: &[u16], stream: &mut SplitMix64) -> Result<BuildOutcome, SeatError>;
}

/// Canonical deck representation: one row per card copy (`count = 1`), sorted
/// in ascending registry card id order so copies of any basic land are
/// consecutive. Every deck entering a game goes through this function.
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
