//! Exact D0.21 owner-view sample plan execution and seed derivations.
//!
//! No game is played. Every draft, pick, pool and build shown on a page is a
//! pure function of its checkpoint, the ordered pool and its `owner-view`
//! seeds. No page reads or depends on a dose read or on another page.

use crate::card_def::{CardType, CARD_DEFS};
use crate::cube_draft_v1::{
    basic_land_ids, card_colour_bits, deck_seed, derive_seed, generate_packs, picker_seed,
    CubeDraftError, CubePool, Deck, SeedStream, BASIC_LAND_NAMES, DECK_CARDS, PACK_SIZE,
    POOL_CARDS, SEATS,
};
use crate::cube_owner_view_v1::checkpoint::{
    format_head_training_label, head_updates_entries, segment_name, CheckpointHeader,
    OwnerViewSeatFactory, Segment,
};
use crate::cube_seat_v1::{
    run_seat_draft, BuildRows, DecisionRow, PassDirection, SeatError, SeatPickView, SeatPicker,
    UniformSeatPicker, M4_INTERFACES_VERSION,
};
use crate::m4_sampler_v1::{counts_v1, GRID_SIZE, SAMPLER_ID_V1};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Signed namespace for owner views (`m4-interfaces/v2` section 4, D0.13, D0.21).
pub const OWNER_VIEW_NAMESPACE: &str = "owner-view";

/// The six independent shakedown owner-view run ids (D0.13, D0.21).
pub const SHAKEDOWN_OWNER_VIEW_RUNS: [&str; 6] = [
    "b1-u0032", "b1-u0064", "b1-u0096", "b1-u0128", "k1-u0064", "k1-u0128",
];

/// Drafts 0 to 99 inclusive: 100 drafts x 8 chairs = 800 fresh 15-card packs.
pub const FIRST_PICK_DRAFT_START: u64 = 0;
pub const FIRST_PICK_DRAFT_COUNT: u64 = 100;
pub const FIRST_PICK_PACK_COUNT: usize = (FIRST_PICK_DRAFT_COUNT as usize) * SEATS;
/// Cards appearing in fewer than 5 of the 800 first-pick packs are listed apart.
pub const FIRST_PICK_MIN_PACKS: usize = 5;

/// 20 sample picks, from drafts 100 to 119.
pub const SAMPLE_PICK_DRAFT_START: u64 = 100;
pub const SAMPLE_PICK_COUNT: usize = 20;

/// 20 sample pools and builds, from drafts 120 to 139, build stream k = 0.
pub const SAMPLE_POOL_DRAFT_START: u64 = 120;
pub const SAMPLE_POOL_COUNT: usize = 20;
pub const SAMPLE_POOL_BUILD_K: u64 = 0;

/// Distribution drafts 200 to 299 inclusive: 100 drafts x 8 chairs x 4 streams = 3,200 builds.
pub const DIST_DRAFT_START: u64 = 200;
pub const DIST_DRAFT_COUNT: u64 = 100;
pub const DIST_BUILDS_PER_CHAIR: u64 = 4;
pub const DIST_TOTAL_BUILDS: usize =
    (DIST_DRAFT_COUNT as usize) * SEATS * (DIST_BUILDS_PER_CHAIR as usize);

/// Verbatim shakedown label required by D0.21 on every shakedown page.
pub const SHAKEDOWN_LABEL: &str =
    "Shakedown, under random play: the pilot plays uniformly at random. Weights are discarded.";

/// Verbatim picks label required by D0.21 on `b1` pages.
pub const UNIFORM_PICKS_LABEL: &str =
    "rendered with the uniform fixture: every pick is uniform over the pack";

/// Verbatim builds label required by D0.21 on every page.
pub const SAMPLED_BUILDS_LABEL: &str = "sampled builds";

/// Verbatim law-boundary notice from D0.21 and design note section 12a.
pub const LAW_BOUNDARY_NOTICE: &str = "The owner's reactions are a go, no-go or bug signal only. They never label cards, enter training data, tune a constant or select a checkpoint.";

const SEED_MASK_63: u64 = (1u64 << 63) - 1;

/// Errors raised while running the owner-view sample plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerViewError {
    InvalidRunId(String),
    UndeclaredRunId(String),
    PickerModeMismatch {
        run_id: String,
        expected: PickerMode,
        got: PickerMode,
    },
    CheckpointMismatch {
        run_id: String,
        expected_segment: Segment,
        expected_training_run: String,
        expected_update: u64,
        got_segment: Segment,
        got_training_run: String,
        got_update: u64,
    },
    InvalidCheckpoint(String),
    InvalidPool(String),
    Draft(CubeDraftError),
    Seat(SeatError),
    SamplerMismatch(String),
}

