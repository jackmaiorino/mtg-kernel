//! Frozen M4 seat interfaces (`m4-interfaces/v2`) for cube draft picking,
//! deck building and game play.
//!
//! The uniform seats ([`UniformSeatPicker`], [`UniformFourStepBuilder`]),
//! [`StatedSeatBuilder`] and [`UniformPilot`] are test fixtures and reference
//! baselines only, never teachers or opponents in training.

use crate::cube_draft_v1::{
    basic_land_ids, drive_uniform_attempt_with_options, generate_packs, picker_seed,
    receiving_seat, uniform_index, CubeDraftError, CubePool, Deck, DraftRecord, GameCaps, GameEnd,
    PickRecord, StatedRuleDeckBuilder, DECK_CARDS, DRAFTED_CARDS, DRAFT_RECORD_SCHEMA, PACK_SIZE,
    POOL_CARDS, SEATS,
};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Frozen interface contract version implemented by this module.
pub const M4_INTERFACES_VERSION: &str = "m4-interfaces/v2";
/// Additive M5 interface contract version (`M4-INTERFACES-v3.md`).
pub const M4_INTERFACES_VERSION_V3: &str = "m4-interfaces/v3";
/// Sampler identifier recorded by learned pickers and builders (D2a exact
/// integer apportionment).
pub const LEARNED_SAMPLER_ID: &str = "softmax-cdf-splitmix64-v2";
/// Sampler identifier recorded by uniform fixture pickers, builders and pilots.
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

fn uniform_index_recorded(stream: &mut SplitMix64, bound: usize) -> (usize, Vec<u64>) {
    let bound_u64 = bound as u64;
    let threshold = bound_u64.wrapping_neg() % bound_u64;
    let mut values = Vec::with_capacity(1);
    loop {
        let value = stream.next_u64();
        values.push(value);
        if value >= threshold {
            return ((value % bound_u64) as usize, values);
        }
    }
}

fn uniform_decision_row(
    kind: DecisionKind,
    options: Vec<u16>,
    chosen: u16,
    stream_values: Vec<u64>,
) -> DecisionRow {
    DecisionRow {
        kind,
        sampler: UNIFORM_SAMPLER_ID.to_owned(),
        options,
        chosen,
        stream_values,
        logits_bits: None,
        p_chosen_bits: None,
        value_bits: None,
    }
}

// ---------------------------------------------------------------------------
// Picker interface (M4 interfaces section 1)
// ---------------------------------------------------------------------------

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

/// Test fixture and reference baseline only: picks uniformly over `view.pack`
/// on every pick (forced picks included) and emits `row: None`.
#[derive(Debug, Clone, Copy, Default)]
pub struct UniformSeatPicker;

impl SeatPicker for UniformSeatPicker {
    fn pick(
        &mut self,
        view: &SeatPickView<'_>,
        stream: &mut SplitMix64,
    ) -> Result<PickOutcome, SeatError> {
        if view.pack.is_empty() {
            return Err(SeatError::Seat("cannot pick from an empty pack".to_owned()));
        }
        let index = uniform_index(stream, view.pack.len());
        Ok(PickOutcome { index, row: None })
    }
}

// ---------------------------------------------------------------------------
// Builder interface (M4 interfaces section 2)
// ---------------------------------------------------------------------------

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

/// Deck builder on the frozen `m4-interfaces/v2` contract.
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

/// Test fixture and reference baseline only: uniform four-step deck builder
/// (`count`, `pointer`, `type_set`, `split`) with full [`BuildRows`].
#[derive(Debug, Clone, Copy, Default)]
pub struct UniformFourStepBuilder;

