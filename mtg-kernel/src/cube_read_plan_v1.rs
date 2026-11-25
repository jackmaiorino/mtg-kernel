//! Full `mtg-kernel-cube-read-plan/v1` support for D5:
//! - Strict order-preserving validation of manifests, read plans, prep
//!   requests, prep results, frozen two-stage `panel.json` documents, and
//!   execution receipts (`READ-PLAN-v1` sections 1-6).
//! - Prep mode (`run_prep_request`, `run_prep_set_execution`,
//!   `accept_prep_execution`): drafts and builds without playing or writing a
//!   pod record, writes unit-keyed receipts in `exec-<e>/prep/` and
//!   `exec-<e>/execution.json`, supports retries (`prep_retry`) after an
//!   interrupted execution with byte-identical output verification, and
//!   handles the two-stage panel (`prep-panel` exemption + `panel.json`
//!   verification).
//! - Plan-set execution directories (`exec-<e>/records/pod-<pod_index>.json`,
//!   `exec-<e>/timing/pod-<pod_index>.jsonl`, `exec-<e>/execution.json`) with
//!   play-time redraft/rebuild verification before game 0, e=0 continuation on
//!   `terminal:watchdog`, and e=1 immediate `watchdog_stop`.
//! - Process-death supervisor (`PodProgressJournal`,
//!   `finalize_stopped_record_from_journal`, `supervise_plan_set_execution`,
//!   `CommandPodWorkerLauncher`) writing the stopped pod record and stop
//!   report from the last durable game prefix and closing `execution.json`
//!   with `ended = "interrupted"`.

use crate::card_def::CARD_DEFS;
use crate::cube_draft_v1::{
    game_seed, pod_game_seats, pod_pairings, CubePool, Deck, DraftRecord, GameEnd, PickRecord,
    DECK_CARDS, DRAFT_RECORD_SCHEMA, POOL_CARDS, ROUNDS, SEATS,
};
use crate::cube_pod_runner_v1::{
    build_seed_for_chair, collect_halts_and_stop, execute_game_tasks_with_options, expect_array,
    expect_bool, expect_object_keys, expect_opt_str, expect_opt_u64, expect_str, expect_u64,
    is_sha256_hex, is_valid_token, seed, seed_keyed, validate_pod_record_json, AttemptRecord,
    AttemptTiming, BuilderKind, BuilderRef, ChairRecord, ChairRole, DealtOrigin, DeckRecord,
    DeckSource, ExecOptions, GameCompleteHook, GameRole, GameTask, JsonParser, OrderedJson,
    PairedBuildKind, PairedBuildRecord, PilotInfo, PilotKind, PodCaps, PodGameRecord, PodPins,
    PodRecord, PodRunnerError, PodSeeds, PodStatus, PodUse, SeatKind, SeatRef, SeatRule, SeedPart,
    StopAttemptReport, StopReport, A2_GAME_CAPS, DEFAULT_WATCHDOG_SECONDS, POD_RECORD_SCHEMA,
    READ_MAX_REPLAYS, WATCHDOG_STOP_KIND,
};
use crate::cube_seat_v1::{
    canonical_deck, run_seat_draft, BuildRows, DecisionKind, DecisionRow, GamePilot, SeatBuilder,
    SeatPicker, StatedSeatBuilder, UniformFourStepBuilder, UniformSeatPicker,
    M4_INTERFACES_VERSION, UNIFORM_SAMPLER_ID,
};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

/// Frozen schema tag of `mtg-kernel-cube-read-plan/v1` files.
pub const READ_PLAN_SCHEMA: &str = "mtg-kernel-cube-read-plan/v1";
/// Number of units in a production frozen panel (`[0, 32)`).
pub const PANEL_UNITS_END: u64 = 32;

// ---------------------------------------------------------------------------
// Canonical JSON serialization (`READ-PLAN-v1` section 1)
// ---------------------------------------------------------------------------

/// Lowercase 64-hex SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn escape_non_ascii_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if code < 0x80 {
            out.push(ch);
        } else if code <= 0xFFFF {
            let _ = write!(out, "\\u{code:04x}");
        } else {
            let mut buf = [0u16; 2];
            for unit in ch.encode_utf16(&mut buf) {
                let _ = write!(out, "\\u{unit:04x}");
            }
        }
    }
    out
}

/// Serializes `value` as canonical UTF-8 JSON matching Python's
/// `json.dumps(doc, indent=2, ensure_ascii=True) + "\n"` with LF line endings.
pub fn to_canonical_pretty_json<T: Serialize>(value: &T) -> Result<String, PodRunnerError> {
    let raw = serde_json::to_string_pretty(value)
        .map_err(|error| PodRunnerError::InvalidPlan(format!("JSON serialization error: {error}")))?;
    let mut escaped = escape_non_ascii_json(&raw);
    escaped.push('\n');
    Ok(escaped)
}

/// Serializes `value` as compact UTF-8 JSON matching Python's
/// `json.dumps(doc, separators=(",", ":"), ensure_ascii=True)` (no trailing LF).
pub fn to_canonical_compact_json<T: Serialize + ?Sized>(value: &T) -> Result<String, PodRunnerError> {
    let raw = serde_json::to_string(value)
        .map_err(|error| PodRunnerError::InvalidPlan(format!("JSON serialization error: {error}")))?;
    Ok(escape_non_ascii_json(&raw))
}

/// Computes `plan_set_sha256` from a manifest's `plans` array
/// (`sha256(json.dumps(plans, separators=(",", ":"), ensure_ascii=True))`).
pub fn compute_plan_set_sha256(entries: &[ManifestEntry]) -> Result<String, PodRunnerError> {
    let compact = to_canonical_compact_json(entries)?;
    Ok(sha256_hex(compact.as_bytes()))
}

fn write_new_bytes(path: &Path, bytes: &[u8]) -> Result<(), PodRunnerError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| PodRunnerError::Io(format!("{}: {error}", parent.display())))?;
    }
    let mut file = File::create_new(path)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", path.display())))?;
    file.write_all(bytes)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", path.display())))?;
    file.sync_all()
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", path.display())))?;
    Ok(())
}

fn write_timing_jsonl(path: &Path, timings: &[AttemptTiming]) -> Result<(), PodRunnerError> {
    let mut buf = String::new();
    for entry in timings {
        let line = to_canonical_compact_json(entry)?;
        buf.push_str(&line);
        buf.push('\n');
    }
    write_new_bytes(path, buf.as_bytes())
}

// ---------------------------------------------------------------------------
// Seat factory (pluggable pickers and builders; uniform/stated built in)
// ---------------------------------------------------------------------------

/// Resolves declared plan/prep picker and builder references into executable
/// [`SeatPicker`] and [`SeatBuilder`] instances.
pub trait SeatFactory: Sync {
    fn make_picker(&self, seat: &SeatRef) -> Result<Box<dyn SeatPicker>, PodRunnerError>;
    fn make_builder(&self, builder: &BuilderRef) -> Result<Box<dyn SeatBuilder>, PodRunnerError>;
}

/// Built-in factory supporting `uniform` pickers and `uniform`/`stated`
/// builders. Learned checkpoints are wired by D6 via a custom [`SeatFactory`].
#[derive(Debug, Clone, Copy, Default)]
pub struct UniformSeatFactory;

impl SeatFactory for UniformSeatFactory {
    fn make_picker(&self, seat: &SeatRef) -> Result<Box<dyn SeatPicker>, PodRunnerError> {
        match seat.kind {
            SeatKind::Uniform => {
                if seat.pin.is_some() {
                    return Err(PodRunnerError::InvalidPlan(
                        "uniform picker must have pin == null".to_owned(),
                    ));
                }
                Ok(Box::new(UniformSeatPicker))
            }
            SeatKind::Learned => Err(PodRunnerError::InvalidPlan(format!(
                "learned picker {:?} requires a checkpoint-capable SeatFactory",
                seat.pin
            ))),
            SeatKind::Stated => Err(PodRunnerError::InvalidPlan(
                "stated picker is not supported by UniformSeatFactory".to_owned(),
            )),
        }
    }