impl fmt::Display for OwnerViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRunId(id) => write!(
                f,
                "invalid run id {id:?}: expected non-empty [a-z0-9-]+ token"
            ),
            Self::UndeclaredRunId(id) => write!(
                f,
                "undeclared owner-view run id {id:?}: expected one of {:?}",
                SHAKEDOWN_OWNER_VIEW_RUNS
            ),
            Self::PickerModeMismatch {
                run_id,
                expected,
                got,
            } => write!(
                f,
                "picker mode mismatch for run {run_id:?}: expected {expected:?}, got {got:?}"
            ),
            Self::CheckpointMismatch {
                run_id,
                expected_segment,
                expected_training_run,
                expected_update,
                got_segment,
                got_training_run,
                got_update,
            } => write!(
                f,
                "checkpoint mismatch for run {run_id:?}: expected segment {:?} training run {expected_training_run:?} update {expected_update}, got segment {:?} training run {got_training_run:?} update {got_update}",
                segment_name(*expected_segment),
                segment_name(*got_segment)
            ),
            Self::InvalidCheckpoint(msg) => write!(f, "invalid checkpoint header: {msg}"),
            Self::InvalidPool(msg) => write!(f, "invalid ordered pool: {msg}"),
            Self::Draft(err) => write!(f, "draft error: {err}"),
            Self::Seat(err) => write!(f, "seat error: {err}"),
            Self::SamplerMismatch(msg) => write!(f, "sampler probability mismatch: {msg}"),
        }
    }
}

impl std::error::Error for OwnerViewError {}

impl From<CubeDraftError> for OwnerViewError {
    fn from(error: CubeDraftError) -> Self {
        Self::Draft(error)
    }
}

impl From<SeatError> for OwnerViewError {
    fn from(error: SeatError) -> Self {
        Self::Seat(error)
    }
}

/// Validates a `[a-z0-9-]+` namespace or run-id token (`m4-interfaces/v2` section 4).
pub fn is_valid_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// One element of a `seed(parts)` tuple (`tools/cube_v1/deal.py`, IF2 section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedPart<'a> {
    Str(&'a str),
    Int(u64),
}