impl SeatBuilder for UniformFourStepBuilder {
    fn build(&mut self, pool: &[u16], stream: &mut SplitMix64) -> Result<BuildOutcome, SeatError> {
        let deck_size = DECK_CARDS as usize;
        if pool.len() < deck_size {
            return Err(SeatError::Seat(format!(
                "pool has {} cards, need at least {deck_size}",
                pool.len()
            )));
        }

        // Step 1: basics count B in 0..=40.
        let count_options: Vec<u16> = (0..=DECK_CARDS as u16).collect();
        let (count_idx, count_stream_values) = uniform_index_recorded(stream, count_options.len());
        let basics_count = count_options[count_idx] as usize;
        let count_row = uniform_decision_row(
            DecisionKind::Count,
            count_options,
            basics_count as u16,
            count_stream_values,
        );

        // Step 2: 40 - B non-basic picks without replacement from remaining pool cards
        // in ascending registry id order.
        let nonbasic_count = deck_size - basics_count;
        let mut remaining = pool.to_vec();
        remaining.sort_unstable();
        let mut cards = Vec::with_capacity(deck_size);
        let mut pointer_rows = Vec::with_capacity(nonbasic_count);
        for _ in 0..nonbasic_count {
            let options = remaining.clone();
            let (idx, stream_values) = uniform_index_recorded(stream, options.len());
            let chosen = remaining.remove(idx);
            cards.push(chosen);
            pointer_rows.push(uniform_decision_row(
                DecisionKind::Pointer,
                options,
                chosen,
                stream_values,
            ));
        }

        // Steps 3 and 4: basic type set T (1 <= |T| <= B) and split of B - |T| extra basics.
        let (type_set_row, split_rows) = if basics_count == 0 {
            (None, Vec::new())
        } else {
            let basics = basic_land_ids()?;
            let mask_options: Vec<u16> = (1u16..=31)
                .filter(|mask| (mask.count_ones() as usize) <= basics_count)
                .collect();
            let (mask_idx, mask_stream_values) = uniform_index_recorded(stream, mask_options.len());
            let chosen_mask = mask_options[mask_idx];
            let type_set = uniform_decision_row(
                DecisionKind::TypeSet,
                mask_options,
                chosen_mask,
                mask_stream_values,
            );

            let t_bits: Vec<u16> = (0u8..5)
                .map(|bit_index| 1u16 << bit_index)
                .filter(|&bit| chosen_mask & bit != 0)
                .collect();
            for &bit in &t_bits {
                cards.push(basics[bit.trailing_zeros() as usize]);
            }
            let extra_basics = basics_count - t_bits.len();
            let mut split = Vec::new();
            if t_bits.len() == 1 {
                let single_basic = basics[t_bits[0].trailing_zeros() as usize];
                cards.extend(std::iter::repeat_n(single_basic, extra_basics));
            } else {
                split.reserve(extra_basics);
                for _ in 0..extra_basics {
                    let options = t_bits.clone();
                    let (bit_idx, bit_stream_values) =
                        uniform_index_recorded(stream, options.len());
                    let chosen_bit = options[bit_idx];
                    cards.push(basics[chosen_bit.trailing_zeros() as usize]);
                    split.push(uniform_decision_row(
                        DecisionKind::Split,
                        options,
                        chosen_bit,
                        bit_stream_values,
                    ));
                }
            }
            (Some(type_set), split)
        };

        let deck = canonical_deck(&cards);
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

/// Test fixture and reference baseline only: wraps
/// [`StatedRuleDeckBuilder::try_build`], canonicalizes its deck rows, draws
/// nothing from `stream` and emits `rows: None`.
#[derive(Debug, Clone, Copy, Default)]
pub struct StatedSeatBuilder;

impl SeatBuilder for StatedSeatBuilder {
    fn build(&mut self, pool: &[u16], _stream: &mut SplitMix64) -> Result<BuildOutcome, SeatError> {
        let raw = StatedRuleDeckBuilder.try_build(pool)?;
        let deck = canonical_deck(&raw.card_ids());
        deck.validate(pool)?;
        Ok(BuildOutcome { deck, rows: None })
    }
}

// ---------------------------------------------------------------------------
// Play hook (M4 interfaces section 5)
// ---------------------------------------------------------------------------

/// Specification of one game attempt.
#[derive(Debug, Clone, Copy)]
pub struct AttemptSpec<'a> {
    pub decks: [&'a Deck; 2],
    pub seats: [usize; 2],
    pub episode: u64,
    pub env_seed: u64,
    pub action_seed: u64,
}

/// Per-source-card and null-source offered and chosen counts gathered on a
/// preselected logical-ID sample of games (`m4-interfaces/v3` r2 section 4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotSampleCounts {
    pub decisions: u64,
    #[serde(default)]
    pub refused_decisions: u64,
    #[serde(default)]
    pub offered_decisions_null: u64,
    #[serde(default)]
    pub offered_actions_null: u64,
    #[serde(default)]
    pub chosen_null: u64,
    pub offered_decisions_by_card: BTreeMap<u16, u64>,
    pub offered_actions_by_card: BTreeMap<u16, u64>,
    pub chosen_by_card: BTreeMap<u16, u64>,
}

impl PilotSampleCounts {
    /// Adds `other`'s counts into `self`.
    pub fn merge_from(&mut self, other: &Self) {
        self.decisions = self.decisions.saturating_add(other.decisions);
        self.refused_decisions = self
            .refused_decisions
            .saturating_add(other.refused_decisions);
        self.offered_decisions_null = self
            .offered_decisions_null
            .saturating_add(other.offered_decisions_null);
        self.offered_actions_null = self
            .offered_actions_null
            .saturating_add(other.offered_actions_null);
        self.chosen_null = self.chosen_null.saturating_add(other.chosen_null);
        for (&card_id, &count) in &other.offered_decisions_by_card {
            *self.offered_decisions_by_card.entry(card_id).or_insert(0) += count;
        }
        for (&card_id, &count) in &other.offered_actions_by_card {
            *self.offered_actions_by_card.entry(card_id).or_insert(0) += count;
        }
        for (&card_id, &count) in &other.chosen_by_card {
            *self.chosen_by_card.entry(card_id).or_insert(0) += count;
        }
    }
}

/// Result of one game attempt returned by a [`GamePilot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptOutcome {
    pub decisions: u64,
    pub physical_decisions: Option<u64>,
    pub policy_steps: Option<u64>,
    pub end: GameEnd,
    pub last_action_source: Option<u16>,
    /// Actual starting seat in `AttemptSpec::seats` coordinates (`None` if the
    /// session errored before starting).
    pub first_player_seat: Option<usize>,
    /// Actual seat in `AttemptSpec::seats` coordinates that submitted the final
    /// decision (`None` when nobody acted, i.e., `decisions == 0`).
    pub last_acting_seat: Option<usize>,
    /// Optional pilot-path offered/chosen counts when sampled.
    pub pilot_sample: Option<PilotSampleCounts>,
}

