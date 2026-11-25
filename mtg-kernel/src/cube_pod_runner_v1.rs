//! Cube pod runner (`mtg-kernel-cube-pod-record/v2`) on the frozen
//! `m4-interfaces/v2` seat and play contracts.
//!
//! Runs 8-chair training pods (6 learning chairs with paired sibling builds,
//! 2 older chairs with single builds, 28 coupled groups, 70 games), client
//! pods (`namespace = "client-pods"`, `use = "client"`), and explicit read
//! plans (`seat_rule: "schedule"`, `max_replays: 0`), with deterministic
//! per-game retries, halt ledger, stop reports, per-attempt watchdog
//! (`terminal:watchdog` on `GameEnd::Terminal`), thread-count-independent
//! assembly, schema verification and summary reporting.
//!
//! Project law: game results are the only signal; uniform seats and the stated
//! builder are test fixtures and baselines, never teachers or opponents in
//! training; halting is neither rewarded nor punished; the runner never drops
//! a pod and never publishes from a stopped training batch.

use crate::card_def::CARD_DEFS;
use crate::cube_draft_v1::{
    deck_seed, derive_seed, game_seed, pod_game_seats, pod_pairings, uniform_index, CubeDraftError,
    CubePool, Deck, DraftRecord, GameCaps, GameEnd, SeedStream, DECK_CARDS, POD_GAMES, SEATS,
};
use crate::cube_seat_v1::{
    canonical_deck, run_seat_draft, AttemptSpec, BuildRows, DecisionRow, GamePilot, SeatBuilder,
    SeatError, SeatPicker, StatedSeatBuilder, UniformFourStepBuilder, UniformSeatPicker,
    M4_INTERFACES_VERSION, UNIFORM_SAMPLER_ID,
};
use crate::cube_worker_liveness_v1::observe_decisions;
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Frozen schema tag of [`PodRecord`].
pub const POD_RECORD_SCHEMA: &str = "mtg-kernel-cube-pod-record/v2";
/// Reserved namespace for human-client pods (`m4-interfaces/v2` change 1).
pub const CLIENT_NAMESPACE: &str = "client-pods";
/// `GameEnd::Terminal` fields for a watchdog expiry (`READ-PLAN-v1` section 5).
pub const WATCHDOG_CODE: &str = "watchdog";
pub const WATCHDOG_CLASSIFICATION: &str = "watchdog";
pub const WATCHDOG_REASON: &str = "watchdog_seconds";
/// `stop_kind` recorded for a watchdog expiry.
pub const WATCHDOG_STOP_KIND: &str = "terminal:watchdog";
/// The A2.1 game caps: 16,384 driver decisions, 16,384 physical decisions,
/// 32,768 policy steps.
pub const A2_GAME_CAPS: GameCaps = crate::cube_b_census_v1::A2_CAPS;
pub(crate) const A2_MAX_STACK_OBJECTS: u32 = A2_GAME_CAPS
    .max_stack_objects
    .expect("A2.1 requires a stack cap");
/// Default limit in seconds between changes in a worker decision count.
pub const DEFAULT_WATCHDOG_SECONDS: u64 = 600;
/// Maximum per-game replays in a training pod (4 total attempts: 0..=3).
pub const TRAINING_MAX_REPLAYS: u8 = 3;
/// Maximum per-game replays in a read plan (reads never replay, A2.4).
pub const READ_MAX_REPLAYS: u8 = 0;
/// Learning chairs in a training pod.
pub const TRAINING_LEARNING_CHAIRS: usize = 6;
/// Older chairs in a training pod.
pub const TRAINING_OLDER_CHAIRS: usize = 2;
/// Games in a training pod: 15 * 3 + 12 * 2 + 1 = 70.
pub const TRAINING_POD_GAMES: usize = 70;

const SEED_MASK: u64 = (1u64 << 63) - 1;
const WORKER_STACK_BYTES: usize = 64 << 20;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors returned by the pod runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodRunnerError {
    /// A namespace, run or other identifier failed `[a-z0-9-]+` validation.
    InvalidToken { field: &'static str, value: String },
    /// A string passed to [`seed`] did not match `[a-z0-9-]+`.
    InvalidSeedPart(String),
    /// A training pod did not have 6 learning chairs and 2 older chairs.
    InvalidTrainingRoles {
        learning: usize,
        older: usize,
        fixture: usize,
    },
    /// Invalid caps configuration.
    InvalidCaps(String),
    /// Invalid read plan or layout configuration.
    InvalidPlan(String),
    /// A seat picker, seat builder or draft driver returned an error.
    Seat(SeatError),
    /// An underlying draft environment error.
    Draft(CubeDraftError),
    /// A pilot returned a natural winner seat that is neither seat of the game.
    InvalidWinnerSeat {
        winner_seat: usize,
        seats: [usize; 2],
    },
    /// A game exhausted its allowed retries, or a watchdog expiry stopped the
    /// pod. Contains the [`PodRecord`] with `status == PodStatus::Stopped` and
    /// `stop == Some(_)`; training callers must publish nothing from the batch.
    Stopped(Box<PodRecord>),
    /// A worker callback or supervisor persistence step failed.
    Io(String),
    /// Worker thread spawn or join failure.
    Thread(String),
}

impl fmt::Display for PodRunnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PodRunnerError::InvalidToken { field, value } => {
                write!(f, "{field} {value:?} does not match [a-z0-9-]+")
            }
            PodRunnerError::InvalidSeedPart(value) => {
                write!(f, "seed string {value:?} does not match [a-z0-9-]+")
            }
            PodRunnerError::InvalidTrainingRoles {
                learning,
                older,
                fixture,
            } => write!(
                f,
                "training pod needs {TRAINING_LEARNING_CHAIRS} learning and {TRAINING_OLDER_CHAIRS} older chairs, got {learning} learning, {older} older, {fixture} fixture"
            ),
            PodRunnerError::InvalidCaps(message) => write!(f, "invalid caps: {message}"),
            PodRunnerError::InvalidPlan(message) => write!(f, "invalid plan: {message}"),
            PodRunnerError::Seat(error) => write!(f, "{error}"),
            PodRunnerError::Draft(error) => write!(f, "{error}"),
            PodRunnerError::InvalidWinnerSeat { winner_seat, seats } => write!(
                f,
                "natural winner seat {winner_seat} is not in game seats {seats:?}"
            ),
            PodRunnerError::Stopped(record) => match &record.stop {
                Some(stop) => write!(
                    f,
                    "pod {} stopped at game {} (pairing {:?}, decks {:?}) after {} halted attempts",
                    record.pod_index,
                    stop.game,
                    stop.pairing,
                    stop.decks,
                    stop.attempts.len()
                ),
                None => write!(f, "pod {} stopped", record.pod_index),
            },
            PodRunnerError::Io(message) => write!(f, "I/O error: {message}"),
            PodRunnerError::Thread(message) => write!(f, "worker thread error: {message}"),
        }
    }
}

impl std::error::Error for PodRunnerError {}

impl From<SeatError> for PodRunnerError {
    fn from(error: SeatError) -> Self {
        PodRunnerError::Seat(error)
    }
}

impl From<CubeDraftError> for PodRunnerError {
    fn from(error: CubeDraftError) -> Self {
        PodRunnerError::Draft(error)
    }
}

// ---------------------------------------------------------------------------
// Seeds (`tools/cube_v1/deal.py` parity)
// ---------------------------------------------------------------------------