/// Exact Rust port of `tools/cube_v1/deal.py::seed`:
/// SHA-256 of compact JSON array with `,` and `:` separators, first 8 bytes
/// little-endian, masked to 63 bits.
pub fn seed_deal_v1(parts: &[SeedPart<'_>]) -> Result<u64, OwnerViewError> {
    let mut compact = String::from("[");
    for (idx, part) in parts.iter().enumerate() {
        if idx > 0 {
            compact.push(',');
        }
        match part {
            SeedPart::Str(s) => {
                if !is_valid_token(s) {
                    return Err(OwnerViewError::InvalidRunId((*s).to_owned()));
                }
                compact.push('"');
                compact.push_str(s);
                compact.push('"');
            }
            SeedPart::Int(n) => {
                use std::fmt::Write;
                let _ = write!(&mut compact, "{n}");
            }
        }
    }
    compact.push(']');
    let digest = Sha256::digest(compact.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    Ok(u64::from_le_bytes(bytes) & SEED_MASK_63)
}

/// Draft `draft_index` seed in namespace `owner-view` (D0.21):
/// `seed(["owner-view", "draft", run_id, draft_index])`.
pub fn owner_view_draft_seed(run_id: &str, draft_index: u64) -> Result<u64, OwnerViewError> {
    if !is_valid_token(run_id) {
        return Err(OwnerViewError::InvalidRunId(run_id.to_owned()));
    }
    seed_deal_v1(&[
        SeedPart::Str(OWNER_VIEW_NAMESPACE),
        SeedPart::Str("draft"),
        SeedPart::Str(run_id),
        SeedPart::Int(draft_index),
    ])
}

/// Build stream seed for chair `chair`, build `build_k` (D0.12, IF2 section 2):
/// `derive_seed(deck_seed(draft_seed, chair), SeedStream::DeckBuild, build_k)`.
pub fn owner_view_build_seed(draft_seed: u64, chair: usize, build_k: u64) -> u64 {
    derive_seed(deck_seed(draft_seed, chair), SeedStream::DeckBuild, build_k)
}

/// Builds a [`CubePool`] whose card order is the exact ascending registry id
/// order of `ordered_ids` (D0.4), together with its canonical JSON-array-plus-LF
/// sha256 digest.
///
/// Refuses non-ascending, duplicate, out-of-range or too-small pools.
pub fn cube_pool_from_ordered_ids(
    ordered_ids: &[u16],
) -> Result<(CubePool, String), OwnerViewError> {
    if ordered_ids.len() < SEATS * POOL_CARDS {
        return Err(OwnerViewError::InvalidPool(format!(
            "ordered pool has {} cards, need at least {}",
            ordered_ids.len(),
            SEATS * POOL_CARDS
        )));
    }
    for window in ordered_ids.windows(2) {
        if window[0] >= window[1] {
            return Err(OwnerViewError::InvalidPool(format!(
                "ordered pool ids must be strictly ascending: {} then {}",
                window[0], window[1]
            )));
        }
    }
    let mut cards_json = Vec::with_capacity(ordered_ids.len());
    for &id in ordered_ids {
        let def = CARD_DEFS
            .get(usize::from(id))
            .ok_or_else(|| OwnerViewError::InvalidPool(format!("registry id {id} out of range")))?;
        cards_json.push(serde_json::json!({ "name": def.name }));
    }
    let cube_list = serde_json::json!({ "cards": cards_json }).to_string();
    let pool = CubePool::from_json_texts(&cube_list, "{}", None)?;
    if pool.ids() != ordered_ids {
        return Err(OwnerViewError::InvalidPool(
            "constructed CubePool ids did not match ascending ordered_ids".to_owned(),
        ));
    }
    let sha = ordered_pool_sha256(ordered_ids);
    Ok((pool, sha))
}

/// Canonical sha256 of the ordered pool JSON array (`[id0,id1,...]\n`, D0.4).
pub fn ordered_pool_sha256(ordered_ids: &[u16]) -> String {
    let mut text = serde_json::to_string(ordered_ids).expect("u16 slice serializes");
    text.push('\n');
    let digest = Sha256::digest(text.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

/// Whether picks on this page come from `UniformSeatPicker` (`b1` pages) or
/// from the checkpoint's learned pick head (`k1` pages) per D0.21.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickerMode {
    UniformFixture,
    LearnedPickHead,
}

/// Declared mapping from each of the six D0.13 / D0.21 shakedown owner-view
/// run ids to its expected checkpoint segment, training run id, checkpoint
/// update, and picker mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerViewPageSpec {
    pub run_id: &'static str,
    pub expected_segment: Segment,
    pub expected_training_run: &'static str,
    pub expected_update: u64,
    pub picker_mode: PickerMode,
}

/// Explicit table of the six declared D16 shakedown pages (D0.13, D0.21,
/// design note 12a).
pub const SHAKEDOWN_PAGE_SPECS: [OwnerViewPageSpec; 6] = [
    OwnerViewPageSpec {
        run_id: "b1-u0032",
        expected_segment: Segment::BuilderOnly,
        expected_training_run: "b1",
        expected_update: 32,
        picker_mode: PickerMode::UniformFixture,
    },
    OwnerViewPageSpec {
        run_id: "b1-u0064",
        expected_segment: Segment::BuilderOnly,
        expected_training_run: "b1",
        expected_update: 64,
        picker_mode: PickerMode::UniformFixture,
    },
    OwnerViewPageSpec {
        run_id: "b1-u0096",
        expected_segment: Segment::BuilderOnly,
        expected_training_run: "b1",
        expected_update: 96,
        picker_mode: PickerMode::UniformFixture,
    },
    OwnerViewPageSpec {
        run_id: "b1-u0128",
        expected_segment: Segment::BuilderOnly,
        expected_training_run: "b1",
        expected_update: 128,
        picker_mode: PickerMode::UniformFixture,
    },
    OwnerViewPageSpec {
        run_id: "k1-u0064",
        expected_segment: Segment::PickPhase,
        expected_training_run: "k1",
        expected_update: 64,
        picker_mode: PickerMode::LearnedPickHead,
    },
    OwnerViewPageSpec {
        run_id: "k1-u0128",
        expected_segment: Segment::PickPhase,
        expected_training_run: "k1",
        expected_update: 128,
        picker_mode: PickerMode::LearnedPickHead,
    },
];

/// Resolves a declared D16 run id to its expected segment, update and picker
/// mode. Any undeclared run id is refused.
pub fn resolve_page_spec(run_id: &str) -> Result<OwnerViewPageSpec, OwnerViewError> {
    if !is_valid_token(run_id) {
        return Err(OwnerViewError::InvalidRunId(run_id.to_owned()));
    }
    SHAKEDOWN_PAGE_SPECS
        .iter()
        .copied()
        .find(|spec| spec.run_id == run_id)
        .ok_or_else(|| OwnerViewError::UndeclaredRunId(run_id.to_owned()))
}

impl PickerMode {
    /// Selects the D0.21 picker mode for one of the six declared shakedown run
    /// ids via [`resolve_page_spec`].
    pub fn for_run_id(run_id: &str) -> Result<Self, OwnerViewError> {
        resolve_page_spec(run_id).map(|spec| spec.picker_mode)
    }
}

/// Specification of one sample pick slot `m` (`0..20`) per D0.21 item 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplePickSpec {
    pub index: usize,
    pub draft_index: u64,
    pub chair: usize,
    pub raw_step: usize,
    pub step: usize,
    pub round: usize,
    pub pick: usize,
    pub adjusted_from_forced: bool,
}

/// Computes D0.21 item 2's exact schedule for pick `m` (`0..20`), including
/// lead ruling L4: when `(7m + 3) mod 45` is pick 14 of its round (one-card
/// pack), step is lowered by 1 (only `m = 8` changes, from step 14 to step 13).
pub fn sample_pick_spec(m: usize) -> SamplePickSpec {
    assert!(
        m < SAMPLE_PICK_COUNT,
        "sample pick index m={m} out of range"
    );
    let raw_step = (7 * m + 3) % POOL_CARDS;
    let adjusted_from_forced = raw_step % PACK_SIZE == PACK_SIZE - 1;
    let step = if adjusted_from_forced {
        raw_step - 1
    } else {
        raw_step
    };
    SamplePickSpec {
        index: m,
        draft_index: SAMPLE_PICK_DRAFT_START + (m as u64),
        chair: m % SEATS,
        raw_step,
        step,
        round: step / PACK_SIZE,
        pick: step % PACK_SIZE,
        adjusted_from_forced,
    }
}

/// Specification of one sample pool slot `m` (`0..20`) per D0.21 item 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplePoolSpec {
    pub index: usize,
    pub draft_index: u64,
    pub chair: usize,
    pub build_k: u64,
}

