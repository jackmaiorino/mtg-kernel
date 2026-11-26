//! D6 checkpoint loading (`load_checkpoint_read_only_v1`), D3 learned seat
//! factory (`ModelSeatForward`, `LearnedSeatPicker`, `LearnedSeatBuilder`),
//! `m4-interfaces/v2` change 2 `head_updates` validation, and the `--fixture`
//! test stand-in path.
//!
//! Real checkpoints load through D6's `load_checkpoint_read_only_v1(path, table)`
//! against a verified D1 `CubeTableV1`. That verifies the checkpoint schema,
//! canonical `state_sha256`, `card_index_sha256`, `cube_table_sha256`,
//! `card_database_sha256`, `parameter_layout_sha256`, and model parameter
//! restoration, and we additionally enforce `m4-interfaces/v2` change 2's raw
//! JSON check that `head_updates` is the final top-level key with exact key
//! order `pick`, `value`, `count`, `pointer`, `type_set`, `split` as unquoted
//! decimal u64 integers.

use crate::card_def::CARD_DEFS;
use crate::cube_draft_build_v1::card_index::CubeTableV1;
use crate::cube_draft_build_v1::model::{ModelCardRowsV1, ModelV1};
use crate::cube_draft_v1::{
    basic_land_ids, card_colour_bits, land_colour_bits, DECK_CARDS, PACK_SIZE, POOL_CARDS, ROUNDS,
};
use crate::cube_learned_seat_v1::{
    LearnedSeatBuilder, LearnedSeatPicker, ModelSeatForward, SeatForward,
};
use crate::cube_seat_v1::{
    canonical_deck, BuildOutcome, BuildRows, DecisionKind, DecisionRow, PickOutcome, SeatBuilder,
    SeatError, SeatPickView, SeatPicker, LEARNED_SAMPLER_ID,
};
use crate::cube_training_driver_v1::{
    load_checkpoint_read_only_v1, Checkpoint as D6Checkpoint, CHECKPOINT_SCHEMA,
};
use crate::m4_sampler_v1::{sample_v1, GRID_SIZE};
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Arc;

pub use crate::cube_draft_build_v1::model::training::HeadUpdates;
pub use crate::cube_training_driver_v1::Segment;

/// Schema string for M4 draft-and-build checkpoints (`mtg-kernel-cube-draft-build-checkpoint/v1`).
pub const CHECKPOINT_SCHEMA_V1: &str = CHECKPOINT_SCHEMA;

/// Exact key order required by `m4-interfaces/v2` change 2 for `head_updates`.
pub const HEAD_UPDATE_KEYS: [&str; 6] = ["pick", "value", "count", "pointer", "type_set", "split"];

/// Human-facing head display names in `m4-interfaces/v2` change 2 order.
pub const HEAD_DISPLAY_NAMES: [&str; 6] =
    ["pick", "value", "count", "pointer", "type set", "split"];

/// Returns all six `(display_name, json_key, count)` entries of a `HeadUpdates`
/// struct in `m4-interfaces/v2` change 2 order.
pub fn head_updates_entries(hu: HeadUpdates) -> [(&'static str, &'static str, u64); 6] {
    [
        (HEAD_DISPLAY_NAMES[0], HEAD_UPDATE_KEYS[0], hu.pick),
        (HEAD_DISPLAY_NAMES[1], HEAD_UPDATE_KEYS[1], hu.value),
        (HEAD_DISPLAY_NAMES[2], HEAD_UPDATE_KEYS[2], hu.count),
        (HEAD_DISPLAY_NAMES[3], HEAD_UPDATE_KEYS[3], hu.pointer),
        (HEAD_DISPLAY_NAMES[4], HEAD_UPDATE_KEYS[4], hu.type_set),
        (HEAD_DISPLAY_NAMES[5], HEAD_UPDATE_KEYS[5], hu.split),
    ]
}

/// D0.21 training-state label for one head's `head_updates` count.
///
/// A count of zero is labelled `"untrained"`. A count `N > 0` is labelled
/// `"trained for N updates under random play"`.
pub fn format_head_training_label(updates: u64) -> String {
    if updates == 0 {
        "untrained".to_owned()
    } else {
        format!("trained for {updates} updates under random play")
    }
}

/// Canonical snake_case string for D6's [`Segment`].
pub fn segment_name(segment: Segment) -> &'static str {
    match segment {
        Segment::BuilderOnly => "builder_only",
        Segment::PickPhase => "pick_phase",
    }
}