/// Plays one game attempt under explicit caps.
pub trait GamePilot {
    fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome;

    /// Plays one game attempt while optionally recording per-source-card
    /// offered and chosen counts for a preselected logical-ID sample.
    /// Defaults to [`GamePilot::play`].
    fn play_sampled(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        self.play(spec, caps)
    }
}

/// Test fixture and reference baseline only: plays uniform-random legal
/// actions through the Limited session, matching `drive_game`.
#[derive(Debug, Clone, Copy, Default)]
pub struct UniformPilot;

impl GamePilot for UniformPilot {
    fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        uniform_attempt(spec, caps, false, false)
    }

    fn play_sampled(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        uniform_attempt(spec, caps, false, true)
    }
}

/// Uniform policy with an in-process Limited transport for trusted Cube decks.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypedUniformPilot;

impl GamePilot for TypedUniformPilot {
    fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        uniform_attempt(spec, caps, true, false)
    }

    fn play_sampled(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        uniform_attempt(spec, caps, true, true)
    }
}

fn uniform_attempt(
    spec: &AttemptSpec<'_>,
    caps: &GameCaps,
    typed: bool,
    sample_pilot_path: bool,
) -> AttemptOutcome {
    let (decisions, end, tally) = drive_uniform_attempt_with_options(
        spec.decks,
        spec.seats,
        spec.episode,
        spec.env_seed,
        spec.action_seed,
        caps,
        typed,
        sample_pilot_path,
    );
    AttemptOutcome {
        decisions,
        physical_decisions: tally.physical_decisions,
        policy_steps: tally.policy_steps,
        end,
        last_action_source: tally.last_action_source,
        first_player_seat: tally.first_player_seat,
        last_acting_seat: if decisions == 0 {
            None
        } else {
            tally.last_acting_seat
        },
        pilot_sample: tally.pilot_sample,
    }
}