/// Computes D0.21 item 3's exact schedule for pool `m` (`0..20`).
pub fn sample_pool_spec(m: usize) -> SamplePoolSpec {
    assert!(
        m < SAMPLE_POOL_COUNT,
        "sample pool index m={m} out of range"
    );
    SamplePoolSpec {
        index: m,
        draft_index: SAMPLE_POOL_DRAFT_START + (m as u64),
        chair: m % SEATS,
        build_k: SAMPLE_POOL_BUILD_K,
    }
}

/// Compact display metadata for one card from `CARD_DEFS`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSummary {
    pub id: u16,
    pub name: String,
    pub colors: String,
    pub types: String,
}

pub fn summarize_card(id: u16) -> CardSummary {
    let def = &CARD_DEFS[usize::from(id)];
    let bits = card_colour_bits(id);
    let mut colors = String::new();
    for (idx, letter) in ['W', 'U', 'B', 'R', 'G'].into_iter().enumerate() {
        if bits & (1u8 << idx) != 0 {
            colors.push(letter);
        }
    }
    if colors.is_empty() {
        colors.push('C');
    }
    let types = def
        .types
        .iter()
        .map(|t| match t {
            CardType::Land => "Land",
            CardType::Creature => "Creature",
            CardType::Instant => "Instant",
            CardType::Sorcery => "Sorcery",
            CardType::Artifact => "Artifact",
            CardType::Enchantment => "Enchantment",
            CardType::Planeswalker => "Planeswalker",
            CardType::Kindred => "Kindred",
        })
        .collect::<Vec<_>>()
        .join(" ");
    CardSummary {
        id,
        name: def.name.to_owned(),
        colors,
        types,
    }
}

/// One row of the first-pick ranking (D0.21 item 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FirstPickRankRow {
    pub rank: usize,
    pub card: CardSummary,
    pub pack_count: usize,
    /// Mean probability over packs holding this card, or `None` if `pack_count == 0`.
    pub mean_probability: Option<f64>,
    /// Exact numerator sum of `q_i` (out of `2^53`) across all packs holding the
    /// card for learned pickers; `None` for uniform fixture.
    pub exact_count_sum: Option<u128>,
}

/// One option in a sample pick (D0.21 item 2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SamplePickOption {
    pub card: CardSummary,
    pub probability: f64,
    pub chosen: bool,
}

/// One rendered sample pick (D0.21 item 2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SamplePickEntry {
    pub spec: SamplePickSpec,
    pub draft_seed: u64,
    pub value_before_pick: Option<f32>,
    pub pool_so_far: Vec<CardSummary>,
    pub options: Vec<SamplePickOption>,
    pub chosen_card: CardSummary,
}

/// One rendered sample pool and its sampled build (D0.21 item 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplePoolEntry {
    pub spec: SamplePoolSpec,
    pub draft_seed: u64,
    pub build_seed: u64,
    pub basics_count: u16,
    pub type_set_mask: u16,
    pub type_set_size: u8,
    pub basic_counts: [u16; 5],
    pub pool_cards: Vec<CardSummary>,
    pub included_pool_cards: Vec<CardSummary>,
    pub excluded_pool_cards: Vec<CardSummary>,
    pub deck_cards: Vec<CardSummary>,
}

/// Distribution counts of `B` (`0..=40`) and `|T|` (`0..=5`) over 3,200 builds
/// (D0.21 item 4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildDistributions {
    pub total_builds: usize,
    pub basics_count_hist: Vec<usize>,
    pub type_set_size_hist: [usize; 6],
    pub mean_basics: f64,
    pub mean_type_set_size: f64,
}

/// Head training-state display row derived from `checkpoint.head_updates`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadLabelRow {
    pub head_key: String,
    pub display_name: String,
    pub updates: u64,
    pub label: String,
}

/// Complete, self-contained deterministic data behind one owner-view page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnerViewPageData {
    pub namespace: String,
    pub run_id: String,
    pub picker_mode: PickerMode,
    pub shakedown_label: String,
    pub picks_label: Option<String>,
    pub builds_label: String,
    pub law_boundary_notice: String,
    pub interfaces_version: String,
    pub sampler_id: String,
    pub used_stand_in_seats: bool,
    pub ordered_pool_sha256: String,
    pub ordered_pool_len: usize,
    pub checkpoint: CheckpointHeader,
    pub head_labels: Vec<HeadLabelRow>,
    pub first_pick_pack_count: usize,
    pub first_pick_ranked: Vec<FirstPickRankRow>,
    pub first_pick_low_count: Vec<FirstPickRankRow>,
    pub sample_picks: Vec<SamplePickEntry>,
    pub sample_pools: Vec<SamplePoolEntry>,
    pub distributions: BuildDistributions,
}