/// Errors while parsing or validating a checkpoint header for D16.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointHeaderError {
    Json(String),
    InvalidSchema(String),
    InvalidField { field: &'static str, detail: String },
    HeadUpdatesOrder(String),
    HeadUpdatesNotLast(String),
}

impl fmt::Display for CheckpointHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(message) => write!(f, "checkpoint JSON error: {message}"),
            Self::InvalidSchema(found) => write!(
                f,
                "unexpected checkpoint schema {found:?}, expected {CHECKPOINT_SCHEMA_V1:?}"
            ),
            Self::InvalidField { field, detail } => {
                write!(f, "invalid checkpoint field {field:?}: {detail}")
            }
            Self::HeadUpdatesOrder(detail) => {
                write!(f, "invalid head_updates key order or encoding: {detail}")
            }
            Self::HeadUpdatesNotLast(detail) => {
                write!(f, "head_updates must be the final top-level key: {detail}")
            }
        }
    }
}

impl std::error::Error for CheckpointHeaderError {}

/// Verified checkpoint metadata fields consumed by D16, matching D6's
/// `Checkpoint` JSON format (`card_database_sha256`, `segment`, `run`,
/// `update`, `head_updates` as the final top-level key).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointHeader {
    pub schema: String,
    pub model_identity: String,
    pub parameter_layout_sha256: String,
    pub card_index_sha256: String,
    pub cube_table_sha256: String,
    pub card_database_sha256: String,
    pub training_math: String,
    pub sampler: String,
    pub segment: Segment,
    pub run: String,
    pub update: u64,
    pub state_sha256: String,
    pub head_updates: HeadUpdates,
}

impl CheckpointHeader {
    /// Extracts and validates D16's header view from a verified D6 [`D6Checkpoint`].
    pub fn from_d6_checkpoint(checkpoint: &D6Checkpoint) -> Result<Self, CheckpointHeaderError> {
        let card_index_sha256 = checkpoint.card_index_sha256.clone().ok_or_else(|| {
            CheckpointHeaderError::InvalidField {
                field: "card_index_sha256",
                detail: "checkpoint lacks card_index_sha256 pin".to_owned(),
            }
        })?;
        let cube_table_sha256 = checkpoint.cube_table_sha256.clone().ok_or_else(|| {
            CheckpointHeaderError::InvalidField {
                field: "cube_table_sha256",
                detail: "checkpoint lacks cube_table_sha256 pin".to_owned(),
            }
        })?;
        let header = Self {
            schema: checkpoint.schema.clone(),
            model_identity: checkpoint.model_identity.clone(),
            parameter_layout_sha256: checkpoint.parameter_layout_sha256.clone(),
            card_index_sha256,
            cube_table_sha256,
            card_database_sha256: checkpoint.card_database_sha256.clone(),
            training_math: checkpoint.training_math.clone(),
            sampler: checkpoint.sampler.clone(),
            segment: checkpoint.segment,
            run: checkpoint.run.clone(),
            update: checkpoint.update,
            state_sha256: checkpoint.state_sha256.clone(),
            head_updates: checkpoint.head_updates,
        };
        header.validate()?;
        Ok(header)
    }

    /// Parses a top-level checkpoint JSON document and validates every header
    /// field and `m4-interfaces/v2` change 2's `head_updates` placement (final
    /// top-level key), exact 6-key order, and unquoted decimal u64 encoding.
    pub fn from_json_str(text: &str) -> Result<Self, CheckpointHeaderError> {
        let header: Self = serde_json::from_str(text)
            .map_err(|err| CheckpointHeaderError::Json(err.to_string()))?;
        header.validate()?;
        validate_head_updates_raw_json(text)?;
        Ok(header)
    }

    /// Validates schema string, sampler identity, and hash field formats.
    pub fn validate(&self) -> Result<(), CheckpointHeaderError> {
        if self.schema != CHECKPOINT_SCHEMA_V1 {
            return Err(CheckpointHeaderError::InvalidSchema(self.schema.clone()));
        }
        check_non_empty("model_identity", &self.model_identity)?;
        check_sha256_hex("parameter_layout_sha256", &self.parameter_layout_sha256)?;
        check_sha256_hex("card_index_sha256", &self.card_index_sha256)?;
        check_sha256_hex("cube_table_sha256", &self.cube_table_sha256)?;
        check_non_empty("card_database_sha256", &self.card_database_sha256)?;
        check_non_empty("training_math", &self.training_math)?;
        if self.sampler != LEARNED_SAMPLER_ID {
            return Err(CheckpointHeaderError::InvalidField {
                field: "sampler",
                detail: format!("expected {LEARNED_SAMPLER_ID:?}, got {:?}", self.sampler),
            });
        }
        check_non_empty("run", &self.run)?;
        check_sha256_hex("state_sha256", &self.state_sha256)?;
        Ok(())
    }
}