    fn make_builder(&self, builder: &BuilderRef) -> Result<Box<dyn SeatBuilder>, PodRunnerError> {
        match builder.kind {
            BuilderKind::Uniform => {
                if builder.pin.is_some() {
                    return Err(PodRunnerError::InvalidPlan(
                        "uniform builder must have pin == null".to_owned(),
                    ));
                }
                Ok(Box::new(UniformFourStepBuilder))
            }
            BuilderKind::Stated => {
                if builder.pin.is_some() {
                    return Err(PodRunnerError::InvalidPlan(
                        "stated builder must have pin == null".to_owned(),
                    ));
                }
                Ok(Box::new(StatedSeatBuilder))
            }
            BuilderKind::Learned => Err(PodRunnerError::InvalidPlan(format!(
                "learned builder {:?} requires a checkpoint-capable SeatFactory",
                builder.pin
            ))),
            BuilderKind::Dealt => Err(PodRunnerError::InvalidPlan(
                "dealt decks are not rebuilt via SeatBuilder".to_owned(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Read-plan, prep, manifest, panel and receipt types (`READ-PLAN-v1`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanFamily {
    Aa,
    A,
    AEarly,
    SizeA,
    SizeAEarly,
    AOne,
    AEarlyOne,
    AAll,
    AEarlyAll,
    B,
    C,
    SizeB,
    SizeC,
    Dose,
    D13,
}

impl PlanFamily {
    pub fn from_str_name(text: &str) -> Option<Self> {
        match text {
            "aa" => Some(PlanFamily::Aa),
            "a" => Some(PlanFamily::A),
            "a_early" => Some(PlanFamily::AEarly),
            "size_a" => Some(PlanFamily::SizeA),
            "size_a_early" => Some(PlanFamily::SizeAEarly),
            "a_one" => Some(PlanFamily::AOne),
            "a_early_one" => Some(PlanFamily::AEarlyOne),
            "a_all" => Some(PlanFamily::AAll),
            "a_early_all" => Some(PlanFamily::AEarlyAll),
            "b" => Some(PlanFamily::B),
            "c" => Some(PlanFamily::C),
            "size_b" => Some(PlanFamily::SizeB),
            "size_c" => Some(PlanFamily::SizeC),
            "dose" => Some(PlanFamily::Dose),
            "d13" => Some(PlanFamily::D13),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            PlanFamily::Aa => "aa",
            PlanFamily::A => "a",
            PlanFamily::AEarly => "a_early",
            PlanFamily::SizeA => "size_a",
            PlanFamily::SizeAEarly => "size_a_early",
            PlanFamily::AOne => "a_one",
            PlanFamily::AEarlyOne => "a_early_one",
            PlanFamily::AAll => "a_all",
            PlanFamily::AEarlyAll => "a_early_all",
            PlanFamily::B => "b",
            PlanFamily::C => "c",
            PlanFamily::SizeB => "size_b",
            PlanFamily::SizeC => "size_c",
            PlanFamily::Dose => "dose",
            PlanFamily::D13 => "d13",
        }
    }

    pub const fn has_prep(self) -> bool {
        matches!(
            self,
            PlanFamily::B
                | PlanFamily::C
                | PlanFamily::SizeB
                | PlanFamily::SizeC
                | PlanFamily::Dose
                | PlanFamily::D13
        )
    }

    pub const fn has_dealt(self) -> bool {
        self.has_prep()
    }

    pub const fn is_a_mirror(self) -> bool {
        matches!(
            self,
            PlanFamily::Aa
                | PlanFamily::A
                | PlanFamily::AEarly
                | PlanFamily::SizeA
                | PlanFamily::SizeAEarly
        )
    }

    pub const fn is_one_seat(self) -> bool {
        matches!(self, PlanFamily::AOne | PlanFamily::AEarlyOne)
    }

    pub const fn is_all_learned(self) -> bool {
        matches!(self, PlanFamily::AAll | PlanFamily::AEarlyAll)
    }

    pub const fn is_panel_family(self) -> bool {
        matches!(self, PlanFamily::Dose | PlanFamily::D13)
    }

    pub fn expected_use(self, namespace: &str) -> PodUse {
        if namespace == "test" {
            return PodUse::Test;
        }
        match self {
            PlanFamily::Aa
            | PlanFamily::A
            | PlanFamily::AEarly
            | PlanFamily::AOne
            | PlanFamily::AEarlyOne
            | PlanFamily::AAll
            | PlanFamily::AEarlyAll => PodUse::ReadA,
            PlanFamily::SizeA
            | PlanFamily::SizeAEarly
            | PlanFamily::SizeB
            | PlanFamily::SizeC => PodUse::Sizing,
            PlanFamily::B => PodUse::ReadB,
            PlanFamily::C | PlanFamily::D13 => PodUse::PairedBuild,
            PlanFamily::Dose => PodUse::Dose,
        }
    }

    pub fn is_valid_namespace(self, namespace: &str) -> bool {
        if namespace == "test" {
            return true;
        }
        match self {
            PlanFamily::Aa => namespace == "read-d8-aa",
            PlanFamily::A => namespace == "read-gate-a",
            PlanFamily::AEarly => namespace == "read-early-a",
            PlanFamily::SizeA => namespace == "size-gate-a",
            PlanFamily::SizeAEarly => namespace == "size-early-a",
            PlanFamily::AOne => namespace == "read-gate-a-one",
            PlanFamily::AEarlyOne => namespace == "read-early-a-one",
            PlanFamily::AAll => namespace == "read-gate-a-all",
            PlanFamily::AEarlyAll => namespace == "read-early-a-all",
            PlanFamily::B => namespace == "read-gate-b",
            PlanFamily::C => namespace == "read-gate-c",
            PlanFamily::SizeB => namespace == "size-gate-b",
            PlanFamily::SizeC => namespace == "size-gate-c",
            PlanFamily::Dose => matches!(namespace, "dose-d8" | "dose-m5"),
            PlanFamily::D13 => namespace == "panel-k1",
        }
    }

    pub fn expected_seed_namespaces<'a>(self, namespace: &'a str) -> (&'a str, &'a str) {
        if namespace == "test" {
            return ("test", "test");
        }
        match self {
            PlanFamily::AOne | PlanFamily::AAll => ("read-gate-a", namespace),
            PlanFamily::AEarlyOne | PlanFamily::AEarlyAll => ("read-early-a", namespace),
            PlanFamily::B | PlanFamily::C => ("read-gate-pools", "read-gate-pools"),
            _ => (namespace, namespace),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrepFamily {
    #[serde(rename = "b_c")]
    BC,
    B,
    C,
    SizeB,
    SizeC,
    Dose,
    D13,
}

impl PrepFamily {
    pub fn from_str_name(text: &str) -> Option<Self> {
        match text {
            "b_c" => Some(PrepFamily::BC),
            "b" => Some(PrepFamily::B),
            "c" => Some(PrepFamily::C),
            "size_b" => Some(PrepFamily::SizeB),
            "size_c" => Some(PrepFamily::SizeC),
            "dose" => Some(PrepFamily::Dose),
            "d13" => Some(PrepFamily::D13),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            PrepFamily::BC => "b_c",
            PrepFamily::B => "b",
            PrepFamily::C => "c",
            PrepFamily::SizeB => "size_b",
            PrepFamily::SizeC => "size_c",
            PrepFamily::Dose => "dose",
            PrepFamily::D13 => "d13",
        }
    }

    pub const fn is_panel_family(self) -> bool {
        matches!(self, PrepFamily::Dose | PrepFamily::D13)
    }

    pub fn is_valid_namespace(self, namespace: &str) -> bool {
        if namespace == "test" {
            return true;
        }
        match self {
            PrepFamily::BC => namespace == "read-gate-pools",
            PrepFamily::B | PrepFamily::C => false,
            PrepFamily::SizeB => namespace == "size-gate-b",
            PrepFamily::SizeC => namespace == "size-gate-c",
            PrepFamily::Dose => matches!(namespace, "dose-d8" | "dose-m5"),
            PrepFamily::D13 => namespace == "panel-k1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    Dose,
    Pilot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanMark {
    pub kind: MarkKind,
    pub label: String,
    pub ordinal: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanInputs {
    pub pool_sha256: Option<String>,
    pub constants_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanChair {
    pub chair: u8,
    pub role: ChairRole,
    pub picker: SeatRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeckRow {
    Candidate,
    Stated,
    Current,
    Initial,
    #[serde(rename = "b1_last")]
    B1Last,
    #[serde(rename = "k1_last")]
    K1Last,
    Opponent,
}

impl DeckRow {
    pub fn from_str_name(text: &str) -> Option<Self> {
        match text {
            "candidate" => Some(DeckRow::Candidate),
            "stated" => Some(DeckRow::Stated),
            "current" => Some(DeckRow::Current),
            "initial" => Some(DeckRow::Initial),
            "b1_last" => Some(DeckRow::B1Last),
            "k1_last" => Some(DeckRow::K1Last),
            "opponent" => Some(DeckRow::Opponent),
            _ => None,
        }
    }

    pub const fn expected_build_index(self) -> Option<u8> {
        match self {
            DeckRow::Candidate
            | DeckRow::Current
            | DeckRow::Initial
            | DeckRow::B1Last
            | DeckRow::K1Last => Some(0),
            DeckRow::Stated => Some(1),
            DeckRow::Opponent => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanDeck {
    pub deck: u32,
    pub source: DeckSource,
    pub chair: Option<u8>,
    pub build: Option<u8>,
    pub builder: BuilderRef,
    pub cards: Option<Vec<u16>>,
    pub dealt: Option<DealtOrigin>,
    pub row: DeckRow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanGame {
    pub game: u32,
    pub group: u32,
    pub role: GameRole,
    pub pairing: Option<u8>,
    pub decks: [u32; 2],
    pub chairs: [Option<u8>; 2],
    pub seat_rule: SeatRule,
    pub game_seed: u64,
    pub episode: u64,
}

/// Complete frozen `mtg-kernel-cube-read-plan/v1` plan document (24 keys in
/// section 2 order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadPlanV1 {
    pub schema: String,
    pub interfaces: String,
    pub family: PlanFamily,
    #[serde(rename = "use")]
    pub pod_use: PodUse,
    pub namespace: String,
    pub run: String,
    pub unit: u64,
    pub pod_index: u64,
    pub mirror: Option<u8>,
    pub mirror_pods: Option<[u64; 2]>,
    pub rotation: Option<u8>,
    pub mark: Option<PlanMark>,
    pub seeds: PodSeeds,
    pub pins: PodPins,
    pub pilot: PilotInfo,
    pub caps: PodCaps,
    pub seat_rule: SeatRule,
    pub inputs: PlanInputs,
    pub prep_sha256: Option<String>,
    pub chairs: Vec<PlanChair>,
    pub pools: Option<Vec<Vec<u16>>>,
    pub decks: Vec<PlanDeck>,
    pub games: Vec<PlanGame>,
    pub paired_builds: Vec<PairedBuildRecord>,
}

/// One declared build inside a [`PrepRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepBuildSpec {
    pub chair: u8,
    pub build: u8,
    pub builder: BuilderRef,
    pub row: DeckRow,
}

/// Prep request written by the planner (`requests/unit-<unit>.json`, 10 keys
/// in `READ-PLAN-v1` section 4 order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepRequest {
    pub schema: String,
    pub kind: String,
    pub family: PrepFamily,
    pub namespace: String,
    pub run: String,
    pub unit: u64,
    pub seeds: PodSeeds,
    pub pins: PodPins,
    pub chairs: Vec<PlanChair>,
    pub builds: Vec<PrepBuildSpec>,
}

/// One built deck inside a [`PrepResult`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepBuildResult {
    pub chair: u8,
    pub build: u8,
    pub builder: BuilderRef,
    pub row: DeckRow,
    pub build_seed: u64,
    pub cards: Vec<u16>,
}

/// Prep result written by D5 (`exec-<e>/prep/unit-<unit>.json`, 5 keys in
/// `READ-PLAN-v1` section 4 order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrepResult {
    pub schema: &'static str,
    pub kind: &'static str,
    pub request_sha256: String,
    pub draft: DraftRecord,
    pub builds: Vec<PrepBuildResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestKind {
    Plans,
    Prep,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPodEntry {
    pub pod_index: u64,
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestUnitEntry {
    pub unit: u64,
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ManifestEntry {
    Pod(ManifestPodEntry),
    Unit(ManifestUnitEntry),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionReason {
    First,
    WatchdogRerun,
    PrepRetry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestExecutionEntry {
    pub execution: u64,
    pub reason: ExecutionReason,
    pub expired_games: Vec<[u64; 2]>,
    pub binary: String,
    pub platform: String,
}

/// Set manifest (`manifest.json`, 19 keys in `READ-PLAN-v1` section 5 order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetManifest {
    pub schema: String,
    pub kind: ManifestKind,
    pub family: String,
    pub namespace: String,
    pub run: String,
    pub set: String,
    pub interfaces: String,
    pub declaration_sha256: String,
    pub look: Option<u8>,
    pub units: [u64; 2],
    pub reserved: Option<[u64; 2]>,
    pub pools_with: Option<String>,
    pub panel_sha256: Option<String>,
    pub prep_set: Option<String>,
    pub replaces: Option<String>,
    pub retired: Vec<[u64; 2]>,
    pub plans: Vec<ManifestEntry>,
    pub plan_set_sha256: String,
    pub executions: Vec<ManifestExecutionEntry>,
}

impl SetManifest {
    /// Returns `true` iff this set is the stage-1 `prep-panel` set (or a
    /// restart of it), which is exempt from pinned-panel checks.
    pub fn is_prep_panel(&self) -> bool {
        self.kind == ManifestKind::Prep
            && (self.set == "prep-panel" || self.set.starts_with("prep-panel-restart-"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanExecutionEnded {
    Complete,
    WatchdogStop,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionRecordEntry {
    pub pod_index: u64,
    pub sha256: String,
    pub status: PodStatus,
}

/// Plan-set execution receipt (`exec-<e>/execution.json`, 6 keys in
/// `READ-PLAN-v1` section 5 order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanExecutionReceipt {
    pub execution: u64,
    pub plan_set_sha256: String,
    pub binary: String,
    pub platform: String,
    pub ended: PlanExecutionEnded,
    pub records: Vec<ExecutionRecordEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrepExecutionEnded {
    Complete,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepOutputEntry {
    pub unit: u64,
    pub sha256: String,
    pub status: String,
}

/// Prep-set execution receipt (`exec-<e>/execution.json`, 6 keys in
/// `READ-PLAN-v1` section 5 order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepExecutionReceipt {
    pub execution: u64,
    pub plan_set_sha256: String,
    pub binary: String,
    pub platform: String,
    pub ended: PrepExecutionEnded,
    pub outputs: Vec<PrepOutputEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelPrepOutput {
    pub unit: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDraftEntry {
    pub unit: u64,
    pub seeds: PodSeeds,
    pub pools: Vec<Vec<u16>>,
    pub opponents: Vec<PlanDeck>,
    pub games: Vec<PlanGame>,
    pub built: Option<Vec<PlanDeck>>,
}

/// Stage-2 frozen panel document (`<root>/<namespace>/<run>/panel/panel.json`,
/// 11 keys in `READ-PLAN-v1` section 5 order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDocument {
    pub schema: String,
    pub kind: String,
    pub namespace: String,
    pub run: String,
    pub units: [u64; 2],
    pub inputs: PlanInputs,
    pub pins: PodPins,
    pub prep_set_sha256: String,
    pub prep_execution: u64,
    pub prep_outputs: Vec<PanelPrepOutput>,
    pub drafts: Vec<PanelDraftEntry>,
}

/// Runtime pin expectations checked by D5 before drafting or playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePins {
    pub binary: String,
    pub cube_list: String,
    pub fence_record: String,
    pub card_index: String,
    pub cube_table: String,
    pub pool_sha256: Option<String>,
    pub constants_sha256: Option<String>,
}

/// Runtime verification context supplied by the launcher/caller.
#[derive(Debug, Clone)]
pub struct RuntimeVerification {
    pub pins: RuntimePins,
    pub platform: String,
    pub host: String,
    /// Exact bytes of the A6 declaration file (or test binding file in `test`).
    pub declaration_bytes: Vec<u8>,
    /// Pinned panel digest for `dose` and `d13` sets other than `prep-panel`.
    pub pinned_panel_sha256: Option<String>,
}

// ---------------------------------------------------------------------------
// OrderedJson decoding helpers (strict order, integer bounds, no floats)
// ---------------------------------------------------------------------------

fn plan_err(msg: impl Into<String>) -> PodRunnerError {
    PodRunnerError::InvalidPlan(msg.into())
}

fn expect_u8(value: &OrderedJson, path: &str) -> Result<u8, String> {
    let n = expect_u64(value, path)?;
    u8::try_from(n).map_err(|_| format!("{path}: integer {n} exceeds u8"))
}

fn expect_opt_u8(value: &OrderedJson, path: &str) -> Result<Option<u8>, String> {
    expect_opt_u64(value, path)?
        .map(|n| u8::try_from(n).map_err(|_| format!("{path}: integer {n} exceeds u8")))
        .transpose()
}

fn expect_u16(value: &OrderedJson, path: &str) -> Result<u16, String> {
    let n = expect_u64(value, path)?;
    u16::try_from(n).map_err(|_| format!("{path}: integer {n} exceeds u16"))
}

fn expect_u32(value: &OrderedJson, path: &str) -> Result<u32, String> {
    let n = expect_u64(value, path)?;
    u32::try_from(n).map_err(|_| format!("{path}: integer {n} exceeds u32"))
}

fn expect_sha256<'a>(value: &'a OrderedJson, path: &str) -> Result<&'a str, String> {
    let text = expect_str(value, path)?;
    if !is_sha256_hex(text) {
        return Err(format!("{path}: expected 64-hex sha256 string, got {text:?}"));
    }
    Ok(text)
}

fn expect_opt_sha256<'a>(value: &'a OrderedJson, path: &str) -> Result<Option<&'a str>, String> {
    match expect_opt_str(value, path)? {
        None => Ok(None),
        Some(text) => {
            if !is_sha256_hex(text) {
                return Err(format!("{path}: expected 64-hex sha256 string, got {text:?}"));
            }
            Ok(Some(text))
        }
    }
}

fn ordered_to_value(value: &OrderedJson) -> Result<Value, String> {
    match value {
        OrderedJson::Null => Ok(Value::Null),
        OrderedJson::Bool(b) => Ok(Value::Bool(*b)),
        OrderedJson::U64(n) => Ok(Value::from(*n)),
        OrderedJson::I64(n) => Ok(Value::from(*n)),
        OrderedJson::F64 => Err("floating-point numbers are forbidden in read-plan JSON".to_owned()),
        OrderedJson::Str(s) => Ok(Value::String(s.clone())),
        OrderedJson::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(ordered_to_value(item)?);
            }
            Ok(Value::Array(out))
        }
        OrderedJson::Object(entries) => {
            let mut map = serde_json::Map::new();
            for (k, v) in entries {
                map.insert(k.clone(), ordered_to_value(v)?);
            }
            Ok(Value::Object(map))
        }
    }
}

fn parse_seeds_v1(value: &OrderedJson, path: &str) -> Result<PodSeeds, String> {
    let entries = expect_object_keys(
        value,
        path,
        &[
            "draft_seed",
            "pod_seed",
            "league_seed",
            "draft_seed_namespace",
            "pod_seed_namespace",
        ],
    )?;
    let draft_seed = expect_u64(&entries[0].1, &format!("{path}.draft_seed"))?;
    let pod_seed = expect_u64(&entries[1].1, &format!("{path}.pod_seed"))?;
    let league_seed = expect_opt_u64(&entries[2].1, &format!("{path}.league_seed"))?;
    if league_seed.is_some() {
        return Err(format!(
            "{path}.league_seed: must be null in every mtg-kernel-cube-read-plan/v1 family"
        ));
    }
    let d_ns = expect_str(&entries[3].1, &format!("{path}.draft_seed_namespace"))?;
    let p_ns = expect_str(&entries[4].1, &format!("{path}.pod_seed_namespace"))?;
    if !is_valid_token(d_ns) || !is_valid_token(p_ns) {
        return Err(format!("{path}: seed namespaces must match [a-z0-9-]+"));
    }
    Ok(PodSeeds::for_read(
        draft_seed,
        pod_seed,
        league_seed,
        d_ns,
        p_ns,
    ))
}

fn parse_pins_non_null(value: &OrderedJson, path: &str) -> Result<PodPins, String> {
    let entries = expect_object_keys(
        value,
        path,
        &[
            "binary",
            "cube_list",
            "fence_record",
            "card_index",
            "cube_table",
        ],
    )?;
    Ok(PodPins {
        binary: Some(expect_sha256(&entries[0].1, &format!("{path}.binary"))?.to_owned()),
        cube_list: Some(expect_sha256(&entries[1].1, &format!("{path}.cube_list"))?.to_owned()),
        fence_record: Some(
            expect_sha256(&entries[2].1, &format!("{path}.fence_record"))?.to_owned(),
        ),
        card_index: Some(expect_sha256(&entries[3].1, &format!("{path}.card_index"))?.to_owned()),
        cube_table: Some(expect_sha256(&entries[4].1, &format!("{path}.cube_table"))?.to_owned()),
    })
}

fn parse_pilot_v1(value: &OrderedJson, path: &str) -> Result<PilotInfo, String> {
    let entries = expect_object_keys(value, path, &["kind", "pin", "decoding"])?;
    let kind_str = expect_str(&entries[0].1, &format!("{path}.kind"))?;
    let pin = expect_opt_sha256(&entries[1].1, &format!("{path}.pin"))?.map(str::to_owned);
    let decoding = expect_str(&entries[2].1, &format!("{path}.decoding"))?.to_owned();
    let kind = match kind_str {
        "uniform" => {
            if pin.is_some() {
                return Err(format!("{path}.pin: must be null for uniform pilot"));
            }
            if decoding != UNIFORM_SAMPLER_ID {
                return Err(format!(
                    "{path}.decoding: uniform pilot requires {UNIFORM_SAMPLER_ID:?}, got {decoding:?}"
                ));
            }
            PilotKind::Uniform
        }
        "checkpoint" => {
            if pin.is_none() {
                return Err(format!(
                    "{path}.pin: checkpoint pilot requires non-null sha256 pin"
                ));
            }
            if decoding.is_empty() {
                return Err(format!("{path}.decoding: must be non-empty"));
            }
            PilotKind::Checkpoint
        }
        other => return Err(format!("{path}.kind: unknown pilot kind {other:?}")),
    };
    Ok(PilotInfo {
        kind,
        pin,
        decoding,
    })
}

fn parse_caps_a67(value: &OrderedJson, path: &str) -> Result<PodCaps, String> {
    let entries = expect_object_keys(
        value,
        path,
        &[
            "max_decisions",
            "max_physical_decisions",
            "max_policy_steps",
            "watchdog_seconds",
            "max_replays",
        ],
    )?;
    let max_decisions = expect_u64(&entries[0].1, &format!("{path}.max_decisions"))?;
    let max_physical_decisions =
        expect_u64(&entries[1].1, &format!("{path}.max_physical_decisions"))?;
    let max_policy_steps = expect_u64(&entries[2].1, &format!("{path}.max_policy_steps"))?;
    let watchdog_seconds = expect_u64(&entries[3].1, &format!("{path}.watchdog_seconds"))?;
    let max_replays = expect_u8(&entries[4].1, &format!("{path}.max_replays"))?;
    if max_decisions != A2_GAME_CAPS.max_decisions
        || max_physical_decisions != A2_GAME_CAPS.max_physical_decisions
        || max_policy_steps != A2_GAME_CAPS.max_policy_steps
        || watchdog_seconds != DEFAULT_WATCHDOG_SECONDS
        || max_replays != READ_MAX_REPLAYS
    {
        return Err(format!(
            "{path}: read plan requires A6.7 caps (16384/16384/32768/600/0), got ({max_decisions}/{max_physical_decisions}/{max_policy_steps}/{watchdog_seconds}/{max_replays})"
        ));
    }
    Ok(PodCaps {
        max_decisions,
        max_physical_decisions,
        max_policy_steps,
        watchdog_seconds,
        max_replays,
    })
}

fn parse_plan_inputs(
    value: &OrderedJson,
    expect_non_null: bool,
    path: &str,
) -> Result<PlanInputs, String> {
    let entries = expect_object_keys(value, path, &["pool_sha256", "constants_sha256"])?;
    let pool_sha256 =
        expect_opt_sha256(&entries[0].1, &format!("{path}.pool_sha256"))?.map(str::to_owned);
    let constants_sha256 =
        expect_opt_sha256(&entries[1].1, &format!("{path}.constants_sha256"))?.map(str::to_owned);
    if expect_non_null {
        if pool_sha256.is_none() || constants_sha256.is_none() {
            return Err(format!(
                "{path}: dealt-deck family requires non-null pool_sha256 and constants_sha256"
            ));
        }
    } else if pool_sha256.is_some() || constants_sha256.is_some() {
        return Err(format!(
            "{path}: non-dealt family requires null pool_sha256 and constants_sha256"
        ));
    }
    Ok(PlanInputs {
        pool_sha256,
        constants_sha256,
    })
}

fn parse_seat_ref_v1(value: &OrderedJson, path: &str) -> Result<SeatRef, String> {
    let entries = expect_object_keys(value, path, &["kind", "pin"])?;
    let kind_str = expect_str(&entries[0].1, &format!("{path}.kind"))?;
    let pin = expect_opt_sha256(&entries[1].1, &format!("{path}.pin"))?.map(str::to_owned);
    let kind = match kind_str {
        "uniform" => {
            if pin.is_some() {
                return Err(format!("{path}.pin: must be null for uniform"));
            }
            SeatKind::Uniform
        }
        "stated" => {
            if pin.is_some() {
                return Err(format!("{path}.pin: must be null for stated"));
            }
            SeatKind::Stated
        }
        "learned" => {
            if pin.is_none() {
                return Err(format!("{path}.pin: learned requires non-null sha256 pin"));
            }
            SeatKind::Learned
        }
        other => return Err(format!("{path}.kind: invalid picker kind {other:?}")),
    };
    Ok(SeatRef { kind, pin })
}

fn parse_dealt_origin(value: &OrderedJson, path: &str) -> Result<DealtOrigin, String> {
    let entries = expect_object_keys(value, path, &["function", "key"])?;
    let function = expect_str(&entries[0].1, &format!("{path}.function"))?.to_owned();
    let key = ordered_to_value(&entries[1].1)?;
    Ok(DealtOrigin { function, key })
}

fn parse_builder_ref_v1(
    value: &OrderedJson,
    source: DeckSource,
    dealt_raw: &OrderedJson,
    path: &str,
) -> Result<BuilderRef, String> {
    let entries = expect_object_keys(value, path, &["kind", "pin"])?;
    let kind_str = expect_str(&entries[0].1, &format!("{path}.kind"))?;
    match source {
        DeckSource::Build => {
            let pin = expect_opt_sha256(&entries[1].1, &format!("{path}.pin"))?.map(str::to_owned);
            match kind_str {
                "uniform" => {
                    if pin.is_some() {
                        return Err(format!("{path}.pin: must be null for uniform"));
                    }
                    Ok(BuilderRef::uniform())
                }
                "stated" => {
                    if pin.is_some() {
                        return Err(format!("{path}.pin: must be null for stated"));
                    }
                    Ok(BuilderRef::stated())
                }
                "learned" => {
                    let Some(digest) = pin else {
                        return Err(format!("{path}.pin: learned builder requires sha256 pin"));
                    };
                    Ok(BuilderRef::learned(digest))
                }
                other => Err(format!("{path}.kind: invalid build builder kind {other:?}")),
            }
        }
        DeckSource::Dealt => {
            if kind_str != "dealt" {
                return Err(format!(
                    "{path}.kind: dealt deck requires builder.kind \"dealt\", got {kind_str:?}"
                ));
            }
            let pin_origin = parse_dealt_origin(&entries[1].1, &format!("{path}.pin"))?;
            if &entries[1].1 != dealt_raw {
                return Err(format!(
                    "{path}.pin: dealt builder pin must equal deck.dealt {{function, key}}"
                ));
            }
            Ok(BuilderRef::dealt(pin_origin))
        }
    }
}

fn parse_plan_chairs(value: &OrderedJson, path: &str) -> Result<Vec<PlanChair>, String> {
    let items = expect_array(value, path)?;
    if items.len() != SEATS {
        return Err(format!("{path}: expected {SEATS} chairs, got {}", items.len()));
    }
    let mut chairs = Vec::with_capacity(SEATS);
    for (idx, item) in items.iter().enumerate() {
        let cp = format!("{path}[{idx}]");
        let entries = expect_object_keys(item, &cp, &["chair", "role", "picker"])?;
        let chair = expect_u8(&entries[0].1, &format!("{cp}.chair"))?;
        if usize::from(chair) != idx {
            return Err(format!("{cp}.chair: expected {idx}, got {chair}"));
        }
        let role = match expect_str(&entries[1].1, &format!("{cp}.role"))? {
            "learning" => ChairRole::Learning,
            "fixture" => ChairRole::Fixture,
            other => {
                return Err(format!(
                    "{cp}.role: read plan allows only learning or fixture, got {other:?}"
                ));
            }
        };
        let picker = parse_seat_ref_v1(&entries[2].1, &format!("{cp}.picker"))?;
        if role == ChairRole::Fixture && picker.kind != SeatKind::Uniform {
            return Err(format!("{cp}.picker: fixture chair must use uniform picker"));
        }
        chairs.push(PlanChair {
            chair,
            role,
            picker,
        });
    }
    Ok(chairs)
}

fn parse_pools_8x45(value: &OrderedJson, path: &str) -> Result<Vec<Vec<u16>>, String> {
    let items = expect_array(value, path)?;
    if items.len() != SEATS {
        return Err(format!("{path}: expected {SEATS} pools, got {}", items.len()));
    }
    let mut pools = Vec::with_capacity(SEATS);
    for (s, seat_val) in items.iter().enumerate() {
        let cards_arr = expect_array(seat_val, &format!("{path}[{s}]"))?;
        if cards_arr.len() != POOL_CARDS {
            return Err(format!(
                "{path}[{s}]: expected {POOL_CARDS} cards, got {}",
                cards_arr.len()
            ));
        }
        let mut seat_pool = Vec::with_capacity(POOL_CARDS);
        for (c_idx, card_val) in cards_arr.iter().enumerate() {
            let card_id = expect_u16(card_val, &format!("{path}[{s}][{c_idx}]"))?;
            if usize::from(card_id) >= CARD_DEFS.len() {
                return Err(format!(
                    "{path}[{s}][{c_idx}]: unknown card id {card_id}"
                ));
            }
            seat_pool.push(card_id);
        }
        pools.push(seat_pool);
    }
    Ok(pools)
}

fn parse_cards_40_ascending(value: &OrderedJson, path: &str) -> Result<Vec<u16>, String> {
    let arr = expect_array(value, path)?;
    if arr.len() != DECK_CARDS as usize {
        return Err(format!(
            "{path}: expected {DECK_CARDS} cards, got {}",
            arr.len()
        ));
    }
    let mut cards = Vec::with_capacity(DECK_CARDS as usize);
    for (idx, item) in arr.iter().enumerate() {
        let id = expect_u16(item, &format!("{path}[{idx}]"))?;
        if usize::from(id) >= CARD_DEFS.len() {
            return Err(format!("{path}[{idx}]: unknown card id {id}"));
        }
        if let Some(&prev) = cards.last() {
            if id < prev {
                return Err(format!(
                    "{path}[{idx}]: cards must be in ascending order ({id} < {prev})"
                ));
            }
        }
        cards.push(id);
    }
    Ok(cards)
}

fn parse_plan_deck(
    value: &OrderedJson,
    expected_deck_id: Option<u32>,
    expect_cards: bool,
    path: &str,
) -> Result<PlanDeck, String> {
    let entries = expect_object_keys(
        value,
        path,
        &[
            "deck",
            "source",
            "chair",
            "build",
            "builder",
            "cards",
            "dealt",
            "row",
        ],
    )?;
    let deck = expect_u32(&entries[0].1, &format!("{path}.deck"))?;
    if let Some(expected) = expected_deck_id {
        if deck != expected {
            return Err(format!("{path}.deck: expected {expected}, got {deck}"));
        }
    }
    let source = match expect_str(&entries[1].1, &format!("{path}.source"))? {
        "build" => DeckSource::Build,
        "dealt" => DeckSource::Dealt,
        other => return Err(format!("{path}.source: invalid value {other:?}")),
    };
    let chair = expect_opt_u8(&entries[2].1, &format!("{path}.chair"))?;
    let build = expect_opt_u8(&entries[3].1, &format!("{path}.build"))?;
    let dealt = match &entries[6].1 {
        OrderedJson::Null => None,
        other => Some(parse_dealt_origin(other, &format!("{path}.dealt"))?),
    };
    let builder = parse_builder_ref_v1(&entries[4].1, source, &entries[6].1, &format!("{path}.builder"))?;
    let cards = match &entries[5].1 {
        OrderedJson::Null => None,
        other => Some(parse_cards_40_ascending(other, &format!("{path}.cards"))?),
    };
    let row_str = expect_str(&entries[7].1, &format!("{path}.row"))?;
    let row = DeckRow::from_str_name(row_str)
        .ok_or_else(|| format!("{path}.row: unknown row {row_str:?}"))?;

    match source {
        DeckSource::Build => {
            if chair.is_none() || chair.unwrap() >= SEATS as u8 {
                return Err(format!("{path}.chair: built deck requires chair in 0..{SEATS}"));
            }
            if dealt.is_some() {
                return Err(format!("{path}.dealt: built deck must have dealt == null"));
            }
            let Some(expected_build) = row.expected_build_index() else {
                return Err(format!("{path}.row: built deck cannot have row \"opponent\""));
            };
            if build != Some(expected_build) {
                return Err(format!(
                    "{path}.build: row {row_str:?} requires build == {expected_build}, got {build:?}"
                ));
            }
            if row == DeckRow::Stated && builder.kind != BuilderKind::Stated {
                return Err(format!(
                    "{path}.builder: stated row requires builder.kind == \"stated\""
                ));
            }
            if expect_cards && cards.is_none() {
                return Err(format!("{path}.cards: expected 40 cards in prep family"));
            }
            if !expect_cards && cards.is_some() {
                return Err(format!("{path}.cards: expected null in non-prep family"));
            }
        }
        DeckSource::Dealt => {
            if chair.is_some() || build.is_some() || dealt.is_none() || cards.is_none() {
                return Err(format!(
                    "{path}: dealt deck requires chair=null, build=null, dealt!=null, cards!=null"
                ));
            }
            if row != DeckRow::Opponent {
                return Err(format!(
                    "{path}.row: dealt deck requires row \"opponent\", got {row_str:?}"
                ));
            }
        }
    }

    Ok(PlanDeck {
        deck,
        source,
        chair,
        build,
        builder,
        cards,
        dealt,
        row,
    })
}

fn parse_plan_game(value: &OrderedJson, expected_idx: u32, path: &str) -> Result<PlanGame, String> {
    let entries = expect_object_keys(
        value,
        path,
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
        ],
    )?;
    let game = expect_u32(&entries[0].1, &format!("{path}.game"))?;
    if game != expected_idx {
        return Err(format!("{path}.game: expected {expected_idx}, got {game}"));
    }
    let group = expect_u32(&entries[1].1, &format!("{path}.group"))?;
    let role = match expect_str(&entries[2].1, &format!("{path}.role"))? {
        "reference" => GameRole::Reference,
        "sibling" => GameRole::Sibling,
        "cross" => GameRole::Cross,
        "read_b" => GameRole::ReadB,
        "common" => GameRole::Common,
        other => return Err(format!("{path}.role: invalid value {other:?}")),
    };
    let pairing = expect_opt_u8(&entries[3].1, &format!("{path}.pairing"))?;
    let decks_arr = expect_array(&entries[4].1, &format!("{path}.decks"))?;
    if decks_arr.len() != 2 {
        return Err(format!("{path}.decks: expected 2 deck ids"));
    }
    let decks = [
        expect_u32(&decks_arr[0], &format!("{path}.decks[0]"))?,
        expect_u32(&decks_arr[1], &format!("{path}.decks[1]"))?,
    ];
    let chairs_arr = expect_array(&entries[5].1, &format!("{path}.chairs"))?;
    if chairs_arr.len() != 2 {
        return Err(format!("{path}.chairs: expected 2 chair slots"));
    }
    let chairs = [
        expect_opt_u8(&chairs_arr[0], &format!("{path}.chairs[0]"))?,
        expect_opt_u8(&chairs_arr[1], &format!("{path}.chairs[1]"))?,
    ];
    if expect_str(&entries[6].1, &format!("{path}.seat_rule"))? != "schedule" {
        return Err(format!("{path}.seat_rule: must be \"schedule\""));
    }
    let game_seed_val = expect_u64(&entries[7].1, &format!("{path}.game_seed"))?;
    let episode = expect_u64(&entries[8].1, &format!("{path}.episode"))?;

    Ok(PlanGame {
        game,
        group,
        role,
        pairing,
        decks,
        chairs,
        seat_rule: SeatRule::Schedule,
        game_seed: game_seed_val,
        episode,
    })
}

fn parse_paired_builds_v1(
    value: &OrderedJson,
    path: &str,
) -> Result<Vec<PairedBuildRecord>, String> {
    let items = expect_array(value, path)?;
    let mut out = Vec::with_capacity(items.len());
    let mut prev_first: Option<u32> = None;
    for (idx, item) in items.iter().enumerate() {
        let p = format!("{path}[{idx}]");
        let entries = expect_object_keys(
            item,
            &p,
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
        let kind = match expect_str(&entries[0].1, &format!("{p}.kind"))? {
            "sibling" => PairedBuildKind::Sibling,
            "candidate_stated" => PairedBuildKind::CandidateStated,
            "current_initial" => PairedBuildKind::CurrentInitial,
            "k1_last_b1_last" => PairedBuildKind::K1LastB1Last,
            other => return Err(format!("{p}.kind: invalid value {other:?}")),
        };
        let chair = expect_u8(&entries[1].1, &format!("{p}.chair"))?;
        let deck_x = expect_u32(&entries[2].1, &format!("{p}.deck_x"))?;
        let deck_y = expect_u32(&entries[3].1, &format!("{p}.deck_y"))?;
        let opponents: Vec<u32> = expect_array(&entries[4].1, &format!("{p}.opponents"))?
            .iter()
            .enumerate()
            .map(|(i, v)| expect_u32(v, &format!("{p}.opponents[{i}]")))
            .collect::<Result<_, _>>()?;
        let games_x: Vec<u32> = expect_array(&entries[5].1, &format!("{p}.games_x"))?
            .iter()
            .enumerate()
            .map(|(i, v)| expect_u32(v, &format!("{p}.games_x[{i}]")))
            .collect::<Result<_, _>>()?;
        let games_y: Vec<u32> = expect_array(&entries[6].1, &format!("{p}.games_y"))?
            .iter()
            .enumerate()
            .map(|(i, v)| expect_u32(v, &format!("{p}.games_y[{i}]")))
            .collect::<Result<_, _>>()?;
        let q = expect_opt_u8(&entries[7].1, &format!("{p}.q"))?;
        if games_x.len() != games_y.len() || opponents.is_empty() {
            return Err(format!("{p}: invalid opponents/games lengths"));
        }
        if let Some(&first_g) = games_x.first() {
            if let Some(prev) = prev_first {
                if first_g < prev {
                    return Err(format!(
                        "{p}.games_x[0]: out of logical order ({first_g} < {prev})"
                    ));
                }
            }
            prev_first = Some(first_g);
        }
        out.push(PairedBuildRecord {
            kind,
            chair,
            deck_x,
            deck_y,
            opponents,
            games_x,
            games_y,
            q,
        });
    }
    Ok(out)
}

fn parse_draft_record(value: &OrderedJson, path: &str) -> Result<DraftRecord, String> {
    let entries = expect_object_keys(
        value,
        path,
        &["schema", "draft_seed", "packs", "picks", "pools"],
    )?;
    if expect_str(&entries[0].1, &format!("{path}.schema"))? != DRAFT_RECORD_SCHEMA {
        return Err(format!("{path}.schema: expected {DRAFT_RECORD_SCHEMA:?}"));
    }
    let draft_seed = expect_u64(&entries[1].1, &format!("{path}.draft_seed"))?;
    let packs_arr = expect_array(&entries[2].1, &format!("{path}.packs"))?;
    if packs_arr.len() != ROUNDS {
        return Err(format!("{path}.packs: expected {ROUNDS} rounds"));
    }
    let mut packs = Vec::with_capacity(ROUNDS);
    for (r, round_val) in packs_arr.iter().enumerate() {
        let round_arr = expect_array(round_val, &format!("{path}.packs[{r}]"))?;
        if round_arr.len() != SEATS {
            return Err(format!("{path}.packs[{r}]: expected {SEATS} seats"));
        }
        let mut round_packs = Vec::with_capacity(SEATS);
        for (s, pack_val) in round_arr.iter().enumerate() {
            let cards = expect_array(pack_val, &format!("{path}.packs[{r}][{s}]"))?
                .iter()
                .enumerate()
                .map(|(c, v)| expect_u16(v, &format!("{path}.packs[{r}][{s}][{c}]")))
                .collect::<Result<Vec<u16>, _>>()?;
            round_packs.push(cards);
        }
        packs.push(round_packs);
    }
    let picks_arr = expect_array(&entries[3].1, &format!("{path}.picks"))?;
    let mut picks = Vec::with_capacity(picks_arr.len());
    for (idx, pick_val) in picks_arr.iter().enumerate() {
        let pp = format!("{path}.picks[{idx}]");
        let p = expect_object_keys(pick_val, &pp, &["seat", "round", "pick", "pack", "chosen"])?;
        let seat = usize::try_from(expect_u64(&p[0].1, &format!("{pp}.seat"))?)
            .map_err(|_| format!("{pp}.seat overflow"))?;
        let round = usize::try_from(expect_u64(&p[1].1, &format!("{pp}.round"))?)
            .map_err(|_| format!("{pp}.round overflow"))?;
        let pick = usize::try_from(expect_u64(&p[2].1, &format!("{pp}.pick"))?)
            .map_err(|_| format!("{pp}.pick overflow"))?;
        let pack = expect_array(&p[3].1, &format!("{pp}.pack"))?
            .iter()
            .enumerate()
            .map(|(c, v)| expect_u16(v, &format!("{pp}.pack[{c}]")))
            .collect::<Result<Vec<u16>, _>>()?;
        let chosen = expect_u16(&p[4].1, &format!("{pp}.chosen"))?;
        picks.push(PickRecord {
            seat,
            round,
            pick,
            pack,
            chosen,
        });
    }
    let pools = parse_pools_8x45(&entries[4].1, &format!("{path}.pools"))?;
    Ok(DraftRecord {
        schema: DRAFT_RECORD_SCHEMA,
        draft_seed,
        packs,
        picks,
        pools,
    })
}

fn parse_decision_row_v1(value: &OrderedJson, path: &str) -> Result<DecisionRow, String> {
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
    let kind = match expect_str(&entries[0].1, &format!("{path}.kind"))? {
        "pick" => DecisionKind::Pick,
        "count" => DecisionKind::Count,
        "pointer" => DecisionKind::Pointer,
        "type_set" => DecisionKind::TypeSet,
        "split" => DecisionKind::Split,
        other => return Err(format!("{path}.kind: invalid {other:?}")),
    };
    let sampler = expect_str(&entries[1].1, &format!("{path}.sampler"))?.to_owned();
    let options = expect_array(&entries[2].1, &format!("{path}.options"))?
        .iter()
        .enumerate()
        .map(|(i, v)| expect_u16(v, &format!("{path}.options[{i}]")))
        .collect::<Result<Vec<u16>, _>>()?;
    let chosen = expect_u16(&entries[3].1, &format!("{path}.chosen"))?;
    let stream_values = expect_array(&entries[4].1, &format!("{path}.stream_values"))?
        .iter()
        .enumerate()
        .map(|(i, v)| expect_u64(v, &format!("{path}.stream_values[{i}]")))
        .collect::<Result<Vec<u64>, _>>()?;
    let logits_bits = match &entries[5].1 {
        OrderedJson::Null => None,
        OrderedJson::Array(items) => Some(
            items
                .iter()
                .enumerate()
                .map(|(i, v)| expect_u32(v, &format!("{path}.logits_bits[{i}]")))
                .collect::<Result<Vec<u32>, _>>()?,
        ),
        _ => return Err(format!("{path}.logits_bits: expected array or null")),
    };
    let p_chosen_bits = expect_opt_u64(&entries[6].1, &format!("{path}.p_chosen_bits"))?;
    let value_bits = expect_opt_u64(&entries[7].1, &format!("{path}.value_bits"))?
        .map(|n| u32::try_from(n).map_err(|_| format!("{path}.value_bits exceeds u32")))
        .transpose()?;
    Ok(DecisionRow {
        kind,
        sampler,
        options,
        chosen,
        stream_values,
        logits_bits,
        p_chosen_bits,
        value_bits,
    })
}

fn parse_build_rows_v1(value: &OrderedJson, path: &str) -> Result<BuildRows, String> {
    let entries = expect_object_keys(value, path, &["count", "pointer", "type_set", "split"])?;
    let count = parse_decision_row_v1(&entries[0].1, &format!("{path}.count"))?;
    let pointer = expect_array(&entries[1].1, &format!("{path}.pointer"))?
        .iter()
        .enumerate()
        .map(|(i, v)| parse_decision_row_v1(v, &format!("{path}.pointer[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;
    let type_set = match &entries[2].1 {
        OrderedJson::Null => None,
        other => Some(parse_decision_row_v1(other, &format!("{path}.type_set"))?),
    };
    let split = expect_array(&entries[3].1, &format!("{path}.split"))?
        .iter()
        .enumerate()
        .map(|(i, v)| parse_decision_row_v1(v, &format!("{path}.split[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BuildRows {
        count,
        pointer,
        type_set,
        split,
    })
}

fn parse_game_end_v1(value: &OrderedJson, path: &str) -> Result<GameEnd, String> {
    let OrderedJson::Object(entries) = value else {
        return Err(format!("{path}: expected object"));
    };
    let Some((first_key, first_val)) = entries.first() else {
        return Err(format!("{path}: empty GameEnd"));
    };
    if first_key != "kind" {
        return Err(format!("{path}: first key must be \"kind\""));
    }
    match expect_str(first_val, &format!("{path}.kind"))? {
        "natural" => {
            let e = expect_object_keys(value, path, &["kind", "winner_seat", "reason"])?;
            let winner_seat = expect_opt_u64(&e[1].1, &format!("{path}.winner_seat"))?
                .map(|n| usize::try_from(n).map_err(|_| format!("{path}.winner_seat overflow")))
                .transpose()?;
            let reason = expect_str(&e[2].1, &format!("{path}.reason"))?.to_owned();
            Ok(GameEnd::Natural {
                winner_seat,
                reason,
            })
        }
        "terminal" => {
            let e = expect_object_keys(
                value,
                path,
                &["kind", "code", "classification", "reason"],
            )?;
            Ok(GameEnd::Terminal {
                code: expect_str(&e[1].1, &format!("{path}.code"))?.to_owned(),
                classification: expect_str(&e[2].1, &format!("{path}.classification"))?.to_owned(),
                reason: expect_str(&e[3].1, &format!("{path}.reason"))?.to_owned(),
            })
        }
        "session_error" => {
            let e = expect_object_keys(value, path, &["kind", "code", "message"])?;
            Ok(GameEnd::SessionError {
                code: expect_str(&e[1].1, &format!("{path}.code"))?.to_owned(),
                message: expect_str(&e[2].1, &format!("{path}.message"))?.to_owned(),
            })
        }
        "decision_cap" => {
            let _ = expect_object_keys(value, path, &["kind"])?;
            Ok(GameEnd::DecisionCap)
        }
        "panic" => {
            let e = expect_object_keys(value, path, &["kind", "message"])?;
            Ok(GameEnd::Panic {
                message: expect_str(&e[1].1, &format!("{path}.message"))?.to_owned(),
            })
        }
        other => Err(format!("{path}.kind: unknown GameEnd {other:?}")),
    }
}

fn parse_pod_game_record(value: &OrderedJson, path: &str) -> Result<PodGameRecord, String> {
    let g = expect_object_keys(
        value,
        path,
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
    let game = expect_u32(&g[0].1, &format!("{path}.game"))?;
    let group = expect_u32(&g[1].1, &format!("{path}.group"))?;
    let role = match expect_str(&g[2].1, &format!("{path}.role"))? {
        "reference" => GameRole::Reference,
        "sibling" => GameRole::Sibling,
        "cross" => GameRole::Cross,
        "read_b" => GameRole::ReadB,
        "common" => GameRole::Common,
        other => return Err(format!("{path}.role: invalid {other:?}")),
    };
    let pairing = expect_opt_u8(&g[3].1, &format!("{path}.pairing"))?;
    let decks_arr = expect_array(&g[4].1, &format!("{path}.decks"))?;
    if decks_arr.len() != 2 {
        return Err(format!("{path}.decks: expected 2"));
    }
    let decks = [
        expect_u32(&decks_arr[0], &format!("{path}.decks[0]"))?,
        expect_u32(&decks_arr[1], &format!("{path}.decks[1]"))?,
    ];
    let chairs_arr = expect_array(&g[5].1, &format!("{path}.chairs"))?;
    if chairs_arr.len() != 2 {
        return Err(format!("{path}.chairs: expected 2"));
    }
    let chairs = [
        expect_opt_u8(&chairs_arr[0], &format!("{path}.chairs[0]"))?,
        expect_opt_u8(&chairs_arr[1], &format!("{path}.chairs[1]"))?,
    ];
    let seat_rule = match expect_str(&g[6].1, &format!("{path}.seat_rule"))? {
        "coin" => SeatRule::Coin,
        "schedule" => SeatRule::Schedule,
        other => return Err(format!("{path}.seat_rule: invalid {other:?}")),
    };
    let game_seed_val = expect_u64(&g[7].1, &format!("{path}.game_seed"))?;
    let episode = expect_u64(&g[8].1, &format!("{path}.episode"))?;
    let attempts_arr = expect_array(&g[9].1, &format!("{path}.attempts"))?;
    let mut attempts = Vec::with_capacity(attempts_arr.len());
    for (idx, a_val) in attempts_arr.iter().enumerate() {
        let ap = format!("{path}.attempts[{idx}]");
        let a = expect_object_keys(
            a_val,
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
        let last_action_source = expect_opt_u64(&a[10].1, &format!("{ap}.last_action_source"))?
            .map(|n| u16::try_from(n).map_err(|_| format!("{ap}.last_action_source exceeds u16")))
            .transpose()?;
        attempts.push(AttemptRecord {
            attempt: expect_u8(&a[0].1, &format!("{ap}.attempt"))?,
            deal_seed: expect_u64(&a[1].1, &format!("{ap}.deal_seed"))?,
            env_seed: expect_u64(&a[2].1, &format!("{ap}.env_seed"))?,
            action_seed: expect_u64(&a[3].1, &format!("{ap}.action_seed"))?,
            decisions: expect_u64(&a[4].1, &format!("{ap}.decisions"))?,
            physical_decisions: expect_opt_u64(&a[5].1, &format!("{ap}.physical_decisions"))?,
            policy_steps: expect_opt_u64(&a[6].1, &format!("{ap}.policy_steps"))?,
            end: parse_game_end_v1(&a[7].1, &format!("{ap}.end"))?,
            halted: expect_bool(&a[8].1, &format!("{ap}.halted"))?,
            stop_kind: expect_opt_str(&a[9].1, &format!("{ap}.stop_kind"))?.map(str::to_owned),
            last_action_source,
        });
    }
    let scores = match &g[10].1 {
        OrderedJson::Null => None,
        OrderedJson::Array(arr) if arr.len() == 2 => {
            let parse_score = |v: &OrderedJson| -> Result<i8, String> {
                match v {
                    OrderedJson::U64(0) | OrderedJson::I64(0) => Ok(0),
                    OrderedJson::U64(1) | OrderedJson::I64(1) => Ok(1),
                    OrderedJson::I64(-1) => Ok(-1),
                    _ => Err(format!("{path}.scores: expected -1, 0, or 1")),
                }
            };
            Some([parse_score(&arr[0])?, parse_score(&arr[1])?])
        }
        _ => return Err(format!("{path}.scores: expected [i8; 2] or null")),
    };
    Ok(PodGameRecord {
        game,
        group,
        role,
        pairing,
        decks,
        chairs,
        seat_rule,
        game_seed: game_seed_val,
        episode,
        attempts,
        scores,
    })
}

// ---------------------------------------------------------------------------
// Strict validators for plans, manifests, prep requests/results, panels, receipts
// ---------------------------------------------------------------------------

/// Strictly parses and validates a `mtg-kernel-cube-read-plan/v1` plan JSON
/// against sections 1, 2, 3 and 6 (exact key order, integer ranges, family
/// layout, seed derivations, seats, pairings, dealt builder pins, `q`).
pub fn parse_and_validate_read_plan_json(json_text: &str) -> Result<ReadPlanV1, PodRunnerError> {
    let root = JsonParser::new(json_text).parse_root().map_err(plan_err)?;
    let top = expect_object_keys(
        &root,
        "plan",
        &[
            "schema",
            "interfaces",
            "family",
            "use",
            "namespace",
            "run",
            "unit",
            "pod_index",
            "mirror",
            "mirror_pods",
            "rotation",
            "mark",
            "seeds",
            "pins",
            "pilot",
            "caps",
            "seat_rule",
            "inputs",
            "prep_sha256",
            "chairs",
            "pools",
            "decks",
            "games",
            "paired_builds",
        ],
    )
    .map_err(plan_err)?;

    let schema = expect_str(&top[0].1, "plan.schema").map_err(plan_err)?;
    if schema != READ_PLAN_SCHEMA {
        return Err(plan_err(format!("plan.schema: expected {READ_PLAN_SCHEMA:?}, got {schema:?}")));
    }
    let interfaces = expect_str(&top[1].1, "plan.interfaces").map_err(plan_err)?;
    if interfaces != M4_INTERFACES_VERSION {
        return Err(plan_err(format!(
            "plan.interfaces: expected {M4_INTERFACES_VERSION:?}, got {interfaces:?}"
        )));
    }
    let family_str = expect_str(&top[2].1, "plan.family").map_err(plan_err)?;
    let family = PlanFamily::from_str_name(family_str)
        .ok_or_else(|| plan_err(format!("plan.family: unknown family {family_str:?}")))?;
    let use_str = expect_str(&top[3].1, "plan.use").map_err(plan_err)?;
    let namespace = expect_str(&top[4].1, "plan.namespace").map_err(plan_err)?;
    if !is_valid_token(namespace) || !family.is_valid_namespace(namespace) {
        return Err(plan_err(format!(
            "plan.namespace: invalid namespace {namespace:?} for family {family_str:?}"
        )));
    }
    let expected_use = family.expected_use(namespace);
    if use_str != expected_use.as_str() {
        return Err(plan_err(format!(
            "plan.use: expected {:?} for family {family_str:?} in {namespace:?}, got {use_str:?}",
            expected_use.as_str()
        )));
    }
    let run = expect_str(&top[5].1, "plan.run").map_err(plan_err)?;
    if !is_valid_token(run) || (namespace != "test" && run != "v1") {
        return Err(plan_err(format!("plan.run: invalid run {run:?}")));
    }
    let unit = expect_u64(&top[6].1, "plan.unit").map_err(plan_err)?;
    let pod_index = expect_u64(&top[7].1, "plan.pod_index").map_err(plan_err)?;
    let mirror = expect_opt_u8(&top[8].1, "plan.mirror").map_err(plan_err)?;
    let mirror_pods = match &top[9].1 {
        OrderedJson::Null => None,
        OrderedJson::Array(arr) if arr.len() == 2 => Some([
            expect_u64(&arr[0], "plan.mirror_pods[0]").map_err(plan_err)?,
            expect_u64(&arr[1], "plan.mirror_pods[1]").map_err(plan_err)?,
        ]),
        _ => return Err(plan_err("plan.mirror_pods: expected [u64; 2] or null")),
    };
    let rotation = expect_opt_u8(&top[10].1, "plan.rotation").map_err(plan_err)?;
    let mark = match &top[11].1 {
        OrderedJson::Null => None,
        other => {
            let m = expect_object_keys(other, "plan.mark", &["kind", "label", "ordinal"])
                .map_err(plan_err)?;
            let kind = match expect_str(&m[0].1, "plan.mark.kind").map_err(plan_err)? {
                "dose" => MarkKind::Dose,
                "pilot" => MarkKind::Pilot,
                other => return Err(plan_err(format!("plan.mark.kind: invalid {other:?}"))),
            };
            let label = expect_str(&m[1].1, "plan.mark.label")
                .map_err(plan_err)?
                .to_owned();
            if label.is_empty() {
                return Err(plan_err("plan.mark.label: must be non-empty"));
            }
            let ordinal = expect_u32(&m[2].1, "plan.mark.ordinal").map_err(plan_err)?;
            Some(PlanMark {
                kind,
                label,
                ordinal,
            })
        }
    };

    // Validate mirror, mirror_pods, rotation, mark, and pod_index per family.
    if family.is_a_mirror() {
        let Some(r) = mirror else {
            return Err(plan_err("plan.mirror: required for (a) layout"));
        };
        if r > 1 {
            return Err(plan_err(format!("plan.mirror: must be 0 or 1, got {r}")));
        }
        let expected_pair = [
            unit.checked_mul(2)
                .ok_or_else(|| plan_err("unit * 2 overflow"))?,
            unit.checked_mul(2)
                .and_then(|v| v.checked_add(1))
                .ok_or_else(|| plan_err("2*unit + 1 overflow"))?,
        ];
        if mirror_pods != Some(expected_pair) {
            return Err(plan_err(format!(
                "plan.mirror_pods: expected {expected_pair:?}, got {mirror_pods:?}"
            )));
        }
        let expected_pod_index = expected_pair[usize::from(r)];
        if pod_index != expected_pod_index {
            return Err(plan_err(format!(
                "plan.pod_index: expected {expected_pod_index} (2*unit + mirror), got {pod_index}"
            )));
        }
        if rotation.is_some() || mark.is_some() {
            return Err(plan_err(
                "plan.rotation and plan.mark must be null for (a) layout",
            ));
        }
    } else if family.is_one_seat() {
        let Some(r) = rotation else {
            return Err(plan_err("plan.rotation: required for one-seat companion"));
        };
        if usize::from(r) >= SEATS {
            return Err(plan_err(format!(
                "plan.rotation: must be in 0..{SEATS}, got {r}"
            )));
        }
        let expected_pod_index = unit
            .checked_mul(8)
            .and_then(|v| v.checked_add(u64::from(r)))
            .ok_or_else(|| plan_err("8*unit + rotation overflow"))?;
        if pod_index != expected_pod_index {
            return Err(plan_err(format!(
                "plan.pod_index: expected {expected_pod_index} (8*unit + rotation), got {pod_index}"
            )));
        }
        if mirror.is_some() || mirror_pods.is_some() || mark.is_some() {
            return Err(plan_err(
                "plan.mirror, mirror_pods and mark must be null for one-seat companion",
            ));
        }
    } else {
        if pod_index != unit {
            return Err(plan_err(format!(
                "plan.pod_index: expected {unit}, got {pod_index}"
            )));
        }
        if mirror.is_some() || mirror_pods.is_some() || rotation.is_some() {
            return Err(plan_err(
                "plan.mirror, mirror_pods and rotation must be null for this family",
            ));
        }
        match family {
            PlanFamily::Dose => {
                let Some(ref m) = mark else {
                    return Err(plan_err("plan.mark: required for dose"));
                };
                if m.kind != MarkKind::Dose {
                    return Err(plan_err("plan.mark.kind: expected \"dose\" for dose family"));
                }
            }
            PlanFamily::D13 => {
                let Some(ref m) = mark else {
                    return Err(plan_err("plan.mark: required for d13"));
                };
                if m.kind != MarkKind::Pilot {
                    return Err(plan_err("plan.mark.kind: expected \"pilot\" for d13 family"));
                }
            }
            _ => {
                if mark.is_some() {
                    return Err(plan_err("plan.mark: must be null for this family"));
                }
            }
        }
    }

    let seeds = parse_seeds_v1(&top[12].1, "plan.seeds").map_err(plan_err)?;
    let (exp_d_ns, exp_p_ns) = family.expected_seed_namespaces(namespace);
    if seeds.draft_seed_namespace.as_deref() != Some(exp_d_ns)
        || seeds.pod_seed_namespace.as_deref() != Some(exp_p_ns)
    {
        return Err(plan_err(format!(
            "plan.seeds: expected seed namespaces ({exp_d_ns:?}, {exp_p_ns:?}), got ({:?}, {:?})",
            seeds.draft_seed_namespace, seeds.pod_seed_namespace
        )));
    }
    let expected_draft_seed = seed(&[
        SeedPart::Str(exp_d_ns),
        SeedPart::Str("draft"),
        SeedPart::Str(run),
        SeedPart::Int(unit),
    ])?;
    let expected_pod_seed = if let Some(r) = rotation {
        seed(&[
            SeedPart::Str(exp_p_ns),
            SeedPart::Str("pod"),
            SeedPart::Str(run),
            SeedPart::Int(unit),
            SeedPart::Int(u64::from(r)),
        ])?
    } else {
        seed(&[
            SeedPart::Str(exp_p_ns),
            SeedPart::Str("pod"),
            SeedPart::Str(run),
            SeedPart::Int(unit),
        ])?
    };
    if seeds.draft_seed != Some(expected_draft_seed) || seeds.pod_seed != Some(expected_pod_seed) {
        return Err(plan_err(format!(
            "plan.seeds: draft_seed/pod_seed disagree with section 3 derivation (expected {expected_draft_seed}/{expected_pod_seed}, got {:?}/{:?})",
            seeds.draft_seed, seeds.pod_seed
        )));
    }

    let pins = parse_pins_non_null(&top[13].1, "plan.pins").map_err(plan_err)?;
    let pilot = parse_pilot_v1(&top[14].1, "plan.pilot").map_err(plan_err)?;
    let caps = parse_caps_a67(&top[15].1, "plan.caps").map_err(plan_err)?;
    if expect_str(&top[16].1, "plan.seat_rule").map_err(plan_err)? != "schedule" {
        return Err(plan_err("plan.seat_rule: must be \"schedule\""));
    }
    let inputs = parse_plan_inputs(&top[17].1, family.has_dealt(), "plan.inputs").map_err(plan_err)?;
    let prep_sha256 = expect_opt_sha256(&top[18].1, "plan.prep_sha256")
        .map_err(plan_err)?
        .map(str::to_owned);
    if family.has_prep() != prep_sha256.is_some() {
        return Err(plan_err(
            "plan.prep_sha256: null pattern does not match family",
        ));
    }

    let chairs = parse_plan_chairs(&top[19].1, "plan.chairs").map_err(plan_err)?;
    let pools = match &top[20].1 {
        OrderedJson::Null => None,
        other => Some(parse_pools_8x45(other, "plan.pools").map_err(plan_err)?),
    };
    if family.has_prep() != pools.is_some() {
        return Err(plan_err("plan.pools: null pattern does not match family"));
    }

    let decks_arr = expect_array(&top[21].1, "plan.decks").map_err(plan_err)?;
    let mut decks = Vec::with_capacity(decks_arr.len());
    for (idx, d_val) in decks_arr.iter().enumerate() {
        decks.push(
            parse_plan_deck(
                d_val,
                Some(idx as u32),
                family.has_prep(),
                &format!("plan.decks[{idx}]"),
            )
            .map_err(plan_err)?,
        );
    }

    let games_arr = expect_array(&top[22].1, "plan.games").map_err(plan_err)?;
    let mut games = Vec::with_capacity(games_arr.len());
    for (idx, g_val) in games_arr.iter().enumerate() {
        games.push(
            parse_plan_game(g_val, idx as u32, &format!("plan.games[{idx}]")).map_err(plan_err)?,
        );
    }

    let paired_builds =
        parse_paired_builds_v1(&top[23].1, "plan.paired_builds").map_err(plan_err)?;

    let plan = ReadPlanV1 {
        schema: schema.to_owned(),
        interfaces: interfaces.to_owned(),
        family,
        pod_use: expected_use,
        namespace: namespace.to_owned(),
        run: run.to_owned(),
        unit,
        pod_index,
        mirror,
        mirror_pods,
        rotation,
        mark,
        seeds,
        pins,
        pilot,
        caps,
        seat_rule: SeatRule::Schedule,
        inputs,
        prep_sha256,
        chairs,
        pools,
        decks,
        games,
        paired_builds,
    };
    validate_plan_family_layout(&plan)?;
    Ok(plan)
}

fn validate_plan_family_layout(plan: &ReadPlanV1) -> Result<(), PodRunnerError> {
    let pod_seed = plan.seeds.pod_seed.expect("validated non-null pod_seed");
    let pairings = pod_pairings();

    // 1. Validate chair roles and family picker constraints.
    for c in 0..SEATS {
        let expected_role = if let Some(m) = plan.mirror {
            if c % 2 == usize::from(m) {
                ChairRole::Learning
            } else {
                ChairRole::Fixture
            }
        } else if let Some(r) = plan.rotation {
            if c == usize::from(r) {
                ChairRole::Learning
            } else {
                ChairRole::Fixture
            }
        } else if plan.family == PlanFamily::Dose {
            ChairRole::Fixture
        } else {
            ChairRole::Learning
        };
        if plan.chairs[c].role != expected_role {
            return Err(plan_err(format!(
                "plan.chairs[{c}].role: expected {expected_role:?}, got {:?}",
                plan.chairs[c].role
            )));
        }
        if matches!(plan.family, PlanFamily::Aa | PlanFamily::Dose)
            && plan.chairs[c].picker.kind != SeatKind::Uniform
        {
            return Err(plan_err(format!(
                "plan.chairs[{c}].picker: family {:?} requires uniform pickers",
                plan.family.as_str()
            )));
        }
    }

    // 2. Validate decks, games and paired_builds per section 3.
    match plan.family {
        PlanFamily::Aa
        | PlanFamily::A
        | PlanFamily::AEarly
        | PlanFamily::SizeA
        | PlanFamily::SizeAEarly => {
            if plan.decks.len() != 16 {
                return Err(plan_err(format!(
                    "plan.decks: (a) layout requires 16 decks, got {}",
                    plan.decks.len()
                )));
            }
            for c in 0..SEATS {
                let cand = &plan.decks[2 * c];
                let stated = &plan.decks[2 * c + 1];
                if cand.source != DeckSource::Build
                    || cand.chair != Some(c as u8)
                    || cand.build != Some(0)
                    || cand.row != DeckRow::Candidate
                {
                    return Err(plan_err(format!("plan.decks[{}]: invalid candidate deck", 2 * c)));
                }
                if plan.family == PlanFamily::Aa && cand.builder.kind != BuilderKind::Uniform {
                    return Err(plan_err(format!(
                        "plan.decks[{}]: aa requires uniform candidate builder",
                        2 * c
                    )));
                }
                if stated.source != DeckSource::Build
                    || stated.chair != Some(c as u8)
                    || stated.build != Some(1)
                    || stated.row != DeckRow::Stated
                    || stated.builder.kind != BuilderKind::Stated
                {
                    return Err(plan_err(format!(
                        "plan.decks[{}]: invalid stated deck",
                        2 * c + 1
                    )));
                }
            }
            let cross_pairings: Vec<usize> = pairings
                .iter()
                .enumerate()
                .filter_map(|(g, &(low, high))| ((low % 2) != (high % 2)).then_some(g))
                .collect();
            if plan.games.len() != 32 {
                return Err(plan_err(format!(
                    "plan.games: (a) layout requires 32 games, got {}",
                    plan.games.len()
                )));
            }
            for (k, &g) in cross_pairings.iter().enumerate() {
                let g_seed = game_seed(pod_seed, g);
                let seats = pod_game_seats(pod_seed, g)?;
                for row in 0..2u32 {
                    let idx = 2 * k + row as usize;
                    let game = &plan.games[idx];
                    let expected_decks = [2 * seats[0] as u32 + row, 2 * seats[1] as u32 + row];
                    let expected_chairs = [Some(seats[0] as u8), Some(seats[1] as u8)];
                    if game.group != k as u32
                        || game.role != GameRole::Cross
                        || game.pairing != Some(g as u8)
                        || game.decks != expected_decks
                        || game.chairs != expected_chairs
                        || game.game_seed != g_seed
                        || game.episode != g as u64
                    {
                        return Err(plan_err(format!(
                            "plan.games[{idx}]: does not match (a) schedule for pairing {g} row {row}"
                        )));
                    }
                }
            }
            if !plan.paired_builds.is_empty() {
                return Err(plan_err("plan.paired_builds: must be empty for (a) layout"));
            }
        }
        PlanFamily::AOne | PlanFamily::AEarlyOne => {
            let rot = usize::from(plan.rotation.expect("validated rotation"));
            if plan.decks.len() != SEATS {
                return Err(plan_err(format!(
                    "plan.decks: one-seat companion requires {SEATS} decks, got {}",
                    plan.decks.len()
                )));
            }
            for c in 0..SEATS {
                let d = &plan.decks[c];
                if d.source != DeckSource::Build
                    || d.chair != Some(c as u8)
                    || d.build != Some(0)
                    || d.row != DeckRow::Candidate
                {
                    return Err(plan_err(format!("plan.decks[{c}]: invalid companion deck")));
                }
            }
            let rot_pairings: Vec<usize> = pairings
                .iter()
                .enumerate()
                .filter_map(|(g, &(low, high))| (low == rot || high == rot).then_some(g))
                .collect();
            if plan.games.len() != rot_pairings.len() {
                return Err(plan_err(format!(
                    "plan.games: one-seat companion requires {} games, got {}",
                    rot_pairings.len(),
                    plan.games.len()
                )));
            }
            for (k, &g) in rot_pairings.iter().enumerate() {
                let g_seed = game_seed(pod_seed, g);
                let seats = pod_game_seats(pod_seed, g)?;
                let game = &plan.games[k];
                if game.group != k as u32
                    || game.role != GameRole::Cross
                    || game.pairing != Some(g as u8)
                    || game.decks != [seats[0] as u32, seats[1] as u32]
                    || game.chairs != [Some(seats[0] as u8), Some(seats[1] as u8)]
                    || game.game_seed != g_seed
                    || game.episode != g as u64
                {
                    return Err(plan_err(format!(
                        "plan.games[{k}]: does not match one-seat schedule for pairing {g}"
                    )));
                }
            }
            if !plan.paired_builds.is_empty() {
                return Err(plan_err(
                    "plan.paired_builds: must be empty for one-seat companion",
                ));
            }
        }
        PlanFamily::AAll | PlanFamily::AEarlyAll => {
            if plan.decks.len() != SEATS {
                return Err(plan_err(format!(
                    "plan.decks: all-learned companion requires {SEATS} decks, got {}",
                    plan.decks.len()
                )));
            }
            for c in 0..SEATS {
                let d = &plan.decks[c];
                if d.source != DeckSource::Build
                    || d.chair != Some(c as u8)
                    || d.build != Some(0)
                    || d.row != DeckRow::Candidate
                {
                    return Err(plan_err(format!("plan.decks[{c}]: invalid companion deck")));
                }
            }
            if plan.games.len() != pairings.len() {
                return Err(plan_err(format!(
                    "plan.games: all-learned companion requires {} games, got {}",
                    pairings.len(),
                    plan.games.len()
                )));
            }
            for g in 0..pairings.len() {
                let g_seed = game_seed(pod_seed, g);
                let seats = pod_game_seats(pod_seed, g)?;
                let game = &plan.games[g];
                if game.group != g as u32
                    || game.role != GameRole::Cross
                    || game.pairing != Some(g as u8)
                    || game.decks != [seats[0] as u32, seats[1] as u32]
                    || game.chairs != [Some(seats[0] as u8), Some(seats[1] as u8)]
                    || game.game_seed != g_seed
                    || game.episode != g as u64
                {
                    return Err(plan_err(format!(
                        "plan.games[{g}]: does not match all-learned schedule"
                    )));
                }
            }
            if !plan.paired_builds.is_empty() {
                return Err(plan_err(
                    "plan.paired_builds: must be empty for all-learned companion",
                ));
            }
        }
        PlanFamily::B | PlanFamily::SizeB => {
            if plan.decks.len() != 16 || plan.games.len() != 32 || !plan.paired_builds.is_empty() {
                return Err(plan_err(
                    "(b) layout requires 16 decks, 32 games, and 0 paired_builds",
                ));
            }
            for s in 0..SEATS {
                let x = plan
                    .unit
                    .checked_mul(8)
                    .and_then(|v| v.checked_add(s as u64))
                    .ok_or_else(|| plan_err("8*unit + s overflow"))?;
                let cand = &plan.decks[2 * s];
                let opp = &plan.decks[2 * s + 1];
                if cand.source != DeckSource::Build
                    || cand.chair != Some(s as u8)
                    || cand.build != Some(0)
                    || cand.row != DeckRow::Candidate
                {
                    return Err(plan_err(format!("plan.decks[{}]: invalid (b) candidate", 2 * s)));
                }
                let expected_dealt = DealtOrigin {
                    function: "cube_deal_b_v1".to_owned(),
                    key: Value::Array(vec![
                        Value::String(plan.namespace.clone()),
                        Value::String(plan.run.clone()),
                        Value::from(x),
                    ]),
                };
                if opp.source != DeckSource::Dealt || opp.dealt.as_ref() != Some(&expected_dealt) {
                    return Err(plan_err(format!(
                        "plan.decks[{}]: invalid (b) dealt opponent",
                        2 * s + 1
                    )));
                }
                for j in 0..4u64 {
                    let idx = 4 * s + j as usize;
                    let game = &plan.games[idx];
                    let expected_seed = seed_keyed(&[
                        SeedPart::Str("cube_deal_b_v1"),
                        SeedPart::Str(&plan.namespace),
                        SeedPart::Str(&plan.run),
                        SeedPart::Int(x),
                        SeedPart::Int(j),
                    ])?;
                    let (expected_decks, expected_chairs) = if j % 2 == 0 {
                        ([(2 * s) as u32, (2 * s + 1) as u32], [Some(s as u8), None])
                    } else {
                        ([(2 * s + 1) as u32, (2 * s) as u32], [None, Some(s as u8)])
                    };
                    if game.group != idx as u32
                        || game.role != GameRole::ReadB
                        || game.pairing.is_some()
                        || game.decks != expected_decks
                        || game.chairs != expected_chairs
                        || game.game_seed != expected_seed
                        || game.episode != j
                    {
                        return Err(plan_err(format!(
                            "plan.games[{idx}]: does not match (b) schedule"
                        )));
                    }
                }
            }
        }
        PlanFamily::C | PlanFamily::SizeC | PlanFamily::Dose => {
            if plan.decks.len() != 24 || plan.games.len() != 64 || plan.paired_builds.len() != SEATS
            {
                return Err(plan_err(
                    "(c)/dose layout requires 24 decks, 64 games, and 8 paired_builds",
                ));
            }
            let (row_x, row_y, build_y, pb_kind) = if plan.family == PlanFamily::Dose {
                (
                    DeckRow::Current,
                    DeckRow::Initial,
                    0u8,
                    PairedBuildKind::CurrentInitial,
                )
            } else {
                (
                    DeckRow::Candidate,
                    DeckRow::Stated,
                    1u8,
                    PairedBuildKind::CandidateStated,
                )
            };
            for s in 0..SEATS {
                let dx = &plan.decks[3 * s];
                let dy = &plan.decks[3 * s + 1];
                let opp = &plan.decks[3 * s + 2];
                if dx.source != DeckSource::Build
                    || dx.chair != Some(s as u8)
                    || dx.build != Some(0)
                    || dx.row != row_x
                {
                    return Err(plan_err(format!("plan.decks[{}]: invalid x deck", 3 * s)));
                }
                if dy.source != DeckSource::Build
                    || dy.chair != Some(s as u8)
                    || dy.build != Some(build_y)
                    || dy.row != row_y
                {
                    return Err(plan_err(format!("plan.decks[{}]: invalid y deck", 3 * s + 1)));
                }
                let expected_dealt = DealtOrigin {
                    function: "cube_deal_common_v1".to_owned(),
                    key: Value::Array(vec![
                        Value::String(plan.namespace.clone()),
                        Value::from(plan.unit),
                        Value::from(s as u64),
                        Value::from(0u64),
                    ]),
                };
                if opp.source != DeckSource::Dealt || opp.dealt.as_ref() != Some(&expected_dealt) {
                    return Err(plan_err(format!(
                        "plan.decks[{}]: invalid C(P) dealt opponent",
                        3 * s + 2
                    )));
                }
                let mut expected_gx = Vec::with_capacity(4);
                let mut expected_gy = Vec::with_capacity(4);
                for j in 0..4u64 {
                    let g_seed = seed(&[
                        SeedPart::Str(&plan.namespace),
                        SeedPart::Str("game"),
                        SeedPart::Str(&plan.run),
                        SeedPart::Int(plan.unit),
                        SeedPart::Int(s as u64),
                        SeedPart::Int(0),
                        SeedPart::Int(j),
                    ])?;
                    for arm in 0..2u32 {
                        let idx = 8 * s + 2 * (j as usize) + arm as usize;
                        if arm == 0 {
                            expected_gx.push(idx as u32);
                        } else {
                            expected_gy.push(idx as u32);
                        }
                        let focal = (3 * s) as u32 + arm;
                        let opp_id = (3 * s + 2) as u32;
                        let (expected_decks, expected_chairs) = if j % 2 == 0 {
                            ([focal, opp_id], [Some(s as u8), None])
                        } else {
                            ([opp_id, focal], [None, Some(s as u8)])
                        };
                        let game = &plan.games[idx];
                        if game.group != (4 * s + j as usize) as u32
                            || game.role != GameRole::Common
                            || game.pairing.is_some()
                            || game.decks != expected_decks
                            || game.chairs != expected_chairs
                            || game.game_seed != g_seed
                            || game.episode != j
                        {
                            return Err(plan_err(format!(
                                "plan.games[{idx}]: does not match (c)/dose schedule"
                            )));
                        }
                    }
                }
                let pb = &plan.paired_builds[s];
                if pb.kind != pb_kind
                    || pb.chair != s as u8
                    || pb.deck_x != (3 * s) as u32
                    || pb.deck_y != (3 * s + 1) as u32
                    || pb.opponents != vec![(3 * s + 2) as u32]
                    || pb.games_x != expected_gx
                    || pb.games_y != expected_gy
                    || pb.q.is_some()
                {
                    return Err(plan_err(format!(
                        "plan.paired_builds[{s}]: does not match (c)/dose layout"
                    )));
                }
            }
        }
        PlanFamily::D13 => {
            if plan.decks.len() != 48 || plan.games.len() != 192 || plan.paired_builds.len() != 24 {
                return Err(plan_err(
                    "d13 layout requires 48 decks, 192 games, and 24 paired_builds",
                ));
            }
            for s in 0..SEATS {
                let base = 6 * s;
                let specs = [
                    (0usize, DeckRow::B1Last, 0u8),
                    (1, DeckRow::Initial, 0),
                    (2, DeckRow::K1Last, 0),
                    (3, DeckRow::Stated, 1),
                ];
                for (off, row, build_idx) in specs {
                    let d = &plan.decks[base + off];
                    if d.source != DeckSource::Build
                        || d.chair != Some(s as u8)
                        || d.build != Some(build_idx)
                        || d.row != row
                    {
                        return Err(plan_err(format!(
                            "plan.decks[{}]: invalid d13 built deck",
                            base + off
                        )));
                    }
                }
                for o in 0..2u64 {
                    let opp = &plan.decks[base + 4 + o as usize];
                    let expected_dealt = DealtOrigin {
                        function: "cube_deal_common_v1".to_owned(),
                        key: Value::Array(vec![
                            Value::String(plan.namespace.clone()),
                            Value::from(plan.unit),
                            Value::from(s as u64),
                            Value::from(o),
                        ]),
                    };
                    if opp.source != DeckSource::Dealt
                        || opp.dealt.as_ref() != Some(&expected_dealt)
                    {
                        return Err(plan_err(format!(
                            "plan.decks[{}]: invalid d13 opponent o={o}",
                            base + 4 + o as usize
                        )));
                    }
                }
                let contrasts = [
                    (0u8, PairedBuildKind::CurrentInitial, (base) as u32, (base + 1) as u32),
                    (1u8, PairedBuildKind::K1LastB1Last, (base + 2) as u32, (base) as u32),
                    (2u8, PairedBuildKind::CandidateStated, (base + 2) as u32, (base + 3) as u32),
                ];
                for (q, kind, deck_x, deck_y) in contrasts {
                    let mut gx = Vec::with_capacity(4);
                    let mut gy = Vec::with_capacity(4);
                    for o in 0..2u64 {
                        for j in 0..2u64 {
                            let group = (12 * s + 4 * usize::from(q) + 2 * (o as usize) + j as usize)
                                as u32;
                            let g_seed = seed(&[
                                SeedPart::Str(&plan.namespace),
                                SeedPart::Str("game"),
                                SeedPart::Str(&plan.run),
                                SeedPart::Int(plan.unit),
                                SeedPart::Int(s as u64),
                                SeedPart::Int(o),
                                SeedPart::Int(j),
                                SeedPart::Int(u64::from(q)),
                            ])?;
                            let opp_id = (base + 4 + o as usize) as u32;
                            for arm in 0..2u32 {
                                let idx = (2 * group + arm) as usize;
                                if arm == 0 {
                                    gx.push(idx as u32);
                                } else {
                                    gy.push(idx as u32);
                                }
                                let focal = if arm == 0 { deck_x } else { deck_y };
                                let (expected_decks, expected_chairs) = if j % 2 == 0 {
                                    ([focal, opp_id], [Some(s as u8), None])
                                } else {
                                    ([opp_id, focal], [None, Some(s as u8)])
                                };
                                let game = &plan.games[idx];
                                if game.group != group
                                    || game.role != GameRole::Common
                                    || game.pairing.is_some()
                                    || game.decks != expected_decks
                                    || game.chairs != expected_chairs
                                    || game.game_seed != g_seed
                                    || game.episode != j
                                {
                                    return Err(plan_err(format!(
                                        "plan.games[{idx}]: does not match d13 schedule"
                                    )));
                                }
                            }
                        }
                    }
                    let pb = &plan.paired_builds[3 * s + usize::from(q)];
                    if pb.kind != kind
                        || pb.chair != s as u8
                        || pb.deck_x != deck_x
                        || pb.deck_y != deck_y
                        || pb.opponents != vec![(base + 4) as u32, (base + 5) as u32]
                        || pb.games_x != gx
                        || pb.games_y != gy
                        || pb.q != Some(q)
                    {
                        return Err(plan_err(format!(
                            "plan.paired_builds[{}]: does not match d13 contrast q={q}",
                            3 * s + usize::from(q)
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Strictly parses and validates a `PrepRequest` JSON (`requests/unit-<unit>.json`).
pub fn parse_and_validate_prep_request_json(json_text: &str) -> Result<PrepRequest, PodRunnerError> {
    let root = JsonParser::new(json_text).parse_root().map_err(plan_err)?;
    let top = expect_object_keys(
        &root,
        "prep_request",
        &[
            "schema",
            "kind",
            "family",
            "namespace",
            "run",
            "unit",
            "seeds",
            "pins",
            "chairs",
            "builds",
        ],
    )
    .map_err(plan_err)?;

    let schema = expect_str(&top[0].1, "prep_request.schema").map_err(plan_err)?;
    if schema != READ_PLAN_SCHEMA {
        return Err(plan_err(format!(
            "prep_request.schema: expected {READ_PLAN_SCHEMA:?}, got {schema:?}"
        )));
    }
    let kind = expect_str(&top[1].1, "prep_request.kind").map_err(plan_err)?;
    if kind != "prep_request" {
        return Err(plan_err(format!(
            "prep_request.kind: expected \"prep_request\", got {kind:?}"
        )));
    }
    let family_str = expect_str(&top[2].1, "prep_request.family").map_err(plan_err)?;
    let family = PrepFamily::from_str_name(family_str)
        .ok_or_else(|| plan_err(format!("prep_request.family: unknown {family_str:?}")))?;
    let namespace = expect_str(&top[3].1, "prep_request.namespace").map_err(plan_err)?;
    if !is_valid_token(namespace) || !family.is_valid_namespace(namespace) {
        return Err(plan_err(format!(
            "prep_request.namespace: invalid {namespace:?} for family {family_str:?}"
        )));
    }
    let run = expect_str(&top[4].1, "prep_request.run").map_err(plan_err)?;
    if !is_valid_token(run) || (namespace != "test" && run != "v1") {
        return Err(plan_err(format!("prep_request.run: invalid {run:?}")));
    }
    let unit = expect_u64(&top[5].1, "prep_request.unit").map_err(plan_err)?;
    let seeds = parse_seeds_v1(&top[6].1, "prep_request.seeds").map_err(plan_err)?;
    if seeds.draft_seed_namespace.as_deref() != Some(namespace)
        || seeds.pod_seed_namespace.as_deref() != Some(namespace)
    {
        return Err(plan_err(
            "prep_request.seeds: seed source namespaces must equal prep request namespace",
        ));
    }
    let expected_draft = seed(&[
        SeedPart::Str(namespace),
        SeedPart::Str("draft"),
        SeedPart::Str(run),
        SeedPart::Int(unit),
    ])?;
    let expected_pod = seed(&[
        SeedPart::Str(namespace),
        SeedPart::Str("pod"),
        SeedPart::Str(run),
        SeedPart::Int(unit),
    ])?;
    if seeds.draft_seed != Some(expected_draft) || seeds.pod_seed != Some(expected_pod) {
        return Err(plan_err(
            "prep_request.seeds: draft_seed/pod_seed disagree with section 3",
        ));
    }
    let pins = parse_pins_non_null(&top[7].1, "prep_request.pins").map_err(plan_err)?;
    let chairs = parse_plan_chairs(&top[8].1, "prep_request.chairs").map_err(plan_err)?;
    for c in 0..SEATS {
        let expected_role = if family == PrepFamily::Dose {
            ChairRole::Fixture
        } else {
            ChairRole::Learning
        };
        if chairs[c].role != expected_role {
            return Err(plan_err(format!(
                "prep_request.chairs[{c}].role: expected {expected_role:?}"
            )));
        }
    }

    let builds_arr = expect_array(&top[9].1, "prep_request.builds").map_err(plan_err)?;
    let mut builds = Vec::with_capacity(builds_arr.len());
    for (idx, item) in builds_arr.iter().enumerate() {
        let bp = format!("prep_request.builds[{idx}]");
        let b = expect_object_keys(item, &bp, &["chair", "build", "builder", "row"])
            .map_err(plan_err)?;
        let chair = expect_u8(&b[0].1, &format!("{bp}.chair")).map_err(plan_err)?;
        if usize::from(chair) >= SEATS {
            return Err(plan_err(format!("{bp}.chair: out of range {chair}")));
        }
        let build = expect_u8(&b[1].1, &format!("{bp}.build")).map_err(plan_err)?;
        let builder = parse_builder_ref_v1(
            &b[2].1,
            DeckSource::Build,
            &OrderedJson::Null,
            &format!("{bp}.builder"),
        )
        .map_err(plan_err)?;
        let row_str = expect_str(&b[3].1, &format!("{bp}.row")).map_err(plan_err)?;
        let row = DeckRow::from_str_name(row_str)
            .ok_or_else(|| plan_err(format!("{bp}.row: unknown row {row_str:?}")))?;
        let Some(expected_build) = row.expected_build_index() else {
            return Err(plan_err(format!(
                "{bp}.row: prep build cannot have row \"opponent\""
            )));
        };
        if build != expected_build {
            return Err(plan_err(format!(
                "{bp}.build: row {row_str:?} requires build {expected_build}, got {build}"
            )));
        }
        if row == DeckRow::Stated && builder.kind != BuilderKind::Stated {
            return Err(plan_err(format!(
                "{bp}.builder: stated row requires builder.kind \"stated\""
            )));
        }
        builds.push(PrepBuildSpec {
            chair,
            build,
            builder,
            row,
        });
    }

    // Validate chair-then-row order per section 4.
    let expected_rows_per_chair: &[DeckRow] = match family {
        PrepFamily::BC | PrepFamily::C | PrepFamily::SizeC => {
            &[DeckRow::Candidate, DeckRow::Stated]
        }
        PrepFamily::B | PrepFamily::SizeB => &[DeckRow::Candidate],
        PrepFamily::Dose => {
            if builds.is_empty() {
                // Stage-1 `prep-panel` for dose has empty builds.
                &[]
            } else {
                &[DeckRow::Current, DeckRow::Initial]
            }
        }
        PrepFamily::D13 => &[
            DeckRow::B1Last,
            DeckRow::Initial,
            DeckRow::K1Last,
            DeckRow::Stated,
        ],
    };
    if !expected_rows_per_chair.is_empty() {
        let expected_len = SEATS * expected_rows_per_chair.len();
        if builds.len() != expected_len {
            return Err(plan_err(format!(
                "prep_request.builds: expected {expected_len} entries for family {family_str:?}, got {}",
                builds.len()
            )));
        }
        for chair in 0..SEATS {
            for (r_idx, &exp_row) in expected_rows_per_chair.iter().enumerate() {
                let b = &builds[chair * expected_rows_per_chair.len() + r_idx];
                if usize::from(b.chair) != chair || b.row != exp_row {
                    return Err(plan_err(format!(
                        "prep_request.builds: expected chair {chair} row {exp_row:?}"
                    )));
                }
            }
        }
    }

    Ok(PrepRequest {
        schema: schema.to_owned(),
        kind: kind.to_owned(),
        family,
        namespace: namespace.to_owned(),
        run: run.to_owned(),
        unit,
        seeds,
        pins,
        chairs,
        builds,
    })
}

/// Strictly parses and validates a `PrepResult` JSON (`exec-<e>/prep/unit-<unit>.json`).
pub fn parse_and_validate_prep_result_json(json_text: &str) -> Result<PrepResult, PodRunnerError> {
    let root = JsonParser::new(json_text).parse_root().map_err(plan_err)?;
    let top = expect_object_keys(
        &root,
        "prep_result",
        &["schema", "kind", "request_sha256", "draft", "builds"],
    )
    .map_err(plan_err)?;
    if expect_str(&top[0].1, "prep_result.schema").map_err(plan_err)? != READ_PLAN_SCHEMA {
        return Err(plan_err("prep_result.schema mismatch"));
    }
    if expect_str(&top[1].1, "prep_result.kind").map_err(plan_err)? != "prep_result" {
        return Err(plan_err("prep_result.kind mismatch"));
    }
    let request_sha256 = expect_sha256(&top[2].1, "prep_result.request_sha256")
        .map_err(plan_err)?
        .to_owned();
    let draft = parse_draft_record(&top[3].1, "prep_result.draft").map_err(plan_err)?;
    let builds_arr = expect_array(&top[4].1, "prep_result.builds").map_err(plan_err)?;
    let mut builds = Vec::with_capacity(builds_arr.len());
    for (idx, item) in builds_arr.iter().enumerate() {
        let bp = format!("prep_result.builds[{idx}]");
        let b = expect_object_keys(
            item,
            &bp,
            &["chair", "build", "builder", "row", "build_seed", "cards"],
        )
        .map_err(plan_err)?;
        let chair = expect_u8(&b[0].1, &format!("{bp}.chair")).map_err(plan_err)?;
        let build = expect_u8(&b[1].1, &format!("{bp}.build")).map_err(plan_err)?;
        let builder = parse_builder_ref_v1(
            &b[2].1,
            DeckSource::Build,
            &OrderedJson::Null,
            &format!("{bp}.builder"),
        )
        .map_err(plan_err)?;
        let row_str = expect_str(&b[3].1, &format!("{bp}.row")).map_err(plan_err)?;
        let row = DeckRow::from_str_name(row_str)
            .ok_or_else(|| plan_err(format!("{bp}.row: unknown {row_str:?}")))?;
        let build_seed = expect_u64(&b[4].1, &format!("{bp}.build_seed")).map_err(plan_err)?;
        let cards = parse_cards_40_ascending(&b[5].1, &format!("{bp}.cards")).map_err(plan_err)?;
        builds.push(PrepBuildResult {
            chair,
            build,
            builder,
            row,
            build_seed,
            cards,
        });
    }
    Ok(PrepResult {
        schema: READ_PLAN_SCHEMA,
        kind: "prep_result",
        request_sha256,
        draft,
        builds,
    })
}

/// Strictly parses and validates a `manifest.json` (`READ-PLAN-v1` section 5).
pub fn parse_and_validate_manifest_json(json_text: &str) -> Result<SetManifest, PodRunnerError> {
    let root = JsonParser::new(json_text).parse_root().map_err(plan_err)?;
    let top = expect_object_keys(
        &root,
        "manifest",
        &[
            "schema",
            "kind",
            "family",
            "namespace",
            "run",
            "set",
            "interfaces",
            "declaration_sha256",
            "look",
            "units",
            "reserved",
            "pools_with",
            "panel_sha256",
            "prep_set",
            "replaces",
            "retired",
            "plans",
            "plan_set_sha256",
            "executions",
        ],
    )
    .map_err(plan_err)?;

    let schema = expect_str(&top[0].1, "manifest.schema").map_err(plan_err)?;
    if schema != READ_PLAN_SCHEMA {
        return Err(plan_err(format!("manifest.schema: expected {READ_PLAN_SCHEMA:?}")));
    }
    let kind = match expect_str(&top[1].1, "manifest.kind").map_err(plan_err)? {
        "plans" => ManifestKind::Plans,
        "prep" => ManifestKind::Prep,
        other => return Err(plan_err(format!("manifest.kind: invalid {other:?}"))),
    };
    let family = expect_str(&top[2].1, "manifest.family").map_err(plan_err)?.to_owned();
    let namespace = expect_str(&top[3].1, "manifest.namespace").map_err(plan_err)?.to_owned();
    if !is_valid_token(&namespace) {
        return Err(plan_err(format!("manifest.namespace: invalid {namespace:?}")));
    }
    let run = expect_str(&top[4].1, "manifest.run").map_err(plan_err)?.to_owned();
    if !is_valid_token(&run) || (namespace != "test" && run != "v1") {
        return Err(plan_err(format!("manifest.run: invalid {run:?}")));
    }
    let set = expect_str(&top[5].1, "manifest.set").map_err(plan_err)?.to_owned();
    if set.is_empty() {
        return Err(plan_err("manifest.set: must be non-empty"));
    }
    let interfaces = expect_str(&top[6].1, "manifest.interfaces").map_err(plan_err)?;
    if interfaces != M4_INTERFACES_VERSION {
        return Err(plan_err(format!(
            "manifest.interfaces: expected {M4_INTERFACES_VERSION:?}, got {interfaces:?}"
        )));
    }
    let declaration_sha256 = expect_sha256(&top[7].1, "manifest.declaration_sha256")
        .map_err(plan_err)?
        .to_owned();
    let look = expect_opt_u8(&top[8].1, "manifest.look").map_err(plan_err)?;
    if !matches!(look, None | Some(1) | Some(2)) {
        return Err(plan_err(format!("manifest.look: invalid {look:?}")));
    }
    let parse_range = |val: &OrderedJson, p: &str| -> Result<[u64; 2], PodRunnerError> {
        let arr = expect_array(val, p).map_err(plan_err)?;
        if arr.len() != 2 {
            return Err(plan_err(format!("{p}: expected [first, end]")));
        }
        let first = expect_u64(&arr[0], &format!("{p}[0]")).map_err(plan_err)?;
        let end = expect_u64(&arr[1], &format!("{p}[1]")).map_err(plan_err)?;
        if first >= end {
            return Err(plan_err(format!("{p}: expected first < end, got [{first}, {end})")));
        }
        Ok([first, end])
    };
    let units = parse_range(&top[9].1, "manifest.units")?;
    let reserved = match &top[10].1 {
        OrderedJson::Null => None,
        other => Some(parse_range(other, "manifest.reserved")?),
    };
    let pools_with = expect_opt_sha256(&top[11].1, "manifest.pools_with")
        .map_err(plan_err)?
        .map(str::to_owned);
    if (look == Some(2)) != pools_with.is_some() {
        return Err(plan_err(
            "manifest.pools_with: must be non-null iff look == 2",
        ));
    }
    let panel_sha256 = expect_opt_sha256(&top[12].1, "manifest.panel_sha256")
        .map_err(plan_err)?
        .map(str::to_owned);
    let prep_set = expect_opt_str(&top[13].1, "manifest.prep_set")
        .map_err(plan_err)?
        .map(str::to_owned);
    let replaces = expect_opt_str(&top[14].1, "manifest.replaces")
        .map_err(plan_err)?
        .map(str::to_owned);
    let retired_arr = expect_array(&top[15].1, "manifest.retired").map_err(plan_err)?;
    let mut retired = Vec::with_capacity(retired_arr.len());
    for (idx, r_val) in retired_arr.iter().enumerate() {
        retired.push(parse_range(r_val, &format!("manifest.retired[{idx}]"))?);
    }

    // Validate family, namespace, panel_sha256, and prep_set consistency.
    let is_prep_panel =
        kind == ManifestKind::Prep && (set == "prep-panel" || set.starts_with("prep-panel-restart-"));
    match kind {
        ManifestKind::Plans => {
            let fam = PlanFamily::from_str_name(&family)
                .ok_or_else(|| plan_err(format!("manifest.family: unknown plan family {family:?}")))?;
            if !fam.is_valid_namespace(&namespace) {
                return Err(plan_err(format!(
                    "manifest.namespace: {namespace:?} invalid for family {family:?}"
                )));
            }
            if fam.has_prep() != prep_set.is_some() {
                return Err(plan_err(
                    "manifest.prep_set: null pattern disagrees with family",
                ));
            }
            if fam.is_panel_family() != panel_sha256.is_some() {
                return Err(plan_err(
                    "manifest.panel_sha256: required for dose and d13 plan sets and null otherwise",
                ));
            }
        }
        ManifestKind::Prep => {
            let fam = PrepFamily::from_str_name(&family)
                .ok_or_else(|| plan_err(format!("manifest.family: unknown prep family {family:?}")))?;
            if !fam.is_valid_namespace(&namespace) {
                return Err(plan_err(format!(
                    "manifest.namespace: {namespace:?} invalid for prep family {family:?}"
                )));
            }
            if prep_set.is_some() {
                return Err(plan_err("manifest.prep_set: must be null on prep manifests"));
            }
            if is_prep_panel {
                if !fam.is_panel_family() {
                    return Err(plan_err(
                        "prep-panel manifest must have family \"dose\" or \"d13\"",
                    ));
                }
                if panel_sha256.is_some() {
                    return Err(plan_err(
                        "manifest.panel_sha256: prep-panel must have panel_sha256 == null",
                    ));
                }
            } else if fam.is_panel_family() != panel_sha256.is_some() {
                return Err(plan_err(
                    "manifest.panel_sha256: required for dose/d13 prep sets other than prep-panel and null otherwise",
                ));
            }
        }
    }

    let plans_arr = expect_array(&top[16].1, "manifest.plans").map_err(plan_err)?;
    let mut plans = Vec::with_capacity(plans_arr.len());
    let mut prev_id: Option<u64> = None;
    for (idx, p_val) in plans_arr.iter().enumerate() {
        let pp = format!("manifest.plans[{idx}]");
        match kind {
            ManifestKind::Plans => {
                let e = expect_object_keys(p_val, &pp, &["pod_index", "file", "sha256"])
                    .map_err(plan_err)?;
                let pod_index = expect_u64(&e[0].1, &format!("{pp}.pod_index")).map_err(plan_err)?;
                if let Some(prev) = prev_id {
                    if pod_index <= prev {
                        return Err(plan_err(format!(
                            "{pp}: pod_index {pod_index} must be strictly greater than {prev}"
                        )));
                    }
                }
                prev_id = Some(pod_index);
                let file = expect_str(&e[1].1, &format!("{pp}.file")).map_err(plan_err)?;
                let expected_file = format!("plans/pod-{pod_index}.json");
                if file != expected_file {
                    return Err(plan_err(format!(
                        "{pp}.file: expected {expected_file:?}, got {file:?}"
                    )));
                }
                let digest = expect_sha256(&e[2].1, &format!("{pp}.sha256"))
                    .map_err(plan_err)?
                    .to_owned();
                plans.push(ManifestEntry::Pod(ManifestPodEntry {
                    pod_index,
                    file: file.to_owned(),
                    sha256: digest,
                }));
            }
            ManifestKind::Prep => {
                let e = expect_object_keys(p_val, &pp, &["unit", "file", "sha256"])
                    .map_err(plan_err)?;
                let unit = expect_u64(&e[0].1, &format!("{pp}.unit")).map_err(plan_err)?;
                if let Some(prev) = prev_id {
                    if unit <= prev {
                        return Err(plan_err(format!(
                            "{pp}: unit {unit} must be strictly greater than {prev}"
                        )));
                    }
                }
                prev_id = Some(unit);
                let file = expect_str(&e[1].1, &format!("{pp}.file")).map_err(plan_err)?;
                let expected_file = format!("requests/unit-{unit}.json");
                if file != expected_file {
                    return Err(plan_err(format!(
                        "{pp}.file: expected {expected_file:?}, got {file:?}"
                    )));
                }
                let digest = expect_sha256(&e[2].1, &format!("{pp}.sha256"))
                    .map_err(plan_err)?
                    .to_owned();
                plans.push(ManifestEntry::Unit(ManifestUnitEntry {
                    unit,
                    file: file.to_owned(),
                    sha256: digest,
                }));
            }
        }
    }

    let plan_set_sha256 = expect_sha256(&top[17].1, "manifest.plan_set_sha256")
        .map_err(plan_err)?
        .to_owned();
    let computed_set_sha = compute_plan_set_sha256(&plans)?;
    if plan_set_sha256 != computed_set_sha {
        return Err(plan_err(format!(
            "manifest.plan_set_sha256: expected {computed_set_sha}, got {plan_set_sha256}"
        )));
    }

    let exec_arr = expect_array(&top[18].1, "manifest.executions").map_err(plan_err)?;
    let mut executions = Vec::with_capacity(exec_arr.len());
    for (idx, ex_val) in exec_arr.iter().enumerate() {
        let ep = format!("manifest.executions[{idx}]");
        let e = expect_object_keys(
            ex_val,
            &ep,
            &["execution", "reason", "expired_games", "binary", "platform"],
        )
        .map_err(plan_err)?;
        let execution = expect_u64(&e[0].1, &format!("{ep}.execution")).map_err(plan_err)?;
        if execution != idx as u64 {
            return Err(plan_err(format!(
                "{ep}.execution: expected sequential index {idx}, got {execution}"
            )));
        }
        let reason = match expect_str(&e[1].1, &format!("{ep}.reason")).map_err(plan_err)? {
            "first" => ExecutionReason::First,
            "watchdog_rerun" => ExecutionReason::WatchdogRerun,
            "prep_retry" => ExecutionReason::PrepRetry,
            other => return Err(plan_err(format!("{ep}.reason: unknown {other:?}"))),
        };
        let exp_arr = expect_array(&e[2].1, &format!("{ep}.expired_games")).map_err(plan_err)?;
        let mut expired_games = Vec::with_capacity(exp_arr.len());
        for (i, pair_val) in exp_arr.iter().enumerate() {
            let pair =
                expect_array(pair_val, &format!("{ep}.expired_games[{i}]")).map_err(plan_err)?;
            if pair.len() != 2 {
                return Err(plan_err(format!(
                    "{ep}.expired_games[{i}]: expected [pod_index, game]"
                )));
            }
            expired_games.push([
                expect_u64(&pair[0], &format!("{ep}.expired_games[{i}][0]")).map_err(plan_err)?,
                expect_u64(&pair[1], &format!("{ep}.expired_games[{i}][1]")).map_err(plan_err)?,
            ]);
        }
        let binary = expect_sha256(&e[3].1, &format!("{ep}.binary"))
            .map_err(plan_err)?
            .to_owned();
        let platform = expect_str(&e[4].1, &format!("{ep}.platform"))
            .map_err(plan_err)?
            .to_owned();
        if platform.is_empty() {
            return Err(plan_err(format!("{ep}.platform: must be non-empty")));
        }

        match kind {
            ManifestKind::Plans => {
                if execution == 0 {
                    if reason != ExecutionReason::First || !expired_games.is_empty() {
                        return Err(plan_err(format!(
                            "{ep}: execution 0 requires reason=\"first\" and empty expired_games"
                        )));
                    }
                } else if execution == 1 {
                    if reason != ExecutionReason::WatchdogRerun || expired_games.is_empty() {
                        return Err(plan_err(format!(
                            "{ep}: execution 1 requires reason=\"watchdog_rerun\" and non-empty expired_games"
                        )));
                    }
                } else {
                    return Err(plan_err(
                        "manifest.executions: plan sets never allow execution >= 2",
                    ));
                }
            }
            ManifestKind::Prep => {
                if !expired_games.is_empty() {
                    return Err(plan_err(format!(
                        "{ep}.expired_games: prep sets must have empty expired_games"
                    )));
                }
                if execution == 0 && reason != ExecutionReason::First {
                    return Err(plan_err(format!(
                        "{ep}.reason: prep execution 0 requires \"first\""
                    )));
                }
                if execution >= 1 && reason != ExecutionReason::PrepRetry {
                    return Err(plan_err(format!(
                        "{ep}.reason: prep execution {execution} requires \"prep_retry\""
                    )));
                }
            }
        }

        executions.push(ManifestExecutionEntry {
            execution,
            reason,
            expired_games,
            binary,
            platform,
        });
    }

    Ok(SetManifest {
        schema: schema.to_owned(),
        kind,
        family,
        namespace,
        run,
        set,
        interfaces: interfaces.to_owned(),
        declaration_sha256,
        look,
        units,
        reserved,
        pools_with,
        panel_sha256,
        prep_set,
        replaces,
        retired,
        plans,
        plan_set_sha256,
        executions,
    })
}

/// Strictly parses and validates a stage-2 `panel.json` document.
pub fn parse_and_validate_panel_json(json_text: &str) -> Result<PanelDocument, PodRunnerError> {
    let root = JsonParser::new(json_text).parse_root().map_err(plan_err)?;
    let top = expect_object_keys(
        &root,
        "panel",
        &[
            "schema",
            "kind",
            "namespace",
            "run",
            "units",
            "inputs",
            "pins",
            "prep_set_sha256",
            "prep_execution",
            "prep_outputs",
            "drafts",
        ],
    )
    .map_err(plan_err)?;

    let schema = expect_str(&top[0].1, "panel.schema").map_err(plan_err)?;
    if schema != READ_PLAN_SCHEMA {
        return Err(plan_err("panel.schema mismatch"));
    }
    let kind = expect_str(&top[1].1, "panel.kind").map_err(plan_err)?;
    if kind != "panel" {
        return Err(plan_err("panel.kind must be \"panel\""));
    }
    let namespace = expect_str(&top[2].1, "panel.namespace").map_err(plan_err)?.to_owned();
    if !is_valid_token(&namespace) {
        return Err(plan_err("panel.namespace invalid"));
    }
    let run = expect_str(&top[3].1, "panel.run").map_err(plan_err)?.to_owned();
    if !is_valid_token(&run) {
        return Err(plan_err("panel.run invalid"));
    }
    let units_arr = expect_array(&top[4].1, "panel.units").map_err(plan_err)?;
    if units_arr.len() != 2 {
        return Err(plan_err("panel.units: expected [first, end]"));
    }
    let units = [
        expect_u64(&units_arr[0], "panel.units[0]").map_err(plan_err)?,
        expect_u64(&units_arr[1], "panel.units[1]").map_err(plan_err)?,
    ];
    if units[0] >= units[1] {
        return Err(plan_err("panel.units: expected first < end"));
    }
    if namespace != "test" && units != [0, PANEL_UNITS_END] {
        return Err(plan_err(format!(
            "panel.units: expected [0, {PANEL_UNITS_END}], got {units:?}"
        )));
    }
    let inputs = parse_plan_inputs(&top[5].1, true, "panel.inputs").map_err(plan_err)?;
    let pins = parse_pins_non_null(&top[6].1, "panel.pins").map_err(plan_err)?;
    let prep_set_sha256 = expect_sha256(&top[7].1, "panel.prep_set_sha256")
        .map_err(plan_err)?
        .to_owned();
    let prep_execution = expect_u64(&top[8].1, "panel.prep_execution").map_err(plan_err)?;

    let out_arr = expect_array(&top[9].1, "panel.prep_outputs").map_err(plan_err)?;
    let mut prep_outputs = Vec::with_capacity(out_arr.len());
    for (idx, item) in out_arr.iter().enumerate() {
        let p = format!("panel.prep_outputs[{idx}]");
        let e = expect_object_keys(item, &p, &["unit", "sha256"]).map_err(plan_err)?;
        prep_outputs.push(PanelPrepOutput {
            unit: expect_u64(&e[0].1, &format!("{p}.unit")).map_err(plan_err)?,
            sha256: expect_sha256(&e[1].1, &format!("{p}.sha256"))
                .map_err(plan_err)?
                .to_owned(),
        });
    }

    let drafts_arr = expect_array(&top[10].1, "panel.drafts").map_err(plan_err)?;
    let expected_count = usize::try_from(units[1] - units[0])
        .map_err(|_| plan_err("panel.units range overflow"))?;
    if prep_outputs.len() != expected_count || drafts_arr.len() != expected_count {
        return Err(plan_err(format!(
            "panel: expected {expected_count} prep_outputs and drafts"
        )));
    }
    let mut drafts = Vec::with_capacity(drafts_arr.len());
    for (idx, item) in drafts_arr.iter().enumerate() {
        let dp = format!("panel.drafts[{idx}]");
        let d = expect_object_keys(
            item,
            &dp,
            &["unit", "seeds", "pools", "opponents", "games", "built"],
        )
        .map_err(plan_err)?;
        let unit = expect_u64(&d[0].1, &format!("{dp}.unit")).map_err(plan_err)?;
        let expected_unit = units[0] + idx as u64;
        if unit != expected_unit {
            return Err(plan_err(format!(
                "{dp}.unit: expected {expected_unit}, got {unit}"
            )));
        }
        let seeds = parse_seeds_v1(&d[1].1, &format!("{dp}.seeds")).map_err(plan_err)?;
        let pools = parse_pools_8x45(&d[2].1, &format!("{dp}.pools")).map_err(plan_err)?;
        let opp_arr = expect_array(&d[3].1, &format!("{dp}.opponents")).map_err(plan_err)?;
        let mut opponents = Vec::with_capacity(opp_arr.len());
        for (o_idx, o_val) in opp_arr.iter().enumerate() {
            opponents.push(
                parse_plan_deck(o_val, None, true, &format!("{dp}.opponents[{o_idx}]"))
                    .map_err(plan_err)?,
            );
        }
        let games_arr = expect_array(&d[4].1, &format!("{dp}.games")).map_err(plan_err)?;
        let mut games = Vec::with_capacity(games_arr.len());
        for (g_idx, g_val) in games_arr.iter().enumerate() {
            games.push(
                parse_plan_game(g_val, g_idx as u32, &format!("{dp}.games[{g_idx}]"))
                    .map_err(plan_err)?,
            );
        }
        let built = match &d[5].1 {
            OrderedJson::Null => None,
            OrderedJson::Array(b_arr) => {
                let mut list = Vec::with_capacity(b_arr.len());
                for (b_idx, b_val) in b_arr.iter().enumerate() {
                    list.push(
                        parse_plan_deck(b_val, None, true, &format!("{dp}.built[{b_idx}]"))
                            .map_err(plan_err)?,
                    );
                }
                Some(list)
            }
            _ => return Err(plan_err(format!("{dp}.built: expected array or null"))),
        };
        drafts.push(PanelDraftEntry {
            unit,
            seeds,
            pools,
            opponents,
            games,
            built,
        });
    }

    Ok(PanelDocument {
        schema: schema.to_owned(),
        kind: kind.to_owned(),
        namespace,
        run,
        units,
        inputs,
        pins,
        prep_set_sha256,
        prep_execution,
        prep_outputs,
        drafts,
    })
}

/// Strictly parses and validates a plan-set `execution.json` receipt.
pub fn parse_and_validate_plan_execution_receipt_json(
    json_text: &str,
) -> Result<PlanExecutionReceipt, PodRunnerError> {
    let root = JsonParser::new(json_text).parse_root().map_err(plan_err)?;
    let top = expect_object_keys(
        &root,
        "execution",
        &[
            "execution",
            "plan_set_sha256",
            "binary",
            "platform",
            "ended",
            "records",
        ],
    )
    .map_err(plan_err)?;
    let execution = expect_u64(&top[0].1, "execution.execution").map_err(plan_err)?;
    let plan_set_sha256 = expect_sha256(&top[1].1, "execution.plan_set_sha256")
        .map_err(plan_err)?
        .to_owned();
    let binary = expect_sha256(&top[2].1, "execution.binary")
        .map_err(plan_err)?
        .to_owned();
    let platform = expect_str(&top[3].1, "execution.platform")
        .map_err(plan_err)?
        .to_owned();
    let ended = match expect_str(&top[4].1, "execution.ended").map_err(plan_err)? {
        "complete" => PlanExecutionEnded::Complete,
        "watchdog_stop" => PlanExecutionEnded::WatchdogStop,
        "interrupted" => PlanExecutionEnded::Interrupted,
        other => return Err(plan_err(format!("execution.ended: invalid {other:?}"))),
    };
    let rec_arr = expect_array(&top[5].1, "execution.records").map_err(plan_err)?;
    let mut records = Vec::with_capacity(rec_arr.len());
    for (idx, item) in rec_arr.iter().enumerate() {
        let rp = format!("execution.records[{idx}]");
        let e = expect_object_keys(item, &rp, &["pod_index", "sha256", "status"])
            .map_err(plan_err)?;
        let pod_index = expect_u64(&e[0].1, &format!("{rp}.pod_index")).map_err(plan_err)?;
        let sha256 = expect_sha256(&e[1].1, &format!("{rp}.sha256"))
            .map_err(plan_err)?
            .to_owned();
        let status = match expect_str(&e[2].1, &format!("{rp}.status")).map_err(plan_err)? {
            "complete" => PodStatus::Complete,
            "stopped" => PodStatus::Stopped,
            other => return Err(plan_err(format!("{rp}.status: invalid {other:?}"))),
        };
        records.push(ExecutionRecordEntry {
            pod_index,
            sha256,
            status,
        });
    }
    Ok(PlanExecutionReceipt {
        execution,
        plan_set_sha256,
        binary,
        platform,
        ended,
        records,
    })
}

/// Strictly parses and validates a prep-set `execution.json` receipt.
pub fn parse_and_validate_prep_execution_receipt_json(
    json_text: &str,
) -> Result<PrepExecutionReceipt, PodRunnerError> {
    let root = JsonParser::new(json_text).parse_root().map_err(plan_err)?;
    let top = expect_object_keys(
        &root,
        "execution",
        &[
            "execution",
            "plan_set_sha256",
            "binary",
            "platform",
            "ended",
            "outputs",
        ],
    )
    .map_err(plan_err)?;
    let execution = expect_u64(&top[0].1, "execution.execution").map_err(plan_err)?;
    let plan_set_sha256 = expect_sha256(&top[1].1, "execution.plan_set_sha256")
        .map_err(plan_err)?
        .to_owned();
    let binary = expect_sha256(&top[2].1, "execution.binary")
        .map_err(plan_err)?
        .to_owned();
    let platform = expect_str(&top[3].1, "execution.platform")
        .map_err(plan_err)?
        .to_owned();
    let ended = match expect_str(&top[4].1, "execution.ended").map_err(plan_err)? {
        "complete" => PrepExecutionEnded::Complete,
        "interrupted" => PrepExecutionEnded::Interrupted,
        other => return Err(plan_err(format!("execution.ended: invalid {other:?}"))),
    };
    let out_arr = expect_array(&top[5].1, "execution.outputs").map_err(plan_err)?;
    let mut outputs = Vec::with_capacity(out_arr.len());
    for (idx, item) in out_arr.iter().enumerate() {
        let op = format!("execution.outputs[{idx}]");
        let e = expect_object_keys(item, &op, &["unit", "sha256", "status"]).map_err(plan_err)?;
        let unit = expect_u64(&e[0].1, &format!("{op}.unit")).map_err(plan_err)?;
        let sha256 = expect_sha256(&e[1].1, &format!("{op}.sha256"))
            .map_err(plan_err)?
            .to_owned();
        let status = expect_str(&e[2].1, &format!("{op}.status"))
            .map_err(plan_err)?
            .to_owned();
        if status != "complete" {
            return Err(plan_err(format!(
                "{op}.status: expected \"complete\", got {status:?}"
            )));
        }
        outputs.push(PrepOutputEntry {
            unit,
            sha256,
            status,
        });
    }
    Ok(PrepExecutionReceipt {
        execution,
        plan_set_sha256,
        binary,
        platform,
        ended,
        outputs,
    })
}

// ---------------------------------------------------------------------------
// Runtime pin, declaration, and two-stage panel verification (`READ-PLAN-v1` s6)
// ---------------------------------------------------------------------------

fn verify_declaration(
    manifest: &SetManifest,
    verification: &RuntimeVerification,
) -> Result<(), PodRunnerError> {
    let actual_sha = sha256_hex(&verification.declaration_bytes);
    if actual_sha != manifest.declaration_sha256 {
        return Err(plan_err(format!(
            "declaration_sha256 mismatch: manifest has {}, file hashes to {actual_sha}",
            manifest.declaration_sha256
        )));
    }
    if manifest.namespace != "test" {
        let text = std::str::from_utf8(&verification.declaration_bytes).map_err(|_| {
            plan_err("A6 declaration file is not valid UTF-8")
        })?;
        let lower = text.to_ascii_lowercase();
        let has_part2 = lower.contains("part 2")
            || lower.contains("part-2")
            || lower.contains("part_2");
        let has_sig = lower.contains("countersign") || lower.contains("## signature");
        if !(has_part2 && has_sig) {
            return Err(plan_err(
                "A6 declaration file lacks the required part 2 countersignature",
            ));
        }
    }
    Ok(())
}

fn verify_runtime_pins(
    pins: &PodPins,
    inputs: Option<&PlanInputs>,
    runtime: &RuntimePins,
) -> Result<(), PodRunnerError> {
    if pins.binary.as_deref() != Some(runtime.binary.as_str())
        || pins.cube_list.as_deref() != Some(runtime.cube_list.as_str())
        || pins.fence_record.as_deref() != Some(runtime.fence_record.as_str())
        || pins.card_index.as_deref() != Some(runtime.card_index.as_str())
        || pins.cube_table.as_deref() != Some(runtime.cube_table.as_str())
    {
        return Err(plan_err(
            "pins differ from the running binary or input pins",
        ));
    }
    if let Some(inp) = inputs {
        if let Some(ref p_sha) = inp.pool_sha256 {
            if runtime.pool_sha256.as_deref() != Some(p_sha.as_str()) {
                return Err(plan_err("inputs.pool_sha256 differs from runtime input"));
            }
        }
        if let Some(ref c_sha) = inp.constants_sha256 {
            if runtime.constants_sha256.as_deref() != Some(c_sha.as_str()) {
                return Err(plan_err(
                    "inputs.constants_sha256 differs from runtime input",
                ));
            }
        }
    }
    Ok(())
}

fn load_and_verify_panel(
    set_dir: &Path,
    manifest: &SetManifest,
    verification: &RuntimeVerification,
) -> Result<Option<PanelDocument>, PodRunnerError> {
    if manifest.is_prep_panel() {
        // Stage-1 `prep-panel` is explicitly exempt from pinned-panel checks.
        return Ok(None);
    }
    let Some(ref expected_digest) = manifest.panel_sha256 else {
        return Ok(None);
    };
    let Some(ref pinned_digest) = verification.pinned_panel_sha256 else {
        return Err(plan_err(
            "panel_sha256 is set on manifest but no pinned panel digest was supplied",
        ));
    };
    if expected_digest != pinned_digest {
        return Err(plan_err(format!(
            "manifest.panel_sha256 ({expected_digest}) differs from pinned digest ({pinned_digest})"
        )));
    }
    let panel_path = set_dir
        .parent()
        .ok_or_else(|| plan_err("set_dir has no parent directory for panel/panel.json"))?
        .join("panel")
        .join("panel.json");
    let raw = fs::read(&panel_path)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", panel_path.display())))?;
    let actual_digest = sha256_hex(&raw);
    if actual_digest != *expected_digest {
        return Err(plan_err(format!(
            "panel.json sha256 ({actual_digest}) differs from panel_sha256 ({expected_digest})"
        )));
    }
    let text = std::str::from_utf8(&raw)
        .map_err(|error| plan_err(format!("panel.json is not valid UTF-8: {error}")))?;
    let panel = parse_and_validate_panel_json(text)?;
    if panel.namespace != manifest.namespace || panel.run != manifest.run {
        return Err(plan_err(
            "panel.json namespace/run disagrees with set manifest",
        ));
    }
    verify_runtime_pins(&panel.pins, Some(&panel.inputs), &verification.pins)?;
    Ok(Some(panel))
}

fn verify_plan_against_panel(plan: &ReadPlanV1, panel: &PanelDocument) -> Result<(), PodRunnerError> {
    let draft = panel
        .drafts
        .iter()
        .find(|d| d.unit == plan.unit)
        .ok_or_else(|| plan_err(format!("panel.json has no entry for unit {}", plan.unit)))?;
    if plan.seeds != draft.seeds {
        return Err(plan_err(format!(
            "unit {}: plan.seeds differs from panel.json",
            plan.unit
        )));
    }
    if plan.pools.as_ref() != Some(&draft.pools) {
        return Err(plan_err(format!(
            "unit {}: plan.pools differs from panel.json",
            plan.unit
        )));
    }
    let dealt_decks: Vec<PlanDeck> = plan
        .decks
        .iter()
        .filter(|d| d.source == DeckSource::Dealt)
        .cloned()
        .collect();
    if dealt_decks != draft.opponents {
        return Err(plan_err(format!(
            "unit {}: plan dealt opponents differ from panel.json",
            plan.unit
        )));
    }
    if plan.games != draft.games {
        return Err(plan_err(format!(
            "unit {}: plan.games differs from panel.json",
            plan.unit
        )));
    }
    if plan.family == PlanFamily::D13 {
        let built_decks: Vec<PlanDeck> = plan
            .decks
            .iter()
            .filter(|d| d.source == DeckSource::Build)
            .cloned()
            .collect();
        if draft.built.as_ref() != Some(&built_decks) {
            return Err(plan_err(format!(
                "unit {}: d13 built decks differ from panel.json",
                plan.unit
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Prep mode (`READ-PLAN-v1` sections 4 and 5)
// ---------------------------------------------------------------------------

/// Runs one [`PrepRequest`] in prep mode: drafts with `run_seat_draft` on
/// `seeds.draft_seed` and builds each declared build on `m4-interfaces/v2`'s
/// build streams. Plays NO game and writes NO pod record.
pub fn run_prep_request(
    pool: &CubePool,
    request: &PrepRequest,
    request_sha256: &str,
    seats: &dyn SeatFactory,
) -> Result<PrepResult, PodRunnerError> {
    if !is_sha256_hex(request_sha256) {
        return Err(plan_err(format!(
            "request_sha256 is not a valid 64-hex digest: {request_sha256:?}"
        )));
    }
    let draft_seed = request
        .seeds
        .draft_seed
        .ok_or_else(|| plan_err("prep request missing draft_seed"))?;

    let mut pickers_vec = Vec::with_capacity(SEATS);
    for chair in &request.chairs {
        pickers_vec.push(seats.make_picker(&chair.picker)?);
    }
    let mut pickers: [Box<dyn SeatPicker>; SEATS] = pickers_vec
        .try_into()
        .map_err(|_| plan_err("prep request requires 8 chairs"))?;

    let seat_draft = run_seat_draft(pool, draft_seed, &mut pickers)?;

    let mut builds = Vec::with_capacity(request.builds.len());
    for spec in &request.builds {
        let chair_idx = usize::from(spec.chair);
        let seat_pool = &seat_draft.record.pools[chair_idx];
        let b_seed = build_seed_for_chair(draft_seed, chair_idx, spec.build);
        let mut builder = seats.make_builder(&spec.builder)?;
        let mut stream = SplitMix64::seed(b_seed);
        let outcome = builder.build(seat_pool, &mut stream)?;
        let deck = canonical_deck(&outcome.deck.card_ids());
        deck.validate(seat_pool)?;
        builds.push(PrepBuildResult {
            chair: spec.chair,
            build: spec.build,
            builder: spec.builder.clone(),
            row: spec.row,
            build_seed: b_seed,
            cards: deck.card_ids(),
        });
    }

    Ok(PrepResult {
        schema: READ_PLAN_SCHEMA,
        kind: "prep_result",
        request_sha256: request_sha256.to_owned(),
        draft: seat_draft.record,
        builds,
    })
}

/// Executes a prep set execution (`exec-<e>/prep/unit-<unit>.json` and
/// `exec-<e>/execution.json`). Watchdog limits do not apply to prep sets.
///
/// If `interrupt_after_units` is `Some(k)` (for testing interrupted prep
/// retries), writes `k` completed units and closes `execution.json` with
/// `ended = "interrupted"`. Production callers pass `None`.
pub fn run_prep_set_execution(
    pool: &CubePool,
    set_dir: &Path,
    execution: u64,
    seats: &dyn SeatFactory,
    verification: &RuntimeVerification,
    interrupt_after_units: Option<usize>,
) -> Result<PrepExecutionReceipt, PodRunnerError> {
    let manifest_path = set_dir.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", manifest_path.display())))?;
    let manifest = parse_and_validate_manifest_json(&manifest_text)?;
    if manifest.kind != ManifestKind::Prep {
        return Err(plan_err("run_prep_set_execution requires manifest kind=\"prep\""));
    }
    verify_declaration(&manifest, verification)?;

    // Execution must be the next appended execution entry.
    let expected_exec = manifest
        .executions
        .len()
        .checked_sub(1)
        .ok_or_else(|| plan_err("manifest.executions has no entry to execute"))?
        as u64;
    if execution != expected_exec {
        return Err(plan_err(format!(
            "execution {execution} is not the next execution ({expected_exec})"
        )));
    }
    let exec_entry = &manifest.executions[execution as usize];
    if exec_entry.binary != verification.pins.binary || exec_entry.platform != verification.platform
    {
        return Err(plan_err(
            "manifest execution entry binary/platform disagrees with runtime verification",
        ));
    }
    if execution >= 1 {
        let prev_receipt_path = set_dir
            .join(format!("exec-{}", execution - 1))
            .join("execution.json");
        let prev_text = fs::read_to_string(&prev_receipt_path).map_err(|error| {
            plan_err(format!(
                "prep_retry requires previous execution receipt {}: {error}",
                prev_receipt_path.display()
            ))
        })?;
        let prev_receipt = parse_and_validate_prep_execution_receipt_json(&prev_text)?;
        if prev_receipt.ended != PrepExecutionEnded::Interrupted {
            return Err(plan_err(format!(
                "prep_retry allowed only after previous execution ended \"interrupted\", got {:?}",
                prev_receipt.ended
            )));
        }
    }

    let panel = load_and_verify_panel(set_dir, &manifest, verification)?;

    let exec_dir = set_dir.join(format!("exec-{execution}"));
    let prep_dir = exec_dir.join("prep");
    let receipt_path = exec_dir.join("execution.json");
    if receipt_path.exists() {
        return Err(plan_err(format!(
            "{} already exists; refusing to overwrite",
            receipt_path.display()
        )));
    }

    // Validate all unit requests up front before writing any output.
    let mut validated_units = Vec::with_capacity(manifest.plans.len());
    for entry in &manifest.plans {
        let ManifestEntry::Unit(unit_entry) = entry else {
            return Err(plan_err("prep manifest contains non-unit entry"));
        };
        let req_path = set_dir.join(&unit_entry.file);
        let req_bytes = fs::read(&req_path)
            .map_err(|error| PodRunnerError::Io(format!("{}: {error}", req_path.display())))?;
        let req_sha = sha256_hex(&req_bytes);
        if req_sha != unit_entry.sha256 {
            return Err(plan_err(format!(
                "{}: sha256 {req_sha} does not match manifest {}",
                unit_entry.file, unit_entry.sha256
            )));
        }
        let req_text = std::str::from_utf8(&req_bytes)
            .map_err(|error| plan_err(format!("{}: invalid UTF-8: {error}", unit_entry.file)))?;
        let req = parse_and_validate_prep_request_json(req_text)?;
        if req.unit != unit_entry.unit
            || req.namespace != manifest.namespace
            || req.run != manifest.run
            || req.family.as_str() != manifest.family
        {
            return Err(plan_err(format!(
                "{}: request header disagrees with manifest",
                unit_entry.file
            )));
        }
        verify_runtime_pins(&req.pins, None, &verification.pins)?;
        if req.family == PrepFamily::Dose {
            if manifest.is_prep_panel() && !req.builds.is_empty() {
                return Err(plan_err(format!(
                    "{}: dose prep-panel requests must have empty builds",
                    unit_entry.file
                )));
            }
            if !manifest.is_prep_panel() && req.builds.is_empty() {
                return Err(plan_err(format!(
                    "{}: prep-dose requests must declare current and initial builds",
                    unit_entry.file
                )));
            }
        }
        let out_path = prep_dir.join(format!("unit-{}.json", unit_entry.unit));
        if out_path.exists() {
            return Err(plan_err(format!(
                "{} already exists; refusing to overwrite",
                out_path.display()
            )));
        }
        validated_units.push((unit_entry.clone(), req, out_path));
    }

    fs::create_dir_all(&prep_dir)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", prep_dir.display())))?;

    let mut outputs = Vec::with_capacity(validated_units.len());
    let mut interrupted = false;
    for (idx, (unit_entry, req, out_path)) in validated_units.iter().enumerate() {
        if interrupt_after_units == Some(idx) {
            interrupted = true;
            break;
        }
        let result = run_prep_request(pool, req, &unit_entry.sha256, seats)?;
        if let Some(ref panel_doc) = panel {
            let panel_draft = panel_doc
                .drafts
                .iter()
                .find(|d| d.unit == req.unit)
                .ok_or_else(|| {
                    plan_err(format!("panel.json missing draft for unit {}", req.unit))
                })?;
            if req.seeds != panel_draft.seeds || result.draft.pools != panel_draft.pools {
                return Err(plan_err(format!(
                    "unit {}: prep draft differs from panel.json",
                    req.unit
                )));
            }
        }
        let json_text = to_canonical_pretty_json(&result)?;
        let digest = sha256_hex(json_text.as_bytes());
        write_new_bytes(out_path, json_text.as_bytes())?;
        outputs.push(PrepOutputEntry {
            unit: unit_entry.unit,
            sha256: digest,
            status: "complete".to_owned(),
        });
    }

    let receipt = PrepExecutionReceipt {
        execution,
        plan_set_sha256: manifest.plan_set_sha256.clone(),
        binary: exec_entry.binary.clone(),
        platform: exec_entry.platform.clone(),
        ended: if interrupted {
            PrepExecutionEnded::Interrupted
        } else {
            PrepExecutionEnded::Complete
        },
        outputs,
    };
    let receipt_text = to_canonical_pretty_json(&receipt)?;
    write_new_bytes(&receipt_path, receipt_text.as_bytes())?;
    Ok(receipt)
}

/// Outcome of accepting a prep set under `READ-PLAN-v1` section 5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedPrepExecution {
    pub execution: u64,
    pub receipt: PrepExecutionReceipt,
    pub output_bytes_by_unit: BTreeMap<u64, Vec<u8>>,
}

fn validate_one_prep_execution(
    set_dir: &Path,
    manifest: &SetManifest,
    exec_entry: &ManifestExecutionEntry,
) -> Result<Option<(PrepExecutionReceipt, BTreeMap<u64, Vec<u8>>)>, PodRunnerError> {
    let exec_dir = set_dir.join(format!("exec-{}", exec_entry.execution));
    let receipt_path = exec_dir.join("execution.json");
    if !receipt_path.exists() {
        return Ok(None);
    }
    let receipt_text = fs::read_to_string(&receipt_path)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", receipt_path.display())))?;
    let receipt = parse_and_validate_prep_execution_receipt_json(&receipt_text)?;
    if receipt.execution != exec_entry.execution
        || receipt.binary != exec_entry.binary
        || receipt.platform != exec_entry.platform
        || receipt.plan_set_sha256 != manifest.plan_set_sha256
    {
        return Err(plan_err(format!(
            "exec-{}: receipt metadata disagrees with manifest",
            exec_entry.execution
        )));
    }
    if receipt.ended != PrepExecutionEnded::Complete {
        return Ok(None);
    }

    // Build expected unit -> request_sha256 map from manifest.
    let mut expected_req_sha: BTreeMap<u64, String> = BTreeMap::new();
    for entry in &manifest.plans {
        let ManifestEntry::Unit(u) = entry else {
            return Err(plan_err("prep manifest contains non-unit entry"));
        };
        expected_req_sha.insert(u.unit, u.sha256.clone());
    }

    // Check exact coverage of manifest units and statuses.
    if receipt.outputs.len() != expected_req_sha.len() {
        return Err(plan_err(format!(
            "exec-{}: outputs count {} does not cover manifest units {}",
            exec_entry.execution,
            receipt.outputs.len(),
            expected_req_sha.len()
        )));
    }
    let mut seen_units = BTreeSet::new();
    let mut expected_files = BTreeSet::new();
    let mut output_bytes = BTreeMap::new();
    let prep_dir = exec_dir.join("prep");

    for out in &receipt.outputs {
        if out.status != "complete" {
            return Err(plan_err(format!(
                "exec-{}: unit {} status is {:?}, expected \"complete\"",
                exec_entry.execution, out.unit, out.status
            )));
        }
        let Some(req_sha) = expected_req_sha.get(&out.unit) else {
            return Err(plan_err(format!(
                "exec-{}: output unit {} not in manifest",
                exec_entry.execution, out.unit
            )));
        };
        if !seen_units.insert(out.unit) {
            return Err(plan_err(format!(
                "exec-{}: duplicate output unit {}",
                exec_entry.execution, out.unit
            )));
        }
        let file_name = format!("unit-{}.json", out.unit);
        expected_files.insert(file_name.clone());
        let file_path = prep_dir.join(&file_name);
        let bytes = fs::read(&file_path)
            .map_err(|error| PodRunnerError::Io(format!("{}: {error}", file_path.display())))?;
        let digest = sha256_hex(&bytes);
        if digest != out.sha256 {
            return Err(plan_err(format!(
                "exec-{}: {file_name} sha256 {digest} differs from receipt {}",
                exec_entry.execution, out.sha256
            )));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| plan_err(format!("{file_name}: invalid UTF-8: {error}")))?;
        let parsed = parse_and_validate_prep_result_json(text)?;
        if parsed.request_sha256 != *req_sha {
            return Err(plan_err(format!(
                "exec-{}: {file_name} request_sha256 {} differs from manifest {req_sha}",
                exec_entry.execution, parsed.request_sha256
            )));
        }
        output_bytes.insert(out.unit, bytes);
    }

    // Check `prep/` holds exact listed files (no extra files).
    let mut actual_files = BTreeSet::new();
    for dir_entry in fs::read_dir(&prep_dir)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", prep_dir.display())))?
    {
        let dir_entry = dir_entry
            .map_err(|error| PodRunnerError::Io(format!("{}: {error}", prep_dir.display())))?;
        let name = dir_entry
            .file_name()
            .into_string()
            .map_err(|_| plan_err("non-UTF-8 file name in prep/"))?;
        actual_files.insert(name);
    }
    if actual_files != expected_files {
        return Err(plan_err(format!(
            "exec-{}: prep/ files {actual_files:?} differ from receipt files {expected_files:?}",
            exec_entry.execution
        )));
    }

    Ok(Some((receipt, output_bytes)))
}

/// Selects the lowest-numbered accepted prep execution of `set_dir` under all
/// six criteria of `READ-PLAN-v1` section 5, and verifies that any later
/// complete execution has byte-identical outputs (reporting a determinism
/// failure otherwise).
pub fn accept_prep_execution(set_dir: &Path) -> Result<AcceptedPrepExecution, PodRunnerError> {
    let manifest_path = set_dir.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", manifest_path.display())))?;
    let manifest = parse_and_validate_manifest_json(&manifest_text)?;
    if manifest.kind != ManifestKind::Prep {
        return Err(plan_err("accept_prep_execution requires manifest kind=\"prep\""));
    }

    let mut accepted: Option<AcceptedPrepExecution> = None;
    for exec_entry in &manifest.executions {
        let Some((receipt, output_bytes)) =
            validate_one_prep_execution(set_dir, &manifest, exec_entry)?
        else {
            continue;
        };
        if let Some(ref first) = accepted {
            if output_bytes != first.output_bytes_by_unit {
                return Err(plan_err(format!(
                    "prep determinism failure: exec-{} outputs are not byte-identical to accepted exec-{}",
                    exec_entry.execution, first.execution
                )));
            }
        } else {
            accepted = Some(AcceptedPrepExecution {
                execution: exec_entry.execution,
                receipt,
                output_bytes_by_unit: output_bytes,
            });
        }
    }

    accepted.ok_or_else(|| plan_err("no accepted complete prep execution found in set"))
}

// ---------------------------------------------------------------------------
// Play-time redraft/rebuild + execution + supervisor progress journal
// ---------------------------------------------------------------------------

/// Header written durably to a pod's supervisor progress journal before game 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodJournalHeader {
    pub draft: DraftRecord,
    pub chairs: Vec<ChairRecord>,
    pub decks: Vec<DeckRecord>,
}

#[derive(Serialize)]
struct JournalHeaderLine<'a> {
    kind: &'static str,
    draft: &'a DraftRecord,
    chairs: &'a [ChairRecord],
    decks: &'a [DeckRecord],
}

#[derive(Serialize)]
struct JournalGameLine<'a> {
    kind: &'static str,
    game: &'a PodGameRecord,
    timings: &'a [AttemptTiming],
}

/// Durable append-only per-pod progress journal written outside `records/` so
/// D5's supervisor can synthesize a valid stopped [`PodRecord`] and stop report
/// from the last durable game if the worker process dies.
pub struct PodProgressJournal {
    path: PathBuf,
    writer: Mutex<BufWriter<File>>,
}

impl PodProgressJournal {
    /// Creates a new progress journal file at `path` (must not exist yet).
    pub fn create(path: PathBuf) -> Result<Self, PodRunnerError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| PodRunnerError::Io(format!("{}: {error}", parent.display())))?;
        }
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|error| PodRunnerError::Io(format!("{}: {error}", path.display())))?;
        Ok(PodProgressJournal {
            path,
            writer: Mutex::new(BufWriter::new(file)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record_header(&self, header: &PodJournalHeader) -> Result<(), String> {
        let line = to_canonical_compact_json(&JournalHeaderLine {
            kind: "header",
            draft: &header.draft,
            chairs: &header.chairs,
            decks: &header.decks,
        })
        .map_err(|e| e.to_string())?;
        self.append_line(&line)
    }

    pub fn record_game(
        &self,
        game: &PodGameRecord,
        timings: &[AttemptTiming],
    ) -> Result<(), String> {
        let line = to_canonical_compact_json(&JournalGameLine {
            kind: "game",
            game,
            timings,
        })
        .map_err(|e| e.to_string())?;
        self.append_line(&line)
    }

    fn append_line(&self, line: &str) -> Result<(), String> {
        let mut guard = self.writer.lock().expect("journal lock");
        writeln!(guard, "{line}").map_err(|e| format!("{}: {e}", self.path.display()))?;
        guard
            .flush()
            .map_err(|e| format!("{}: {e}", self.path.display()))?;
        guard
            .get_ref()
            .sync_data()
            .map_err(|e| format!("{}: {e}", self.path.display()))?;
        Ok(())
    }
}

fn parse_chair_record(value: &OrderedJson, path: &str) -> Result<ChairRecord, String> {
    let c = expect_object_keys(value, path, &["chair", "role", "picker", "pick_rows"])?;
    let chair = expect_u8(&c[0].1, &format!("{path}.chair"))?;
    let role = match expect_str(&c[1].1, &format!("{path}.role"))? {
        "learning" => ChairRole::Learning,
        "older" => ChairRole::Older,
        "fixture" => ChairRole::Fixture,
        other => return Err(format!("{path}.role: invalid {other:?}")),
    };
    let picker = parse_seat_ref_v1(&c[2].1, &format!("{path}.picker"))?;
    let pick_rows = expect_array(&c[3].1, &format!("{path}.pick_rows"))?
        .iter()
        .enumerate()
        .map(|(i, v)| parse_decision_row_v1(v, &format!("{path}.pick_rows[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ChairRecord {
        chair,
        role,
        picker,
        pick_rows,
    })
}

fn parse_deck_record(value: &OrderedJson, path: &str) -> Result<DeckRecord, String> {
    let d = expect_object_keys(
        value,
        path,
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
    let deck = expect_u32(&d[0].1, &format!("{path}.deck"))?;
    let source = match expect_str(&d[1].1, &format!("{path}.source"))? {
        "build" => DeckSource::Build,
        "dealt" => DeckSource::Dealt,
        other => return Err(format!("{path}.source: invalid {other:?}")),
    };
    let chair = expect_opt_u8(&d[2].1, &format!("{path}.chair"))?;
    let build = expect_opt_u8(&d[3].1, &format!("{path}.build"))?;
    let dealt = match &d[8].1 {
        OrderedJson::Null => None,
        other => Some(parse_dealt_origin(other, &format!("{path}.dealt"))?),
    };
    let builder =
        parse_builder_ref_v1(&d[4].1, source, &d[8].1, &format!("{path}.builder"))?;
    let build_seed = expect_opt_u64(&d[5].1, &format!("{path}.build_seed"))?;
    let cards = parse_cards_40_ascending(&d[6].1, &format!("{path}.cards"))?;
    let build_rows = match &d[7].1 {
        OrderedJson::Null => None,
        other => Some(parse_build_rows_v1(other, &format!("{path}.build_rows"))?),
    };
    Ok(DeckRecord {
        deck,
        source,
        chair,
        build,
        builder,
        build_seed,
        cards,
        build_rows,
        dealt,
    })
}

fn parse_attempt_timing(value: &OrderedJson, path: &str) -> Result<AttemptTiming, String> {
    let e = expect_object_keys(value, path, &["game", "attempt", "elapsed_ms", "host"])?;
    Ok(AttemptTiming {
        game: expect_u32(&e[0].1, &format!("{path}.game"))?,
        attempt: expect_u8(&e[1].1, &format!("{path}.attempt"))?,
        elapsed_ms: expect_u64(&e[2].1, &format!("{path}.elapsed_ms"))?,
        host: expect_str(&e[3].1, &format!("{path}.host"))?.to_owned(),
    })
}

/// Redrafts and rebuilds a [`ReadPlanV1`], checking every pool and pre-built
/// deck card list against the plan before game 0 (`READ-PLAN-v1` section 4/6).
pub fn prepare_read_plan_decks(
    pool: &CubePool,
    plan: &ReadPlanV1,
    seats: &dyn SeatFactory,
) -> Result<(PodJournalHeader, Vec<Deck>), PodRunnerError> {
    let draft_seed = plan
        .seeds
        .draft_seed
        .ok_or_else(|| plan_err("plan missing draft_seed"))?;

    let mut pickers_vec = Vec::with_capacity(SEATS);
    for chair in &plan.chairs {
        pickers_vec.push(seats.make_picker(&chair.picker)?);
    }
    let mut pickers: [Box<dyn SeatPicker>; SEATS] = pickers_vec
        .try_into()
        .map_err(|_| plan_err("plan requires 8 chairs"))?;

    let seat_draft = run_seat_draft(pool, draft_seed, &mut pickers)?;
    if let Some(ref expected_pools) = plan.pools {
        if &seat_draft.record.pools != expected_pools {
            return Err(plan_err(
                "play-time redraft pools differ from plan.pools; refusing before game 0",
            ));
        }
    }

    let chair_records: Vec<ChairRecord> = (0..SEATS)
        .map(|c| ChairRecord {
            chair: c as u8,
            role: plan.chairs[c].role,
            picker: plan.chairs[c].picker.clone(),
            pick_rows: seat_draft.pick_rows[c].clone(),
        })
        .collect();

    let mut deck_records = Vec::with_capacity(plan.decks.len());
    let mut built_decks = Vec::with_capacity(plan.decks.len());
    for spec in &plan.decks {
        match spec.source {
            DeckSource::Build => {
                let chair_idx = usize::from(
                    spec.chair
                        .ok_or_else(|| plan_err("built deck missing chair"))?,
                );
                let build_k = spec
                    .build
                    .ok_or_else(|| plan_err("built deck missing build index"))?;
                let seat_pool = &seat_draft.record.pools[chair_idx];
                let b_seed = build_seed_for_chair(draft_seed, chair_idx, build_k);
                let mut builder = seats.make_builder(&spec.builder)?;
                let mut stream = SplitMix64::seed(b_seed);
                let outcome = builder.build(seat_pool, &mut stream)?;
                let deck = canonical_deck(&outcome.deck.card_ids());
                deck.validate(seat_pool)?;
                let card_ids = deck.card_ids();
                if let Some(ref expected_cards) = spec.cards {
                    if &card_ids != expected_cards {
                        return Err(plan_err(format!(
                            "play-time rebuild of deck {} differs from plan.decks[{}].cards; refusing before game 0",
                            spec.deck, spec.deck
                        )));
                    }
                }
                deck_records.push(DeckRecord {
                    deck: spec.deck,
                    source: DeckSource::Build,
                    chair: spec.chair,
                    build: spec.build,
                    builder: spec.builder.clone(),
                    build_seed: Some(b_seed),
                    cards: card_ids,
                    build_rows: outcome.rows,
                    dealt: None,
                });
                built_decks.push(deck);
            }
            DeckSource::Dealt => {
                let cards = spec
                    .cards
                    .as_ref()
                    .ok_or_else(|| plan_err("dealt deck missing cards"))?;
                let deck = canonical_deck(cards);
                deck_records.push(DeckRecord {
                    deck: spec.deck,
                    source: DeckSource::Dealt,
                    chair: None,
                    build: None,
                    builder: spec.builder.clone(),
                    build_seed: None,
                    cards: deck.card_ids(),
                    build_rows: None,
                    dealt: spec.dealt.clone(),
                });
                built_decks.push(deck);
            }
        }
    }

    Ok((
        PodJournalHeader {
            draft: seat_draft.record,
            chairs: chair_records,
            decks: deck_records,
        },
        built_decks,
    ))
}

/// Options for executing a single [`ReadPlanV1`] pod.
pub struct ReadPlanRunOptions<'a> {
    pub execution: u64,
    pub workers: usize,
    pub host: &'a str,
    pub watchdog_override: Option<Duration>,
    pub journal: Option<&'a PodProgressJournal>,
}

/// Result of executing a single [`ReadPlanV1`] pod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPlanPodOutput {
    pub record: PodRecord,
    pub timings: Vec<AttemptTiming>,
    pub watchdog_stopped: bool,
}

/// Executes one validated [`ReadPlanV1`] pod: redrafts and rebuilds (refusing
/// before game 0 on any difference from `plan.pools` or `plan.decks[].cards`),
/// plays all scheduled games with `max_replays = 0`, and handles `e = 0` vs
/// `e = 1` watchdog semantics.
pub fn execute_read_plan_v1(
    pool: &CubePool,
    plan: &ReadPlanV1,
    seats: &dyn SeatFactory,
    pilot: &(dyn GamePilot + Sync),
    options: &ReadPlanRunOptions<'_>,
) -> Result<ReadPlanPodOutput, PodRunnerError> {
    if options.execution > 1 {
        return Err(plan_err(format!(
            "plan execution {} is forbidden (only 0 or 1 allowed)",
            options.execution
        )));
    }
    let (header, built_decks) = prepare_read_plan_decks(pool, plan, seats)?;
    if let Some(journal) = options.journal {
        journal
            .record_header(&header)
            .map_err(PodRunnerError::Io)?;
    }

    let tasks: Vec<GameTask> = plan
        .games
        .iter()
        .map(|g| {
            let seats = match g.chairs {
                [Some(c0), Some(c1)] if c0 != c1 => [usize::from(c0), usize::from(c1)],
                _ => [0, 1],
            };
            GameTask {
                game: g.game,
                group: g.group,
                role: g.role,
                pairing: g.pairing,
                decks: g.decks,
                chairs: g.chairs,
                seats,
                seat_rule: SeatRule::Schedule,
                game_seed: g.game_seed,
                episode: g.episode,
            }
        })
        .collect();

    let journal_opt = options.journal;
    let hook_closure = move |game: &PodGameRecord, timings: &[AttemptTiming]| -> Result<(), String> {
        if let Some(journal) = journal_opt {
            journal.record_game(game, timings)?;
        }
        Ok(())
    };
    let on_game_complete: Option<&GameCompleteHook<'_>> = if journal_opt.is_some() {
        Some(&hook_closure)
    } else {
        None
    };

    let stop_on_watchdog = options.execution == 1;
    let exec_opts = ExecOptions {
        caps: plan.caps,
        watchdog_override: options.watchdog_override,
        workers: options.workers,
        host: options.host,
        stop_on_watchdog,
        on_game_complete,
    };
    let exec_out = execute_game_tasks_with_options(&tasks, &built_decks, pilot, &exec_opts)?;

    let (halts, status, stop, watchdog_stopped) = if let Some(stop_idx) = exec_out.first_watchdog_stop
    {
        let (halts, _, _) = collect_halts_and_stop(&exec_out.games, false);
        let expired = &exec_out.games[stop_idx];
        let a0 = &expired.attempts[0];
        let stop_report = StopReport {
            game: expired.game,
            pairing: expired.pairing,
            decks: expired.decks,
            attempts: vec![StopAttemptReport {
                deal_seed: a0.deal_seed,
                stop_kind: WATCHDOG_STOP_KIND.to_owned(),
                last_action_source: None,
            }],
        };
        (halts, PodStatus::Stopped, Some(stop_report), true)
    } else {
        let (halts, status, stop) = collect_halts_and_stop(&exec_out.games, false);
        (halts, status, stop, false)
    };

    let record = PodRecord {
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
        draft: Some(header.draft),
        chairs: header.chairs,
        decks: header.decks,
        games: exec_out.games,
        paired_builds: plan.paired_builds.clone(),
        halts,
        status,
        stop,
    };

    Ok(ReadPlanPodOutput {
        record,
        timings: exec_out.timings,
        watchdog_stopped,
    })
}

/// Synthesizes a valid stopped [`PodRecord`] (and matching timing entries) from
/// a worker's durable progress journal after process death (`m4-interfaces/v2`
/// change 5, `READ-PLAN-v1` section 5).
///
/// Keeps only the longest contiguous prefix `0..k` of durably completed games;
/// later plan games remain unplayed slots with no halt row and no imputation.
pub fn finalize_stopped_record_from_journal(
    plan: &ReadPlanV1,
    journal_path: &Path,
    fallback_header: Option<PodJournalHeader>,
) -> Result<(PodRecord, Vec<AttemptTiming>), PodRunnerError> {
    let mut header: Option<PodJournalHeader> = None;
    let mut completed_by_game: BTreeMap<u32, (PodGameRecord, Vec<AttemptTiming>)> = BTreeMap::new();

    if journal_path.exists() {
        let raw = fs::read_to_string(journal_path)
            .map_err(|error| PodRunnerError::Io(format!("{}: {error}", journal_path.display())))?;
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Tolerate a truncated trailing line caused by abrupt process death.
            let Ok(parsed) = JsonParser::new(trimmed).parse_root() else {
                break;
            };
            let OrderedJson::Object(ref entries) = parsed else {
                break;
            };
            let Some((_, OrderedJson::Str(kind))) = entries.first() else {
                break;
            };
            match kind.as_str() {
                "header" => {
                    let Ok(h) =
                        expect_object_keys(&parsed, "journal.header", &["kind", "draft", "chairs", "decks"])
                    else {
                        break;
                    };
                    let Ok(draft) = parse_draft_record(&h[1].1, "journal.header.draft") else {
                        break;
                    };
                    let Ok(chairs_arr) = expect_array(&h[2].1, "journal.header.chairs") else {
                        break;
                    };
                    let Ok(chairs) = chairs_arr
                        .iter()
                        .enumerate()
                        .map(|(i, v)| parse_chair_record(v, &format!("journal.header.chairs[{i}]")))
                        .collect::<Result<Vec<_>, _>>()
                    else {
                        break;
                    };
                    let Ok(decks_arr) = expect_array(&h[3].1, "journal.header.decks") else {
                        break;
                    };
                    let Ok(decks) = decks_arr
                        .iter()
                        .enumerate()
                        .map(|(i, v)| parse_deck_record(v, &format!("journal.header.decks[{i}]")))
                        .collect::<Result<Vec<_>, _>>()
                    else {
                        break;
                    };
                    header = Some(PodJournalHeader {
                        draft,
                        chairs,
                        decks,
                    });
                }
                "game" => {
                    let Ok(g) = expect_object_keys(&parsed, "journal.game", &["kind", "game", "timings"])
                    else {
                        break;
                    };
                    let Ok(game) = parse_pod_game_record(&g[1].1, "journal.game.game") else {
                        break;
                    };
                    let Ok(t_arr) = expect_array(&g[2].1, "journal.game.timings") else {
                        break;
                    };
                    let Ok(timings) = t_arr
                        .iter()
                        .enumerate()
                        .map(|(i, v)| parse_attempt_timing(v, &format!("journal.game.timings[{i}]")))
                        .collect::<Result<Vec<_>, _>>()
                    else {
                        break;
                    };
                    completed_by_game.insert(game.game, (game, timings));
                }
                _ => break,
            }
        }
    }

    let resolved_header = header
        .or(fallback_header)
        .ok_or_else(|| plan_err("cannot finalize stopped record: journal has no header"))?;

    // Collect the longest contiguous prefix `0..k` of `plan.games`.
    let mut prefix_games = Vec::new();
    let mut prefix_timings = Vec::new();
    for planned in &plan.games {
        if let Some((game_rec, game_timings)) = completed_by_game.remove(&planned.game) {
            prefix_games.push(game_rec);
            prefix_timings.extend(game_timings);
        } else {
            break;
        }
    }

    let (halts, _, _) = collect_halts_and_stop(&prefix_games, false);
    let target_idx = if prefix_games.len() < plan.games.len() {
        prefix_games.len()
    } else {
        plan.games.len().saturating_sub(1)
    };
    let stop_report = if let Some(target) = plan.games.get(target_idx) {
        StopReport {
            game: target.game,
            pairing: target.pairing,
            decks: target.decks,
            attempts: vec![StopAttemptReport {
                deal_seed: target.game_seed,
                stop_kind: "process_death".to_owned(),
                last_action_source: None,
            }],
        }
    } else {
        StopReport {
            game: 0,
            pairing: None,
            decks: [0, 0],
            attempts: vec![StopAttemptReport {
                deal_seed: 0,
                stop_kind: "process_death".to_owned(),
                last_action_source: None,
            }],
        }
    };

    let record = PodRecord {
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
        draft: Some(resolved_header.draft),
        chairs: resolved_header.chairs,
        decks: resolved_header.decks,
        games: prefix_games,
        paired_builds: plan.paired_builds.clone(),
        halts,
        status: PodStatus::Stopped,
        stop: Some(stop_report),
    };

    Ok((record, prefix_timings))
}

// ---------------------------------------------------------------------------
// Plan-set execution and supervision (`exec-<e>/`, receipts, timing, acceptance)
// ---------------------------------------------------------------------------

struct ValidatedPlanSet {
    manifest: SetManifest,
    exec_entry: ManifestExecutionEntry,
    plans: Vec<(ManifestPodEntry, PathBuf, ReadPlanV1)>,
}

fn preflight_plan_set_execution(
    set_dir: &Path,
    execution: u64,
    verification: &RuntimeVerification,
) -> Result<ValidatedPlanSet, PodRunnerError> {
    if execution > 1 {
        return Err(plan_err(format!(
            "plan set execution {execution} is forbidden (never e >= 2)"
        )));
    }
    let manifest_path = set_dir.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", manifest_path.display())))?;
    let manifest = parse_and_validate_manifest_json(&manifest_text)?;
    if manifest.kind != ManifestKind::Plans {
        return Err(plan_err("plan set execution requires manifest kind=\"plans\""));
    }
    verify_declaration(&manifest, verification)?;

    let expected_exec = manifest
        .executions
        .len()
        .checked_sub(1)
        .ok_or_else(|| plan_err("manifest.executions has no entry to execute"))?
        as u64;
    if execution != expected_exec {
        return Err(plan_err(format!(
            "execution {execution} is not the next execution ({expected_exec})"
        )));
    }
    let exec_entry = manifest.executions[execution as usize].clone();
    if exec_entry.binary != verification.pins.binary || exec_entry.platform != verification.platform
    {
        return Err(plan_err(
            "manifest execution entry binary/platform disagrees with runtime verification",
        ));
    }

    if execution == 1 {
        let e0 = accept_plan_execution(set_dir, 0)?;
        if e0.plan_set_sha256 != manifest.plan_set_sha256
            || e0.binary != exec_entry.binary
            || e0.platform != exec_entry.platform
        {
            return Err(plan_err(
                "execution 1 must match execution 0's plan_set_sha256, binary, and platform",
            ));
        }
        let mut e0_watchdogs = Vec::new();
        for rec_entry in &e0.records {
            let rec_path = set_dir
                .join("exec-0")
                .join("records")
                .join(format!("pod-{}.json", rec_entry.pod_index));
            let rec_text = fs::read_to_string(&rec_path)
                .map_err(|error| PodRunnerError::Io(format!("{}: {error}", rec_path.display())))?;
            let parsed = JsonParser::new(&rec_text).parse_root().map_err(plan_err)?;
            let OrderedJson::Object(top) = parsed else {
                return Err(plan_err("exec-0 record is not an object"));
            };
            if let Some((_, OrderedJson::Array(halts))) = top.iter().find(|(k, _)| k == "halts") {
                for h in halts {
                    let h_obj = expect_object_keys(
                        h,
                        "exec-0.halt",
                        &["game", "attempt", "stop_kind", "last_action_source", "deal_seed"],
                    )
                    .map_err(plan_err)?;
                    let sk = expect_str(&h_obj[2].1, "exec-0.halt.stop_kind").map_err(plan_err)?;
                    if sk == WATCHDOG_STOP_KIND {
                        let game =
                            expect_u64(&h_obj[0].1, "exec-0.halt.game").map_err(plan_err)?;
                        e0_watchdogs.push([rec_entry.pod_index, game]);
                    }
                }
            }
        }
        if e0_watchdogs.is_empty() {
            return Err(plan_err(
                "execution 1 (watchdog_rerun) is allowed only when execution 0 holds a terminal:watchdog halt",
            ));
        }
        if exec_entry.expired_games != e0_watchdogs {
            return Err(plan_err(format!(
                "execution 1 expired_games {:?} disagrees with execution 0 watchdog halts {:?}",
                exec_entry.expired_games, e0_watchdogs
            )));
        }
    }

    let panel = load_and_verify_panel(set_dir, &manifest, verification)?;

    let exec_dir = set_dir.join(format!("exec-{execution}"));
    if exec_dir.join("execution.json").exists() {
        return Err(plan_err(format!(
            "{} already exists; refusing to overwrite",
            exec_dir.join("execution.json").display()
        )));
    }

    let mut validated_plans = Vec::with_capacity(manifest.plans.len());
    for entry in &manifest.plans {
        let ManifestEntry::Pod(pod_entry) = entry else {
            return Err(plan_err("plan manifest contains non-pod entry"));
        };
        let plan_path = set_dir.join(&pod_entry.file);
        let plan_bytes = fs::read(&plan_path)
            .map_err(|error| PodRunnerError::Io(format!("{}: {error}", plan_path.display())))?;
        let plan_sha = sha256_hex(&plan_bytes);
        if plan_sha != pod_entry.sha256 {
            return Err(plan_err(format!(
                "{}: sha256 {plan_sha} does not match manifest {}",
                pod_entry.file, pod_entry.sha256
            )));
        }
        let plan_text = std::str::from_utf8(&plan_bytes)
            .map_err(|error| plan_err(format!("{}: invalid UTF-8: {error}", pod_entry.file)))?;
        let plan = parse_and_validate_read_plan_json(plan_text)?;
        if plan.pod_index != pod_entry.pod_index
            || plan.namespace != manifest.namespace
            || plan.run != manifest.run
            || plan.family.as_str() != manifest.family
        {
            return Err(plan_err(format!(
                "{}: plan header disagrees with manifest",
                pod_entry.file
            )));
        }
        verify_runtime_pins(&plan.pins, Some(&plan.inputs), &verification.pins)?;
        if let Some(ref panel_doc) = panel {
            verify_plan_against_panel(&plan, panel_doc)?;
        }
        let rec_path = exec_dir
            .join("records")
            .join(format!("pod-{}.json", pod_entry.pod_index));
        let timing_path = exec_dir
            .join("timing")
            .join(format!("pod-{}.jsonl", pod_entry.pod_index));
        if rec_path.exists() || timing_path.exists() {
            return Err(plan_err(format!(
                "output already exists for pod {}; refusing to overwrite",
                pod_entry.pod_index
            )));
        }
        validated_plans.push((pod_entry.clone(), plan_path, plan));
    }

    Ok(ValidatedPlanSet {
        manifest,
        exec_entry,
        plans: validated_plans,
    })
}

/// Runs a complete plan-set execution in `set_dir/exec-<execution>/`, writing
/// `records/pod-<pod_index>.json`, `timing/pod-<pod_index>.jsonl`, and
/// `execution.json` last.
#[allow(clippy::too_many_arguments)]
pub fn run_plan_set_execution(
    pool: &CubePool,
    set_dir: &Path,
    execution: u64,
    seats: &dyn SeatFactory,
    pilot: &(dyn GamePilot + Sync),
    verification: &RuntimeVerification,
    workers: usize,
    watchdog_override: Option<Duration>,
) -> Result<PlanExecutionReceipt, PodRunnerError> {
    let preflight = preflight_plan_set_execution(set_dir, execution, verification)?;
    let exec_dir = set_dir.join(format!("exec-{execution}"));
    let records_dir = exec_dir.join("records");
    let timing_dir = exec_dir.join("timing");
    fs::create_dir_all(&records_dir)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", records_dir.display())))?;
    fs::create_dir_all(&timing_dir)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", timing_dir.display())))?;

    let mut records = Vec::with_capacity(preflight.plans.len());
    let mut ended = PlanExecutionEnded::Complete;

    for (pod_entry, _, plan) in &preflight.plans {
        let run_opts = ReadPlanRunOptions {
            execution,
            workers,
            host: &verification.host,
            watchdog_override,
            journal: None,
        };
        let out = execute_read_plan_v1(pool, plan, seats, pilot, &run_opts)?;
        let record_text = to_canonical_pretty_json(&out.record)?;
        let record_sha = sha256_hex(record_text.as_bytes());
        let record_path = records_dir.join(format!("pod-{}.json", pod_entry.pod_index));
        let timing_path = timing_dir.join(format!("pod-{}.jsonl", pod_entry.pod_index));
        write_new_bytes(&record_path, record_text.as_bytes())?;
        write_timing_jsonl(&timing_path, &out.timings)?;
        records.push(ExecutionRecordEntry {
            pod_index: pod_entry.pod_index,
            sha256: record_sha,
            status: out.record.status,
        });
        if out.watchdog_stopped {
            ended = PlanExecutionEnded::WatchdogStop;
            break;
        }
    }

    let receipt = PlanExecutionReceipt {
        execution,
        plan_set_sha256: preflight.manifest.plan_set_sha256.clone(),
        binary: preflight.exec_entry.binary.clone(),
        platform: preflight.exec_entry.platform.clone(),
        ended,
        records,
    };
    let receipt_text = to_canonical_pretty_json(&receipt)?;
    write_new_bytes(&exec_dir.join("execution.json"), receipt_text.as_bytes())?;
    Ok(receipt)
}

/// Outcome of one supervised worker invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisedPodOutcome {
    Complete(ExecutionRecordEntry),
    WatchdogStopped(ExecutionRecordEntry),
    ProcessDied { reason: String },
}

/// Launches one pod worker for D5's supervisor.
pub trait PodWorkerLauncher {
    fn run_pod(
        &self,
        plan: &ReadPlanV1,
        plan_path: &Path,
        record_path: &Path,
        timing_path: &Path,
        journal_path: &Path,
        execution: u64,
    ) -> Result<SupervisedPodOutcome, PodRunnerError>;
}

/// OS subprocess launcher for D5's supervisor: spawns a child command per pod,
/// monitors its exit status, and reports [`SupervisedPodOutcome::ProcessDied`]
/// whenever the child exits non-zero or dies without writing the final record.
pub struct CommandPodWorkerLauncher<F>
where
    F: Fn(&Path, &Path, &Path, &Path, u64) -> Command,
{
    build_command: F,
}

impl<F> CommandPodWorkerLauncher<F>
where
    F: Fn(&Path, &Path, &Path, &Path, u64) -> Command,
{
    pub fn new(build_command: F) -> Self {
        CommandPodWorkerLauncher { build_command }
    }
}

impl<F> PodWorkerLauncher for CommandPodWorkerLauncher<F>
where
    F: Fn(&Path, &Path, &Path, &Path, u64) -> Command,
{
    fn run_pod(
        &self,
        plan: &ReadPlanV1,
        plan_path: &Path,
        record_path: &Path,
        timing_path: &Path,
        journal_path: &Path,
        execution: u64,
    ) -> Result<SupervisedPodOutcome, PodRunnerError> {
        let mut cmd = (self.build_command)(
            plan_path,
            record_path,
            timing_path,
            journal_path,
            execution,
        );
        let status = cmd
            .status()
            .map_err(|error| PodRunnerError::Io(format!("spawn worker failed: {error}")))?;
        if !status.success() || !record_path.exists() {
            return Ok(SupervisedPodOutcome::ProcessDied {
                reason: format!("worker exited with {status}"),
            });
        }
        let bytes = fs::read(record_path)
            .map_err(|error| PodRunnerError::Io(format!("{}: {error}", record_path.display())))?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| plan_err(format!("worker record invalid UTF-8: {error}")))?;
        validate_pod_record_json(text).map_err(plan_err)?;
        let parsed = JsonParser::new(text).parse_root().map_err(plan_err)?;
        let OrderedJson::Object(entries) = parsed else {
            return Err(plan_err("worker record is not a JSON object"));
        };
        let pod_status = match entries
            .iter()
            .find(|(k, _)| k == "status")
            .and_then(|(_, v)| match v {
                OrderedJson::Str(s) => Some(s.as_str()),
                _ => None,
            }) {
            Some("complete") => PodStatus::Complete,
            Some("stopped") => PodStatus::Stopped,
            other => return Err(plan_err(format!("unexpected worker record status {other:?}"))),
        };
        let entry = ExecutionRecordEntry {
            pod_index: plan.pod_index,
            sha256: sha256_hex(&bytes),
            status: pod_status,
        };
        if pod_status == PodStatus::Stopped {
            Ok(SupervisedPodOutcome::WatchdogStopped(entry))
        } else {
            Ok(SupervisedPodOutcome::Complete(entry))
        }
    }
}

/// Supervises a plan-set execution pod by pod. If a worker process dies, the
/// supervisor reads its durable progress journal (stored outside `records/`),
/// writes `records/pod-<pod_index>.json` with `status = "stopped"`, its stop
/// report, and the contiguous prefix of durably completed games, writes its
/// timing file, cleans up the journal directory, and closes `execution.json`
/// with `ended = "interrupted"`.
pub fn supervise_plan_set_execution(
    pool: &CubePool,
    set_dir: &Path,
    execution: u64,
    seats: &dyn SeatFactory,
    verification: &RuntimeVerification,
    launcher: &dyn PodWorkerLauncher,
) -> Result<PlanExecutionReceipt, PodRunnerError> {
    let preflight = preflight_plan_set_execution(set_dir, execution, verification)?;
    let exec_dir = set_dir.join(format!("exec-{execution}"));
    let records_dir = exec_dir.join("records");
    let timing_dir = exec_dir.join("timing");
    let journal_dir = set_dir.join(format!(".supervisor-exec-{execution}"));
    fs::create_dir_all(&records_dir)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", records_dir.display())))?;
    fs::create_dir_all(&timing_dir)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", timing_dir.display())))?;
    fs::create_dir_all(&journal_dir)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", journal_dir.display())))?;

    let mut records = Vec::with_capacity(preflight.plans.len());
    let mut ended = PlanExecutionEnded::Complete;

    for (pod_entry, plan_path, plan) in &preflight.plans {
        let record_path = records_dir.join(format!("pod-{}.json", pod_entry.pod_index));
        let timing_path = timing_dir.join(format!("pod-{}.jsonl", pod_entry.pod_index));
        let journal_path = journal_dir.join(format!("pod-{}.journal.jsonl", pod_entry.pod_index));

        match launcher.run_pod(
            plan,
            plan_path,
            &record_path,
            &timing_path,
            &journal_path,
            execution,
        )? {
            SupervisedPodOutcome::Complete(entry) => {
                let _ = fs::remove_file(&journal_path);
                records.push(entry);
            }
            SupervisedPodOutcome::WatchdogStopped(entry) => {
                let _ = fs::remove_file(&journal_path);
                records.push(entry);
                ended = PlanExecutionEnded::WatchdogStop;
                break;
            }
            SupervisedPodOutcome::ProcessDied { .. } => {
                // Remove any partial unfinalized files the dying worker left.
                if record_path.exists() {
                    let _ = fs::remove_file(&record_path);
                }
                if timing_path.exists() {
                    let _ = fs::remove_file(&timing_path);
                }
                let fallback_header = prepare_read_plan_decks(pool, plan, seats)
                    .ok()
                    .map(|(header, _)| header);
                let (stopped_record, timings) =
                    finalize_stopped_record_from_journal(plan, &journal_path, fallback_header)?;
                let record_text = to_canonical_pretty_json(&stopped_record)?;
                let record_sha = sha256_hex(record_text.as_bytes());
                write_new_bytes(&record_path, record_text.as_bytes())?;
                write_timing_jsonl(&timing_path, &timings)?;
                records.push(ExecutionRecordEntry {
                    pod_index: pod_entry.pod_index,
                    sha256: record_sha,
                    status: PodStatus::Stopped,
                });
                ended = PlanExecutionEnded::Interrupted;
                break;
            }
        }
    }

    let _ = fs::remove_dir_all(&journal_dir);

    let receipt = PlanExecutionReceipt {
        execution,
        plan_set_sha256: preflight.manifest.plan_set_sha256.clone(),
        binary: preflight.exec_entry.binary.clone(),
        platform: preflight.exec_entry.platform.clone(),
        ended,
        records,
    };
    let receipt_text = to_canonical_pretty_json(&receipt)?;
    write_new_bytes(&exec_dir.join("execution.json"), receipt_text.as_bytes())?;
    Ok(receipt)
}

/// Validates a plan-set execution directory `exec-<execution>/` under all six
/// acceptance checks of `READ-PLAN-v1` section 7.
pub fn accept_plan_execution(
    set_dir: &Path,
    execution: u64,
) -> Result<PlanExecutionReceipt, PodRunnerError> {
    if execution > 1 {
        return Err(plan_err("plan execution must be 0 or 1"));
    }
    let manifest_text = fs::read_to_string(set_dir.join("manifest.json"))
        .map_err(|error| PodRunnerError::Io(format!("manifest.json: {error}")))?;
    let manifest = parse_and_validate_manifest_json(&manifest_text)?;
    if manifest.kind != ManifestKind::Plans {
        return Err(plan_err("accept_plan_execution requires manifest kind=\"plans\""));
    }
    let exec_entry = manifest
        .executions
        .get(execution as usize)
        .ok_or_else(|| plan_err(format!("manifest lacks execution {execution}")))?;

    let exec_dir = set_dir.join(format!("exec-{execution}"));
    let receipt_path = exec_dir.join("execution.json");
    let receipt_text = fs::read_to_string(&receipt_path)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", receipt_path.display())))?;
    let receipt = parse_and_validate_plan_execution_receipt_json(&receipt_text)?;

    if receipt.execution != exec_entry.execution
        || receipt.binary != exec_entry.binary
        || receipt.platform != exec_entry.platform
        || receipt.plan_set_sha256 != manifest.plan_set_sha256
    {
        return Err(plan_err(format!(
            "exec-{execution}: execution.json metadata disagrees with manifest"
        )));
    }

    let manifest_pods: BTreeSet<u64> = manifest
        .plans
        .iter()
        .filter_map(|e| match e {
            ManifestEntry::Pod(p) => Some(p.pod_index),
            ManifestEntry::Unit(_) => None,
        })
        .collect();

    let records_dir = exec_dir.join("records");
    let mut seen_pods = BTreeSet::new();
    let mut expected_files = BTreeSet::new();

    for rec in &receipt.records {
        if !manifest_pods.contains(&rec.pod_index) {
            return Err(plan_err(format!(
                "exec-{execution}: pod {} not in manifest",
                rec.pod_index
            )));
        }
        if !seen_pods.insert(rec.pod_index) {
            return Err(plan_err(format!(
                "exec-{execution}: duplicate pod {} in receipt",
                rec.pod_index
            )));
        }
        if receipt.ended == PlanExecutionEnded::Complete && rec.status != PodStatus::Complete {
            return Err(plan_err(format!(
                "exec-{execution}: ended=\"complete\" but pod {} has status {:?}",
                rec.pod_index, rec.status
            )));
        }
        let file_name = format!("pod-{}.json", rec.pod_index);
        expected_files.insert(file_name.clone());
        let file_path = records_dir.join(&file_name);
        let bytes = fs::read(&file_path)
            .map_err(|error| PodRunnerError::Io(format!("{}: {error}", file_path.display())))?;
        let digest = sha256_hex(&bytes);
        if digest != rec.sha256 {
            return Err(plan_err(format!(
                "exec-{execution}: {file_name} sha256 {digest} differs from receipt {}",
                rec.sha256
            )));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| plan_err(format!("{file_name}: invalid UTF-8: {error}")))?;
        validate_pod_record_json(text).map_err(plan_err)?;
        let parsed = JsonParser::new(text).parse_root().map_err(plan_err)?;
        let OrderedJson::Object(top) = parsed else {
            return Err(plan_err(format!("{file_name}: not an object")));
        };
        let actual_status = match top
            .iter()
            .find(|(k, _)| k == "status")
            .and_then(|(_, v)| match v {
                OrderedJson::Str(s) => Some(s.as_str()),
                _ => None,
            }) {
            Some("complete") => PodStatus::Complete,
            Some("stopped") => PodStatus::Stopped,
            other => return Err(plan_err(format!("{file_name}: invalid status {other:?}"))),
        };
        if actual_status != rec.status {
            return Err(plan_err(format!(
                "exec-{execution}: {file_name} status {actual_status:?} differs from receipt {:?}",
                rec.status
            )));
        }
    }

    if receipt.ended == PlanExecutionEnded::Complete && seen_pods != manifest_pods {
        return Err(plan_err(format!(
            "exec-{execution}: ended=\"complete\" does not cover every manifest pod"
        )));
    }

    let mut actual_files = BTreeSet::new();
    for entry in fs::read_dir(&records_dir)
        .map_err(|error| PodRunnerError::Io(format!("{}: {error}", records_dir.display())))?
    {
        let entry = entry
            .map_err(|error| PodRunnerError::Io(format!("{}: {error}", records_dir.display())))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| plan_err("non-UTF-8 file name in records/"))?;
        actual_files.insert(name);
    }
    if actual_files != expected_files {
        return Err(plan_err(format!(
            "exec-{execution}: records/ files {actual_files:?} differ from receipt files {expected_files:?}"
        )));
    }

    Ok(receipt)
}