/// Extracts exact option probabilities from a learned pick [`DecisionRow`]
/// using `m4_sampler_v1::counts_v1` and verifies bit-exact equality with
/// `row.p_chosen_bits` (`q_c / 2^53`).
pub fn learned_row_option_counts(row: &DecisionRow) -> Result<Vec<u64>, OwnerViewError> {
    if row.sampler != SAMPLER_ID_V1 {
        return Err(OwnerViewError::SamplerMismatch(format!(
            "expected sampler {SAMPLER_ID_V1:?}, got {:?}",
            row.sampler
        )));
    }
    let logits_bits = row.logits_bits.as_ref().ok_or_else(|| {
        OwnerViewError::SamplerMismatch("learned DecisionRow lacks logits_bits".to_owned())
    })?;
    if logits_bits.len() != row.options.len() {
        return Err(OwnerViewError::SamplerMismatch(format!(
            "logits_bits length {} != options length {}",
            logits_bits.len(),
            row.options.len()
        )));
    }
    let logits: Vec<f32> = logits_bits.iter().map(|&b| f32::from_bits(b)).collect();
    let mask = vec![true; logits.len()];
    let counts = counts_v1(&logits, &mask)
        .map_err(|err| OwnerViewError::SamplerMismatch(format!("counts_v1 failed: {err}")))?;
    let chosen_idx = row.options.binary_search(&row.chosen).map_err(|_| {
        OwnerViewError::SamplerMismatch(format!(
            "chosen card {} missing from canonical options",
            row.chosen
        ))
    })?;
    let expected_p_bits = ((counts[chosen_idx] as f64) / (GRID_SIZE as f64)).to_bits();
    let recorded_p_bits = row.p_chosen_bits.ok_or_else(|| {
        OwnerViewError::SamplerMismatch("learned DecisionRow lacks p_chosen_bits".to_owned())
    })?;
    if expected_p_bits != recorded_p_bits {
        return Err(OwnerViewError::SamplerMismatch(format!(
            "p_chosen_bits mismatch: counts_v1 gave {expected_p_bits:#018x}, row recorded {recorded_p_bits:#018x}"
        )));
    }
    Ok(counts)
}

fn make_pickers(
    picker_mode: PickerMode,
    factory: &dyn OwnerViewSeatFactory,
) -> [Box<dyn SeatPicker>; SEATS] {
    std::array::from_fn(|_| match picker_mode {
        PickerMode::UniformFixture => Box::new(UniformSeatPicker) as Box<dyn SeatPicker>,
        PickerMode::LearnedPickHead => factory.make_learned_picker(),
    })
}

fn extract_basics_and_types(
    deck: &Deck,
    rows: Option<&BuildRows>,
    basic_ids: &[u16; 5],
) -> Result<(u16, u16, u8, [u16; 5]), OwnerViewError> {
    let mut basic_counts = [0u16; 5];
    let mut total_basics = 0u16;
    for entry in &deck.entries {
        if let Some(pos) = basic_ids.iter().position(|&id| id == entry.id) {
            let count_u16 = entry.count as u16;
            basic_counts[pos] += count_u16;
            total_basics += count_u16;
        }
    }
    let mut deck_mask = 0u16;
    for (idx, &count) in basic_counts.iter().enumerate() {
        if count > 0 {
            deck_mask |= 1u16 << idx;
        }
    }
    let deck_type_count = deck_mask.count_ones() as u8;

    if let Some(build_rows) = rows {
        if build_rows.count.chosen != total_basics {
            return Err(OwnerViewError::SamplerMismatch(format!(
                "BuildRows count.chosen={} disagrees with deck basic count {total_basics}",
                build_rows.count.chosen
            )));
        }
        let row_mask = build_rows
            .type_set
            .as_ref()
            .map(|r| r.chosen)
            .unwrap_or(0u16);
        if row_mask != deck_mask {
            return Err(OwnerViewError::SamplerMismatch(format!(
                "BuildRows type_set mask {row_mask} disagrees with deck basic mask {deck_mask}"
            )));
        }
    }
    Ok((total_basics, deck_mask, deck_type_count, basic_counts))
}