fn check_non_empty(field: &'static str, value: &str) -> Result<(), CheckpointHeaderError> {
    if value.trim().is_empty() {
        return Err(CheckpointHeaderError::InvalidField {
            field,
            detail: "must be non-empty".to_owned(),
        });
    }
    Ok(())
}

fn check_sha256_hex(field: &'static str, value: &str) -> Result<(), CheckpointHeaderError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(CheckpointHeaderError::InvalidField {
            field,
            detail: format!("expected 64 lowercase hex characters, got {value:?}"),
        });
    }
    Ok(())
}

/// Inspects raw top-level JSON text to enforce `m4-interfaces/v2` change 2 on
/// `head_updates`:
/// 1. `head_updates` is the final key of the top-level JSON object.
/// 2. Keys inside `head_updates` appear in the exact order
///    `pick`, `value`, `count`, `pointer`, `type_set`, `split`.
/// 3. Each count is encoded as an unquoted decimal integer.
pub fn validate_head_updates_raw_json(text: &str) -> Result<(), CheckpointHeaderError> {
    let mut parser = RawJsonScanner::new(text);
    let top_entries = parser.scan_object_keys_with_spans()?;
    let head_updates_idx = top_entries
        .iter()
        .position(|entry| entry.key == "head_updates")
        .ok_or_else(|| {
            CheckpointHeaderError::HeadUpdatesOrder("missing top-level head_updates key".to_owned())
        })?;

    if head_updates_idx + 1 != top_entries.len() {
        let trailing: Vec<&str> = top_entries[head_updates_idx + 1..]
            .iter()
            .map(|e| e.key.as_str())
            .collect();
        return Err(CheckpointHeaderError::HeadUpdatesNotLast(format!(
            "keys {:?} appear after head_updates",
            trailing
        )));
    }

    let head_obj_slice =
        &text[top_entries[head_updates_idx].value_start..top_entries[head_updates_idx].value_end];
    let mut inner = RawJsonScanner::new(head_obj_slice);
    let inner_entries = inner.scan_object_keys_with_spans()?;
    let found_keys: Vec<&str> = inner_entries.iter().map(|e| e.key.as_str()).collect();
    if found_keys != HEAD_UPDATE_KEYS {
        return Err(CheckpointHeaderError::HeadUpdatesOrder(format!(
            "expected keys {:?}, got {:?}",
            HEAD_UPDATE_KEYS, found_keys
        )));
    }
    for entry in &inner_entries {
        let raw_val = head_obj_slice[entry.value_start..entry.value_end].trim();
        if raw_val.is_empty()
            || !raw_val.bytes().all(|b| b.is_ascii_digit())
            || (raw_val.len() > 1 && raw_val.starts_with('0'))
        {
            return Err(CheckpointHeaderError::HeadUpdatesOrder(format!(
                "head_updates.{}: expected unquoted decimal u64, got {raw_val:?}",
                entry.key
            )));
        }
    }
    Ok(())
}

struct ObjectKeySpan {
    key: String,
    value_start: usize,
    value_end: usize,
}