/// Returns `true` iff `text` is non-empty and matches `[a-z0-9-]+`.
pub fn is_valid_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Returns `true` iff `text` is non-empty and matches `[a-z0-9_-]+` (used for
/// dealer keys such as `"cube_deal_b_v1"`).
pub fn is_valid_seed_key_str(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
}

/// Returns `true` iff `text` is a 64-character lowercase hexadecimal SHA-256
/// digest string.
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// One element of a [`seed`] list: a `[a-z0-9-]+` string or a non-negative
/// integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedPart<'a> {
    Str(&'a str),
    Int(u64),
}

fn seed_with_validator(
    parts: &[SeedPart<'_>],
    valid_str: fn(&str) -> bool,
) -> Result<u64, PodRunnerError> {
    let mut json = String::from("[");
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        match part {
            SeedPart::Str(text) => {
                if !valid_str(text) {
                    return Err(PodRunnerError::InvalidSeedPart((*text).to_owned()));
                }
                json.push('"');
                json.push_str(text);
                json.push('"');
            }
            SeedPart::Int(value) => {
                write!(json, "{value}").expect("writing to a String");
            }
        }
    }
    json.push(']');
    let digest = Sha256::digest(json.as_bytes());
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    Ok(u64::from_le_bytes(first) & SEED_MASK)
}

/// Rust port of `tools/cube_v1/deal.py`'s `seed(parts)`: SHA-256 of compact
/// JSON (`json.dumps(parts, separators=(",", ":"))`), first 8 bytes
/// little-endian, masked to 63 bits. Accepts only strings matching
/// `[a-z0-9-]+` and non-negative integers.
pub fn seed(parts: &[SeedPart<'_>]) -> Result<u64, PodRunnerError> {
    seed_with_validator(parts, is_valid_token)
}

/// Same SHA-256 seed derivation as [`seed`], additionally allowing `_` in
/// string parts for dealer rule keys such as `"cube_deal_b_v1"`.
pub fn seed_keyed(parts: &[SeedPart<'_>]) -> Result<u64, PodRunnerError> {
    seed_with_validator(parts, is_valid_seed_key_str)
}

/// `seed([namespace, "draft", run, pod_index])`.
pub fn pod_draft_seed(namespace: &str, run: &str, pod_index: u64) -> Result<u64, PodRunnerError> {
    seed(&[
        SeedPart::Str(namespace),
        SeedPart::Str("draft"),
        SeedPart::Str(run),
        SeedPart::Int(pod_index),
    ])
}

/// `seed([namespace, "pod", run, pod_index])`.
pub fn pod_play_seed(namespace: &str, run: &str, pod_index: u64) -> Result<u64, PodRunnerError> {
    seed(&[
        SeedPart::Str(namespace),
        SeedPart::Str("pod"),
        SeedPart::Str(run),
        SeedPart::Int(pod_index),
    ])
}

/// `seed([namespace, "league", run, pod_index])`.
pub fn pod_league_seed(namespace: &str, run: &str, pod_index: u64) -> Result<u64, PodRunnerError> {
    seed(&[
        SeedPart::Str(namespace),
        SeedPart::Str("league"),
        SeedPart::Str(run),
        SeedPart::Int(pod_index),
    ])
}

/// Build stream seed for build `k` (`0` = sibling a / single build,
/// `1` = sibling b) of chair `chair`.
pub fn build_seed_for_chair(draft_seed: u64, chair: usize, build_k: u8) -> u64 {
    derive_seed(
        deck_seed(draft_seed, chair),
        SeedStream::DeckBuild,
        u64::from(build_k),
    )
}

/// Returns `(deal_seed, env_seed, action_seed)` for `attempt` of a game with
/// seed `game_seed`. Attempt 0 uses `deal_seed = game_seed`; attempt `k >= 1`
/// uses `derive_seed(game_seed, SeedStream::GameRetry, k)`.
pub fn attempt_seeds(game_seed: u64, attempt: u8) -> (u64, u64, u64) {
    let deal_seed = if attempt == 0 {
        game_seed
    } else {
        derive_seed(game_seed, SeedStream::GameRetry, u64::from(attempt))
    };
    let env_seed = derive_seed(deal_seed, SeedStream::GameEnv, 0);
    let action_seed = derive_seed(deal_seed, SeedStream::GameActions, 0);
    (deal_seed, env_seed, action_seed)
}

// ---------------------------------------------------------------------------
// Results helpers
// ---------------------------------------------------------------------------

/// Constructs the frozen `GameEnd::Terminal` payload for a watchdog expiry
/// (`stop_kind == "terminal:watchdog"`, charging no card).
pub fn watchdog_game_end() -> GameEnd {
    GameEnd::Terminal {
        code: WATCHDOG_CODE.to_owned(),
        classification: WATCHDOG_CLASSIFICATION.to_owned(),
        reason: WATCHDOG_REASON.to_owned(),
    }
}

/// Returns `true` iff `end` is the frozen `terminal:watchdog` end.
pub fn is_watchdog_end(end: &GameEnd) -> bool {
    matches!(
        end,
        GameEnd::Terminal {
            code,
            classification,
            reason,
        } if code == WATCHDOG_CODE
            && classification == WATCHDOG_CLASSIFICATION
            && reason == WATCHDOG_REASON
    )
}

/// An attempt is halted unless its end is [`GameEnd::Natural`].
pub fn is_halted(end: &GameEnd) -> bool {
    !matches!(end, GameEnd::Natural { .. })
}

/// Frozen `stop_kind` of `m4-interfaces/v2`: `None` for `natural`, else
/// `end.kind`, followed by `:` and `code` for `terminal` and `session_error`.
pub fn stop_kind(end: &GameEnd) -> Option<String> {
    match end {
        GameEnd::Natural { .. } => None,
        GameEnd::Terminal { code, .. } => Some(format!("terminal:{code}")),
        GameEnd::SessionError { code, .. } => Some(format!("session_error:{code}")),
        GameEnd::DecisionCap => Some("decision_cap".to_owned()),
        GameEnd::Panic { .. } => Some("panic".to_owned()),
    }
}

fn scores_for_end(end: &GameEnd, seats: [usize; 2]) -> Result<Option<[i8; 2]>, PodRunnerError> {
    match end {
        GameEnd::Natural { winner_seat, .. } => match winner_seat {
            None => Ok(Some([0, 0])),
            Some(seat) if *seat == seats[0] => Ok(Some([1, -1])),
            Some(seat) if *seat == seats[1] => Ok(Some([-1, 1])),
            Some(other) => Err(PodRunnerError::InvalidWinnerSeat {
                winner_seat: *other,
                seats,
            }),
        },
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Record types (`mtg-kernel-cube-pod-record/v2`, frozen key order)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PodUse {
    Training,
    Shakedown,
    Dose,
    ReadA,
    ReadB,
    PairedBuild,
    Sizing,
    Test,
    Client,
}

impl PodUse {
    /// Returns `true` for uses that are always run from a read plan.
    pub const fn is_read_use(self) -> bool {
        matches!(
            self,
            PodUse::Dose | PodUse::ReadA | PodUse::ReadB | PodUse::PairedBuild | PodUse::Sizing
        )
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            PodUse::Training => "training",
            PodUse::Shakedown => "shakedown",
            PodUse::Dose => "dose",
            PodUse::ReadA => "read_a",
            PodUse::ReadB => "read_b",
            PodUse::PairedBuild => "paired_build",
            PodUse::Sizing => "sizing",
            PodUse::Test => "test",
            PodUse::Client => "client",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotKind {
    Uniform,
    Checkpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChairRole {
    Learning,
    Older,
    Fixture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatKind {
    Uniform,
    Learned,
    Stated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuilderKind {
    Uniform,
    Learned,
    Stated,
    Dealt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeckSource {
    Build,
    Dealt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameRole {
    Reference,
    Sibling,
    Cross,
    ReadB,
    Common,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatRule {
    Coin,
    Schedule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairedBuildKind {
    Sibling,
    CandidateStated,
    CurrentInitial,
    #[serde(rename = "k1_last_b1_last")]
    K1LastB1Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PodStatus {
    Complete,
    Stopped,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PodSeeds {
    pub draft_seed: Option<u64>,
    pub pod_seed: Option<u64>,
    pub league_seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_seed_namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod_seed_namespace: Option<String>,
}

impl PodSeeds {
    /// Constructs read-plan seeds (field 5 with source namespaces appended and
    /// `league_seed` copied from the plan or null).
    pub fn for_read(
        draft_seed: u64,
        pod_seed: u64,
        league_seed: Option<u64>,
        draft_seed_namespace: impl Into<String>,
        pod_seed_namespace: impl Into<String>,
    ) -> Self {
        PodSeeds {
            draft_seed: Some(draft_seed),
            pod_seed: Some(pod_seed),
            league_seed,
            draft_seed_namespace: Some(draft_seed_namespace.into()),
            pod_seed_namespace: Some(pod_seed_namespace.into()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PodPins {
    pub binary: Option<String>,
    pub cube_list: Option<String>,
    pub fence_record: Option<String>,
    pub card_index: Option<String>,
    pub cube_table: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotInfo {
    pub kind: PilotKind,
    pub pin: Option<String>,
    pub decoding: String,
}

impl PilotInfo {
    /// Pilot metadata for [`crate::cube_seat_v1::UniformPilot`].
    pub fn uniform() -> Self {
        PilotInfo {
            kind: PilotKind::Uniform,
            pin: None,
            decoding: UNIFORM_SAMPLER_ID.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PodCaps {
    pub max_decisions: u64,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    pub max_stack_objects: u32,
    pub watchdog_seconds: u64,
    pub max_replays: u8,
}

impl PodCaps {
    /// The explicit A2.1 caps for a training pod (600s watchdog, 3 replays).
    pub const fn training_a2() -> Self {
        PodCaps {
            max_decisions: A2_GAME_CAPS.max_decisions,
            max_physical_decisions: A2_GAME_CAPS.max_physical_decisions,
            max_policy_steps: A2_GAME_CAPS.max_policy_steps,
            max_stack_objects: A2_MAX_STACK_OBJECTS,
            watchdog_seconds: DEFAULT_WATCHDOG_SECONDS,
            max_replays: TRAINING_MAX_REPLAYS,
        }
    }

    /// The explicit A2.1 caps for a read plan (600s watchdog, 0 replays).
    pub const fn read_a2() -> Self {
        PodCaps {
            max_decisions: A2_GAME_CAPS.max_decisions,
            max_physical_decisions: A2_GAME_CAPS.max_physical_decisions,
            max_policy_steps: A2_GAME_CAPS.max_policy_steps,
            max_stack_objects: A2_MAX_STACK_OBJECTS,
            watchdog_seconds: DEFAULT_WATCHDOG_SECONDS,
            max_replays: READ_MAX_REPLAYS,
        }
    }

    /// Extracts the [`GameCaps`] passed to [`GamePilot::play`].
    pub const fn game_caps(&self) -> GameCaps {
        GameCaps {
            max_decisions: self.max_decisions,
            max_physical_decisions: self.max_physical_decisions,
            max_policy_steps: self.max_policy_steps,
            max_stack_objects: Some(self.max_stack_objects),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatRef {
    pub kind: SeatKind,
    pub pin: Option<String>,
}

impl SeatRef {
    pub fn uniform() -> Self {
        SeatRef {
            kind: SeatKind::Uniform,
            pin: None,
        }
    }

    pub fn stated() -> Self {
        SeatRef {
            kind: SeatKind::Stated,
            pin: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChairRecord {
    pub chair: u8,
    pub role: ChairRole,
    pub picker: SeatRef,
    pub pick_rows: Vec<DecisionRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DealtOrigin {
    pub function: String,
    pub key: Value,
}

/// Pin carried inside [`BuilderRef`]: a checkpoint digest string for a learned
/// builder, or the dealer rule identity and complete deal key for a dealt deck
/// (`m4-interfaces/v2` change 6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BuilderPin {
    Checkpoint(String),
    Dealt(DealtOrigin),
}

/// Builder reference recorded on every [`DeckRecord`] (`m4-interfaces/v2`
/// field 11 and change 6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderRef {
    pub kind: BuilderKind,
    pub pin: Option<BuilderPin>,
}

impl BuilderRef {
    pub fn uniform() -> Self {
        BuilderRef {
            kind: BuilderKind::Uniform,
            pin: None,
        }
    }

    pub fn stated() -> Self {
        BuilderRef {
            kind: BuilderKind::Stated,
            pin: None,
        }
    }

    pub fn learned(pin: impl Into<String>) -> Self {
        BuilderRef {
            kind: BuilderKind::Learned,
            pin: Some(BuilderPin::Checkpoint(pin.into())),
        }
    }

    pub fn dealt(origin: DealtOrigin) -> Self {
        BuilderRef {
            kind: BuilderKind::Dealt,
            pin: Some(BuilderPin::Dealt(origin)),
        }
    }

    pub fn from_seat_ref(seat: &SeatRef) -> Self {
        let kind = match seat.kind {
            SeatKind::Uniform => BuilderKind::Uniform,
            SeatKind::Learned => BuilderKind::Learned,
            SeatKind::Stated => BuilderKind::Stated,
        };
        BuilderRef {
            kind,
            pin: seat.pin.clone().map(BuilderPin::Checkpoint),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckRecord {
    pub deck: u32,
    pub source: DeckSource,
    pub chair: Option<u8>,
    pub build: Option<u8>,
    pub builder: BuilderRef,
    pub build_seed: Option<u64>,
    pub cards: Vec<u16>,
    pub build_rows: Option<BuildRows>,
    pub dealt: Option<DealtOrigin>,
}

/// One line of `timing/pod-<pod_index>.jsonl` (`READ-PLAN-v1` section 5).
/// Never enters a deterministic [`PodRecord`] or score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptTiming {
    pub game: u32,
    pub attempt: u8,
    pub elapsed_ms: u64,
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttemptRecord {
    pub attempt: u8,
    pub deal_seed: u64,
    pub env_seed: u64,
    pub action_seed: u64,
    pub decisions: u64,
    pub physical_decisions: Option<u64>,
    pub policy_steps: Option<u64>,
    pub end: GameEnd,
    pub halted: bool,
    pub stop_kind: Option<String>,
    pub last_action_source: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodGameRecord {
    pub game: u32,
    pub group: u32,
    pub role: GameRole,
    pub pairing: Option<u8>,
    pub decks: [u32; 2],
    pub chairs: [Option<u8>; 2],
    pub seat_rule: SeatRule,
    pub game_seed: u64,
    pub episode: u64,
    pub attempts: Vec<AttemptRecord>,
    pub scores: Option<[i8; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairedBuildRecord {
    pub kind: PairedBuildKind,
    pub chair: u8,
    pub deck_x: u32,
    pub deck_y: u32,
    pub opponents: Vec<u32>,
    pub games_x: Vec<u32>,
    pub games_y: Vec<u32>,
    pub q: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HaltLedgerEntry {
    pub game: u32,
    pub attempt: u8,
    pub stop_kind: String,
    pub last_action_source: Option<u16>,
    pub deal_seed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopAttemptReport {
    pub deal_seed: u64,
    pub stop_kind: String,
    pub last_action_source: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopReport {
    pub game: u32,
    pub pairing: Option<u8>,
    pub decks: [u32; 2],
    pub attempts: Vec<StopAttemptReport>,
}

/// Complete pod record (`mtg-kernel-cube-pod-record/v2`), with fields in the
/// exact frozen key order of `m4-interfaces/v2` section 4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodRecord {
    pub schema: &'static str,
    pub interfaces: &'static str,
    #[serde(rename = "use")]
    pub pod_use: PodUse,
    pub namespace: String,
    pub run: String,
    pub pod_index: u64,
    pub seeds: PodSeeds,
    pub pins: PodPins,
    pub pilot: PilotInfo,
    pub caps: PodCaps,
    pub draft: Option<DraftRecord>,
    pub chairs: Vec<ChairRecord>,
    pub decks: Vec<DeckRecord>,
    pub games: Vec<PodGameRecord>,
    pub paired_builds: Vec<PairedBuildRecord>,
    pub halts: Vec<HaltLedgerEntry>,
    pub status: PodStatus,
    pub stop: Option<StopReport>,
}

impl PodRecord {
    /// Serializes this pod record as compact JSON without a trailing newline,
    /// preserving the exact frozen field order.
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).expect("a pod record is plain data")
    }
}

// ---------------------------------------------------------------------------
// Training pod configuration and execution
// ---------------------------------------------------------------------------

/// Chair setup for one seat of a training pod.
pub struct TrainingChair {
    pub role: ChairRole,
    pub picker_ref: SeatRef,
    pub picker: Box<dyn SeatPicker>,
    pub builder_ref: SeatRef,
    pub builder: Box<dyn SeatBuilder>,
}

impl TrainingChair {
    /// Uniform fixture chair: [`UniformSeatPicker`] + [`UniformFourStepBuilder`].
    pub fn uniform(role: ChairRole) -> Self {
        TrainingChair {
            role,
            picker_ref: SeatRef::uniform(),
            picker: Box::new(UniformSeatPicker),
            builder_ref: SeatRef::uniform(),
            builder: Box::new(UniformFourStepBuilder),
        }
    }

    /// Uniform-draft, stated-build fixture chair: [`UniformSeatPicker`] +
    /// [`StatedSeatBuilder`].
    pub fn uniform_stated(role: ChairRole) -> Self {
        TrainingChair {
            role,
            picker_ref: SeatRef::uniform(),
            picker: Box::new(UniformSeatPicker),
            builder_ref: SeatRef::stated(),
            builder: Box::new(StatedSeatBuilder),
        }
    }
}

/// Draws D0.11 older chairs and their league indices, in ascending chair order.
pub fn draw_league_chairs(
    league_seed: u64,
    league_len: usize,
) -> Result<[(usize, usize); 2], PodRunnerError> {
    if league_len == 0 {
        return Err(PodRunnerError::InvalidPlan(
            "league must contain a snapshot".to_owned(),
        ));
    }
    let mut stream = SplitMix64::seed(league_seed);
    let first = uniform_index(&mut stream, SEATS);
    let draw = uniform_index(&mut stream, SEATS - 1);
    let second = if draw < first { draw } else { draw + 1 };
    let mut chairs = [first, second];
    chairs.sort_unstable();
    Ok(chairs.map(|chair| (chair, uniform_index(&mut stream, league_len))))
}

/// Constructs an 8-chair training role assignment with the two specified
/// distinct chairs marked [`ChairRole::Older`] and the remaining six
/// [`ChairRole::Learning`].
pub fn training_roles_with_older(older: [usize; 2]) -> Result<[ChairRole; SEATS], PodRunnerError> {
    if older[0] >= SEATS || older[1] >= SEATS || older[0] == older[1] {
        return Err(PodRunnerError::InvalidPlan(format!(
            "older chairs {older:?} must be two distinct indices in 0..{SEATS}"
        )));
    }
    let mut roles = [ChairRole::Learning; SEATS];
    roles[older[0]] = ChairRole::Older;
    roles[older[1]] = ChairRole::Older;
    Ok(roles)
}

/// Eight uniform fixture chairs (`UniformSeatPicker` + `UniformFourStepBuilder`)
/// with the given training roles.
pub fn uniform_training_chairs(roles: [ChairRole; SEATS]) -> [TrainingChair; SEATS] {
    std::array::from_fn(|chair| TrainingChair::uniform(roles[chair]))
}

/// Eight uniform-picker, stated-builder fixture chairs with the given roles.
pub fn stated_training_chairs(roles: [ChairRole; SEATS]) -> [TrainingChair; SEATS] {
    std::array::from_fn(|chair| TrainingChair::uniform_stated(roles[chair]))
}

/// Parameters for running one training pod.
#[derive(Debug, Clone)]
pub struct TrainingPodConfig {
    pub pod_use: PodUse,
    pub namespace: String,
    pub run: String,
    pub pod_index: u64,
    pub pins: PodPins,
    pub pilot_info: PilotInfo,
    pub caps: PodCaps,
    pub workers: usize,
}

impl TrainingPodConfig {
    /// Standard uniform-pilot training pod configuration with explicit A2.1 caps.
    pub fn uniform_a2(
        pod_use: PodUse,
        namespace: impl Into<String>,
        run: impl Into<String>,
        pod_index: u64,
        workers: usize,
    ) -> Self {
        TrainingPodConfig {
            pod_use,
            namespace: namespace.into(),
            run: run.into(),
            pod_index,
            pins: PodPins::default(),
            pilot_info: PilotInfo::uniform(),
            caps: PodCaps::training_a2(),
            workers,
        }
    }
}

/// Parameters for running a batch of uniform training pods.
#[derive(Debug, Clone)]
pub struct UniformBatchConfig {
    pub pod_use: PodUse,
    pub namespace: String,
    pub run: String,
    pub pod_indices: Vec<u64>,
    pub roles: [ChairRole; SEATS],
    pub pins: PodPins,
    pub pilot_info: PilotInfo,
    pub caps: PodCaps,
    pub workers: usize,
}

// ---------------------------------------------------------------------------
// Read plans (`seat_rule: "schedule"`, `max_replays: 0`)
// ---------------------------------------------------------------------------

/// One explicit deck in a [`ReadPlan`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDeckSpec {
    pub source: DeckSource,
    pub chair: Option<u8>,
    pub build: Option<u8>,
    pub builder: BuilderRef,
    pub build_seed: Option<u64>,
    pub cards: Vec<u16>,
    pub build_rows: Option<BuildRows>,
    pub dealt: Option<DealtOrigin>,
}

/// One explicit scheduled game in a [`ReadPlan`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadGameSpec {
    pub group: u32,
    pub role: GameRole,
    pub pairing: Option<u8>,
    /// Indices into `ReadPlan::decks` for session players p0 and p1.
    pub decks: [u32; 2],
    pub chairs: [Option<u8>; 2],
    /// Distinct seat identifiers passed to [`AttemptSpec::seats`].
    pub seats: [usize; 2],
    pub game_seed: u64,
    pub episode: u64,
}

/// Explicit read plan (`seat_rule: "schedule"`, `max_replays: 0`) for direct
/// in-memory execution. For full file-backed `mtg-kernel-cube-read-plan/v3`
/// plans, manifests, prep sets, panels and execution directories, see
/// [`crate::cube_read_plan_v1`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadPlan {
    #[serde(rename = "use")]
    pub pod_use: PodUse,
    pub namespace: String,
    pub run: String,
    pub pod_index: u64,
    pub seeds: PodSeeds,
    pub pins: PodPins,
    pub pilot: PilotInfo,
    pub caps: PodCaps,
    pub chairs: Vec<ChairRecord>,
    pub decks: Vec<ReadDeckSpec>,
    pub games: Vec<ReadGameSpec>,
    pub paired_builds: Vec<PairedBuildRecord>,
}

// ---------------------------------------------------------------------------
// Internal parallel game execution
// ---------------------------------------------------------------------------

pub(crate) struct GameTask {
    pub(crate) game: u32,
    pub(crate) group: u32,
    pub(crate) role: GameRole,
    pub(crate) pairing: Option<u8>,
    pub(crate) decks: [u32; 2],
    pub(crate) chairs: [Option<u8>; 2],
    pub(crate) seats: [usize; 2],
    pub(crate) seat_rule: SeatRule,
    pub(crate) game_seed: u64,
    pub(crate) episode: u64,
}

pub(crate) type GameCompleteHook<'a> =
    dyn Fn(&PodGameRecord, &[AttemptTiming]) -> Result<(), String> + Sync + 'a;

pub(crate) struct ExecOptions<'a, 'b> {
    pub(crate) caps: PodCaps,
    pub(crate) watchdog_override: Option<Duration>,
    pub(crate) workers: usize,
    pub(crate) host: &'a str,
    pub(crate) stop_on_watchdog: bool,
    pub(crate) on_game_complete: Option<&'b GameCompleteHook<'b>>,
}

pub(crate) struct ExecOutput {
    pub(crate) games: Vec<PodGameRecord>,
    pub(crate) timings: Vec<AttemptTiming>,
    /// Lowest logical game index whose attempt tripped `terminal:watchdog`
    /// when `stop_on_watchdog` is enabled.
    pub(crate) first_watchdog_stop: Option<usize>,
}

struct ExecContext<'a, 'b> {
    tasks: &'a [GameTask],
    built_decks: &'a [Deck],
    pilot: &'a (dyn GamePilot + Sync),
    game_caps: GameCaps,
    max_replays: u8,
    watchdog: Duration,
    host: &'a str,
    stop_on_watchdog: bool,
    watchdog_tripped: &'a AtomicBool,
    on_game_complete: Option<&'b GameCompleteHook<'b>>,
}

fn play_game_task(
    task: &GameTask,
    ctx: &ExecContext<'_, '_>,
) -> Result<(PodGameRecord, Vec<AttemptTiming>), PodRunnerError> {
    let deck_pair = [
        &ctx.built_decks[task.decks[0] as usize],
        &ctx.built_decks[task.decks[1] as usize],
    ];
    let mut attempts = Vec::with_capacity(usize::from(ctx.max_replays) + 1);
    let mut timings = Vec::with_capacity(usize::from(ctx.max_replays) + 1);
    let mut final_scores = None;

    for attempt in 0..=ctx.max_replays {
        let (deal_seed, env_seed, action_seed) = attempt_seeds(task.game_seed, attempt);
        let spec = AttemptSpec {
            decks: deck_pair,
            seats: task.seats,
            episode: task.episode,
            env_seed,
            action_seed,
        };
        let started = Instant::now();
        let (outcome, stalled) =
            observe_decisions(ctx.watchdog, || ctx.pilot.play(&spec, &ctx.game_caps));
        let elapsed = started.elapsed();
        let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        timings.push(AttemptTiming {
            game: task.game,
            attempt,
            elapsed_ms,
            host: ctx.host.to_owned(),
        });

        let pilot_watchdog = is_watchdog_end(&outcome.end);
        let timed_out = pilot_watchdog || stalled;
        let (end, halted, sk, last_action_source, physical_decisions, policy_steps) = if timed_out {
            (
                watchdog_game_end(),
                true,
                Some(WATCHDOG_STOP_KIND.to_owned()),
                None,
                if pilot_watchdog {
                    outcome.physical_decisions
                } else {
                    None
                },
                if pilot_watchdog {
                    outcome.policy_steps
                } else {
                    None
                },
            )
        } else {
            let halted = is_halted(&outcome.end);
            let sk = stop_kind(&outcome.end);
            (
                outcome.end,
                halted,
                sk,
                outcome.last_action_source,
                outcome.physical_decisions,
                outcome.policy_steps,
            )
        };

        if !halted {
            final_scores = scores_for_end(&end, task.seats)?;
        }
        attempts.push(AttemptRecord {
            attempt,
            deal_seed,
            env_seed,
            action_seed,
            decisions: outcome.decisions,
            physical_decisions,
            policy_steps,
            end,
            halted,
            stop_kind: sk,
            last_action_source,
        });
        if !halted {
            break;
        }
        if timed_out && ctx.stop_on_watchdog {
            ctx.watchdog_tripped.store(true, Ordering::Release);
            break;
        }
    }

    let record = PodGameRecord {
        game: task.game,
        group: task.group,
        role: task.role,
        pairing: task.pairing,
        decks: task.decks,
        chairs: task.chairs,
        seat_rule: task.seat_rule,
        game_seed: task.game_seed,
        episode: task.episode,
        attempts,
        scores: final_scores,
    };
    if let Some(hook) = ctx.on_game_complete {
        hook(&record, &timings).map_err(PodRunnerError::Io)?;
    }
    Ok((record, timings))
}

pub(crate) fn execute_game_tasks_with_options(
    tasks: &[GameTask],
    built_decks: &[Deck],
    pilot: &(dyn GamePilot + Sync),
    options: &ExecOptions<'_, '_>,
) -> Result<ExecOutput, PodRunnerError> {
    if tasks.is_empty() {
        return Ok(ExecOutput {
            games: Vec::new(),
            timings: Vec::new(),
            first_watchdog_stop: None,
        });
    }
    let watchdog = options
        .watchdog_override
        .unwrap_or_else(|| Duration::from_secs(options.caps.watchdog_seconds));
    let watchdog_tripped = AtomicBool::new(false);
    let ctx = ExecContext {
        tasks,
        built_decks,
        pilot,
        game_caps: options.caps.game_caps(),
        max_replays: options.caps.max_replays,
        watchdog,
        host: options.host,
        stop_on_watchdog: options.stop_on_watchdog,
        watchdog_tripped: &watchdog_tripped,
        on_game_complete: options.on_game_complete,
    };

    let worker_count = options.workers.max(1).min(tasks.len());
    let next_index = AtomicUsize::new(0);
    type GameResult = Result<(PodGameRecord, Vec<AttemptTiming>), PodRunnerError>;
    type GameSlot = Mutex<Option<GameResult>>;
    let slots: Vec<GameSlot> = (0..tasks.len()).map(|_| Mutex::new(None)).collect();

    std::thread::scope(|scope| -> Result<(), PodRunnerError> {
        let mut handles = Vec::with_capacity(worker_count);
        for worker_id in 0..worker_count {
            let next_index = &next_index;
            let slots = &slots;
            let ctx = &ctx;
            let handle = std::thread::Builder::new()
                .name(format!("cube-pod-w{worker_id}"))
                .stack_size(WORKER_STACK_BYTES)
                .spawn_scoped(scope, move || loop {
                    if ctx.stop_on_watchdog && ctx.watchdog_tripped.load(Ordering::Acquire) {
                        break;
                    }
                    let index = next_index.fetch_add(1, Ordering::Relaxed);
                    if index >= ctx.tasks.len() {
                        break;
                    }
                    let result = play_game_task(&ctx.tasks[index], ctx);
                    *slots[index].lock().expect("game result slot lock") = Some(result);
                })
                .map_err(|error| PodRunnerError::Thread(error.to_string()))?;
            handles.push(handle);
        }
        for handle in handles {
            handle
                .join()
                .map_err(|_| PodRunnerError::Thread("worker thread panicked".to_owned()))?;
        }
        Ok(())
    })?;

    let raw_slots: Vec<Option<GameResult>> = slots
        .into_iter()
        .map(|slot| slot.into_inner().expect("game result slot lock"))
        .collect();

    if options.stop_on_watchdog && watchdog_tripped.load(Ordering::Acquire) {
        let mut first_expired: Option<usize> = None;
        for (index, slot) in raw_slots.iter().enumerate() {
            if let Some(Ok((game, _))) = slot {
                if game.attempts.iter().any(|a| is_watchdog_end(&a.end)) {
                    first_expired = Some(index);
                    break;
                }
            }
        }
        let stop_idx = first_expired.ok_or_else(|| {
            PodRunnerError::Thread("watchdog tripped without recorded expired game".to_owned())
        })?;
        let mut games = Vec::with_capacity(stop_idx + 1);
        let mut timings = Vec::new();
        for (index, slot) in raw_slots.into_iter().take(stop_idx + 1).enumerate() {
            let (game, game_timings) = slot.ok_or_else(|| {
                PodRunnerError::Thread(format!("missing result for game {index} before watchdog"))
            })??;
            games.push(game);
            timings.extend(game_timings);
        }
        return Ok(ExecOutput {
            games,
            timings,
            first_watchdog_stop: Some(stop_idx),
        });
    }

    let mut games = Vec::with_capacity(tasks.len());
    let mut timings = Vec::new();
    for (index, slot) in raw_slots.into_iter().enumerate() {
        let (game, game_timings) = slot
            .ok_or_else(|| PodRunnerError::Thread(format!("missing result for game {index}")))??;
        games.push(game);
        timings.extend(game_timings);
    }
    Ok(ExecOutput {
        games,
        timings,
        first_watchdog_stop: None,
    })
}

pub(crate) fn collect_halts_and_stop(
    games: &[PodGameRecord],
    stop_on_exhausted: bool,
) -> (Vec<HaltLedgerEntry>, PodStatus, Option<StopReport>) {
    let mut halts = Vec::new();
    let mut first_stop: Option<StopReport> = None;

    for game in games {
        for attempt in &game.attempts {
            if attempt.halted {
                halts.push(HaltLedgerEntry {
                    game: game.game,
                    attempt: attempt.attempt,
                    stop_kind: attempt
                        .stop_kind
                        .clone()
                        .unwrap_or_else(|| "unknown".to_owned()),
                    last_action_source: attempt.last_action_source,
                    deal_seed: attempt.deal_seed,
                });
            }
        }
        if stop_on_exhausted && first_stop.is_none() && game.scores.is_none() {
            let stop_attempts = game
                .attempts
                .iter()
                .map(|attempt| StopAttemptReport {
                    deal_seed: attempt.deal_seed,
                    stop_kind: attempt
                        .stop_kind
                        .clone()
                        .unwrap_or_else(|| "unknown".to_owned()),
                    last_action_source: attempt.last_action_source,
                })
                .collect();
            first_stop = Some(StopReport {
                game: game.game,
                pairing: game.pairing,
                decks: game.decks,
                attempts: stop_attempts,
            });
        }
    }

    let status = if first_stop.is_some() {
        PodStatus::Stopped
    } else {
        PodStatus::Complete
    };
    (halts, status, first_stop)
}

fn validate_use_and_namespace(pod_use: PodUse, namespace: &str) -> Result<(), PodRunnerError> {
    if pod_use == PodUse::Client && namespace != CLIENT_NAMESPACE {
        return Err(PodRunnerError::InvalidPlan(format!(
            "use \"client\" requires namespace {CLIENT_NAMESPACE:?}, got {namespace:?}"
        )));
    }
    if namespace == CLIENT_NAMESPACE && pod_use != PodUse::Client {
        return Err(PodRunnerError::InvalidPlan(format!(
            "namespace {CLIENT_NAMESPACE:?} is reserved for use \"client\", got {:?}",
            pod_use.as_str()
        )));
    }
    Ok(())
}

fn validate_common_config(
    pod_use: PodUse,
    namespace: &str,
    run: &str,
    caps: PodCaps,
) -> Result<(), PodRunnerError> {
    if !is_valid_token(namespace) {
        return Err(PodRunnerError::InvalidToken {
            field: "namespace",
            value: namespace.to_owned(),
        });
    }
    if !is_valid_token(run) {
        return Err(PodRunnerError::InvalidToken {
            field: "run",
            value: run.to_owned(),
        });
    }
    validate_use_and_namespace(pod_use, namespace)?;
    if caps.max_stack_objects != A2_MAX_STACK_OBJECTS {
        return Err(PodRunnerError::InvalidCaps(format!(
            "max_stack_objects must be {A2_MAX_STACK_OBJECTS}"
        )));
    }
    if caps.watchdog_seconds == 0 {
        return Err(PodRunnerError::InvalidCaps(
            "watchdog_seconds must be at least 1".to_owned(),
        ));
    }
    Ok(())
}

/// Checks training or client pod settings before drafting or writing a run record.
pub fn validate_training_pod_config(
    pod_use: PodUse,
    namespace: &str,
    run: &str,
    caps: PodCaps,
) -> Result<(), PodRunnerError> {
    validate_common_config(pod_use, namespace, run, caps)?;
    if pod_use.is_read_use() {
        return Err(PodRunnerError::InvalidPlan(format!(
            "read use {:?} must run from a read plan, not run_training_pod",
            pod_use.as_str()
        )));
    }
    if caps.max_replays > TRAINING_MAX_REPLAYS {
        return Err(PodRunnerError::InvalidCaps(format!(
            "training max_replays {} exceeds {TRAINING_MAX_REPLAYS}",
            caps.max_replays
        )));
    }

    Ok(())
}

pub(crate) fn validate_builder_and_dealt(
    source: DeckSource,
    builder: &BuilderRef,
    dealt: Option<&DealtOrigin>,
    label: &str,
) -> Result<(), PodRunnerError> {
    match source {
        DeckSource::Build => {
            if dealt.is_some() {
                return Err(PodRunnerError::InvalidPlan(format!(
                    "{label}: build deck must have dealt == null"
                )));
            }
            match builder.kind {
                BuilderKind::Uniform | BuilderKind::Stated => {
                    if builder.pin.is_some() {
                        return Err(PodRunnerError::InvalidPlan(format!(
                            "{label}: uniform/stated builder must have pin == null"
                        )));
                    }
                }
                BuilderKind::Learned => match &builder.pin {
                    Some(BuilderPin::Checkpoint(digest)) if !digest.is_empty() => {}
                    _ => {
                        return Err(PodRunnerError::InvalidPlan(format!(
                            "{label}: learned builder requires a checkpoint pin string"
                        )));
                    }
                },
                BuilderKind::Dealt => {
                    return Err(PodRunnerError::InvalidPlan(format!(
                        "{label}: build deck cannot have builder.kind == \"dealt\""
                    )));
                }
            }
        }
        DeckSource::Dealt => {
            let Some(origin) = dealt else {
                return Err(PodRunnerError::InvalidPlan(format!(
                    "{label}: dealt deck requires non-null dealt"
                )));
            };
            if builder.kind != BuilderKind::Dealt {
                return Err(PodRunnerError::InvalidPlan(format!(
                    "{label}: dealt deck requires builder.kind == \"dealt\""
                )));
            }
            match &builder.pin {
                Some(BuilderPin::Dealt(pin_origin)) if pin_origin == origin => {}
                _ => {
                    return Err(PodRunnerError::InvalidPlan(format!(
                        "{label}: dealt deck builder.pin must equal dealt {{function, key}}"
                    )));
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public runners
// ---------------------------------------------------------------------------

/// Runs one 8-chair training pod (6 learning chairs with sibling a and b
/// builds, 2 older chairs with single builds, 28 coupled groups, 70 games).
///
/// If any game halts on all `1 + caps.max_replays` attempts, returns
/// `Err(PodRunnerError::Stopped(Box<PodRecord>))` containing the complete
/// record with `status == PodStatus::Stopped` and `stop == Some(_)`.
pub fn run_training_pod(
    pool: &CubePool,
    config: &TrainingPodConfig,
    chairs: &mut [TrainingChair; SEATS],
    pilot: &(dyn GamePilot + Sync),
) -> Result<PodRecord, PodRunnerError> {
    validate_training_pod_config(config.pod_use, &config.namespace, &config.run, config.caps)?;

    let (mut learning, mut older, mut fixture) = (0usize, 0usize, 0usize);
    for chair in chairs.iter() {
        match chair.role {
            ChairRole::Learning => learning += 1,
            ChairRole::Older => older += 1,
            ChairRole::Fixture => fixture += 1,
        }
    }
    if learning != TRAINING_LEARNING_CHAIRS || older != TRAINING_OLDER_CHAIRS {
        return Err(PodRunnerError::InvalidTrainingRoles {
            learning,
            older,
            fixture,
        });
    }

    let draft_seed = pod_draft_seed(&config.namespace, &config.run, config.pod_index)?;
    let play_seed = pod_play_seed(&config.namespace, &config.run, config.pod_index)?;
    let league_seed = pod_league_seed(&config.namespace, &config.run, config.pod_index)?;

    // 1. Draft with per-chair picker streams.
    let mut draft_pickers: [Box<dyn SeatPicker>; SEATS] = std::array::from_fn(|chair| {
        std::mem::replace(&mut chairs[chair].picker, Box::new(UniformSeatPicker))
    });
    let draft_result = run_seat_draft(pool, draft_seed, &mut draft_pickers);
    for (chair, picker) in draft_pickers.into_iter().enumerate() {
        chairs[chair].picker = picker;
    }
    let seat_draft = draft_result?;

    let chair_records: Vec<ChairRecord> = (0..SEATS)
        .map(|chair| ChairRecord {
            chair: chair as u8,
            role: chairs[chair].role,
            picker: chairs[chair].picker_ref.clone(),
            pick_rows: seat_draft.pick_rows[chair].clone(),
        })
        .collect();

    // 2. Build decks in chair order (sibling a then sibling b for learning chairs).
    let mut deck_records: Vec<DeckRecord> = Vec::with_capacity(SEATS + TRAINING_LEARNING_CHAIRS);
    let mut built_decks: Vec<Deck> = Vec::with_capacity(SEATS + TRAINING_LEARNING_CHAIRS);
    let mut ref_deck_id = [0u32; SEATS];
    let mut sib_deck_id: [Option<u32>; SEATS] = [None; SEATS];

    for (chair, chair_cfg) in chairs.iter_mut().enumerate() {
        let seat_pool = &seat_draft.record.pools[chair];
        let build_count: u8 = if chair_cfg.role == ChairRole::Learning {
            2
        } else {
            1
        };
        for build_k in 0..build_count {
            let b_seed = build_seed_for_chair(draft_seed, chair, build_k);
            let mut stream = SplitMix64::seed(b_seed);
            let outcome = chair_cfg.builder.build(seat_pool, &mut stream)?;
            let deck = canonical_deck(&outcome.deck.card_ids());
            deck.validate(seat_pool)?;
            let deck_index = deck_records.len() as u32;
            if build_k == 0 {
                ref_deck_id[chair] = deck_index;
            } else {
                sib_deck_id[chair] = Some(deck_index);
            }
            deck_records.push(DeckRecord {
                deck: deck_index,
                source: DeckSource::Build,
                chair: Some(chair as u8),
                build: Some(build_k),
                builder: BuilderRef::from_seat_ref(&chair_cfg.builder_ref),
                build_seed: Some(b_seed),
                cards: deck.card_ids(),
                build_rows: outcome.rows,
                dealt: None,
            });
            built_decks.push(deck);
        }
    }

    // 3. Plan the 70 coupled games across the 28 pairings and paired_builds.
    let pairings = pod_pairings();
    debug_assert_eq!(pairings.len(), POD_GAMES);
    let mut tasks = Vec::with_capacity(TRAINING_POD_GAMES);
    let mut paired_opponents: [Vec<u32>; SEATS] = std::array::from_fn(|_| Vec::new());
    let mut paired_games_x: [Vec<u32>; SEATS] = std::array::from_fn(|_| Vec::new());
    let mut paired_games_y: [Vec<u32>; SEATS] = std::array::from_fn(|_| Vec::new());

    for (group_index, &(low, high)) in pairings.iter().enumerate() {
        let g_seed = game_seed(play_seed, group_index);
        let seats = pod_game_seats(play_seed, group_index)?;
        let episode = group_index as u64;
        let seat_chairs = [Some(seats[0] as u8), Some(seats[1] as u8)];

        // Reference game: reference deck vs reference deck.
        let ref_game_id = tasks.len() as u32;
        tasks.push(GameTask {
            game: ref_game_id,
            group: group_index as u32,
            role: GameRole::Reference,
            pairing: Some(group_index as u8),
            decks: [ref_deck_id[seats[0]], ref_deck_id[seats[1]]],
            chairs: seat_chairs,
            seats,
            seat_rule: SeatRule::Coin,
            game_seed: g_seed,
            episode,
        });

        // Sibling b games in chair order (low, then high).
        for (learning_chair, opp_chair) in [(low, high), (high, low)] {
            let Some(sib_id) = sib_deck_id[learning_chair] else {
                continue;
            };
            let opp_ref = ref_deck_id[opp_chair];
            let decks = if seats[0] == learning_chair {
                [sib_id, opp_ref]
            } else {
                [opp_ref, sib_id]
            };
            let sib_game_id = tasks.len() as u32;
            tasks.push(GameTask {
                game: sib_game_id,
                group: group_index as u32,
                role: GameRole::Sibling,
                pairing: Some(group_index as u8),
                decks,
                chairs: seat_chairs,
                seats,
                seat_rule: SeatRule::Coin,
                game_seed: g_seed,
                episode,
            });
            paired_opponents[learning_chair].push(opp_ref);
            paired_games_x[learning_chair].push(ref_game_id);
            paired_games_y[learning_chair].push(sib_game_id);
        }
    }

    let paired_builds: Vec<PairedBuildRecord> = (0..SEATS)
        .filter_map(|chair| {
            let deck_y = sib_deck_id[chair]?;
            Some(PairedBuildRecord {
                kind: PairedBuildKind::Sibling,
                chair: chair as u8,
                deck_x: ref_deck_id[chair],
                deck_y,
                opponents: paired_opponents[chair].clone(),
                games_x: paired_games_x[chair].clone(),
                games_y: paired_games_y[chair].clone(),
                q: None,
            })
        })
        .collect();

    // 4. Execute all 70 games across worker threads and assemble in logical order.
    let exec_opts = ExecOptions {
        caps: config.caps,
        watchdog_override: None,
        workers: config.workers,
        host: "local",
        stop_on_watchdog: true,
        on_game_complete: None,
    };
    let exec_out = execute_game_tasks_with_options(&tasks, &built_decks, pilot, &exec_opts)?;

    let (halts, status, stop) = collect_halts_and_stop(&exec_out.games, true);
    let record = PodRecord {
        schema: POD_RECORD_SCHEMA,
        interfaces: M4_INTERFACES_VERSION,
        pod_use: config.pod_use,
        namespace: config.namespace.clone(),
        run: config.run.clone(),
        pod_index: config.pod_index,
        seeds: PodSeeds {
            draft_seed: Some(draft_seed),
            pod_seed: Some(play_seed),
            league_seed: Some(league_seed),
            draft_seed_namespace: None,
            pod_seed_namespace: None,
        },
        pins: config.pins.clone(),
        pilot: config.pilot_info.clone(),
        caps: config.caps,
        draft: Some(seat_draft.record),
        chairs: chair_records,
        decks: deck_records,
        games: exec_out.games,
        paired_builds,
        halts,
        status,
        stop,
    };

    if record.status == PodStatus::Stopped {
        return Err(PodRunnerError::Stopped(Box::new(record)));
    }
    Ok(record)
}

/// Runs a batch of uniform training pods in `config.pod_indices` order.
/// Refuses client pods (`m4-interfaces/v2` change 1: client pods never enter a
/// training update's `pods.json`). Returns `Ok(records)` only when every pod
/// completes without stopping; otherwise returns the error so the caller
/// publishes nothing from the batch.
pub fn run_uniform_training_batch(
    pool: &CubePool,
    config: &UniformBatchConfig,
    pilot: &(dyn GamePilot + Sync),
) -> Result<Vec<PodRecord>, PodRunnerError> {
    if config.pod_use == PodUse::Client || config.namespace == CLIENT_NAMESPACE {
        return Err(PodRunnerError::InvalidPlan(
            "client pods never enter a training batch".to_owned(),
        ));
    }
    let mut records = Vec::with_capacity(config.pod_indices.len());
    for &pod_index in &config.pod_indices {
        let pod_config = TrainingPodConfig {
            pod_use: config.pod_use,
            namespace: config.namespace.clone(),
            run: config.run.clone(),
            pod_index,
            pins: config.pins.clone(),
            pilot_info: config.pilot_info.clone(),
            caps: config.caps,
            workers: config.workers,
        };
        let mut chairs = uniform_training_chairs(config.roles);
        let record = run_training_pod(pool, &pod_config, &mut chairs, pilot)?;
        records.push(record);
    }
    Ok(records)
}

/// Executes an explicit read plan (`seat_rule: "schedule"`, `max_replays: 0`).
/// Halts (including `terminal:watchdog` on execution 0) are recorded with one
/// attempt, null scores, and a matching ledger row, and the pod finishes with
/// `status = "complete"` and `stop = null` (`m4-interfaces/v2` change 5).
pub fn run_read_plan(
    plan: &ReadPlan,
    pilot: &(dyn GamePilot + Sync),
    workers: usize,
) -> Result<PodRecord, PodRunnerError> {
    validate_common_config(plan.pod_use, &plan.namespace, &plan.run, plan.caps)?;
    if plan.pod_use == PodUse::Client || plan.namespace == CLIENT_NAMESPACE {
        return Err(PodRunnerError::InvalidPlan(
            "client pods are not read plans".to_owned(),
        ));
    }
    if plan.caps.max_replays != READ_MAX_REPLAYS {
        return Err(PodRunnerError::InvalidCaps(format!(
            "read plan requires max_replays == 0 (reads never replay), got {}",
            plan.caps.max_replays
        )));
    }
    if plan.seeds.draft_seed.is_none() || plan.seeds.pod_seed.is_none() {
        return Err(PodRunnerError::InvalidPlan(
            "read plan requires non-null draft_seed and pod_seed".to_owned(),
        ));
    }
    match (
        &plan.seeds.draft_seed_namespace,
        &plan.seeds.pod_seed_namespace,
    ) {
        (Some(d_ns), Some(p_ns)) if is_valid_token(d_ns) && is_valid_token(p_ns) => {}
        _ => {
            return Err(PodRunnerError::InvalidPlan(
                "read plan requires valid draft_seed_namespace and pod_seed_namespace".to_owned(),
            ));
        }
    }

    let mut deck_records = Vec::with_capacity(plan.decks.len());
    let mut built_decks = Vec::with_capacity(plan.decks.len());
    for (index, spec) in plan.decks.iter().enumerate() {
        let label = format!("decks[{index}]");
        validate_builder_and_dealt(spec.source, &spec.builder, spec.dealt.as_ref(), &label)?;
        if spec.source == DeckSource::Dealt
            && (spec.chair.is_some()
                || spec.build.is_some()
                || spec.build_seed.is_some()
                || spec.build_rows.is_some())
        {
            return Err(PodRunnerError::InvalidPlan(format!(
                "{label}: dealt deck must have chair, build, build_seed and build_rows null"
            )));
        }
        if spec.cards.len() != DECK_CARDS as usize {
            return Err(PodRunnerError::InvalidPlan(format!(
                "deck {index} has {} cards, expected {DECK_CARDS}",
                spec.cards.len()
            )));
        }
        for &card_id in &spec.cards {
            if usize::from(card_id) >= CARD_DEFS.len() {
                return Err(PodRunnerError::InvalidPlan(format!(
                    "deck {index} contains unknown card id {card_id}"
                )));
            }
        }
        let deck = canonical_deck(&spec.cards);
        deck_records.push(DeckRecord {
            deck: index as u32,
            source: spec.source,
            chair: spec.chair,
            build: spec.build,
            builder: spec.builder.clone(),
            build_seed: spec.build_seed,
            cards: deck.card_ids(),
            build_rows: spec.build_rows.clone(),
            dealt: spec.dealt.clone(),
        });
        built_decks.push(deck);
    }

    let mut tasks = Vec::with_capacity(plan.games.len());
    for (index, spec) in plan.games.iter().enumerate() {
        for &deck_id in &spec.decks {
            if deck_id as usize >= built_decks.len() {
                return Err(PodRunnerError::InvalidPlan(format!(
                    "game {index} references deck {deck_id} outside 0..{}",
                    built_decks.len()
                )));
            }
        }
        if spec.seats[0] == spec.seats[1] {
            return Err(PodRunnerError::InvalidPlan(format!(
                "game {index} has identical seats {:?}",
                spec.seats
            )));
        }
        tasks.push(GameTask {
            game: index as u32,
            group: spec.group,
            role: spec.role,
            pairing: spec.pairing,
            decks: spec.decks,
            chairs: spec.chairs,
            seats: spec.seats,
            seat_rule: SeatRule::Schedule,
            game_seed: spec.game_seed,
            episode: spec.episode,
        });
    }

    let mut prev_first_game: Option<u32> = None;
    for (index, pb) in plan.paired_builds.iter().enumerate() {
        if pb.deck_x as usize >= built_decks.len() || pb.deck_y as usize >= built_decks.len() {
            return Err(PodRunnerError::InvalidPlan(format!(
                "paired_builds[{index}] references an out-of-range deck"
            )));
        }
        if pb.games_x.len() != pb.games_y.len() || pb.opponents.is_empty() {
            return Err(PodRunnerError::InvalidPlan(format!(
                "paired_builds[{index}] has invalid opponents/games lengths"
            )));
        }
        if let Some(q) = pb.q {
            let expected_kind = match q {
                0 => PairedBuildKind::CurrentInitial,
                1 => PairedBuildKind::K1LastB1Last,
                2 => PairedBuildKind::CandidateStated,
                other => {
                    return Err(PodRunnerError::InvalidPlan(format!(
                        "paired_builds[{index}].q: invalid contrast index {other}"
                    )));
                }
            };
            if pb.kind != expected_kind {
                return Err(PodRunnerError::InvalidPlan(format!(
                    "paired_builds[{index}]: kind {:?} disagrees with q={q}",
                    pb.kind
                )));
            }
        } else if pb.kind == PairedBuildKind::K1LastB1Last {
            return Err(PodRunnerError::InvalidPlan(format!(
                "paired_builds[{index}]: k1_last_b1_last requires q=1"
            )));
        }
        if let Some(&first_g) = pb.games_x.first() {
            if let Some(prev) = prev_first_game {
                if first_g < prev {
                    return Err(PodRunnerError::InvalidPlan(format!(
                        "paired_builds[{index}] is out of logical games_x order ({first_g} < {prev})"
                    )));
                }
            }
            prev_first_game = Some(first_g);
        }
        for &opp in &pb.opponents {
            if opp as usize >= built_decks.len() {
                return Err(PodRunnerError::InvalidPlan(format!(
                    "paired_builds[{index}] references out-of-range opponent deck {opp}"
                )));
            }
        }
        for &game_id in pb.games_x.iter().chain(pb.games_y.iter()) {
            if game_id as usize >= tasks.len() {
                return Err(PodRunnerError::InvalidPlan(format!(
                    "paired_builds[{index}] references out-of-range game {game_id}"
                )));
            }
        }
    }

    let exec_opts = ExecOptions {
        caps: plan.caps,
        watchdog_override: None,
        workers,
        host: "local",
        stop_on_watchdog: false,
        on_game_complete: None,
    };
    let exec_out = execute_game_tasks_with_options(&tasks, &built_decks, pilot, &exec_opts)?;

    // Change 5: with max_replays == 0, halts are recorded and the pod completes.
    let (halts, status, stop) = collect_halts_and_stop(&exec_out.games, false);
    Ok(PodRecord {
        schema: POD_RECORD_SCHEMA,
        interfaces: M4_INTERFACES_VERSION,
        pod_use: plan.pod_use,
        namespace: plan.namespace.clone(),
        run: plan.run.clone(),
        pod_index: plan.pod_index,
        seeds: plan.seeds.clone(),
        pins: plan.pins.clone(),
        pilot: plan.pilot.clone(),
        caps: plan.caps,
        draft: None,
        chairs: plan.chairs.clone(),
        decks: deck_records,
        games: exec_out.games,
        paired_builds: plan.paired_builds.clone(),
        halts,
        status,
        stop,
    })
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

/// Summary counts for one pod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PodReport {
    pub pod_index: u64,
    pub status: PodStatus,
    pub total_games: u64,
    /// Games counted by their final attempt's [`GameEnd::kind`].
    pub games_by_end_kind: BTreeMap<String, u64>,
    /// Halted attempts counted by `stop_kind`, then `attempt`.
    pub halts_by_stop_kind_and_attempt: BTreeMap<String, BTreeMap<u8, u64>>,
    pub total_halts: u64,
    /// Total replay attempts (`attempt >= 1`) executed in the pod.
    pub replays: u64,
}

impl PodReport {
    pub fn from_record(record: &PodRecord) -> Self {
        let mut games_by_end_kind = BTreeMap::new();
        let mut replays = 0u64;
        for game in &record.games {
            if let Some(last) = game.attempts.last() {
                *games_by_end_kind
                    .entry(last.end.kind().to_owned())
                    .or_default() += 1;
            }
            replays += game.attempts.len().saturating_sub(1) as u64;
        }
        let mut halts_by_stop_kind_and_attempt: BTreeMap<String, BTreeMap<u8, u64>> =
            BTreeMap::new();
        for halt in &record.halts {
            *halts_by_stop_kind_and_attempt
                .entry(halt.stop_kind.clone())
                .or_default()
                .entry(halt.attempt)
                .or_default() += 1;
        }
        PodReport {
            pod_index: record.pod_index,
            status: record.status,
            total_games: record.games.len() as u64,
            games_by_end_kind,
            halts_by_stop_kind_and_attempt,
            total_halts: record.halts.len() as u64,
            replays,
        }
    }
}

/// Within-pair Pearson correlation of sibling results across one or more pods.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiblingCorrelationReport {
    /// Completed matched `(game_x, game_y)` pairs across all sibling entries.
    pub game_pairs: usize,
    /// Pearson correlation over matched game win fractions (`1.0`, `0.5`, `0.0`).
    pub game_correlation: Option<f64>,
    /// Sibling pairs with at least one completed matched game.
    pub build_pairs: usize,
    /// Pearson correlation over per-build mean win fractions across matched games.
    pub build_mean_correlation: Option<f64>,
}

/// Aggregated report over a slice of pods.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchReport {
    pub pods: Vec<PodReport>,
    pub games_by_end_kind: BTreeMap<String, u64>,
    pub halts_by_stop_kind_and_attempt: BTreeMap<String, BTreeMap<u8, u64>>,
    pub total_halts: u64,
    pub total_replays: u64,
    pub sibling_correlation: SiblingCorrelationReport,
}

fn deck_win_fraction(game: &PodGameRecord, deck_id: u32) -> Option<f64> {
    let scores = game.scores?;
    let score = if game.decks[0] == deck_id {
        scores[0]
    } else if game.decks[1] == deck_id {
        scores[1]
    } else {
        return None;
    };
    Some(0.5 * (f64::from(score) + 1.0))
}

fn pearson_correlation(xs: &[f64], ys: &[f64]) -> Option<f64> {
    let n = xs.len();
    if n < 2 || ys.len() != n {
        return None;
    }
    let n_f = n as f64;
    let mean_x = xs.iter().sum::<f64>() / n_f;
    let mean_y = ys.iter().sum::<f64>() / n_f;
    let mut sum_xy = 0.0f64;
    let mut sum_xx = 0.0f64;
    let mut sum_yy = 0.0f64;
    for (&x, &y) in xs.iter().zip(ys.iter()) {
        let dx = x - mean_x;
        let dy = y - mean_y;
        sum_xy += dx * dy;
        sum_xx += dx * dx;
        sum_yy += dy * dy;
    }
    if sum_xx == 0.0 || sum_yy == 0.0 {
        return None;
    }
    Some((sum_xy / (sum_xx * sum_yy).sqrt()).clamp(-1.0, 1.0))
}

/// Computes the within-pair correlation of sibling results over `records`.
pub fn sibling_correlation(records: &[PodRecord]) -> SiblingCorrelationReport {
    let mut game_xs = Vec::new();
    let mut game_ys = Vec::new();
    let mut build_xs = Vec::new();
    let mut build_ys = Vec::new();

    for record in records {
        for pair in &record.paired_builds {
            if pair.kind != PairedBuildKind::Sibling {
                continue;
            }
            let mut sum_x = 0.0f64;
            let mut sum_y = 0.0f64;
            let mut matched = 0usize;
            for (&gx_id, &gy_id) in pair.games_x.iter().zip(pair.games_y.iter()) {
                let Some(gx) = record.games.get(gx_id as usize) else {
                    continue;
                };
                let Some(gy) = record.games.get(gy_id as usize) else {
                    continue;
                };
                if let (Some(wx), Some(wy)) = (
                    deck_win_fraction(gx, pair.deck_x),
                    deck_win_fraction(gy, pair.deck_y),
                ) {
                    game_xs.push(wx);
                    game_ys.push(wy);
                    sum_x += wx;
                    sum_y += wy;
                    matched += 1;
                }
            }
            if matched > 0 {
                let count = matched as f64;
                build_xs.push(sum_x / count);
                build_ys.push(sum_y / count);
            }
        }
    }

    SiblingCorrelationReport {
        game_pairs: game_xs.len(),
        game_correlation: pearson_correlation(&game_xs, &game_ys),
        build_pairs: build_xs.len(),
        build_mean_correlation: pearson_correlation(&build_xs, &build_ys),
    }
}

impl BatchReport {
    pub fn from_records(records: &[PodRecord]) -> Self {
        let mut pods = Vec::with_capacity(records.len());
        let mut games_by_end_kind = BTreeMap::new();
        let mut halts_by_stop_kind_and_attempt: BTreeMap<String, BTreeMap<u8, u64>> =
            BTreeMap::new();
        let mut total_halts = 0u64;
        let mut total_replays = 0u64;

        for record in records {
            let report = PodReport::from_record(record);
            for (kind, &count) in &report.games_by_end_kind {
                *games_by_end_kind.entry(kind.clone()).or_default() += count;
            }
            for (stop_kind, attempts) in &report.halts_by_stop_kind_and_attempt {
                let entry = halts_by_stop_kind_and_attempt
                    .entry(stop_kind.clone())
                    .or_default();
                for (&attempt, &count) in attempts {
                    *entry.entry(attempt).or_default() += count;
                }
            }
            total_halts += report.total_halts;
            total_replays += report.replays;
            pods.push(report);
        }

        BatchReport {
            pods,
            games_by_end_kind,
            halts_by_stop_kind_and_attempt,
            total_halts,
            total_replays,
            sibling_correlation: sibling_correlation(records),
        }
    }
}

// ---------------------------------------------------------------------------
// Strict order-preserving JSON schema validator (`mtg-kernel-cube-pod-record/v2`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OrderedJson {
    Null,
    Bool(bool),
    U64(u64),
    I64(i64),
    F64,
    Str(String),
    Array(Vec<OrderedJson>),
    Object(Vec<(String, OrderedJson)>),
}

pub(crate) struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        JsonParser {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    pub(crate) fn parse_root(mut self) -> Result<OrderedJson, String> {
        let value = self.parse_value()?;
        self.skip_ws();
        if self.pos != self.bytes.len() {
            return Err(format!("trailing bytes at offset {}", self.pos));
        }
        Ok(value)
    }

    fn parse_value(&mut self) -> Result<OrderedJson, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null", OrderedJson::Null),
            Some(b't') => self.parse_literal(b"true", OrderedJson::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", OrderedJson::Bool(false)),
            Some(b'"') => self.parse_string().map(OrderedJson::Str),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(other) => Err(format!(
                "unexpected byte 0x{other:02x} at offset {}",
                self.pos
            )),
            None => Err("unexpected end of JSON".to_owned()),
        }
    }

    fn parse_literal(
        &mut self,
        expected: &[u8],
        value: OrderedJson,
    ) -> Result<OrderedJson, String> {
        if self.bytes[self.pos..].starts_with(expected) {
            self.pos += expected.len();
            Ok(value)
        } else {
            Err(format!("invalid literal at offset {}", self.pos))
        }
    }

    fn parse_hex4(&mut self) -> Result<u16, String> {
        let slice = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| format!("truncated unicode escape at offset {}", self.pos))?;
        let text = std::str::from_utf8(slice).map_err(|_| "invalid UTF-8 in unicode escape")?;
        let value = u16::from_str_radix(text, 16)
            .map_err(|_| format!("invalid hex escape \\u{text} at offset {}", self.pos))?;
        self.pos += 4;
        Ok(value)
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if self.peek() != Some(b'"') {
            return Err(format!("expected '\"' at offset {}", self.pos));
        }
        self.pos += 1;
        let mut out = String::new();
        while let Some(&b) = self.bytes.get(self.pos) {
            if b == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.pos += 1;
                let esc = *self.bytes.get(self.pos).ok_or("truncated string escape")?;
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let first = self.parse_hex4()?;
                        if (0xD800..=0xDBFF).contains(&first) {
                            if self.bytes.get(self.pos..self.pos + 2) != Some(b"\\u") {
                                return Err("unpaired high surrogate in string".to_owned());
                            }
                            self.pos += 2;
                            let second = self.parse_hex4()?;
                            if !(0xDC00..=0xDFFF).contains(&second) {
                                return Err("invalid low surrogate in string".to_owned());
                            }
                            let code = 0x10000
                                + (u32::from(first - 0xD800) << 10)
                                + u32::from(second - 0xDC00);
                            let ch = char::from_u32(code).ok_or("invalid unicode scalar")?;
                            out.push(ch);
                        } else {
                            let ch =
                                char::from_u32(u32::from(first)).ok_or("invalid unicode scalar")?;
                            out.push(ch);
                        }
                    }
                    other => {
                        return Err(format!(
                            "invalid escape '\\{}' at offset {}",
                            other as char, self.pos
                        ));
                    }
                }
            } else {
                let rest = std::str::from_utf8(&self.bytes[self.pos..])
                    .map_err(|_| "invalid UTF-8 in JSON string")?;
                let ch = rest.chars().next().ok_or("unexpected end of string")?;
                if (ch as u32) < 0x20 {
                    return Err(format!(
                        "unescaped control character at offset {}",
                        self.pos
                    ));
                }
                out.push(ch);
                self.pos += ch.len_utf8();
            }
        }
        Err("unterminated JSON string".to_owned())
    }

    fn parse_number(&mut self) -> Result<OrderedJson, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(format!("invalid number at offset {start}")),
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(format!("invalid fraction at offset {}", self.pos));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(format!("invalid exponent at offset {}", self.pos));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| "invalid UTF-8 in number")?;
        if is_float {
            text.parse::<f64>()
                .map_err(|_| format!("invalid float {text:?}"))?;
            Ok(OrderedJson::F64)
        } else if text.starts_with('-') {
            text.parse::<i64>()
                .map(OrderedJson::I64)
                .map_err(|_| format!("invalid i64 {text:?}"))
        } else {
            text.parse::<u64>()
                .map(OrderedJson::U64)
                .map_err(|_| format!("invalid u64 {text:?}"))
        }
    }

    fn parse_array(&mut self) -> Result<OrderedJson, String> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(OrderedJson::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(OrderedJson::Array(items));
                }
                _ => return Err(format!("expected ',' or ']' at offset {}", self.pos)),
            }
        }
    }

    fn parse_object(&mut self) -> Result<OrderedJson, String> {
        self.pos += 1;
        let mut entries: Vec<(String, OrderedJson)> = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(OrderedJson::Object(entries));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            if entries.iter().any(|(existing, _)| existing == &key) {
                return Err(format!("duplicate JSON key {key:?}"));
            }
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(format!("expected ':' at offset {}", self.pos));
            }
            self.pos += 1;
            let value = self.parse_value()?;
            entries.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(OrderedJson::Object(entries));
                }
                _ => return Err(format!("expected ',' or '}}' at offset {}", self.pos)),
            }
        }
    }
}

pub(crate) fn expect_object_keys<'a>(
    value: &'a OrderedJson,
    path: &str,
    expected: &[&str],
) -> Result<&'a [(String, OrderedJson)], String> {
    let OrderedJson::Object(entries) = value else {
        return Err(format!("{path}: expected JSON object"));
    };
    let actual: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
    if actual != expected {
        return Err(format!(
            "{path}: keys {actual:?} do not match frozen order {expected:?}"
        ));
    }
    Ok(entries)
}

pub(crate) fn expect_str<'a>(value: &'a OrderedJson, path: &str) -> Result<&'a str, String> {
    match value {
        OrderedJson::Str(text) => Ok(text.as_str()),
        _ => Err(format!("{path}: expected string")),
    }
}

pub(crate) fn expect_opt_str<'a>(
    value: &'a OrderedJson,
    path: &str,
) -> Result<Option<&'a str>, String> {
    match value {
        OrderedJson::Null => Ok(None),
        OrderedJson::Str(text) => Ok(Some(text.as_str())),
        _ => Err(format!("{path}: expected string or null")),
    }
}

pub(crate) fn expect_u64(value: &OrderedJson, path: &str) -> Result<u64, String> {
    match value {
        OrderedJson::U64(n) => Ok(*n),
        _ => Err(format!("{path}: expected unsigned integer")),
    }
}

pub(crate) fn expect_opt_u64(value: &OrderedJson, path: &str) -> Result<Option<u64>, String> {
    match value {
        OrderedJson::Null => Ok(None),
        OrderedJson::U64(n) => Ok(Some(*n)),
        _ => Err(format!("{path}: expected unsigned integer or null")),
    }
}

pub(crate) fn expect_bool(value: &OrderedJson, path: &str) -> Result<bool, String> {
    match value {
        OrderedJson::Bool(b) => Ok(*b),
        _ => Err(format!("{path}: expected boolean")),
    }
}

pub(crate) fn expect_array<'a>(
    value: &'a OrderedJson,
    path: &str,
) -> Result<&'a [OrderedJson], String> {
    match value {
        OrderedJson::Array(items) => Ok(items.as_slice()),
        _ => Err(format!("{path}: expected array")),
    }
}

pub(crate) fn validate_seat_ref(value: &OrderedJson, path: &str) -> Result<(), String> {
    let entries = expect_object_keys(value, path, &["kind", "pin"])?;
    let kind = expect_str(&entries[0].1, &format!("{path}.kind"))?;
    if !matches!(kind, "uniform" | "learned" | "stated") {
        return Err(format!("{path}.kind: invalid value {kind:?}"));
    }
    let pin = expect_opt_str(&entries[1].1, &format!("{path}.pin"))?;
    match kind {
        "uniform" | "stated" => {
            if pin.is_some() {
                return Err(format!("{path}.pin: expected null for {kind}"));
            }
        }
        "learned" => {
            if pin.is_none() {
                return Err(format!("{path}.pin: expected pin string for learned"));
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_builder_ref(
    value: &OrderedJson,
    source: &str,
    dealt: &OrderedJson,
    path: &str,
) -> Result<(), String> {
    let entries = expect_object_keys(value, path, &["kind", "pin"])?;
    let kind = expect_str(&entries[0].1, &format!("{path}.kind"))?;
    match source {
        "build" => {
            if !matches!(kind, "uniform" | "learned" | "stated") {
                return Err(format!("{path}.kind: invalid build builder kind {kind:?}"));
            }
            let pin = expect_opt_str(&entries[1].1, &format!("{path}.pin"))?;
            match kind {
                "uniform" | "stated" if pin.is_some() => {
                    return Err(format!("{path}.pin: expected null for {kind}"));
                }
                "learned" if pin.is_none() => {
                    return Err(format!("{path}.pin: expected string for learned"));
                }
                _ => {}
            }
        }
        "dealt" => {
            if kind != "dealt" {
                return Err(format!(
                    "{path}.kind: dealt deck requires builder.kind \"dealt\", got {kind:?}"
                ));
            }
            let pin_obj =
                expect_object_keys(&entries[1].1, &format!("{path}.pin"), &["function", "key"])?;
            let _ = expect_str(&pin_obj[0].1, &format!("{path}.pin.function"))?;
            if &entries[1].1 != dealt {
                return Err(format!(
                    "{path}.pin: must equal deck.dealt {{function, key}}"
                ));
            }
        }
        other => return Err(format!("{path}: unknown deck source {other:?}")),
    }
    Ok(())
}

fn validate_decision_row(value: &OrderedJson, path: &str) -> Result<(), String> {
    let entries = expect_object_keys(
        value,
        path,
        &[
            "kind",
            "sampler",
            "options",
            "chosen",
            "stream_values",
            "logits_bits",
            "p_chosen_bits",
            "value_bits",
        ],
    )?;
    let kind = expect_str(&entries[0].1, &format!("{path}.kind"))?;
    if !matches!(kind, "pick" | "count" | "pointer" | "type_set" | "split") {
        return Err(format!("{path}.kind: invalid value {kind:?}"));
    }
    let _ = expect_str(&entries[1].1, &format!("{path}.sampler"))?;
    for (idx, item) in expect_array(&entries[2].1, &format!("{path}.options"))?
        .iter()
        .enumerate()
    {
        let _ = expect_u64(item, &format!("{path}.options[{idx}]"))?;
    }
    let _ = expect_u64(&entries[3].1, &format!("{path}.chosen"))?;
    for (idx, item) in expect_array(&entries[4].1, &format!("{path}.stream_values"))?
        .iter()
        .enumerate()
    {
        let _ = expect_u64(item, &format!("{path}.stream_values[{idx}]"))?;
    }
    match &entries[5].1 {
        OrderedJson::Null => {}
        OrderedJson::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                let _ = expect_u64(item, &format!("{path}.logits_bits[{idx}]"))?;
            }
        }
        _ => return Err(format!("{path}.logits_bits: expected array or null")),
    }
    expect_opt_u64(&entries[6].1, &format!("{path}.p_chosen_bits"))?;
    expect_opt_u64(&entries[7].1, &format!("{path}.value_bits"))?;
    Ok(())
}

fn validate_build_rows(value: &OrderedJson, path: &str) -> Result<(), String> {
    let entries = expect_object_keys(value, path, &["count", "pointer", "type_set", "split"])?;
    validate_decision_row(&entries[0].1, &format!("{path}.count"))?;
    for (idx, row) in expect_array(&entries[1].1, &format!("{path}.pointer"))?
        .iter()
        .enumerate()
    {
        validate_decision_row(row, &format!("{path}.pointer[{idx}]"))?;
    }
    if !matches!(entries[2].1, OrderedJson::Null) {
        validate_decision_row(&entries[2].1, &format!("{path}.type_set"))?;
    }
    for (idx, row) in expect_array(&entries[3].1, &format!("{path}.split"))?
        .iter()
        .enumerate()
    {
        validate_decision_row(row, &format!("{path}.split[{idx}]"))?;
    }
    Ok(())
}

fn validate_game_end(value: &OrderedJson, path: &str) -> Result<(), String> {
    let OrderedJson::Object(entries) = value else {
        return Err(format!("{path}: expected object"));
    };
    let Some((first_key, first_val)) = entries.first() else {
        return Err(format!("{path}: empty GameEnd object"));
    };
    if first_key != "kind" {
        return Err(format!("{path}: first key must be \"kind\""));
    }
    let kind = expect_str(first_val, &format!("{path}.kind"))?;
    match kind {
        "natural" => {
            let e = expect_object_keys(value, path, &["kind", "winner_seat", "reason"])?;
            expect_opt_u64(&e[1].1, &format!("{path}.winner_seat"))?;
            let _ = expect_str(&e[2].1, &format!("{path}.reason"))?;
        }
        "terminal" => {
            let e = expect_object_keys(value, path, &["kind", "code", "classification", "reason"])?;
            let _ = expect_str(&e[1].1, &format!("{path}.code"))?;
            let _ = expect_str(&e[2].1, &format!("{path}.classification"))?;
            let _ = expect_str(&e[3].1, &format!("{path}.reason"))?;
        }
        "session_error" => {
            let e = expect_object_keys(value, path, &["kind", "code", "message"])?;
            let _ = expect_str(&e[1].1, &format!("{path}.code"))?;
            let _ = expect_str(&e[2].1, &format!("{path}.message"))?;
        }
        "decision_cap" => {
            let _ = expect_object_keys(value, path, &["kind"])?;
        }
        "panic" => {
            let e = expect_object_keys(value, path, &["kind", "message"])?;
            let _ = expect_str(&e[1].1, &format!("{path}.message"))?;
        }
        other => return Err(format!("{path}.kind: unknown GameEnd kind {other:?}")),
    }
    Ok(())
}

/// Validates that `json_text` conforms to `mtg-kernel-cube-pod-record/v2`,
/// with every object's keys in the exact frozen order of `m4-interfaces/v2`
/// section 4 and no missing, extra, or duplicate fields.
pub fn validate_pod_record_json(json_text: &str) -> Result<(), String> {
    let root = JsonParser::new(json_text).parse_root()?;
    let top = expect_object_keys(
        &root,
        "root",
        &[
            "schema",
            "interfaces",
            "use",
            "namespace",
            "run",
            "pod_index",
            "seeds",
            "pins",
            "pilot",
            "caps",
            "draft",
            "chairs",
            "decks",
            "games",
            "paired_builds",
            "halts",
            "status",
            "stop",
        ],
    )?;

    if expect_str(&top[0].1, "schema")? != POD_RECORD_SCHEMA {
        return Err("schema mismatch".to_owned());
    }
    if expect_str(&top[1].1, "interfaces")? != M4_INTERFACES_VERSION {
        return Err("interfaces version mismatch".to_owned());
    }
    let pod_use = expect_str(&top[2].1, "use")?;
    if !matches!(
        pod_use,
        "training"
            | "shakedown"
            | "dose"
            | "read_a"
            | "read_b"
            | "paired_build"
            | "sizing"
            | "test"
            | "client"
    ) {
        return Err(format!("use: invalid value {pod_use:?}"));
    }
    let namespace = expect_str(&top[3].1, "namespace")?;
    if !is_valid_token(namespace) {
        return Err(format!("namespace {namespace:?} does not match [a-z0-9-]+"));
    }
    if (pod_use == "client") != (namespace == CLIENT_NAMESPACE) {
        return Err(format!(
            "client use and namespace {CLIENT_NAMESPACE:?} must be paired (got use={pod_use:?}, namespace={namespace:?})"
        ));
    }
    let run = expect_str(&top[4].1, "run")?;
    if !is_valid_token(run) {
        return Err(format!("run {run:?} does not match [a-z0-9-]+"));
    }
    let _ = expect_u64(&top[5].1, "pod_index")?;

    let is_read_use = matches!(
        pod_use,
        "dose" | "read_a" | "read_b" | "paired_build" | "sizing"
    );
    let is_read_record = match &top[6].1 {
        OrderedJson::Object(entries)
            if entries.len() == 5 && (is_read_use || pod_use == "test") =>
        {
            let seeds = expect_object_keys(
                &top[6].1,
                "seeds",
                &[
                    "draft_seed",
                    "pod_seed",
                    "league_seed",
                    "draft_seed_namespace",
                    "pod_seed_namespace",
                ],
            )?;
            let _ = expect_u64(&seeds[0].1, "seeds.draft_seed")?;
            let _ = expect_u64(&seeds[1].1, "seeds.pod_seed")?;
            let _ = expect_opt_u64(&seeds[2].1, "seeds.league_seed")?;
            let d_ns = expect_str(&seeds[3].1, "seeds.draft_seed_namespace")?;
            let p_ns = expect_str(&seeds[4].1, "seeds.pod_seed_namespace")?;
            if !is_valid_token(d_ns) || !is_valid_token(p_ns) {
                return Err("seeds source namespaces must match [a-z0-9-]+".to_owned());
            }
            true
        }
        _ if !is_read_use => {
            let seeds = expect_object_keys(
                &top[6].1,
                "seeds",
                &["draft_seed", "pod_seed", "league_seed"],
            )?;
            let _ = expect_opt_u64(&seeds[0].1, "seeds.draft_seed")?;
            let _ = expect_opt_u64(&seeds[1].1, "seeds.pod_seed")?;
            let _ = expect_opt_u64(&seeds[2].1, "seeds.league_seed")?;
            false
        }
        _ => {
            return Err(
                "seeds: read pod requires [draft_seed, pod_seed, league_seed, draft_seed_namespace, pod_seed_namespace]"
                    .to_owned(),
            );
        }
    };

    let pins = expect_object_keys(
        &top[7].1,
        "pins",
        &[
            "binary",
            "cube_list",
            "fence_record",
            "card_index",
            "cube_table",
        ],
    )?;
    for (key, val) in pins {
        let _ = expect_opt_str(val, &format!("pins.{key}"))?;
    }

    let pilot = expect_object_keys(&top[8].1, "pilot", &["kind", "pin", "decoding"])?;
    let pilot_kind = expect_str(&pilot[0].1, "pilot.kind")?;
    if !matches!(pilot_kind, "uniform" | "checkpoint") {
        return Err(format!("pilot.kind: invalid value {pilot_kind:?}"));
    }
    let _ = expect_opt_str(&pilot[1].1, "pilot.pin")?;
    let _ = expect_str(&pilot[2].1, "pilot.decoding")?;

    let caps = expect_object_keys(
        &top[9].1,
        "caps",
        &[
            "max_decisions",
            "max_physical_decisions",
            "max_policy_steps",
            "max_stack_objects",
            "watchdog_seconds",
            "max_replays",
        ],
    )?;
    for (key, val) in caps {
        let _ = expect_u64(val, &format!("caps.{key}"))?;
    }
    let max_stack_objects = expect_u64(&caps[3].1, "caps.max_stack_objects")?;
    if max_stack_objects != u64::from(A2_MAX_STACK_OBJECTS) {
        return Err(format!(
            "caps.max_stack_objects: expected {A2_MAX_STACK_OBJECTS}, got {max_stack_objects}"
        ));
    }
    let max_replays = expect_u64(&caps[5].1, "caps.max_replays")?;
    if is_read_record && max_replays != 0 {
        return Err(format!(
            "caps.max_replays: read pod requires 0, got {max_replays}"
        ));
    }

    if !matches!(top[10].1, OrderedJson::Null) {
        let draft = expect_object_keys(
            &top[10].1,
            "draft",
            &["schema", "draft_seed", "packs", "picks", "pools"],
        )?;
        let _ = expect_str(&draft[0].1, "draft.schema")?;
        let _ = expect_u64(&draft[1].1, "draft.draft_seed")?;
        let _ = expect_array(&draft[2].1, "draft.packs")?;
        for (idx, pick) in expect_array(&draft[3].1, "draft.picks")?.iter().enumerate() {
            let p = expect_object_keys(
                pick,
                &format!("draft.picks[{idx}]"),
                &["seat", "round", "pick", "pack", "chosen"],
            )?;
            let _ = expect_u64(&p[0].1, "draft.picks.seat")?;
            let _ = expect_u64(&p[1].1, "draft.picks.round")?;
            let _ = expect_u64(&p[2].1, "draft.picks.pick")?;
            let _ = expect_array(&p[3].1, "draft.picks.pack")?;
            let _ = expect_u64(&p[4].1, "draft.picks.chosen")?;
        }
        let _ = expect_array(&draft[4].1, "draft.pools")?;
    }

    for (idx, chair) in expect_array(&top[11].1, "chairs")?.iter().enumerate() {
        let path = format!("chairs[{idx}]");
        let c = expect_object_keys(chair, &path, &["chair", "role", "picker", "pick_rows"])?;
        let _ = expect_u64(&c[0].1, &format!("{path}.chair"))?;
        let role = expect_str(&c[1].1, &format!("{path}.role"))?;
        if !matches!(role, "learning" | "older" | "fixture") {
            return Err(format!("{path}.role: invalid value {role:?}"));
        }
        validate_seat_ref(&c[2].1, &format!("{path}.picker"))?;
        for (r_idx, row) in expect_array(&c[3].1, &format!("{path}.pick_rows"))?
            .iter()
            .enumerate()
        {
            validate_decision_row(row, &format!("{path}.pick_rows[{r_idx}]"))?;
        }
    }

    for (idx, deck) in expect_array(&top[12].1, "decks")?.iter().enumerate() {
        let path = format!("decks[{idx}]");
        let d = expect_object_keys(
            deck,
            &path,
            &[
                "deck",
                "source",
                "chair",
                "build",
                "builder",
                "build_seed",
                "cards",
                "build_rows",
                "dealt",
            ],
        )?;
        let _ = expect_u64(&d[0].1, &format!("{path}.deck"))?;
        let source = expect_str(&d[1].1, &format!("{path}.source"))?;
        if !matches!(source, "build" | "dealt") {
            return Err(format!("{path}.source: invalid value {source:?}"));
        }
        let chair = expect_opt_u64(&d[2].1, &format!("{path}.chair"))?;
        let build = expect_opt_u64(&d[3].1, &format!("{path}.build"))?;
        if !matches!(d[8].1, OrderedJson::Null) {
            let dealt =
                expect_object_keys(&d[8].1, &format!("{path}.dealt"), &["function", "key"])?;
            let _ = expect_str(&dealt[0].1, &format!("{path}.dealt.function"))?;
        }
        validate_builder_ref(&d[4].1, source, &d[8].1, &format!("{path}.builder"))?;
        let build_seed = expect_opt_u64(&d[5].1, &format!("{path}.build_seed"))?;
        if source == "dealt"
            && (chair.is_some()
                || build.is_some()
                || build_seed.is_some()
                || !matches!(d[7].1, OrderedJson::Null)
                || matches!(d[8].1, OrderedJson::Null))
        {
            return Err(format!("{path}: dealt deck has invalid null pattern"));
        }
        if source == "build"
            && (chair.is_none() || build.is_none() || !matches!(d[8].1, OrderedJson::Null))
        {
            return Err(format!(
                "{path}: build deck requires non-null chair and build and dealt == null"
            ));
        }
        let cards = expect_array(&d[6].1, &format!("{path}.cards"))?;
        if cards.len() != DECK_CARDS as usize {
            return Err(format!(
                "{path}.cards: expected {DECK_CARDS} cards, got {}",
                cards.len()
            ));
        }
        for (c_idx, card) in cards.iter().enumerate() {
            let _ = expect_u64(card, &format!("{path}.cards[{c_idx}]"))?;
        }
        if !matches!(d[7].1, OrderedJson::Null) {
            validate_build_rows(&d[7].1, &format!("{path}.build_rows"))?;
        }
    }

    for (idx, game) in expect_array(&top[13].1, "games")?.iter().enumerate() {
        let path = format!("games[{idx}]");
        let g = expect_object_keys(
            game,
            &path,
            &[
                "game",
                "group",
                "role",
                "pairing",
                "decks",
                "chairs",
                "seat_rule",
                "game_seed",
                "episode",
                "attempts",
                "scores",
            ],
        )?;
        let _ = expect_u64(&g[0].1, &format!("{path}.game"))?;
        let _ = expect_u64(&g[1].1, &format!("{path}.group"))?;
        let role = expect_str(&g[2].1, &format!("{path}.role"))?;
        if !matches!(
            role,
            "reference" | "sibling" | "cross" | "read_b" | "common"
        ) {
            return Err(format!("{path}.role: invalid value {role:?}"));
        }
        let _ = expect_opt_u64(&g[3].1, &format!("{path}.pairing"))?;
        let decks = expect_array(&g[4].1, &format!("{path}.decks"))?;
        if decks.len() != 2 {
            return Err(format!("{path}.decks: expected 2 deck ids"));
        }
        let _ = expect_u64(&decks[0], &format!("{path}.decks[0]"))?;
        let _ = expect_u64(&decks[1], &format!("{path}.decks[1]"))?;
        let chairs = expect_array(&g[5].1, &format!("{path}.chairs"))?;
        if chairs.len() != 2 {
            return Err(format!("{path}.chairs: expected 2 chair slots"));
        }
        let _ = expect_opt_u64(&chairs[0], &format!("{path}.chairs[0]"))?;
        let _ = expect_opt_u64(&chairs[1], &format!("{path}.chairs[1]"))?;
        let seat_rule = expect_str(&g[6].1, &format!("{path}.seat_rule"))?;
        if !matches!(seat_rule, "coin" | "schedule") {
            return Err(format!("{path}.seat_rule: invalid value {seat_rule:?}"));
        }
        if is_read_record && seat_rule != "schedule" {
            return Err(format!("{path}.seat_rule: read pod requires \"schedule\""));
        }
        let _ = expect_u64(&g[7].1, &format!("{path}.game_seed"))?;
        let _ = expect_u64(&g[8].1, &format!("{path}.episode"))?;
        let attempts = expect_array(&g[9].1, &format!("{path}.attempts"))?;
        if attempts.is_empty() || attempts.len() > 4 {
            return Err(format!(
                "{path}.attempts: expected 1..=4 attempts, got {}",
                attempts.len()
            ));
        }
        if is_read_record && attempts.len() != 1 {
            return Err(format!(
                "{path}.attempts: read pod requires 1 attempt, got {}",
                attempts.len()
            ));
        }
        for (a_idx, attempt) in attempts.iter().enumerate() {
            let ap = format!("{path}.attempts[{a_idx}]");
            let a = expect_object_keys(
                attempt,
                &ap,
                &[
                    "attempt",
                    "deal_seed",
                    "env_seed",
                    "action_seed",
                    "decisions",
                    "physical_decisions",
                    "policy_steps",
                    "end",
                    "halted",
                    "stop_kind",
                    "last_action_source",
                ],
            )?;
            let _ = expect_u64(&a[0].1, &format!("{ap}.attempt"))?;
            let _ = expect_u64(&a[1].1, &format!("{ap}.deal_seed"))?;
            let _ = expect_u64(&a[2].1, &format!("{ap}.env_seed"))?;
            let _ = expect_u64(&a[3].1, &format!("{ap}.action_seed"))?;
            let _ = expect_u64(&a[4].1, &format!("{ap}.decisions"))?;
            let _ = expect_opt_u64(&a[5].1, &format!("{ap}.physical_decisions"))?;
            let _ = expect_opt_u64(&a[6].1, &format!("{ap}.policy_steps"))?;
            validate_game_end(&a[7].1, &format!("{ap}.end"))?;
            let _ = expect_bool(&a[8].1, &format!("{ap}.halted"))?;
            let sk = expect_opt_str(&a[9].1, &format!("{ap}.stop_kind"))?;
            let src = expect_opt_u64(&a[10].1, &format!("{ap}.last_action_source"))?;
            if sk == Some(WATCHDOG_STOP_KIND) && src.is_some() {
                return Err(format!(
                    "{ap}.last_action_source: terminal:watchdog must not charge a card"
                ));
            }
        }
        match &g[10].1 {
            OrderedJson::Null => {}
            OrderedJson::Array(scores) if scores.len() == 2 => {
                for (s_idx, s) in scores.iter().enumerate() {
                    let valid = matches!(s, OrderedJson::U64(0 | 1) | OrderedJson::I64(-1..=1));
                    if !valid {
                        return Err(format!("{path}.scores[{s_idx}]: expected -1, 0, or 1"));
                    }
                }
            }
            _ => return Err(format!("{path}.scores: expected [i8; 2] or null")),
        }
    }

    let mut prev_first_game: Option<u64> = None;
    for (idx, pb) in expect_array(&top[14].1, "paired_builds")?
        .iter()
        .enumerate()
    {
        let path = format!("paired_builds[{idx}]");
        let p = expect_object_keys(
            pb,
            &path,
            &[
                "kind",
                "chair",
                "deck_x",
                "deck_y",
                "opponents",
                "games_x",
                "games_y",
                "q",
            ],
        )?;
        let kind = expect_str(&p[0].1, &format!("{path}.kind"))?;
        if !matches!(
            kind,
            "sibling" | "candidate_stated" | "current_initial" | "k1_last_b1_last"
        ) {
            return Err(format!("{path}.kind: invalid value {kind:?}"));
        }
        let _ = expect_u64(&p[1].1, &format!("{path}.chair"))?;
        let _ = expect_u64(&p[2].1, &format!("{path}.deck_x"))?;
        let _ = expect_u64(&p[3].1, &format!("{path}.deck_y"))?;
        for (i, v) in expect_array(&p[4].1, &format!("{path}.opponents"))?
            .iter()
            .enumerate()
        {
            let _ = expect_u64(v, &format!("{path}.opponents[{i}]"))?;
        }
        let games_x = expect_array(&p[5].1, &format!("{path}.games_x"))?;
        for (i, v) in games_x.iter().enumerate() {
            let g_id = expect_u64(v, &format!("{path}.games_x[{i}]"))?;
            if i == 0 {
                if let Some(prev) = prev_first_game {
                    if g_id < prev {
                        return Err(format!(
                            "{path}.games_x[0]: out of logical order ({g_id} < {prev})"
                        ));
                    }
                }
                prev_first_game = Some(g_id);
            }
        }
        for (i, v) in expect_array(&p[6].1, &format!("{path}.games_y"))?
            .iter()
            .enumerate()
        {
            let _ = expect_u64(v, &format!("{path}.games_y[{i}]"))?;
        }
        let q = expect_opt_u64(&p[7].1, &format!("{path}.q"))?;
        match q {
            None => {
                if kind == "k1_last_b1_last" {
                    return Err(format!("{path}.q: k1_last_b1_last requires q=1"));
                }
            }
            Some(0) if kind == "current_initial" => {}
            Some(1) if kind == "k1_last_b1_last" => {}
            Some(2) if kind == "candidate_stated" => {}
            Some(other) => {
                return Err(format!(
                    "{path}.q: value {other} does not match kind {kind:?}"
                ));
            }
        }
    }

    for (idx, halt) in expect_array(&top[15].1, "halts")?.iter().enumerate() {
        let path = format!("halts[{idx}]");
        let h = expect_object_keys(
            halt,
            &path,
            &[
                "game",
                "attempt",
                "stop_kind",
                "last_action_source",
                "deal_seed",
            ],
        )?;
        let _ = expect_u64(&h[0].1, &format!("{path}.game"))?;
        let _ = expect_u64(&h[1].1, &format!("{path}.attempt"))?;
        let _ = expect_str(&h[2].1, &format!("{path}.stop_kind"))?;
        let _ = expect_opt_u64(&h[3].1, &format!("{path}.last_action_source"))?;
        let _ = expect_u64(&h[4].1, &format!("{path}.deal_seed"))?;
    }

    let status = expect_str(&top[16].1, "status")?;
    if !matches!(status, "complete" | "stopped") {
        return Err(format!("status: invalid value {status:?}"));
    }

    match &top[17].1 {
        OrderedJson::Null => {
            if status == "stopped" {
                return Err("status is \"stopped\" but stop report is null".to_owned());
            }
        }
        other => {
            if status != "stopped" {
                return Err("stop report is non-null but status is not \"stopped\"".to_owned());
            }
            let stop =
                expect_object_keys(other, "stop", &["game", "pairing", "decks", "attempts"])?;
            let _ = expect_u64(&stop[0].1, "stop.game")?;
            let _ = expect_opt_u64(&stop[1].1, "stop.pairing")?;
            let decks = expect_array(&stop[2].1, "stop.decks")?;
            if decks.len() != 2 {
                return Err("stop.decks: expected 2 deck ids".to_owned());
            }
            let _ = expect_u64(&decks[0], "stop.decks[0]")?;
            let _ = expect_u64(&decks[1], "stop.decks[1]")?;
            for (idx, attempt) in expect_array(&stop[3].1, "stop.attempts")?
                .iter()
                .enumerate()
            {
                let ap = format!("stop.attempts[{idx}]");
                let a = expect_object_keys(
                    attempt,
                    &ap,
                    &["deal_seed", "stop_kind", "last_action_source"],
                )?;
                let _ = expect_u64(&a[0].1, &format!("{ap}.deal_seed"))?;
                let _ = expect_str(&a[1].1, &format!("{ap}.stop_kind"))?;
                let _ = expect_opt_u64(&a[2].1, &format!("{ap}.last_action_source"))?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod league_draw_tests {
    #[test]
    fn draws_snapshots_in_ascending_chair_order() {
        assert_eq!(super::draw_league_chairs(0, 9).unwrap(), [(1, 1), (7, 7)]);
        assert_eq!(super::draw_league_chairs(42, 9).unwrap(), [(5, 0), (6, 0)]);
        assert!(super::draw_league_chairs(0, 0).is_err());
    }
}