/// Runs the complete D0.21 sample plan for one independent owner-view page.
///
/// Resolves `run_id` through [`resolve_page_spec`], refuses any mismatch in
/// `picker_mode`, checkpoint segment, checkpoint update, checkpoint header
/// format, or pinned ordered pool hash, and plays zero games. Never reads a
/// dose read or another owner-view page.
pub fn generate_owner_view_data(
    run_id: &str,
    picker_mode: PickerMode,
    pool: &CubePool,
    ordered_pool_sha256_expected: &str,
    checkpoint: &CheckpointHeader,
    factory: &dyn OwnerViewSeatFactory,
) -> Result<OwnerViewPageData, OwnerViewError> {
    let page_spec = resolve_page_spec(run_id)?;
    if picker_mode != page_spec.picker_mode {
        return Err(OwnerViewError::PickerModeMismatch {
            run_id: run_id.to_owned(),
            expected: page_spec.picker_mode,
            got: picker_mode,
        });
    }
    checkpoint
        .validate()
        .map_err(|err| OwnerViewError::InvalidCheckpoint(err.to_string()))?;
    if checkpoint.segment != page_spec.expected_segment
        || checkpoint.run != page_spec.expected_training_run
        || checkpoint.update != page_spec.expected_update
    {
        return Err(OwnerViewError::CheckpointMismatch {
            run_id: run_id.to_owned(),
            expected_segment: page_spec.expected_segment,
            expected_training_run: page_spec.expected_training_run.to_owned(),
            expected_update: page_spec.expected_update,
            got_segment: checkpoint.segment,
            got_training_run: checkpoint.run.clone(),
            got_update: checkpoint.update,
        });
    }
    let pool_ids = pool.ids();
    if pool_ids.len() < SEATS * POOL_CARDS {
        return Err(OwnerViewError::InvalidPool(format!(
            "ordered pool has {} cards, need at least {}",
            pool_ids.len(),
            SEATS * POOL_CARDS
        )));
    }
    for window in pool_ids.windows(2) {
        if window[0] >= window[1] {
            return Err(OwnerViewError::InvalidPool(format!(
                "ordered pool ids must be strictly ascending: {} then {}",
                window[0], window[1]
            )));
        }
    }
    let actual_pool_sha = ordered_pool_sha256(pool_ids);
    if actual_pool_sha != ordered_pool_sha256_expected {
        return Err(OwnerViewError::InvalidPool(format!(
            "ordered pool sha256 mismatch: expected {ordered_pool_sha256_expected:?}, computed {actual_pool_sha:?}"
        )));
    }
    let basic_ids = basic_land_ids()?;

    // Head labels directly from checkpoint.head_updates (IF2 change 2, D0.21).
    let head_labels: Vec<HeadLabelRow> = head_updates_entries(checkpoint.head_updates)
        .into_iter()
        .map(|(display_name, head_key, updates)| HeadLabelRow {
            head_key: head_key.to_owned(),
            display_name: display_name.to_owned(),
            updates,
            label: format_head_training_label(updates),
        })
        .collect();

    // -----------------------------------------------------------------------
    // 1. First-pick ranking: drafts 0..100, round 0 pick 0 (800 fresh packs)
    // -----------------------------------------------------------------------
    let mut pack_counts: BTreeMap<u16, usize> = BTreeMap::new();
    let mut exact_q_sums: BTreeMap<u16, u128> = BTreeMap::new();
    let mut learned_picker_r0p0 = if picker_mode == PickerMode::LearnedPickHead {
        Some(factory.make_learned_picker())
    } else {
        None
    };

    for draft_idx in FIRST_PICK_DRAFT_START..(FIRST_PICK_DRAFT_START + FIRST_PICK_DRAFT_COUNT) {
        let draft_seed = owner_view_draft_seed(run_id, draft_idx)?;
        let opened = generate_packs(pool, draft_seed)?;
        for (chair, pack) in opened[0].iter().enumerate() {
            debug_assert_eq!(pack.len(), PACK_SIZE);
            match picker_mode {
                PickerMode::UniformFixture => {
                    for &card_id in pack {
                        *pack_counts.entry(card_id).or_insert(0) += 1;
                    }
                }
                PickerMode::LearnedPickHead => {
                    let picker = learned_picker_r0p0
                        .as_mut()
                        .expect("learned picker initialized");
                    let mut stream = SplitMix64::seed(picker_seed(draft_seed, chair));
                    let view = SeatPickView {
                        round: 0,
                        pick: 0,
                        direction: PassDirection::Left,
                        pack,
                        pool: &[],
                        seen_not_taken: &[],
                    };
                    let outcome = picker.pick(&view, &mut stream)?;
                    let row = outcome.row.as_ref().ok_or_else(|| {
                        OwnerViewError::SamplerMismatch(
                            "learned picker returned row: None on 15-card first pick".to_owned(),
                        )
                    })?;
                    let counts = learned_row_option_counts(row)?;
                    for (&card_id, &q) in row.options.iter().zip(&counts) {
                        *pack_counts.entry(card_id).or_insert(0) += 1;
                        *exact_q_sums.entry(card_id).or_insert(0) += u128::from(q);
                    }
                }
            }
        }
    }

    let mut all_rank_rows: Vec<(u16, usize, Option<u128>, Option<f64>)> = Vec::new();
    for &card_id in pool.ids() {
        let count = pack_counts.get(&card_id).copied().unwrap_or(0);
        if count == 0 {
            all_rank_rows.push((card_id, 0, None, None));
        } else {
            match picker_mode {
                PickerMode::UniformFixture => {
                    all_rank_rows.push((card_id, count, None, Some(1.0f64 / (PACK_SIZE as f64))));
                }
                PickerMode::LearnedPickHead => {
                    let sum_q = exact_q_sums.get(&card_id).copied().unwrap_or(0);
                    let mean = (sum_q as f64) / ((count as f64) * (GRID_SIZE as f64));
                    all_rank_rows.push((card_id, count, Some(sum_q), Some(mean)));
                }
            }
        }
    }

    // Sort highest mean probability first, ties by registry id ascending.
    // For learned pickers, compare exact rational sums `sum_q_a / count_a` vs
    // `sum_q_b / count_b` by u128 cross-multiplication so floating-point
    // rounding can never disturb tie ordering.
    let cmp_rows = |a: &(u16, usize, Option<u128>, Option<f64>),
                    b: &(u16, usize, Option<u128>, Option<f64>)| {
        match (a.1 == 0, b.1 == 0) {
            (true, true) => a.0.cmp(&b.0),
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => match (a.2, b.2) {
                (Some(sa), Some(sb)) => {
                    let lhs = sb * (a.1 as u128);
                    let rhs = sa * (b.1 as u128);
                    lhs.cmp(&rhs).then_with(|| a.0.cmp(&b.0))
                }
                _ => a.0.cmp(&b.0),
            },
        }
    };

    let (mut high_rows, mut low_rows): (Vec<_>, Vec<_>) = all_rank_rows
        .into_iter()
        .partition(|entry| entry.1 >= FIRST_PICK_MIN_PACKS);
    high_rows.sort_by(cmp_rows);
    low_rows.sort_by(cmp_rows);

    let first_pick_ranked: Vec<FirstPickRankRow> = high_rows
        .into_iter()
        .enumerate()
        .map(
            |(idx, (id, pack_count, exact_count_sum, mean_probability))| FirstPickRankRow {
                rank: idx + 1,
                card: summarize_card(id),
                pack_count,
                mean_probability,
                exact_count_sum,
            },
        )
        .collect();
    let first_pick_low_count: Vec<FirstPickRankRow> = low_rows
        .into_iter()
        .enumerate()
        .map(
            |(idx, (id, pack_count, exact_count_sum, mean_probability))| FirstPickRankRow {
                rank: idx + 1,
                card: summarize_card(id),
                pack_count,
                mean_probability,
                exact_count_sum,
            },
        )
        .collect();

    // -----------------------------------------------------------------------
    // 2. 20 sample picks: drafts 100..120
    // -----------------------------------------------------------------------
    let mut sample_picks = Vec::with_capacity(SAMPLE_PICK_COUNT);
    for m in 0..SAMPLE_PICK_COUNT {
        let spec = sample_pick_spec(m);
        let draft_seed = owner_view_draft_seed(run_id, spec.draft_index)?;
        let mut pickers = make_pickers(picker_mode, factory);
        let seat_draft = run_seat_draft(pool, draft_seed, &mut pickers)?;

        let record_pick_idx = spec.step * SEATS + spec.chair;
        let pick_rec = &seat_draft.record.picks[record_pick_idx];
        debug_assert_eq!(pick_rec.seat, spec.chair);
        debug_assert_eq!(pick_rec.round, spec.round);
        debug_assert_eq!(pick_rec.pick, spec.pick);

        let pool_so_far: Vec<CardSummary> = seat_draft.record.pools[spec.chair][..spec.step]
            .iter()
            .map(|&id| summarize_card(id))
            .collect();

        let (options, value_before_pick) = match picker_mode {
            PickerMode::UniformFixture => {
                let mut sorted_pack = pick_rec.pack.clone();
                sorted_pack.sort_unstable();
                let p = 1.0f64 / (sorted_pack.len() as f64);
                let opts = sorted_pack
                    .into_iter()
                    .map(|id| SamplePickOption {
                        card: summarize_card(id),
                        probability: p,
                        chosen: id == pick_rec.chosen,
                    })
                    .collect();
                (opts, None)
            }
            PickerMode::LearnedPickHead => {
                let row_idx = spec.round * (PACK_SIZE - 1) + spec.pick;
                let row = &seat_draft.pick_rows[spec.chair][row_idx];
                debug_assert_eq!(row.chosen, pick_rec.chosen);
                let counts = learned_row_option_counts(row)?;
                let mut opts: Vec<SamplePickOption> = row
                    .options
                    .iter()
                    .zip(&counts)
                    .map(|(&id, &q)| SamplePickOption {
                        card: summarize_card(id),
                        probability: (q as f64) / (GRID_SIZE as f64),
                        chosen: id == row.chosen,
                    })
                    .collect();
                // Sort sample pick options by probability descending, ties by card id ascending
                // for readable inspection while preserving exact probabilities.
                opts.sort_by(|a, b| {
                    b.probability
                        .total_cmp(&a.probability)
                        .then_with(|| a.card.id.cmp(&b.card.id))
                });
                (opts, row.value_bits.map(f32::from_bits))
            }
        };

        sample_picks.push(SamplePickEntry {
            spec,
            draft_seed,
            value_before_pick,
            pool_so_far,
            options,
            chosen_card: summarize_card(pick_rec.chosen),
        });
    }

    // -----------------------------------------------------------------------
    // 3. 20 sample pools and builds: drafts 120..140, chair m mod 8, k = 0
    // -----------------------------------------------------------------------
    let mut builder = factory.make_learned_builder();
    let mut sample_pools = Vec::with_capacity(SAMPLE_POOL_COUNT);
    for m in 0..SAMPLE_POOL_COUNT {
        let spec = sample_pool_spec(m);
        let draft_seed = owner_view_draft_seed(run_id, spec.draft_index)?;
        let mut pickers = make_pickers(picker_mode, factory);
        let seat_draft = run_seat_draft(pool, draft_seed, &mut pickers)?;
        let seat_pool = &seat_draft.record.pools[spec.chair];

        let build_seed = owner_view_build_seed(draft_seed, spec.chair, spec.build_k);
        let mut build_stream = SplitMix64::seed(build_seed);
        let build = builder.build(seat_pool, &mut build_stream)?;
        build.deck.validate(seat_pool)?;

        let (basics_count, type_set_mask, type_set_size, basic_counts) =
            extract_basics_and_types(&build.deck, build.rows.as_ref(), &basic_ids)?;

        let included_set: BTreeSet<u16> = build
            .deck
            .card_ids()
            .into_iter()
            .filter(|id| !basic_ids.contains(id))
            .collect();
        let pool_cards: Vec<CardSummary> = seat_pool.iter().map(|&id| summarize_card(id)).collect();
        let mut included_pool_cards: Vec<CardSummary> = seat_pool
            .iter()
            .copied()
            .filter(|id| included_set.contains(id))
            .map(summarize_card)
            .collect();
        included_pool_cards.sort_by_key(|c| c.id);
        let mut excluded_pool_cards: Vec<CardSummary> = seat_pool
            .iter()
            .copied()
            .filter(|id| !included_set.contains(id))
            .map(summarize_card)
            .collect();
        excluded_pool_cards.sort_by_key(|c| c.id);
        let deck_cards: Vec<CardSummary> = build
            .deck
            .card_ids()
            .into_iter()
            .map(summarize_card)
            .collect();

        sample_pools.push(SamplePoolEntry {
            spec,
            draft_seed,
            build_seed,
            basics_count,
            type_set_mask,
            type_set_size,
            basic_counts,
            pool_cards,
            included_pool_cards,
            excluded_pool_cards,
            deck_cards,
        });
    }

    // -----------------------------------------------------------------------
    // 4. Distributions: drafts 200..300, 8 chairs, k = 0..4 (3,200 builds)
    // -----------------------------------------------------------------------
    let mut basics_count_hist = vec![0usize; (DECK_CARDS as usize) + 1];
    let mut type_set_size_hist = [0usize; 6];
    let mut basics_sum = 0u64;
    let mut type_size_sum = 0u64;
    let mut total_builds = 0usize;

    for draft_idx in DIST_DRAFT_START..(DIST_DRAFT_START + DIST_DRAFT_COUNT) {
        let draft_seed = owner_view_draft_seed(run_id, draft_idx)?;
        let mut pickers = make_pickers(picker_mode, factory);
        let seat_draft = run_seat_draft(pool, draft_seed, &mut pickers)?;
        for chair in 0..SEATS {
            let seat_pool = &seat_draft.record.pools[chair];
            for build_k in 0..DIST_BUILDS_PER_CHAIR {
                let build_seed = owner_view_build_seed(draft_seed, chair, build_k);
                let mut build_stream = SplitMix64::seed(build_seed);
                let build = builder.build(seat_pool, &mut build_stream)?;
                build.deck.validate(seat_pool)?;
                let (basics_count, _mask, type_set_size, _basic_counts) =
                    extract_basics_and_types(&build.deck, build.rows.as_ref(), &basic_ids)?;
                basics_count_hist[usize::from(basics_count)] += 1;
                type_set_size_hist[usize::from(type_set_size)] += 1;
                basics_sum += u64::from(basics_count);
                type_size_sum += u64::from(type_set_size);
                total_builds += 1;
            }
        }
    }
    debug_assert_eq!(total_builds, DIST_TOTAL_BUILDS);
    debug_assert_eq!(BASIC_LAND_NAMES.len(), 5);

    let distributions = BuildDistributions {
        total_builds,
        basics_count_hist,
        type_set_size_hist,
        mean_basics: (basics_sum as f64) / (total_builds as f64),
        mean_type_set_size: (type_size_sum as f64) / (total_builds as f64),
    };

    let picks_label = match picker_mode {
        PickerMode::UniformFixture => Some(UNIFORM_PICKS_LABEL.to_owned()),
        PickerMode::LearnedPickHead => None,
    };

    Ok(OwnerViewPageData {
        namespace: OWNER_VIEW_NAMESPACE.to_owned(),
        run_id: run_id.to_owned(),
        picker_mode,
        shakedown_label: SHAKEDOWN_LABEL.to_owned(),
        picks_label,
        builds_label: SAMPLED_BUILDS_LABEL.to_owned(),
        law_boundary_notice: LAW_BOUNDARY_NOTICE.to_owned(),
        interfaces_version: M4_INTERFACES_VERSION.to_owned(),
        sampler_id: SAMPLER_ID_V1.to_owned(),
        used_stand_in_seats: factory.is_stand_in(),
        ordered_pool_sha256: actual_pool_sha,
        ordered_pool_len: pool.card_count(),
        checkpoint: checkpoint.clone(),
        head_labels,
        first_pick_pack_count: FIRST_PICK_PACK_COUNT,
        first_pick_ranked,
        first_pick_low_count,
        sample_picks,
        sample_pools,
        distributions,
    })
}