struct RawJsonScanner<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> RawJsonScanner<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn scan_string(&mut self) -> Result<String, CheckpointHeaderError> {
        self.skip_ws();
        if self.pos >= self.bytes.len() || self.bytes[self.pos] != b'"' {
            return Err(CheckpointHeaderError::Json(
                "expected JSON string".to_owned(),
            ));
        }
        let start = self.pos;
        self.pos += 1;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            self.pos += 1;
            if b == b'\\' {
                if self.pos < self.bytes.len() {
                    self.pos += 1;
                }
            } else if b == b'"' {
                let raw = std::str::from_utf8(&self.bytes[start..self.pos])
                    .map_err(|err| CheckpointHeaderError::Json(err.to_string()))?;
                return serde_json::from_str(raw)
                    .map_err(|err| CheckpointHeaderError::Json(err.to_string()));
            }
        }
        Err(CheckpointHeaderError::Json(
            "unterminated JSON string".to_owned(),
        ))
    }

    fn skip_value(&mut self) -> Result<(usize, usize), CheckpointHeaderError> {
        self.skip_ws();
        let start = self.pos;
        if self.pos >= self.bytes.len() {
            return Err(CheckpointHeaderError::Json(
                "unexpected end of JSON".to_owned(),
            ));
        }
        match self.bytes[self.pos] {
            b'"' => {
                let _ = self.scan_string()?;
            }
            b'{' | b'[' => {
                let open = self.bytes[self.pos];
                let close = if open == b'{' { b'}' } else { b']' };
                self.pos += 1;
                let mut depth = 1usize;
                let mut in_string = false;
                while self.pos < self.bytes.len() && depth > 0 {
                    let b = self.bytes[self.pos];
                    self.pos += 1;
                    if in_string {
                        if b == b'\\' && self.pos < self.bytes.len() {
                            self.pos += 1;
                        } else if b == b'"' {
                            in_string = false;
                        }
                    } else if b == b'"' {
                        in_string = true;
                    } else if b == open {
                        depth += 1;
                    } else if b == close {
                        depth -= 1;
                    }
                }
                if depth != 0 {
                    return Err(CheckpointHeaderError::Json(
                        "unterminated JSON container".to_owned(),
                    ));
                }
            }
            _ => {
                while self.pos < self.bytes.len()
                    && !matches!(self.bytes[self.pos], b',' | b'}' | b']')
                    && !self.bytes[self.pos].is_ascii_whitespace()
                {
                    self.pos += 1;
                }
            }
        }
        Ok((start, self.pos))
    }

    fn scan_object_keys_with_spans(&mut self) -> Result<Vec<ObjectKeySpan>, CheckpointHeaderError> {
        self.skip_ws();
        if self.pos >= self.bytes.len() || self.bytes[self.pos] != b'{' {
            return Err(CheckpointHeaderError::Json(
                "expected JSON object".to_owned(),
            ));
        }
        self.pos += 1;
        let mut entries = Vec::new();
        loop {
            self.skip_ws();
            if self.pos >= self.bytes.len() {
                return Err(CheckpointHeaderError::Json(
                    "unterminated JSON object".to_owned(),
                ));
            }
            if self.bytes[self.pos] == b'}' {
                self.pos += 1;
                return Ok(entries);
            }
            let key = self.scan_string()?;
            self.skip_ws();
            if self.pos >= self.bytes.len() || self.bytes[self.pos] != b':' {
                return Err(CheckpointHeaderError::Json(
                    "expected ':' after object key".to_owned(),
                ));
            }
            self.pos += 1;
            let (value_start, value_end) = self.skip_value()?;
            entries.push(ObjectKeySpan {
                key,
                value_start,
                value_end,
            });
            self.skip_ws();
            if self.pos >= self.bytes.len() {
                return Err(CheckpointHeaderError::Json(
                    "unterminated JSON object".to_owned(),
                ));
            }
            match self.bytes[self.pos] {
                b',' => self.pos += 1,
                b'}' => {
                    self.pos += 1;
                    return Ok(entries);
                }
                other => {
                    return Err(CheckpointHeaderError::Json(format!(
                        "expected ',' or '}}', got {:?}",
                        other as char
                    )));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Seat factory trait and real D3 + D6 implementation
// ---------------------------------------------------------------------------

/// Supplies the checkpoint's learned [`SeatPicker`] and [`SeatBuilder`].
pub trait OwnerViewSeatFactory {
    fn make_learned_picker(&self) -> Box<dyn SeatPicker>;
    fn make_learned_builder(&self) -> Box<dyn SeatBuilder>;
    fn is_stand_in(&self) -> bool {
        false
    }
}

/// Real seat factory backed by D3's [`ModelSeatForward`], [`LearnedSeatPicker`],
/// and [`LearnedSeatBuilder`] over a verified D2 [`ModelV1`] loaded by D6's
/// `load_checkpoint_read_only_v1`.
pub struct LearnedCheckpointSeats {
    forward: Arc<dyn SeatForward>,
}

impl LearnedCheckpointSeats {
    /// Precomputes both trunk card-hidden tables once for `model` and `rows`.
    pub fn from_model(
        model: Arc<ModelV1>,
        rows: Arc<ModelCardRowsV1>,
        threads: usize,
    ) -> Result<Self, SeatError> {
        let forward: Arc<dyn SeatForward> = Arc::new(ModelSeatForward::new(model, rows, threads)?);
        Ok(Self { forward })
    }
}

impl OwnerViewSeatFactory for LearnedCheckpointSeats {
    fn make_learned_picker(&self) -> Box<dyn SeatPicker> {
        Box::new(LearnedSeatPicker::new(Arc::clone(&self.forward)))
    }

    fn make_learned_builder(&self) -> Box<dyn SeatBuilder> {
        Box::new(LearnedSeatBuilder::new(Arc::clone(&self.forward)))
    }

    fn is_stand_in(&self) -> bool {
        false
    }
}

/// Loads a real D6 checkpoint through D6's [`load_checkpoint_read_only_v1`]
/// against a verified D1 [`CubeTableV1`], checks IF2 change 2's raw JSON
/// `head_updates` final-key format, and builds D3's [`LearnedCheckpointSeats`].
pub fn load_real_checkpoint_v1(
    path: &Path,
    table: &CubeTableV1,
    threads: usize,
) -> Result<(CheckpointHeader, LearnedCheckpointSeats), String> {
    let raw_text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read checkpoint {}: {err}", path.display()))?;
    validate_head_updates_raw_json(&raw_text)
        .map_err(|err| format!("invalid checkpoint {}: {err}", path.display()))?;
    let (checkpoint, model, rows) = load_checkpoint_read_only_v1(path, table)
        .map_err(|err| format!("failed to load checkpoint {}: {err}", path.display()))?;
    let header = CheckpointHeader::from_d6_checkpoint(&checkpoint)
        .map_err(|err| format!("invalid checkpoint header {}: {err}", path.display()))?;
    let seats = LearnedCheckpointSeats::from_model(model, rows, threads)
        .map_err(|err| format!("failed to build D3 seats from {}: {err}", path.display()))?;
    Ok((header, seats))
}

// ---------------------------------------------------------------------------
// Test-only fixture checkpoint and stand-in seats (CLI `--fixture` path only)
// ---------------------------------------------------------------------------

/// Model identity string used exclusively by test fixture checkpoints.
pub const FIXTURE_MODEL_IDENTITY_V1: &str = "m4-draft-build-standin-fixture/v1";

/// Small deterministic fixture parameters embedded in a test fixture checkpoint
/// JSON file (used only when `--fixture` is passed explicitly).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureStandInParams {
    pub fixture_seed: u64,
    pub target_basics: u16,
    pub count_sharpness_milli: i32,
    pub pick_scale_milli: i32,
    pub pointer_scale_milli: i32,
    pub type_set_scale_milli: i32,
    pub split_scale_milli: i32,
}

impl Default for FixtureStandInParams {
    fn default() -> Self {
        Self {
            fixture_seed: 0xD16_F1A7_0001,
            target_basics: 17,
            count_sharpness_milli: 250,
            pick_scale_milli: 600,
            pointer_scale_milli: 450,
            type_set_scale_milli: 500,
            split_scale_milli: 350,
        }
    }
}

/// Small fixture checkpoint JSON matching D6's top-level format (`card_database_sha256`,
/// `segment`, `run`, `update`, and `head_updates` as the final top-level key).
///
/// `fixture_standin` has no serde default: a real D6 checkpoint JSON that omits
/// `fixture_standin` is refused on the `--fixture` path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureCheckpointFile {
    pub schema: String,
    pub model_identity: String,
    pub parameter_layout_sha256: String,
    pub card_index_sha256: String,
    pub cube_table_sha256: String,
    pub card_database_sha256: String,
    pub training_math: String,
    pub sampler: String,
    pub segment: Segment,
    pub run: String,
    pub update: u64,
    pub state_sha256: String,
    pub fixture_standin: FixtureStandInParams,
    pub head_updates: HeadUpdates,
}

impl FixtureCheckpointFile {
    /// Constructs a small deterministic fixture checkpoint for any of the six
    /// shakedown checkpoints.
    pub fn for_shakedown_fixture(
        segment: Segment,
        run: &str,
        update: u64,
        head_updates: HeadUpdates,
        card_database_sha256: &str,
    ) -> Self {
        let seg_str = segment_name(segment);
        let fixture_seed = fixture_digest_u64(&format!("fixture-seed:{seg_str}:{run}:{update}"));
        let fixture_standin = FixtureStandInParams {
            fixture_seed,
            ..FixtureStandInParams::default()
        };
        let state_sha256 = fixture_digest_hex(&format!(
            "state:{seg_str}:{run}:{update}:{}:{}:{}:{}:{}:{}:{fixture_seed}",
            head_updates.pick,
            head_updates.value,
            head_updates.count,
            head_updates.pointer,
            head_updates.type_set,
            head_updates.split,
        ));
        Self {
            schema: CHECKPOINT_SCHEMA_V1.to_owned(),
            model_identity: FIXTURE_MODEL_IDENTITY_V1.to_owned(),
            parameter_layout_sha256: fixture_digest_hex("fixture-parameter-layout-v1"),
            card_index_sha256: fixture_digest_hex("fixture-card-index-v1"),
            cube_table_sha256: fixture_digest_hex("fixture-cube-table-v1"),
            card_database_sha256: card_database_sha256.to_owned(),
            training_math: "portable-math-v1+m4-ln-atanh23-f64-v1".to_owned(),
            sampler: LEARNED_SAMPLER_ID.to_owned(),
            segment,
            run: run.to_owned(),
            update,
            state_sha256,
            fixture_standin,
            head_updates,
        }
    }

    /// Extracts the validated [`CheckpointHeader`].
    pub fn header(&self) -> CheckpointHeader {
        CheckpointHeader {
            schema: self.schema.clone(),
            model_identity: self.model_identity.clone(),
            parameter_layout_sha256: self.parameter_layout_sha256.clone(),
            card_index_sha256: self.card_index_sha256.clone(),
            cube_table_sha256: self.cube_table_sha256.clone(),
            card_database_sha256: self.card_database_sha256.clone(),
            training_math: self.training_math.clone(),
            sampler: self.sampler.clone(),
            segment: self.segment,
            run: self.run.clone(),
            update: self.update,
            state_sha256: self.state_sha256.clone(),
            head_updates: self.head_updates,
        }
    }

    /// Parses a fixture checkpoint JSON document, enforcing all header rules,
    /// requiring explicit `fixture_standin` parameters, and refusing any file
    /// whose `model_identity` is not [`FIXTURE_MODEL_IDENTITY_V1`].
    pub fn from_json_str(text: &str) -> Result<Self, CheckpointHeaderError> {
        let _header = CheckpointHeader::from_json_str(text)?;
        let parsed: Self = serde_json::from_str(text)
            .map_err(|err| CheckpointHeaderError::Json(err.to_string()))?;
        if parsed.model_identity != FIXTURE_MODEL_IDENTITY_V1 {
            return Err(CheckpointHeaderError::InvalidField {
                field: "model_identity",
                detail: format!(
                    "expected fixture model identity {:?}, got {:?}; real checkpoints are refused on the fixture path",
                    FIXTURE_MODEL_IDENTITY_V1, parsed.model_identity
                ),
            });
        }
        Ok(parsed)
    }
}

fn fixture_digest_hex(label: &str) -> String {
    let digest = Sha256::digest(label.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn fixture_digest_u64(label: &str) -> u64 {
    let digest = Sha256::digest(label.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(bytes)
}

/// Stand-in seat factory for test fixture runs (`--fixture` only).
#[derive(Debug, Clone)]
pub struct StandInCheckpoint {
    pub header: CheckpointHeader,
    pub params: FixtureStandInParams,
}

impl StandInCheckpoint {
    pub fn from_fixture(file: &FixtureCheckpointFile) -> Self {
        Self {
            header: file.header(),
            params: file.fixture_standin.clone(),
        }
    }
}

impl OwnerViewSeatFactory for StandInCheckpoint {
    fn make_learned_picker(&self) -> Box<dyn SeatPicker> {
        Box::new(StandInLearnedPicker {
            head_updates: self.header.head_updates,
            params: self.params.clone(),
        })
    }

    fn make_learned_builder(&self) -> Box<dyn SeatBuilder> {
        Box::new(StandInLearnedBuilder {
            head_updates: self.header.head_updates,
            params: self.params.clone(),
        })
    }

    fn is_stand_in(&self) -> bool {
        true
    }
}

fn hash_score_unit(seed: u64, tag: u64, key: u64) -> f32 {
    let mut rng = SplitMix64::seed(seed ^ tag.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ key);
    let word = rng.next_u64();
    let u = ((word >> 40) as f32) * (1.0f32 / ((1u32 << 24) as f32));
    2.0f32 * u - 1.0f32
}

fn decide_learned(
    kind: DecisionKind,
    options: &[u16],
    logits: &[f32],
    value: Option<f32>,
    stream: &mut SplitMix64,
) -> Result<(usize, DecisionRow), SeatError> {
    if options.is_empty() || options.len() != logits.len() {
        return Err(SeatError::Seat(
            "options and logits length mismatch in stand-in seat".to_owned(),
        ));
    }
    let mask = vec![true; options.len()];
    let word = stream.next_u64();
    let sample = sample_v1(logits, &mask, word)
        .map_err(|err| SeatError::Seat(format!("m4_sampler_v1 error: {err}")))?;
    let chosen_idx = sample.selected_index;
    let p_chosen = (sample.counts[chosen_idx] as f64) / (GRID_SIZE as f64);
    let row = DecisionRow {
        kind,
        sampler: LEARNED_SAMPLER_ID.to_owned(),
        options: options.to_vec(),
        chosen: options[chosen_idx],
        stream_values: vec![word],
        logits_bits: Some(logits.iter().map(|v| v.to_bits()).collect()),
        p_chosen_bits: Some(p_chosen.to_bits()),
        value_bits: value.map(f32::to_bits),
    };
    Ok((chosen_idx, row))
}

#[derive(Debug, Clone)]
struct StandInLearnedPicker {
    head_updates: HeadUpdates,
    params: FixtureStandInParams,
}

impl SeatPicker for StandInLearnedPicker {
    fn pick(
        &mut self,
        view: &SeatPickView<'_>,
        stream: &mut SplitMix64,
    ) -> Result<PickOutcome, SeatError> {
        if view.pack.len() == 1 {
            return Ok(PickOutcome {
                index: 0,
                row: None,
            });
        }
        let round = usize::from(view.round);
        let pick = usize::from(view.pick);
        if round >= ROUNDS || pick >= PACK_SIZE || view.pack.len() != PACK_SIZE - pick {
            return Err(SeatError::Seat(
                "invalid draft step or pack size".to_owned(),
            ));
        }
        let mut pack_sorted = view.pack.to_vec();
        pack_sorted.sort_unstable();

        let mut pool_colour_counts = [0u32; 5];
        for &id in view.pool {
            let bits = card_colour_bits(id) | land_colour_bits(id);
            for (c, count) in pool_colour_counts.iter_mut().enumerate() {
                if bits & (1u8 << c) != 0 {
                    *count += 1;
                }
            }
        }
        let scale = (self.params.pick_scale_milli as f32) / 1000.0f32;
        let logits: Vec<f32> = pack_sorted
            .iter()
            .map(|&card_id| {
                if self.head_updates.pick == 0 {
                    0.0f32
                } else {
                    let base = hash_score_unit(self.params.fixture_seed, 1, u64::from(card_id));
                    let bits = card_colour_bits(card_id) | land_colour_bits(card_id);
                    let mut synergy = 0.0f32;
                    for (c, &cnt) in pool_colour_counts.iter().enumerate() {
                        if bits & (1u8 << c) != 0 {
                            synergy += (cnt.min(6) as f32) * 0.05f32;
                        }
                    }
                    base * scale + synergy
                }
            })
            .collect();

        let value = if self.head_updates.value == 0 {
            0.0f32
        } else {
            let step = (round * PACK_SIZE + pick) as u64;
            0.1f32 * hash_score_unit(self.params.fixture_seed, 2, step ^ (view.pool.len() as u64))
        };

        let (_sorted_idx, row) = decide_learned(
            DecisionKind::Pick,
            &pack_sorted,
            &logits,
            Some(value),
            stream,
        )?;
        let index = view
            .pack
            .iter()
            .position(|&id| id == row.chosen)
            .ok_or_else(|| SeatError::Seat("chosen card missing from pack".to_owned()))?;
        Ok(PickOutcome {
            index,
            row: Some(row),
        })
    }
}

#[derive(Debug, Clone)]
struct StandInLearnedBuilder {
    head_updates: HeadUpdates,
    params: FixtureStandInParams,
}

impl SeatBuilder for StandInLearnedBuilder {
    fn build(&mut self, pool: &[u16], stream: &mut SplitMix64) -> Result<BuildOutcome, SeatError> {
        let mut sorted_pool = pool.to_vec();
        sorted_pool.sort_unstable();
        if sorted_pool.len() != POOL_CARDS
            || sorted_pool.windows(2).any(|w| w[0] == w[1])
            || sorted_pool
                .iter()
                .any(|&id| usize::from(id) >= CARD_DEFS.len())
        {
            return Err(SeatError::Seat(
                "expected 45 distinct valid registry cards".to_owned(),
            ));
        }

        let count_options: Vec<u16> = (0..=DECK_CARDS as u16).collect();
        let sharpness = (self.params.count_sharpness_milli as f32) / 1000.0f32;
        let count_logits: Vec<f32> = count_options
            .iter()
            .map(|&b| {
                if self.head_updates.count == 0 {
                    0.0f32
                } else {
                    let dist = (i32::from(b) - i32::from(self.params.target_basics)).abs() as f32;
                    -sharpness * dist
                }
            })
            .collect();
        let (_count_idx, count_row) = decide_learned(
            DecisionKind::Count,
            &count_options,
            &count_logits,
            None,
            stream,
        )?;
        let basics = count_row.chosen;

        let nonbasics_needed = (DECK_CARDS as usize) - usize::from(basics);
        let mut remaining = sorted_pool.clone();
        let mut chosen_cards = Vec::with_capacity(DECK_CARDS as usize);
        let mut pointer_rows = Vec::with_capacity(nonbasics_needed);
        let pointer_scale = (self.params.pointer_scale_milli as f32) / 1000.0f32;
        for _ in 0..nonbasics_needed {
            let logits: Vec<f32> = remaining
                .iter()
                .map(|&card_id| {
                    if self.head_updates.pointer == 0 {
                        0.0f32
                    } else {
                        pointer_scale
                            * hash_score_unit(self.params.fixture_seed, 3, u64::from(card_id))
                    }
                })
                .collect();
            let (idx, row) =
                decide_learned(DecisionKind::Pointer, &remaining, &logits, None, stream)?;
            let card = remaining.remove(idx);
            chosen_cards.push(card);
            pointer_rows.push(row);
        }

        let mut type_set_row = None;
        let mut split_rows = Vec::new();
        if basics > 0 {
            let masks: Vec<u16> = (1u16..32)
                .filter(|mask| mask.count_ones() <= u32::from(basics))
                .collect();
            let mut chosen_colours = [0u32; 5];
            for &id in &chosen_cards {
                let bits = card_colour_bits(id);
                for (c, count) in chosen_colours.iter_mut().enumerate() {
                    if bits & (1u8 << c) != 0 {
                        *count += 1;
                    }
                }
            }
            let type_scale = (self.params.type_set_scale_milli as f32) / 1000.0f32;
            let type_logits: Vec<f32> = masks
                .iter()
                .map(|&mask| {
                    if self.head_updates.type_set == 0 {
                        0.0f32
                    } else {
                        let size_penalty = (mask.count_ones() as i32 - 2).abs() as f32 * -0.8f32;
                        let mut colour_match = 0.0f32;
                        for (c, &cnt) in chosen_colours.iter().enumerate() {
                            if mask & (1u16 << c) != 0 {
                                colour_match += (cnt.min(10) as f32) * 0.08f32;
                            }
                        }
                        type_scale * (size_penalty + colour_match)
                    }
                })
                .collect();
            let (_mask_idx, ts_row) =
                decide_learned(DecisionKind::TypeSet, &masks, &type_logits, None, stream)?;
            let chosen_mask = ts_row.chosen;
            type_set_row = Some(ts_row);

            let bits: Vec<u16> = [1u16, 2, 4, 8, 16]
                .into_iter()
                .filter(|&bit| chosen_mask & bit != 0)
                .collect();
            let extra = usize::from(basics) - bits.len();
            let mut copies = vec![1usize; bits.len()];
            if bits.len() == 1 {
                copies[0] += extra;
            } else if extra > 0 {
                let split_scale = (self.params.split_scale_milli as f32) / 1000.0f32;
                let split_logits: Vec<f32> = bits
                    .iter()
                    .map(|&bit| {
                        if self.head_updates.split == 0 {
                            0.0f32
                        } else {
                            let c = bit.trailing_zeros() as usize;
                            split_scale * (chosen_colours[c] as f32) * 0.1f32
                        }
                    })
                    .collect();
                for _ in 0..extra {
                    let (idx, row) =
                        decide_learned(DecisionKind::Split, &bits, &split_logits, None, stream)?;
                    copies[idx] += 1;
                    split_rows.push(row);
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
